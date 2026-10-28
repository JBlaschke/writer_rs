//! Real `writer serve` processes, HTTP/SSE and watched-source recovery.
//! Harness conventions are lifted from presenter_rs, with bounded waits and
//! owned output/SSE readers. Actual compiler pages are added by T24.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::ExitStatus;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use regex::Regex;
use serde_json::{Value, json};

const WAIT: Duration = Duration::from_secs(10);
const HTTP_WAIT: Duration = Duration::from_secs(2);

fn serial() -> MutexGuard<'static, ()> {
    static CHILDREN: Mutex<()> = Mutex::new(());
    CHILDREN.lock().unwrap_or_else(PoisonError::into_inner)
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/serve")
}

fn copy_dir(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            fs::create_dir(&target).unwrap();
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(&fixture(), dir.path());
    dir
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .proxy(None)
        .timeout_global(Some(HTTP_WAIT))
        .build()
        .into()
}

struct Response {
    status: u16,
    headers: ureq::http::HeaderMap,
    body: Vec<u8>,
}

impl Response {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|value| value.to_str().ok())
    }
}

/// Each subprocess owns every reader it starts. Drop closes sockets, kills and
/// reaps the child, then joins readers after their pipes have reached EOF.
struct Server {
    child: Child,
    url: String,
    stderr: Arc<Mutex<String>>,
    stdout: mpsc::Receiver<String>,
    readers: Vec<JoinHandle<()>>,
    sockets: Vec<TcpStream>,
}

