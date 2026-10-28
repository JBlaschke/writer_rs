//! Opt-in browser checks against the real serve CLI (live-reload.md §8–9).
//!
//! Fixtures are copied into private scratch directories and changed on disk.
//! T24 adds compiler-backed pages assertions; T23 exercises the live web view.

#![cfg(all(feature = "browser-tests", unix))]

use std::fs;
use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use writer::pdf::devtools::{Page, find_chrome, readable_temp_dir};

const WAIT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(25);
const PROBE: &str = r#"<script>
window.__qaLoads = +(sessionStorage.getItem('writer:qa:loads') || 0) + 1;
sessionStorage.setItem('writer:qa:loads', window.__qaLoads);
window.__qaLogs = [];
const qaDebug = console.debug;
console.debug = (...args) => {
    window.__qaLogs.push(args.map(String));
    qaDebug.apply(console, args);
};
</script>"#;

const READY: &str = r#"(async () => {
    const deadline = performance.now() + 8000;
    const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
    const bounded = (promise) => Promise.race([
        promise.then(() => true),
        sleep(Math.max(0, deadline - performance.now())).then(() => false)
    ]);
    if (!await bounded(document.fonts.ready)) return false;
    const done = () =>
        window.__writerLiveReload?.state.connected &&
        [...document.images].every((img) => img.complete && img.naturalWidth > 0) &&
        [...document.querySelectorAll('[data-math-style]')]
            .every((el) => el.dataset.rendered === '1');
    while (!done()) {
        if (performance.now() >= deadline) return false;
        await sleep(25);
    }
    return bounded(new Promise((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(resolve))));
})()"#;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn serial() -> MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    TURN.lock().unwrap_or_else(PoisonError::into_inner)
}