impl Server {
    fn start(dir: &Path, input: &str, extra: &[&str]) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_writer"));
        command.args(["serve", input, "--port", "0"]).args(extra);
        let mut server = Self::spawn(dir, command);
        let first = server.stdout.recv_timeout(WAIT).unwrap_or_else(|error| {
            panic!("no serving URL: {error}; {}", server.stderr())
        });
        let url = first
            .strip_prefix("Serving ")
            .unwrap_or_else(|| panic!("first stdout line: {first:?}"));
        assert!(url.starts_with("http://") && url.ends_with('/'), "{first}");
        let addr: SocketAddr = url
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .parse()
            .unwrap();
        assert_ne!(addr.port(), 0);
        server.url = url.to_string();
        server
    }

    fn spawn(dir: &Path, mut command: Command) -> Self {
        let mut child = command
            .current_dir(dir)
            .env_remove("RUST_LOG")
            .env_remove("CLICOLOR_FORCE")
            .env("NO_COLOR", "1")
            .env("SOURCE_DATE_EPOCH", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stderr = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&stderr);
        let mut err = child.stderr.take().unwrap();
        let err_reader = thread::spawn(move || {
            let mut buf = [0; 4096];
            while let Ok(count) = err.read(&mut buf) {
                if count == 0 {
                    break;
                }
                sink.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push_str(&String::from_utf8_lossy(&buf[..count]));
            }
        });
        let (tx, stdout) = mpsc::channel();
        let out = child.stdout.take().unwrap();
        let out_reader = thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            url: String::new(),
            stderr,
            stdout,
            readers: vec![err_reader, out_reader],
            sockets: Vec::new(),
        }
    }

    fn stderr(&self) -> String {
        self.stderr.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn wait_stderr(&self, text: &str, count: usize) -> String {
        let deadline = Instant::now() + WAIT;
        loop {
            let stderr = self.stderr();
            if stderr.matches(text).count() >= count {
                return stderr;
            }
            assert!(Instant::now() < deadline, "missing {text:?}:\n{stderr}");
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn addr(&self) -> SocketAddr {
        self.url
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .parse()
            .unwrap()
    }

    fn get(&self, path: &str) -> Response {
        let url = format!("{}{}", self.url, path.trim_start_matches('/'));
        let mut response = agent().get(&url).call().unwrap_or_else(|error| {
            panic!("GET {path}: {error}; {}", self.stderr())
        });
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let body = response
            .body_mut()
            .with_config()
            .limit(64 * 1024 * 1024)
            .read_to_vec()
            .unwrap();
        Response { status, headers, body }
    }

    fn text(&self, path: &str) -> String {
        let response = self.get(path);
        assert_eq!(response.status, 200, "GET {path}: {:?}", response.body);
        String::from_utf8(response.body).unwrap()
    }

    fn state(&self) -> Value {
        serde_json::from_str(&self.text("/__writer/state")).unwrap()
    }

    fn wait_state(&self, after: u64, done: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + WAIT;
        loop {
            let state = self.state();
            if id(&state) > after && done(&state) {
                return state;
            }
            assert!(
                Instant::now() < deadline,
                "no matching build after {after}: {state}; {}",
                self.stderr()
            );
            thread::sleep(Duration::from_millis(30));
        }
    }

    fn events(&mut self) -> Events {
        // HTTP/1.0 gives an EOF-delimited event body, avoiding a second chunk
        // decoder. A cloned socket lets Drop wake this owned reader immediately.
        let mut stream =
            TcpStream::connect_timeout(&self.addr(), HTTP_WAIT).unwrap();
        stream.set_read_timeout(Some(WAIT)).unwrap();
        stream.set_write_timeout(Some(HTTP_WAIT)).unwrap();
        let request = format!(
            "GET /__writer/events HTTP/1.0\r\nHost: {}\r\n\r\n",
            self.addr()
        );
        stream.write_all(request.as_bytes()).unwrap();
        self.sockets.push(stream.try_clone().unwrap());
        let (tx, rx) = mpsc::channel();
        self.readers.push(thread::spawn(move || {
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                return;
            }
            assert!(line.contains(" 200 "), "SSE status: {line}");
            loop {
                line.clear();
                if !matches!(reader.read_line(&mut line), Ok(n) if n > 0) {
                    return;
                }
                if line.trim().is_empty() {
                    break;
                }
            }
            let mut event = String::new();
            let mut data = String::new();
            loop {
                line.clear();
                if !matches!(reader.read_line(&mut line), Ok(n) if n > 0) {
                    return;
                }
                let line = line.trim_end_matches(['\r', '\n']);
                if line.is_empty() {
                    if !data.is_empty() {
                        let value = serde_json::from_str(&data).unwrap();
                        if tx.send((event.clone(), value)).is_err() {
                            return;
                        }
                    }
                    event.clear();
                    data.clear();
                } else if let Some(name) = line.strip_prefix("event:") {
                    event = name.trim_start().to_string();
                } else if let Some(value) = line.strip_prefix("data:") {
                    assert!(data.is_empty(), "JSON must occupy one data line");
                    data = value.trim_start().to_string();
                }
            }
        }));
        Events(rx)
    }

    #[cfg(unix)]
    fn wait_exit(&mut self) -> ExitStatus {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                self.join_readers();
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "child did not exit: {}",
                self.stderr()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn join_readers(&mut self) {
        for socket in self.sockets.drain(..) {
            let _ = socket.shutdown(Shutdown::Both);
        }
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }

    #[cfg(unix)]
    fn interrupt(&mut self) -> ExitStatus {
        let output = assert_cmd::Command::new("/bin/kill")
            .args(["-INT", &self.child.id().to_string()])
            .timeout(WAIT)
            .output()
            .unwrap();
        assert!(output.status.success());
        self.wait_exit()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        for socket in &self.sockets {
            let _ = socket.shutdown(Shutdown::Both);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.join_readers();
    }
}

struct Events(mpsc::Receiver<(String, Value)>);

impl Events {
    fn hello(&self, server: &Server) -> Value {
        let (name, value) = self.0.recv_timeout(WAIT).unwrap_or_else(|error| {
            panic!("no hello: {error}; {}", server.stderr())
        });
        assert_eq!(name, "hello");
        value
    }

    fn build(
        &self, server: &Server, after: u64, done: impl Fn(&Value) -> bool,
    ) -> Value {
        let deadline = Instant::now() + WAIT;
        loop {
            let (name, value) = self
                .0
                .recv_timeout(
                    deadline.saturating_duration_since(Instant::now()),
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "no matching build after {after}: {error}; {}",
                        server.stderr()
                    )
                });
            assert_eq!(name, "build");
            if id(&value) > after && done(&value) {
                return value;
            }
        }
    }
}

fn id(state: &Value) -> u64 {
    state["build_id"].as_u64().unwrap()
}

fn source_line(text: &str, marker: &str) -> usize {
    text.lines().position(|line| line.contains(marker)).unwrap() + 1
}

fn asset_url(html: &str) -> String {
    let pattern = Regex::new(
        r#"src="(/__writer/doc-asset/[0-9a-f]{12}\?v=[0-9A-Za-z]+)""#,
    )
    .unwrap();
    pattern.captures(html).unwrap_or_else(|| panic!("no asset URL in {html}"))
        [1]
    .to_string()
}

fn status_for_host(server: &Server, path: &str, host: &str) -> u16 {
    let mut stream =
        TcpStream::connect_timeout(&server.addr(), HTTP_WAIT).unwrap();
    stream.set_read_timeout(Some(HTTP_WAIT)).unwrap();
    stream.set_write_timeout(Some(HTTP_WAIT)).unwrap();
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).unwrap();
    line.split_whitespace().nth(1).unwrap().parse().unwrap()
}