fn chrome() -> Option<PathBuf> {
    let found = find_chrome(None);
    if found.is_none() {
        assert!(
            std::env::var_os("WRITER_REQUIRE_CHROME")
                .is_none_or(|value| value.is_empty()),
            "no chrome found, but WRITER_REQUIRE_CHROME is set"
        );
        eprintln!("skipping: no chrome found");
    }
    found
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn append(path: &Path, text: &str) {
    let previous = fs::read_to_string(path).unwrap();
    write(path, &format!("{previous}{text}"));
}

fn copy_dir(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            fs::create_dir_all(&target).unwrap();
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

struct Project(tempfile::TempDir);

impl Project {
    fn new(chrome: &Path) -> Self {
        let base = readable_temp_dir(chrome)
            .expect("browser needs a readable scratch directory");
        let dir = tempfile::tempdir_in(base).unwrap();
        copy_dir(&root().join("tests/fixtures/serve"), dir.path());
        let template = fs::read_to_string(
            root().join("assets/themes/default/templates/document.html"),
        )
        .unwrap();
        let template = template.replace(
            "{% block body_start %}{% endblock %}",
            &format!("{{% block body_start %}}{PROBE}{{% endblock %}}")
        ).replace(
            "{% block body_end %}{% endblock %}",
            "{% block body_end %}<aside id=\"qa-outside\">Outside one</aside>{% endblock %}"
        );
        write(&dir.path().join("theme/templates/document.html"), &template);
        Self(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.path().join(name)
    }

    fn source(&self) -> PathBuf {
        self.path("document.md")
    }

    fn edit_line(&self, name: &str, line: usize, suffix: &str) {
        let path = self.path(name);
        let source = fs::read_to_string(&path).unwrap();
        let mut lines: Vec<String> =
            source.lines().map(str::to_owned).collect();
        lines[line - 1].push_str(suffix);
        write(&path, &format!("{}\n", lines.join("\n")));
    }
}

/// Child and both pipe readers are always stopped, reaped and joined.
struct Server {
    child: Option<Child>,
    url: String,
    port: u16,
    stderr: Arc<Mutex<String>>,
    readers: Vec<JoinHandle<()>>,
}

impl Server {
    fn start(project: &Project, input: &str, port: u16) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_writer"))
            .current_dir(project.0.path())
            .args(["serve", input, "--host", "127.0.0.1", "--port"])
            .arg(port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_remove("RUST_LOG")
            .env_remove("CLICOLOR_FORCE")
            .env("NO_COLOR", "1")
            .env("SOURCE_DATE_EPOCH", "0")
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let errors = Arc::new(Mutex::new(String::new()));
        let sink = errors.clone();
        let (tx, rx) = mpsc::channel();
        let out = thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
        let err = thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let mut log =
                    sink.lock().unwrap_or_else(PoisonError::into_inner);
                log.push_str(&line);
                log.push('\n');
            }
        });
        let mut server = Self {
            child: Some(child),
            url: String::new(),
            port: 0,
            stderr: errors,
            readers: vec![out, err],
        };
        let first = rx.recv_timeout(WAIT).unwrap_or_else(|error| {
            panic!("no serve URL: {error}; {}", server.errors())
        });
        let url = first
            .strip_prefix("Serving ")
            .unwrap_or_else(|| panic!("unexpected first line: {first:?}"));
        let address: SocketAddr = url
            .strip_prefix("http://")
            .unwrap()
            .trim_end_matches('/')
            .parse()
            .unwrap();
        assert_eq!(address.ip(), std::net::Ipv4Addr::LOCALHOST);
        assert_ne!(address.port(), 0);
        server.url = url.into();
        server.port = address.port();
        server
    }

    fn errors(&self) -> String {
        self.stderr.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

fn evaluate(page: &mut Page, expression: &str) -> Value {
    page.evaluate(expression)
        .unwrap_or_else(|error| panic!("{error}\nexpression: {expression}"))
}

fn navigation_error(error: &writer::Error) -> bool {
    let text = error.to_string();
    text.starts_with("Chrome (DevTools): Runtime.evaluate:")
        && ["navigated", "context", "closed"]
            .iter()
            .any(|part| text.contains(part))
}

fn trigger_navigation(page: &mut Page, expression: &str) {
    if let Err(error) = page.evaluate(expression) {
        assert!(navigation_error(&error), "{error}");
    }
}

fn wait_for(page: &mut Page, server: &Server, expression: &str) -> Value {
    let deadline = Instant::now() + WAIT;
    let mut last = Value::Null;
    loop {
        match page.evaluate(expression) {
            Ok(value) if !value.is_null() && value != json!(false) => {
                return value;
            }
            Ok(value) => last = value,
            Err(error) if navigation_error(&error) => {}
            Err(error) => {
                panic!("{error}\nexpression: {expression}\n{}", server.errors())
            }
        }
        assert!(
            Instant::now() < deadline,
            "condition timed out: {expression}\nlast: {last}\n{}",
            server.errors()
        );
        thread::sleep(POLL);
    }
}

fn open(chrome: &Path, server: &Server) -> Page {
    let mut page = Page::launch(chrome, WAIT).unwrap();
    page.set_timeout(WAIT);
    page.navigate(&server.url).unwrap();
    wait_for(
        &mut page,
        server,
        "!!window.__writerLiveReload?.state.connected && window.__writerLiveReload.state.buildId !== null",
    );
    assert_eq!(evaluate(&mut page, READY), true);
    page
}

fn build_id(page: &mut Page) -> u64 {
    evaluate(page, "window.__writerLiveReload.state.buildId").as_u64().unwrap()
}

fn wait_build(page: &mut Page, server: &Server, after: u64, extra: &str) {
    let expression = format!(
        "!!window.__writerLiveReload?.state.connected && \
         window.__writerLiveReload.state.buildId > {after} && ({extra})"
    );
    wait_for(page, server, &expression);
    assert_eq!(evaluate(page, READY), true);
}

fn assert_console_clean(page: &mut Page) {
    evaluate(page, "0");
    assert!(
        page.errors().is_empty(),
        "page errors: {}",
        page.errors().join("\n")
    );
    let logs = evaluate(
        page,
        "(window.__qaLogs || []).filter((args) => args[0] !== '[writer]')",
    );
    assert_eq!(logs, json!([]), "unexpected client log prefix");
}

fn record(name: &str, facts: &Value) {
    eprintln!("real-server {name}: {facts}");
}

fn anchor(page: &mut Page, paragraph: &str) -> Value {
    let paragraph = serde_json::to_string(paragraph).unwrap();
    evaluate(
        page,
        &format!(
            r#"(() => {{
        const el = [...document.querySelectorAll('p[data-source="0"]')]
            .find((el) => el.textContent.startsWith({paragraph}));
        if (!el) throw new Error('missing anchor paragraph');
        window.scrollTo(0, window.scrollY + el.getBoundingClientRect().top - 12);
        window.__qaMarker = 1;
        return {{ top: el.getBoundingClientRect().top, text: el.textContent,
            line: +el.dataset.sourcepos.split('-')[0] }};
    }})()"#
        ),
    )
}

fn paragraph_top(page: &mut Page, prefix: &str) -> f64 {
    let prefix = serde_json::to_string(prefix).unwrap();
    evaluate(page, &format!(
        "[...document.querySelectorAll('p[data-source=\"0\"]')].find((el) => \
         el.textContent.startsWith({prefix})).getBoundingClientRect().top"
    )).as_f64().unwrap()
}

#[test]
fn visible_text_and_css_changes_keep_the_reading_anchor_and_page() {
    let _turn = serial();
    let Some(chrome) = chrome() else { return };
    let project = Project::new(&chrome);
    let server = Server::start(&project, "document.md", 0);
    let mut page = open(&chrome, &server);
    let before = anchor(&mut page, "Paragraph 35:");
    let id = build_id(&mut page);
    project.edit_line(
        "document.md",
        before["line"].as_u64().unwrap() as usize,
        " Visible edit marker.",
    );
    wait_build(
        &mut page,
        &server,
        id,
        "document.querySelector('[data-writer-document]').textContent.includes('Visible edit marker.')",
    );
    let after = paragraph_top(&mut page, "Paragraph 35:");
    assert!((after - before["top"].as_f64().unwrap()).abs() <= 1.0);
    assert_eq!(evaluate(&mut page, "window.__qaMarker"), 1);

    evaluate(
        &mut page,
        r#"(() => {
        window.__qaCssObserver = new EventSource('/__writer/events');
        window.__qaCssObserver.addEventListener('build', (event) => {
            const data = JSON.parse(event.data);
            if (data.kind !== 'css') return;
            window.__qaCssEvent = data;
            window.__qaCssObserver.close();
        });
    })()"#,
    );
    wait_for(
        &mut page,
        &server,
        "window.__qaCssObserver.readyState === EventSource.OPEN",
    );
    let id = build_id(&mut page);
    append(
        &project.path("theme/styles/extra.css"),
        "\n:root { --serve-probe: after; }\n",
    );
    wait_build(
        &mut page,
        &server,
        id,
        "getComputedStyle(document.documentElement).getPropertyValue('--serve-probe').trim() === 'after' && window.__qaCssEvent?.pages.status === 'skipped'",
    );
    assert_eq!(evaluate(&mut page, "window.__qaMarker"), 1);
    assert_eq!(
        evaluate(
            &mut page,
            "window.__writerLiveReload.state.views.pages.status"
        ),
        "ok"
    );
    assert!((paragraph_top(&mut page, "Paragraph 35:") - after).abs() <= 1.0);
    let facts = evaluate(
        &mut page,
        "({marker: window.__qaMarker, build: window.__writerLiveReload.state.buildId, theme: document.querySelector('#writer-theme').getAttribute('href'), eventPages: window.__qaCssEvent.pages.status, clientPages: window.__writerLiveReload.state.views.pages.status})",
    );
    record(
        "visible edit and CSS",
        &json!({"before": before["top"], "after": after, "state": facts}),
    );
    assert_console_clean(&mut page);
}

#[test]
fn toc_growth_and_deleted_anchor_preserve_the_defined_reading_place() {
    let _turn = serial();
    let Some(chrome) = chrome() else { return };
    let project = Project::new(&chrome);
    let source = fs::read_to_string(project.source())
        .unwrap()
        .replace("toc: true\n", "");
    write(&project.source(), &source);
    append(&project.path("writer.toml"), "\n[document]\ntoc = false\n");
    let server = Server::start(&project, "document.md", 0);
    let mut page = open(&chrome, &server);
    assert_eq!(
        evaluate(&mut page, "!!document.querySelector('nav.toc')"),
        false
    );
    let before = anchor(&mut page, "Paragraph 40:");
    let id = build_id(&mut page);
    let config = fs::read_to_string(project.path("writer.toml")).unwrap();
    write(
        &project.path("writer.toml"),
        &config.replace("toc = false", "toc = true"),
    );
    wait_build(&mut page, &server, id, "!!document.querySelector('nav.toc')");
    let after_toc = paragraph_top(&mut page, "Paragraph 40:");
    assert!((after_toc - before["top"].as_f64().unwrap()).abs() <= 1.0);

    let before_delete = anchor(&mut page, "Paragraph 40:");
    let line = before_delete["line"].as_u64().unwrap() as usize;
    let source = fs::read_to_string(project.source()).unwrap();
    let mut lines: Vec<_> = source.lines().map(str::to_owned).collect();
    lines.remove(line - 1);
    if lines.get(line - 1).is_some_and(|line| line.is_empty()) {
        lines.remove(line - 1);
    }
    let id = build_id(&mut page);
    write(&project.source(), &format!("{}\n", lines.join("\n")));
    wait_build(
        &mut page,
        &server,
        id,
        "!document.querySelector('[data-writer-document]').textContent.includes('Paragraph 40:')",
    );
    let after_delete = paragraph_top(&mut page, "Paragraph 41:");
    assert!(
        (after_delete - before_delete["top"].as_f64().unwrap()).abs() <= 1.0
    );
    assert_eq!(evaluate(&mut page, "window.__qaMarker"), 1);
    record(
        "TOC and deleted anchor",
        &json!({
            "before": before["top"], "after_toc": after_toc,
            "before_delete": before_delete["top"], "after_delete": after_delete,
        }),
    );
    assert_console_clean(&mut page);
}