#[test]
fn serve_exposes_the_protocol_and_rebuilds_source_include_css_and_image() {
    let _turn = serial();
    let dir = project();
    let mut server = Server::start(dir.path(), "document.md", &[]);
    let response = server.get("/");
    assert_eq!(response.status, 200);
    assert_eq!(response.header("cache-control"), Some("no-store"));
    assert_eq!(
        response.header("content-type"),
        Some("text/html; charset=utf-8")
    );
    let html = String::from_utf8(response.body).unwrap();
    for hook in [
        "data-writer-document", "data-sourcepos=\"", "data-source=\"0\"",
        "id=\"writer-theme\"", "/__writer/live-reload.js",
    ] {
        assert!(html.contains(hook), "missing {hook}");
    }
    assert!(
        !dir.path().join("dist").exists(),
        "serve must not write publication files"
    );
    let initial = server.state();
    assert_eq!(id(&initial), 1);
    assert_eq!(initial["status"], "ok");
    assert_eq!(initial["web"]["status"], "ok");
    assert_eq!(initial["pages"]["status"], "ok");
    assert_eq!(initial["pages"]["count"], 0, "T24 owns actual pages");
    let events = server.events();
    assert_eq!(events.hello(&server), initial);
    assert!(
        server
            .text("/__writer/live-reload.js")
            .starts_with("/* morphdom 2.7.8")
    );

    let source = dir.path().join("document.md");
    let old = fs::read_to_string(&source).unwrap();
    let line = source_line(&old, "Final paragraph marker");
    write(
        &source,
        &old.replace("the report ends here", "the revised report ends here"),
    );
    let changed =
        events.build(&server, 1, |event| event["changed"]["line"] == line);
    assert_eq!(changed["kind"], "content");
    assert_eq!(
        changed["changed"],
        json!({"file":"document.md", "source":0, "line":line})
    );
    assert!(server.text("/").contains("the revised report ends here"));

    let chapter = dir.path().join("chapter.md");
    let old = fs::read_to_string(&chapter).unwrap();
    write(&chapter, &old.replace("first observation", "revised observation"));
    let included = events
        .build(&server, id(&changed), |event| event["changed"]["source"] == 1);
    assert_eq!(
        included["changed"],
        json!({"file":"chapter.md", "source":1, "line":3})
    );
    assert!(server.text("/").contains("revised observation"));

    write(
        &dir.path().join("theme/styles/extra.css"),
        ":root { --serve-probe: after; }\n",
    );
    let css =
        events.build(&server, id(&included), |event| event["kind"] == "css");
    assert_eq!(css["changed"], Value::Null);
    assert_eq!(css["web"]["status"], "ok");
    assert_eq!(css["pages"]["status"], "skipped");
    assert_eq!(css["pages"]["hashes"], initial["pages"]["hashes"]);
    assert!(
        server.text("/__writer/theme.css").contains("--serve-probe: after")
    );

    let before = asset_url(&server.text("/"));
    fs::copy(
        dir.path().join("figures/plot-v2.png"),
        dir.path().join("figures/plot.png"),
    )
    .unwrap();
    let image =
        events.build(&server, id(&css), |event| event["kind"] == "content");
    assert_eq!(image["status"], "ok");
    assert_eq!(image["changed"], Value::Null);
    let after = asset_url(&server.text("/"));
    assert_ne!(before, after);
    assert_eq!(
        before.split('?').next(),
        after.split('?').next(),
        "file IDs stay stable"
    );
    assert_eq!(after.matches("?v=").count(), 1);
    assert_eq!(
        server.get(&after).body,
        fs::read(dir.path().join("figures/plot-v2.png")).unwrap()
    );
    let stderr = server.wait_stderr("rebuilt in ", 5);
    assert!(
        Regex::new(r"rebuilt in \d+ ms \(css\)").unwrap().is_match(&stderr),
        "{stderr}"
    );
    assert!(server.stdout.try_iter().next().is_none());
}