#[test]
fn offscreen_main_include_and_notebook_edits_scroll_to_their_source() {
    let _turn = serial();
    let Some(chrome) = chrome() else { return };
    let project = Project::new(&chrome);
    let source = fs::read_to_string(project.source())
        .unwrap()
        .replace(":::include chapter.md\n:::\n", "");
    let trailing = "Trailing context after the include.\n\n".repeat(20);
    write(
        &project.source(),
        &format!("{source}\n:::include chapter.md\n:::\n\n{trailing}"),
    );
    let server = Server::start(&project, "document.md", 0);
    let mut page = open(&chrome, &server);
    evaluate(&mut page, "scrollTo(0, 0)");
    let line = evaluate(&mut page,
        "+[...document.querySelectorAll('p[data-source=\"0\"]')].find((el) => el.textContent.startsWith('Paragraph 42:')).dataset.sourcepos.split('-')[0]").as_u64().unwrap() as usize;
    let id = build_id(&mut page);
    project.edit_line("document.md", line, " Main offscreen marker.");
    wait_build(
        &mut page,
        &server,
        id,
        "document.querySelector('[data-writer-document]').textContent.includes('Main offscreen marker.')",
    );
    let main = evaluate(
        &mut page,
        r#"(() => {
        const el = [...document.querySelectorAll('p[data-source="0"]')].find((el) => el.textContent.startsWith('Paragraph 42:'));
        const rect = el.getBoundingClientRect();
        return { center: (rect.top + rect.bottom) / 2, viewport: innerHeight / 2 };
    })()"#,
    );
    assert!(
        (main["center"].as_f64().unwrap() - main["viewport"].as_f64().unwrap())
            .abs()
            <= 2.0
    );
    let id = build_id(&mut page);
    project.edit_line("chapter.md", 3, " Included offscreen marker.");
    wait_build(
        &mut page,
        &server,
        id,
        "document.querySelector('[data-writer-document]').textContent.includes('Included offscreen marker.')",
    );
    let include = evaluate(
        &mut page,
        r#"(() => {
        const el = [...document.querySelectorAll('[data-source="1"][data-sourcepos]')].find((el) => el.textContent.includes('Included offscreen marker.'));
        const rect = el.getBoundingClientRect();
        return { center: (rect.top + rect.bottom) / 2, viewport: innerHeight / 2, source: el.dataset.source };
    })()"#,
    );
    assert!(
        (include["center"].as_f64().unwrap()
            - include["viewport"].as_f64().unwrap())
        .abs()
            <= 2.0
    );
    assert_eq!(include["source"], "1");
    record(
        "offscreen main/include",
        &json!({"main": main, "include": include}),
    );
    assert_console_clean(&mut page);
    drop(page);
    drop(server);

    let mut notebook: Value = serde_json::from_str(
        &fs::read_to_string(project.path("notebook.ipynb")).unwrap(),
    )
    .unwrap();
    notebook["cells"][0]["source"] =
        json!(["Notebook opening context.\n\n".repeat(60)]);
    notebook["cells"].as_array_mut().unwrap().push(json!({
        "cell_type": "markdown", "metadata": {},
        "source": ["Notebook trailing context.\n\n".repeat(20)],
    }));
    write(
        &project.path("notebook.ipynb"),
        &serde_json::to_string(&notebook).unwrap(),
    );
    let server = Server::start(&project, "notebook.ipynb", 0);
    let mut page = open(&chrome, &server);
    evaluate(&mut page, "scrollTo(0, 0)");
    let id = build_id(&mut page);
    notebook["cells"][3]["source"] =
        json!(["print('Edited notebook cell three')\n"]);
    write(
        &project.path("notebook.ipynb"),
        &serde_json::to_string(&notebook).unwrap(),
    );
    wait_build(
        &mut page,
        &server,
        id,
        "document.querySelector('[data-writer-document]').textContent.includes('Edited notebook cell three')",
    );
    let cell = evaluate(
        &mut page,
        r#"(() => {
        const el = document.querySelector('[data-source="0"][data-cell="3"]');
        const rect = el.getBoundingClientRect();
        return { center: (rect.top + rect.bottom) / 2, viewport: innerHeight / 2, cell: el.dataset.cell };
    })()"#,
    );
    assert!(
        (cell["center"].as_f64().unwrap() - cell["viewport"].as_f64().unwrap())
            .abs()
            <= 2.0
    );
    assert_eq!(cell["cell"], "3");
    record("offscreen notebook", &cell);
    assert_console_clean(&mut page);
}

#[test]
fn unchanged_math_and_images_keep_identity_while_changed_resources_refresh() {
    let _turn = serial();
    let Some(chrome) = chrome() else { return };
    let project = Project::new(&chrome);
    fs::copy(
        project.path("figures/plot.png"),
        project.path("figures/control.png"),
    )
    .unwrap();
    append(
        &project.source(),
        "\nStable equation $u^{123}$.\n\nChanging equation $v^{456}$.\n\n![Control image](figures/control.png)\n",
    );
    let server = Server::start(&project, "document.md", 0);
    let mut page = open(&chrome, &server);
    evaluate(
        &mut page,
        r#"(() => {
        const math = (tex) => [...document.querySelectorAll('[data-tex]')].find((el) => el.dataset.tex === tex);
        window.__qaStable = math('u^{123}');
        window.__qaStable.__qa = 1;
        window.__qaStableKatex = window.__qaStable.querySelector('.katex');
        window.__qaChangedKatex = math('v^{456}').querySelector('.katex');
        window.__qaMarker = 1;
    })()"#,
    );
    let id = build_id(&mut page);
    let source = fs::read_to_string(project.source()).unwrap();
    write(&project.source(), &source.replace("$v^{456}$", "$v^{457}$"));
    wait_build(
        &mut page,
        &server,
        id,
        "!![...document.querySelectorAll('[data-tex]')].find((el) => el.dataset.tex === 'v^{457}' && el.dataset.rendered === '1')",
    );
    let math = evaluate(
        &mut page,
        r#"(() => {
        const nodes = [...document.querySelectorAll('[data-tex]')];
        const stable = nodes.find((el) => el.dataset.tex === 'u^{123}');
        const edited = nodes.find((el) => el.dataset.tex === 'v^{457}');
        return { stable: stable === window.__qaStable && stable.__qa === 1,
            stableInk: stable.querySelector('.katex') === window.__qaStableKatex,
            changedInk: edited.querySelector('.katex') !== window.__qaChangedKatex };
    })()"#,
    );
    assert_eq!(
        math,
        json!({"stable": true, "stableInk": true, "changedInk": true})
    );
    let old = evaluate(
        &mut page,
        r#"(() => {
        window.__qaControl = document.querySelector('img[alt="Control image"]');
        const plot = document.querySelector('#fig-plot img');
        performance.clearResourceTimings();
        return { plot: plot.getAttribute('src'), control: window.__qaControl.getAttribute('src'), width: plot.naturalWidth };
    })()"#,
    );
    let id = build_id(&mut page);
    fs::copy(
        project.path("figures/plot-v2.png"),
        project.path("figures/plot.png"),
    )
    .unwrap();
    wait_build(
        &mut page,
        &server,
        id,
        "document.querySelector('#fig-plot img').naturalWidth === 24",
    );
    let images = evaluate(
        &mut page,
        r#"(() => {
        const plot = document.querySelector('#fig-plot img');
        const control = document.querySelector('img[alt="Control image"]');
        return { plot: plot.getAttribute('src'), control: control.getAttribute('src'), width: plot.naturalWidth,
            sameControl: control === window.__qaControl,
            requests: performance.getEntriesByType('resource').map((entry) => new URL(entry.name).pathname + new URL(entry.name).search)
                .filter((path) => path.startsWith('/__writer/doc-asset/')) };
    })()"#,
    );
    assert_ne!(old["plot"], images["plot"]);
    assert_eq!(old["control"], images["control"]);
    assert_eq!(images["sameControl"], true);
    assert_eq!(images["requests"], json!([images["plot"].clone()]));
    assert_eq!(evaluate(&mut page, "window.__qaMarker"), 1);
    record(
        "math and image identity",
        &json!({"math": math, "before": old, "after": images}),
    );
    assert_console_clean(&mut page);
}