#[test]
fn a_transitive_include_failure_retains_good_html_then_repairs() {
    let _turn = serial();
    let dir = project();
    let mut server = Server::start(dir.path(), "document.md", &[]);
    let events = server.events();
    let initial = events.hello(&server);
    let good = server.text("/");
    let chapter = dir.path().join("chapter.md");
    let original = fs::read_to_string(&chapter).unwrap();
    write(&chapter, &format!("{original}\n:::include missing/deep.md\n:::\n"));
    let failed =
        events.build(&server, id(&initial), |event| event["status"] == "error");
    assert_eq!(failed["web"]["error"], failed["pages"]["error"]);
    assert_eq!(failed["web"]["error"]["kind"], "parse");
    assert_eq!(failed["changed"], Value::Null);
    assert_eq!(server.text("/"), good);
    write(
        &dir.path().join("missing/deep.md"),
        "### Repaired transitive include\n\nRecovered body.\n",
    );
    let fixed =
        events.build(&server, id(&failed), |event| event["status"] == "ok");
    assert!(fixed["web"].get("error").is_none());
    assert!(fixed["pages"].get("error").is_none());
    assert!(server.text("/").contains("Recovered body."));
    server.wait_stderr("cannot include", 1);
}

#[test]
fn notebook_cell_edits_and_metadata_only_changes_have_distinct_positions() {
    let _turn = serial();
    let dir = project();
    let mut server = Server::start(dir.path(), "notebook.ipynb", &[]);
    let events = server.events();
    let initial = events.hello(&server);
    assert!(server.text("/").contains("saved output 2"));
    assert!(server.text("/").contains("data-cell=\"2\""));
    let path = dir.path().join("notebook.ipynb");
    let mut notebook: Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    notebook["cells"][2]["source"] = json!(["print('changed cell two')\n"]);
    write(&path, &serde_json::to_string_pretty(&notebook).unwrap());
    let changed = events
        .build(&server, id(&initial), |event| event["changed"]["cell"] == 2);
    assert_eq!(
        changed["changed"],
        json!({"file":"notebook.ipynb", "source":0, "cell":2})
    );
    assert!(server.text("/").contains("changed cell two"));
    notebook["metadata"]["test_note"] = json!("metadata changed");
    write(&path, &serde_json::to_string_pretty(&notebook).unwrap());
    let metadata = events.build(&server, id(&changed), |_| true);
    assert_eq!(metadata["changed"], Value::Null);
}