#[test]
fn title_updates_in_place_and_template_or_raw_script_changes_reload_once() {
    let _turn = serial();
    let Some(chrome) = chrome() else { return };
    let project = Project::new(&chrome);
    let server = Server::start(&project, "document.md", 0);
    let mut page = open(&chrome, &server);
    evaluate(&mut page, "window.__qaMarker = 1");
    let source = fs::read_to_string(project.source()).unwrap();
    let id = build_id(&mut page);
    write(
        &project.source(),
        &source.replace("title: Live document", "title: Updated live title"),
    );
    wait_build(
        &mut page,
        &server,
        id,
        "document.title.includes('Updated live title')",
    );
    assert_eq!(
        evaluate(&mut page, "[window.__qaMarker, window.__qaLoads]"),
        json!([1, 1])
    );
    let template = project.path("theme/templates/document.html");
    let text = fs::read_to_string(&template).unwrap();
    let id = build_id(&mut page);
    write(&template, &text.replace("Outside one", "Outside two"));
    wait_build(
        &mut page,
        &server,
        id,
        "window.__qaLoads === 2 && document.querySelector('#qa-outside').textContent === 'Outside two'",
    );
    assert_eq!(evaluate(&mut page, "typeof window.__qaMarker"), "undefined");
    let id = build_id(&mut page);
    append(
        &project.source(),
        "\n<script>window.__qaRawRuns = (window.__qaRawRuns || 0) + 1;</script>\n",
    );
    wait_build(
        &mut page,
        &server,
        id,
        "window.__qaLoads === 3 && window.__qaRawRuns === 1",
    );
    let id = build_id(&mut page);
    append(
        &project.source(),
        "\nContent edited while the raw script remains.\n",
    );
    wait_build(
        &mut page,
        &server,
        id,
        "window.__qaLoads === 4 && window.__qaRawRuns === 1",
    );
    let facts = evaluate(
        &mut page,
        "({loads: window.__qaLoads, rawRuns: window.__qaRawRuns, title: document.title})",
    );
    record("title/template/raw script", &facts);
    assert_console_clean(&mut page);
}

#[test]
fn warnings_and_shared_parse_errors_have_live_overlays_and_clear_on_repair() {
    let _turn = serial();
    let Some(chrome) = chrome() else { return };
    let project = Project::new(&chrome);
    let original = fs::read_to_string(project.source()).unwrap();
    let server = Server::start(&project, "document.md", 0);
    let mut page = open(&chrome, &server);
    let id = build_id(&mut page);
    append(&project.source(), "\nAn unresolved reference is @nope.\n");
    wait_build(
        &mut page,
        &server,
        id,
        "window.__writerLiveReload.state.warnings.some((warning) => warning.code === 'unresolved-reference')",
    );
    let warnings = evaluate(
        &mut page,
        r#"(() => {
        const panel = document.querySelector('#writer-warnings');
        panel.click();
        return { text: panel.textContent, expanded: !!panel.querySelector('ol'), visible: getComputedStyle(panel).display !== 'none' };
    })()"#,
    );
    assert_eq!(warnings["visible"], true);
    assert_eq!(warnings["expanded"], true);
    assert!(
        warnings["text"].as_str().unwrap().contains("unresolved-reference")
    );
    let id = build_id(&mut page);
    append(&project.source(), "\n:::include missing-browser-include.md\n:::\n");
    wait_build(
        &mut page,
        &server,
        id,
        "window.__writerLiveReload.state.views.web.status === 'error'",
    );
    let overlay = evaluate(
        &mut page,
        r#"(() => {
        const el = document.querySelector('#writer-overlay');
        return { text: el.textContent, excerpts: el.querySelectorAll('pre').length,
            web: window.__writerLiveReload.state.views.web.status,
            pages: window.__writerLiveReload.state.views.pages.status,
            rootRetained: !!document.querySelector('[data-writer-document]') };
    })()"#,
    );
    assert!(
        overlay["text"]
            .as_str()
            .unwrap()
            .contains("[web, pages] build error (parse)")
    );
    assert_eq!(overlay["excerpts"], 1);
    assert_eq!(overlay["rootRetained"], true);
    evaluate(
        &mut page,
        r#"
            document.querySelector('#writer-overlay').click();
            window.__writerLiveReload.disconnect();
            window.__writerLiveReload.connect();
        "#,
    );
    wait_for(
        &mut page,
        &server,
        "!!window.__writerLiveReload?.state.connected",
    );
    assert_eq!(
        evaluate(
            &mut page,
            "document.querySelector('#writer-overlay').style.display"
        ),
        "none"
    );
    let id = build_id(&mut page);
    write(&project.source(), &original);
    wait_build(
        &mut page,
        &server,
        id,
        "window.__writerLiveReload.state.views.web.status === 'ok' && window.__writerLiveReload.state.warnings.length === 0",
    );
    assert_eq!(
        evaluate(
            &mut page,
            "[document.querySelector('#writer-overlay').style.display, document.querySelector('#writer-warnings').style.display]"
        ),
        json!(["none", "none"])
    );
    record(
        "warning and merged parse overlay",
        &json!({"warnings": warnings, "overlay": overlay}),
    );
    assert_console_clean(&mut page);
}

#[test]
fn fallback_dismissal_survives_reconnect_and_first_good_build_reloads() {
    let _turn = serial();
    let Some(chrome) = chrome() else { return };
    let project = Project::new(&chrome);
    let original = fs::read_to_string(project.source()).unwrap();
    write(&project.source(), ":::include initial-missing.md\n:::\n");
    let server = Server::start(&project, "document.md", 0);
    let mut page = open(&chrome, &server);
    assert_eq!(
        evaluate(
            &mut page,
            "!!document.querySelector('[data-writer-document]')"
        ),
        false
    );
    assert_eq!(evaluate(&mut page, "document.title"), "writer: build error");
    evaluate(
        &mut page,
        r#"
            window.__qaMarker = 1;
            document.querySelector('#writer-overlay').click();
            window.__writerLiveReload.disconnect();
            window.__writerLiveReload.connect();
        "#,
    );
    wait_for(
        &mut page,
        &server,
        "!!window.__writerLiveReload?.state.connected",
    );
    assert_eq!(
        evaluate(
            &mut page,
            "document.querySelector('#writer-overlay').style.display"
        ),
        "none"
    );
    let id = build_id(&mut page);
    write(&project.source(), &original);
    wait_build(
        &mut page,
        &server,
        id,
        "!!document.querySelector('[data-writer-document]')",
    );
    let facts = evaluate(
        &mut page,
        "({marker: typeof window.__qaMarker, loads: window.__qaLoads, title: document.title, status: window.__writerLiveReload.state.views.web.status})",
    );
    assert_eq!(facts["marker"], "undefined");
    assert_eq!(facts["loads"], 1);
    assert_eq!(facts["status"], "ok");
    record("fallback recovery", &facts);
    assert_console_clean(&mut page);
}

#[test]
fn view_keys_ignore_typing_and_root_remembers_the_last_explicit_view() {
    let _turn = serial();
    let Some(chrome) = chrome() else { return };
    let project = Project::new(&chrome);
    let server = Server::start(&project, "document.md", 0);
    let mut page = open(&chrome, &server);
    let ignored = evaluate(
        &mut page,
        r#"(() => {
        const input = document.body.appendChild(document.createElement('input'));
        input.focus();
        input.dispatchEvent(new KeyboardEvent('keydown', {key: 'p', bubbles: true}));
        input.blur();
        for (const modifier of ['ctrlKey', 'metaKey', 'altKey']) {
            document.body.dispatchEvent(new KeyboardEvent('keydown', {key: 'p', bubbles: true, [modifier]: true}));
        }
        const prevented = new KeyboardEvent('keydown', {key: 'p', bubbles: true, cancelable: true});
        prevented.preventDefault();
        document.body.dispatchEvent(prevented);
        return window.__writerLiveReload.state.view;
    })()"#,
    );
    assert_eq!(ignored, "web");
    trigger_navigation(
        &mut page,
        "document.body.dispatchEvent(new KeyboardEvent('keydown', {key: 'p', bubbles: true}))",
    );
    wait_for(
        &mut page,
        &server,
        "!!document.querySelector('[data-writer-pages]') && window.__writerLiveReload?.state.connected",
    );
    page.navigate(&server.url).unwrap();
    wait_for(
        &mut page,
        &server,
        "!!document.querySelector('[data-writer-pages]') && window.__writerLiveReload?.state.connected",
    );
    assert_eq!(evaluate(&mut page, "location.search"), "?view=pages");
    trigger_navigation(
        &mut page,
        "document.querySelector('#writer-view-toggle').click()",
    );
    wait_for(
        &mut page,
        &server,
        "!!document.querySelector('[data-writer-document]') && window.__writerLiveReload?.state.connected",
    );
    page.navigate(&format!("{}?view=pages", server.url)).unwrap();
    wait_for(
        &mut page,
        &server,
        "!!document.querySelector('[data-writer-pages]') && window.__writerLiveReload?.state.connected",
    );
    page.navigate(&format!("{}?view=web", server.url)).unwrap();
    wait_for(
        &mut page,
        &server,
        "!!document.querySelector('[data-writer-document]') && window.__writerLiveReload?.state.connected",
    );
    assert_eq!(
        evaluate(&mut page, "sessionStorage.getItem('writer:view')"),
        "web"
    );
    record(
        "view navigation",
        &evaluate(
            &mut page,
            "({view: window.__writerLiveReload.state.view, query: location.search, stored: sessionStorage.getItem('writer:view')})",
        ),
    );
    assert_console_clean(&mut page);
}