#[test]
fn every_rendered_equation_image_is_in_the_served_registry() {
    let _turn = serial();
    let dir = project();
    let server = Server::start(dir.path(), "math-image.md", &[]);
    let html = server.text("/");
    let pattern = Regex::new(r#"src="(/__writer/doc-asset/[^"]+)""#).unwrap();
    let urls: Vec<_> = pattern
        .captures_iter(&html)
        .map(|capture| capture[1].to_string())
        .collect();
    assert_eq!(urls.len(), 2, "{html}");
    for url in urls {
        let response = server.get(&url);
        assert_eq!(response.status, 200, "{url}");
        assert_eq!(response.header("content-type"), Some("image/svg+xml"));
        assert!(String::from_utf8_lossy(&response.body).contains("<svg"));
    }
    assert_eq!(server.state()["status"], "ok");
}

#[test]
fn view_defaults_and_explicit_queries_follow_cli_and_config() {
    let _turn = serial();
    let dir = project();
    for (extra, configured, expected) in [
        (vec![], None, false),
        (vec![], Some("pages"), true),
        (vec!["--view", "pages"], None, true),
        (vec!["--view", "web"], Some("pages"), false),
    ] {
        let mut config =
            fs::read_to_string(fixture().join("writer.toml")).unwrap();
        if let Some(view) = configured {
            config.push_str(&format!("\n[serve]\nview = '{view}'\n"));
        }
        write(&dir.path().join("writer.toml"), &config);
        let server = Server::start(dir.path(), "document.md", &extra);
        let pages =
            |path| server.text(path).contains("data-writer-view=\"pages\"");
        assert_eq!(pages("/"), expected);
        assert_eq!(pages("/?view=unknown"), expected);
        assert!(!pages("/?view=web"));
        assert!(pages("/?view=pages"));
        assert!(pages("/__writer/pages"));
    }
}

#[test]
fn quiet_keeps_the_url_and_errors_but_hides_rebuild_reports() {
    let _turn = serial();
    let dir = project();
    write(&dir.path().join("document.md"), "---\ntitle: a: b\n---\n");
    let mut server = Server::start(dir.path(), "document.md", &["--quiet"]);
    let events = server.events();
    let failed = events.hello(&server);
    assert_eq!(failed["status"], "error");
    let payload = &failed["web"]["error"];
    assert_eq!(payload["kind"], "front-matter");
    assert_eq!(payload["file"], "document.md");
    assert_eq!(payload["line"], 2);
    assert_eq!(payload["excerpt"], "title: a: b");
    assert_eq!(failed["pages"]["error"], *payload);
    let fallback = server.text("/");
    assert!(fallback.contains("writer: build error"));
    assert!(fallback.contains("title: a: b</pre>"));
    server.wait_stderr("error:", 1);
    write(&dir.path().join("document.md"), "# Fixed quietly\n");
    events.build(&server, id(&failed), |event| event["status"] == "ok");
    assert!(server.text("/").contains("Fixed quietly"));
    assert!(!server.stderr().contains("rebuilt in "));
    assert_eq!(
        server.stderr().matches("error:").count(),
        1,
        "{}",
        server.stderr()
    );
    assert!(server.stdout.try_iter().next().is_none());
}

#[test]
fn invalid_startup_config_recovers_and_address_restart_notes_are_deduplicated()
{
    let _turn = serial();
    let dir = project();
    write(&dir.path().join("writer.toml"), "[html\n");
    let mut server = Server::start(dir.path(), "document.md", &[]);
    let events = server.events();
    let failed = events.hello(&server);
    assert_eq!(failed["web"]["error"]["kind"], "config");
    assert_eq!(failed["web"]["error"]["file"], "writer.toml");
    assert!(server.stderr().contains("the [serve] settings cannot be read"));
    let config = "[pdf]\nsystem_fonts=false\n[serve]\nhost='localhost'\n";
    write(&dir.path().join("writer.toml"), config);
    let fixed =
        events.build(&server, id(&failed), |event| event["status"] == "ok");
    server.wait_stderr("restart writer serve", 1);
    write(
        &dir.path().join("writer.toml"),
        &format!("{config}# edit same desired address\n"),
    );
    let again =
        events.build(&server, id(&fixed), |event| event["status"] == "ok");
    assert_eq!(server.stderr().matches("restart writer serve").count(), 1);
    write(&dir.path().join("writer.toml"), "[pdf]\nsystem_fonts=false\n");
    let reset =
        events.build(&server, id(&again), |event| event["status"] == "ok");
    write(&dir.path().join("writer.toml"), config);
    events.build(&server, id(&reset), |event| event["status"] == "ok");
    server.wait_stderr("restart writer serve", 2);
    assert!(server.url.starts_with("http://127.0.0.1:"));
}

#[test]
fn missing_explicit_config_follows_new_ancestors_until_the_file_exists() {
    let _turn = serial();
    let dir = project();
    let server = Server::start(
        dir.path(),
        "document.md",
        &["--config", "new/a/writer.toml"],
    );
    let initial = server.state();
    assert_eq!(initial["status"], "error");
    fs::create_dir(dir.path().join("new")).unwrap();
    let first =
        server.wait_state(id(&initial), |state| state["status"] == "error");
    fs::create_dir(dir.path().join("new/a")).unwrap();
    let second =
        server.wait_state(id(&first), |state| state["status"] == "error");
    write(&dir.path().join("new/a/writer.toml"), "[pdf]\nsystem_fonts=false\n");
    let fixed = server.wait_state(id(&second), |state| state["status"] == "ok");
    assert!(fixed["web"].get("error").is_none());
    assert!(server.text("/").contains("Live document"));
}

#[test]
fn a_missing_theme_at_start_can_be_created_without_restarting() {
    let _turn = serial();
    let dir = project();
    fs::remove_dir_all(dir.path().join("theme")).unwrap();
    let server = Server::start(dir.path(), "document.md", &[]);
    let initial = server.state();
    assert_eq!(initial["web"]["error"]["kind"], "theme");
    fs::create_dir(dir.path().join("theme")).unwrap();
    let next =
        server.wait_state(id(&initial), |state| state["status"] == "error");
    write(
        &dir.path().join("theme/theme.toml"),
        "name='recovered'\nextends='default'\n",
    );
    server.wait_state(id(&next), |state| state["status"] == "ok");
    assert!(server.text("/").contains("Live document"));
}

#[test]
fn warning_and_nonfatal_error_diagnostics_are_served_then_clear() {
    let _turn = serial();
    let dir = project();
    write(&dir.path().join("document.md"), "# Warning preview\n\nSee @nope.\n");
    let mut server = Server::start(dir.path(), "document.md", &[]);
    let events = server.events();
    let initial = events.hello(&server);
    assert_eq!(initial["status"], "ok");
    assert!(
        initial["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["code"] == "unresolved-reference")
    );
    write(
        &dir.path().join("document.md"),
        "# One {#sec-dup}\n\n# Two {#sec-dup}\n",
    );
    let duplicate = events.build(&server, id(&initial), |_| true);
    assert_eq!(duplicate["status"], "ok");
    assert!(
        duplicate["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["level"] == "error"
                && item["code"] == "duplicate-label")
    );
    assert!(server.text("/").contains("Two"));
    assert!(
        !duplicate["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["code"] == "unresolved-reference")
    );
    write(&dir.path().join("document.md"), "# No warnings now\n");
    let fixed = events.build(&server, id(&duplicate), |_| true);
    assert_eq!(fixed["warnings"], json!([]));
}

#[test]
fn title_changes_update_web_and_pages_shells_without_changing_the_default_view()
{
    let _turn = serial();
    let dir = project();
    let server = Server::start(dir.path(), "document.md", &[]);
    let initial = server.state();
    let path = dir.path().join("document.md");
    let source = fs::read_to_string(&path).unwrap();
    write(
        &path,
        &source.replace("title: Live document", "title: Changed live title"),
    );
    server.wait_state(id(&initial), |_| true);
    assert!(server.text("/").contains("Changed live title"));
    assert!(
        server.text("/__writer/pages").contains("Changed live title · pages")
    );
    let config = fs::read_to_string(dir.path().join("writer.toml")).unwrap();
    let after = server.state();
    write(
        &dir.path().join("writer.toml"),
        &format!("{config}\n[serve]\nview='pages'\n"),
    );
    server.wait_state(id(&after), |_| true);
    assert!(server.text("/").contains("data-writer-document"));
}

#[test]
fn host_checks_cover_cli_routes_and_unknown_paths() {
    let _turn = serial();
    let dir = project();
    let server = Server::start(dir.path(), "document.md", &[]);
    let evil = format!("evil.example:{}", server.addr().port());
    for path in [
        "/",
        "/__writer/state",
        "/__writer/events",
        "/__writer/theme.css",
        "/__writer/live-reload.js",
        "/__writer/pages",
        "/__writer/pages.js",
        "/__writer/pages/1.svg",
        "/__writer/asset/vendor/katex/katex.min.js",
        "/no-such-page",
    ] {
        assert_eq!(status_for_host(&server, path, &evil), 403, "{path}");
    }
    let local = format!("localhost:{}", server.addr().port());
    assert_eq!(status_for_host(&server, "/", &local), 200);
}

fn command_output(dir: &Path, arguments: &[&str]) -> std::process::Output {
    assert_cmd::Command::new(env!("CARGO_BIN_EXE_writer"))
        .current_dir(dir)
        .args(arguments)
        .env_remove("RUST_LOG")
        .env_remove("CLICOLOR_FORCE")
        .env("NO_COLOR", "1")
        .timeout(WAIT)
        .output()
        .unwrap()
}

#[test]
fn occupied_localhost_and_wildcard_ports_exit_one_with_a_free_port_hint() {
    let _turn = serial();
    let dir = project();
    for host in ["127.0.0.1", "0.0.0.0"] {
        let listener = TcpListener::bind((host, 0)).unwrap();
        let port = listener.local_addr().unwrap().port().to_string();
        let output = command_output(
            dir.path(),
            &["serve", "document.md", "--port", &port],
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{host}: {stderr}");
        assert!(output.stdout.is_empty());
        assert!(stderr.contains("--port 0"), "{stderr}");
    }
}

#[test]
fn unusable_host_and_unknown_input_have_actionable_exit_codes() {
    let _turn = serial();
    let dir = project();
    let bad_host = command_output(
        dir.path(),
        &["serve", "document.md", "--host", "[broken", "--port", "0"],
    );
    assert_eq!(bad_host.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&bad_host.stderr).contains("--host"));
    let unknown =
        command_output(dir.path(), &["serve", "unknown.txt", "--port", "0"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(unknown.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&unknown.stderr).contains("--format"));
}

#[cfg(unix)]
#[test]
fn interrupt_ends_an_active_sse_connection_and_exits_zero() {
    let _turn = serial();
    let dir = project();
    let mut server = Server::start(dir.path(), "document.md", &[]);
    let events = server.events();
    events.hello(&server);
    assert_eq!(server.interrupt().code(), Some(0), "{}", server.stderr());
    assert!(events.0.recv_timeout(WAIT).is_err());
    assert!(server.stdout.try_iter().next().is_none());
    assert!(
        TcpStream::connect_timeout(&server.addr(), Duration::from_millis(300))
            .is_err()
    );
}

#[cfg(unix)]
#[test]
fn interrupt_during_the_first_build_exits_zero_without_binding() {
    let _turn = serial();
    let dir = project();
    let large: String = (0..30000).map(|index| format!("Paragraph {index}: Text with *emphasis*, `code` and a reference.\n\n")).collect();
    write(&dir.path().join("big.md"), &large);
    let mut command = Command::new(env!("CARGO_BIN_EXE_writer"));
    command.args(["-v", "serve", "big.md", "--port", "0"]);
    let mut server = Server::spawn(dir.path(), command);
    server.wait_stderr("first build of", 1);
    assert_eq!(server.interrupt().code(), Some(0), "{}", server.stderr());
    assert!(
        server.stdout.try_iter().next().is_none(),
        "signal arrived after bind"
    );
}