#[test]
fn outage_backoff_restart_missed_builds_and_stale_tabs_follow_the_protocol() {
    let _turn = serial();
    let Some(chrome) = chrome() else { return };
    let project = Project::new(&chrome);
    let mut server = Server::start(&project, "document.md", 0);
    let mut page = open(&chrome, &server);
    evaluate(
        &mut page,
        r#"(() => {
        window.__qaMarker = 1;
        window.__qaRetries = [performance.now()];
        const Native = window.EventSource;
        window.EventSource = function (url) {
            window.__qaRetries.push(performance.now());
            return new Native(url);
        };
    })()"#,
    );
    let port = server.port;
    server.stop();
    wait_for(&mut page, &server, "window.__qaRetries.length >= 4");
    wait_for(&mut page, &server, "window.__qaRetries.length >= 6");
    wait_for(&mut page, &server, "window.__qaRetries.length >= 7");
    let retry = evaluate(
        &mut page,
        "window.__qaRetries.slice(1).map((time, index) => time - window.__qaRetries[index])",
    );
    let expected = [500.0, 1000.0, 2000.0, 4000.0, 5000.0, 5000.0];
    for (actual, expected) in retry.as_array().unwrap().iter().zip(expected) {
        let actual = actual.as_f64().unwrap();
        assert!(
            actual >= expected * 0.8 && actual < expected + 1500.0,
            "retry {retry}"
        );
    }
    assert_eq!(
        evaluate(&mut page, "[window.__qaMarker, window.__qaLoads]"),
        json!([1, 1])
    );
    append(&project.source(), "\nThe server restarted with this new text.\n");
    let server = Server::start(&project, "document.md", port);
    wait_for(
        &mut page,
        &server,
        "window.__writerLiveReload?.state.connected && document.querySelector('[data-writer-document]').textContent.includes('The server restarted with this new text.')",
    );
    assert_eq!(
        evaluate(&mut page, "[window.__qaMarker, window.__qaLoads]"),
        json!([1, 1])
    );
    evaluate(&mut page, "window.__writerLiveReload.disconnect()");
    append(&project.source(), "\nA build was missed while disconnected.\n");
    wait_for(
        &mut page,
        &server,
        "fetch('/__writer/state', {cache: 'no-store'}).then((response) => response.json()).then((state) => state.build_id > 1)",
    );
    evaluate(&mut page, "window.__writerLiveReload.connect()");
    wait_for(
        &mut page,
        &server,
        "window.__writerLiveReload?.state.connected && document.querySelector('[data-writer-document]').textContent.includes('A build was missed while disconnected.')",
    );
    assert_eq!(
        evaluate(&mut page, "[window.__qaMarker, window.__qaLoads]"),
        json!([1, 1])
    );
    evaluate(&mut page, "sessionStorage.setItem('writer:build_id:/', '0')");
    page.navigate(&server.url).unwrap();
    wait_for(
        &mut page,
        &server,
        "window.__writerLiveReload?.state.connected && window.__qaLoads === 3",
    );
    let facts = evaluate(
        &mut page,
        "({loads: window.__qaLoads, status: window.__writerLiveReload.state.lastStatus, marker: typeof window.__qaMarker})",
    );
    record(
        "outage/reconnect/stale tab",
        &json!({"retry_ms": retry, "fresh_tab": facts}),
    );
    evaluate(&mut page, "0");
    assert!(
        page.errors().iter().all(|error| error.contains("net::ERR_")),
        "unexpected page errors during outage: {:?}",
        page.errors()
    );
    let logs = evaluate(
        &mut page,
        "(window.__qaLogs || []).filter((args) => args[0] !== '[writer]')",
    );
    assert_eq!(logs, json!([]));
}
