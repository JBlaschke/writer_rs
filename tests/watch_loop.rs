//! T20 library rebuild-loop integration, isolated in bounded child processes.
//!
//! The callback API has no external stop handle. A child process lets a failed
//! assertion stop a blocked loop, reap it, and join both pipe readers reliably.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write as _};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use writer::config::Target;
use writer::docx::{ValidateOptions, validate};
use writer::ir::SourceKind;
use writer::pipeline::{BuildOptions, InputRole, Mode};
use writer::serve::watch::{
    ChangeKind, WatchStatus, canonical, run_build_watch_with, watch_set,
    watch_set_before_build,
};

const REQUEST: &str = "WRITER_WATCH_LOOP_TEST_REQUEST";
const PREFIX: &str = "WRITER_WATCH_LOOP_STATUS ";
const WAIT: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_millis(1400);
// Each child starts a native watcher and its own compiler/font state. Avoid
// concurrent startup exhausting a loaded runner before the bounded waits begin.
static CHILD: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Stop {
    Built(usize),
    Watching,
    Changed,
}

#[derive(Debug, Serialize, Deserialize)]
struct Request {
    input: PathBuf,
    out: PathBuf,
    targets: Vec<Target>,
    theme: Option<String>,
    config: Option<PathBuf>,
    format: Option<SourceKind>,
    template: Option<PathBuf>,
    standalone: Option<bool>,
    stop: Stop,
    edit_first_build: Option<String>,
    check_mode: bool,
}

impl Request {
    fn html(input: &Path, out: &Path, stop: Stop) -> Self {
        Self {
            input: input.into(),
            out: out.into(),
            targets: vec![Target::Html],
            theme: None,
            config: None,
            format: None,
            template: None,
            standalone: None,
            stop,
            edit_first_build: None,
            check_mode: false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct TargetSummary {
    target: Target,
    written: Option<Vec<PathBuf>>,
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Status {
    Built {
        prepared: bool,
        errors: bool,
        failure: Option<String>,
        targets: Vec<TargetSummary>,
        diagnostics: Vec<String>,
    },
    Watching {
        files: Vec<PathBuf>,
        themes: Vec<PathBuf>,
        pages: Vec<PathBuf>,
    },
    Changed {
        paths: Vec<PathBuf>,
        kind: ChangeKind,
    },
    Done {
        error: Option<String>,
    },
}

fn status(status: &WatchStatus<'_>) -> Status {
    match status {
        WatchStatus::Built(report) => Status::Built {
            prepared: report.prepared.is_ok(),
            errors: report.diagnostics.has_errors(),
            failure: report
                .prepared
                .as_ref()
                .err()
                .map(|failure| failure.error.to_string()),
            targets: report
                .targets
                .iter()
                .map(|target| TargetSummary {
                    target: target.target,
                    written: target.written.as_ref().ok().cloned(),
                    error: target
                        .written
                        .as_ref()
                        .err()
                        .map(ToString::to_string),
                })
                .collect(),
            diagnostics: report
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.to_string())
                .collect(),
        },
        WatchStatus::Watching(set) => Status::Watching {
            files: set.files.clone(),
            themes: set.theme_dirs.clone(),
            pages: set.pages_paths.clone(),
        },
        WatchStatus::Changed(batch) => {
            Status::Changed { paths: batch.paths.clone(), kind: batch.kind }
        }
    }
}

fn emit(status: &Status) {
    println!("{PREFIX}{}", serde_json::to_string(status).unwrap());
    std::io::stdout().flush().unwrap();
}

/// Invoked by `Loop::spawn`, rather than by the normal test run.
#[test]
#[ignore = "private driver for bounded library-loop integration tests"]
fn watch_loop_driver() {
    let request: Request =
        serde_json::from_str(&std::env::var(REQUEST).unwrap()).unwrap();
    let mut options = BuildOptions::new(&request.input);
    options.cli.out = Some(request.out.clone());
    options.cli.theme = request.theme;
    options.cli.standalone = request.standalone;
    options.cli.template = request.template;
    options.config_path = request.config;
    options.format = request.format;
    if request.check_mode {
        options.mode = Mode::Check;
    }
    let mut builds = 0;
    let result = run_build_watch_with(&options, &request.targets, |event| {
        if matches!(event, WatchStatus::Built(_)) {
            builds += 1;
        }
        emit(&status(&event));
        if builds == 1
            && matches!(event, WatchStatus::Built(_))
            && let Some(edit) = &request.edit_first_build
        {
            append(&request.input, edit);
        }
        match (&request.stop, event) {
            (Stop::Built(limit), WatchStatus::Built(_)) if builds >= *limit => {
                ControlFlow::Break(())
            }
            (Stop::Watching, WatchStatus::Watching(_))
            | (Stop::Changed, WatchStatus::Changed(_)) => {
                ControlFlow::Break(())
            }
            _ => ControlFlow::Continue(()),
        }
    });
    emit(&Status::Done { error: result.err().map(|error| error.to_string()) });
}

struct Loop {
    _serial: MutexGuard<'static, ()>,
    child: Child,
    statuses: Receiver<std::result::Result<Status, String>>,
    output: Arc<Mutex<String>>,
    readers: Vec<JoinHandle<()>>,
}

impl Loop {
    fn spawn(request: Request) -> Self {
        let serial = CHILD.lock().unwrap_or_else(PoisonError::into_inner);
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact", "watch_loop_driver", "--ignored", "--nocapture",
                "--test-threads=1",
            ])
            .env(REQUEST, serde_json::to_string(&request).unwrap())
            .env("SOURCE_DATE_EPOCH", "0")
            .env_remove("RUST_LOG")
            .current_dir(request.input.parent().unwrap())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (tx, statuses) = mpsc::channel();
        let output = Arc::new(Mutex::new(String::new()));
        let stdout_log = output.clone();
        let stdout_reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let line = match line {
                    Ok(line) => line,
                    Err(error) => {
                        let _ = tx.send(Err(error.to_string()));
                        break;
                    }
                };
                stdout_log.lock().unwrap().push_str(&format!("{line}\n"));
                if let Some((_, json)) = line.split_once(PREFIX) {
                    let event = serde_json::from_str(json)
                        .map_err(|error| error.to_string());
                    if tx.send(event).is_err() {
                        break;
                    }
                }
            }
        });
        let stderr_log = output.clone();
        let stderr_reader = std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                let Ok(line) = line else { break };
                stderr_log
                    .lock()
                    .unwrap()
                    .push_str(&format!("stderr: {line}\n"));
            }
        });
        Self {
            _serial: serial,
            child,
            statuses,
            output,
            readers: vec![stdout_reader, stderr_reader],
        }
    }

    fn next(&self, what: &str) -> Status {
        match self.statuses.recv_timeout(WAIT) {
            Ok(Ok(status)) => status,
            other => panic!("waiting for {what}: {other:?}\n{}", self.log()),
        }
    }

    fn log(&self) -> String {
        self.output.lock().unwrap().clone()
    }

    fn quiet(&self) {
        assert!(
            matches!(
                self.statuses.recv_timeout(QUIET),
                Err(RecvTimeoutError::Timeout)
            ),
            "unexpected rebuild during quiet window:\n{}",
            self.log()
        );
    }

    fn done(&mut self) {
        assert!(
            matches!(
                self.next("callback Break completion"),
                Status::Done { error: None }
            ),
            "{}",
            self.log()
        );
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "child failed: {status}\n{}",
                    self.log()
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "child did not exit:\n{}",
                self.log()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        self.join_readers();
    }

    fn join_readers(&mut self) {
        for reader in self.readers.drain(..) {
            reader.join().unwrap();
        }
    }
}

impl Drop for Loop {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.join_readers();
    }
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn append(path: &Path, text: &str) {
    OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
}

fn project(text: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("document.md");
    let output = directory.path().join("out.html");
    write(&input, text);
    write(
        &directory.path().join("writer.toml"),
        "[pdf]\nsystem_fonts = false\n",
    );
    (directory, input, output)
}

fn child_theme(root: &Path) -> (PathBuf, PathBuf) {
    let theme = root.join("theme");
    let css = theme.join("styles/x.css");
    write(
        &theme.join("theme.toml"),
        concat!(
            "name = \"watch-test\"\nextends = \"default\"\n",
            "[styles]\nentries = [\"styles/x.css\"]\n",
        ),
    );
    write(&css, "p { --watch-marker: initial; }\n");
    (theme, css)
}

fn built(status: Status, expected: &[Target]) -> Vec<TargetSummary> {
    let Status::Built { prepared, failure, targets, diagnostics, .. } = status
    else {
        panic!("expected Built, got {status:?}");
    };
    assert!(prepared, "prepare failed: {failure:?}; {diagnostics:?}");
    assert_eq!(
        targets.iter().map(|target| target.target).collect::<Vec<_>>(),
        expected
    );
    targets
}

fn successful(status: Status, expected: &[Target]) -> Vec<Vec<PathBuf>> {
    built(status, expected)
        .into_iter()
        .map(|target| {
            assert!(target.error.is_none(), "target failed: {target:?}");
            target.written.unwrap()
        })
        .collect()
}

fn changed(status: Status, kind: ChangeKind, expected: &Path) {
    let Status::Changed { paths, kind: found } = status else {
        panic!("expected Changed, got {status:?}");
    };
    assert_eq!(found, kind, "paths: {paths:?}");
    assert!(paths.contains(&canonical(expected)), "{paths:?}");
}

fn watching(status: Status) -> Vec<PathBuf> {
    let Status::Watching { files, .. } = status else {
        panic!("expected Watching, got {status:?}");
    };
    files
}

#[test]
fn callback_break_after_first_build_returns_without_watching() {
    let (_directory, input, output) = project("# First build\n");
    let mut running =
        Loop::spawn(Request::html(&input, &output, Stop::Built(1)));
    let written = successful(running.next("initial build"), &[Target::Html]);
    assert_eq!(written[0][0], output);
    running.done();
    assert!(fs::read_to_string(output).unwrap().contains("First build"));
}

#[test]
fn callback_break_after_watching_preserves_initial_status_order() {
    let (_directory, input, output) = project("# Watch once\n");
    let mut running =
        Loop::spawn(Request::html(&input, &output, Stop::Watching));
    successful(running.next("initial build"), &[Target::Html]);
    assert!(watching(running.next("watch set")).contains(&canonical(&input)));
    running.done();
}

#[test]
fn callback_break_after_changed_does_not_start_another_build() {
    let (directory, input, output) = project("# Stop before rebuild\n");
    let (theme, css) = child_theme(directory.path());
    let mut request = Request::html(&input, &output, Stop::Changed);
    request.theme = Some(theme.to_string_lossy().into_owned());
    let mut running = Loop::spawn(request);
    successful(running.next("initial build"), &[Target::Html]);
    watching(running.next("watch set"));
    write(&css, "p { --watch-marker: changed; }\n");
    changed(running.next("stylesheet change"), ChangeKind::Css, &css);
    running.done();
    let html = fs::read_to_string(output).unwrap();
    assert!(html.contains("--watch-marker: initial"));
    assert!(!html.contains("--watch-marker: changed"));
}

#[test]
fn save_during_first_build_callback_is_not_lost() {
    let (_directory, input, output) = project("# Original\n");
    let mut request = Request::html(&input, &output, Stop::Built(2));
    request.edit_first_build =
        Some("\nSaved while first build was reporting.\n".into());
    let mut running = Loop::spawn(request);
    successful(running.next("initial build"), &[Target::Html]);
    watching(running.next("watch set"));
    changed(
        running.next("save during initial callback"),
        ChangeKind::Content,
        &input,
    );
    successful(running.next("rebuild"), &[Target::Html]);
    running.done();
    assert!(
        fs::read_to_string(output).unwrap().contains("Saved while first build")
    );
}

#[test]
fn initial_parse_failure_recovers_without_restarting() {
    let (_directory, input, output) =
        project("---\ntitle: [unclosed\n---\n# Broken\n");
    let mut running =
        Loop::spawn(Request::html(&input, &output, Stop::Built(2)));
    assert!(matches!(
        running.next("parse failure"),
        Status::Built { prepared: false, .. }
    ));
    assert!(
        watching(running.next("failed-build watch set"))
            .contains(&canonical(&input))
    );
    assert!(!output.exists());
    write(&input, "# Recovered parse\n");
    changed(running.next("source repair"), ChangeKind::Content, &input);
    successful(running.next("recovered build"), &[Target::Html]);
    running.done();
    assert!(fs::read_to_string(output).unwrap().contains("Recovered parse"));
}

#[test]
fn initially_invalid_project_config_is_watched_until_repaired() {
    let (directory, input, output) = project("# Config recovered\n");
    let config = directory.path().join("writer.toml");
    write(&config, "[pdf\n");
    let mut running =
        Loop::spawn(Request::html(&input, &output, Stop::Built(2)));
    assert!(matches!(
        running.next("config failure"),
        Status::Built { prepared: false, .. }
    ));
    assert!(
        watching(running.next("config watch set"))
            .contains(&canonical(&config))
    );
    write(&config, "[pdf]\nsystem_fonts = false\n");
    changed(running.next("config repair"), ChangeKind::Content, &config);
    successful(running.next("recovered build"), &[Target::Html]);
    running.done();
    assert!(fs::read_to_string(output).unwrap().contains("Config recovered"));
}

#[test]
fn first_failed_build_watches_missing_asset_creation_in_new_directories() {
    let (directory, input, output) =
        project("# Asset recovery\n\n![plot](new/deep/plot.png)\n");
    let image = directory.path().join("new/deep/plot.png");
    let mut running =
        Loop::spawn(Request::html(&input, &output, Stop::Built(2)));
    let first = running.next("missing asset failure");
    assert!(
        matches!(&first, Status::Built { prepared: false, failure: Some(error), .. } if error.contains("plot.png")),
        "{first:?}"
    );
    assert!(
        watching(running.next("missing asset watch set"))
            .contains(&canonical(&image))
    );
    fs::create_dir_all(image.parent().unwrap()).unwrap();
    fs::write(&image, include_bytes!("fixtures/md/images/plot.png")).unwrap();
    let Status::Changed { paths, kind } = running.next("asset creation") else {
        panic!("expected Changed");
    };
    assert_eq!(kind, ChangeKind::Content);
    // Flat ancestor watches can report the newly created directory first.
    assert!(
        paths.iter().any(|path| canonical(&image).starts_with(path)),
        "{paths:?}"
    );
    successful(running.next("recovered image build"), &[Target::Html]);
    running.done();
    assert!(
        fs::read_to_string(output).unwrap().contains("data:image/png;base64,")
    );
}

#[test]
fn transitive_include_parse_error_can_be_repaired_at_the_leaf() {
    let (directory, input, output) =
        project("# Root\n\n:::include parts/middle.md\n:::\n");
    let middle = directory.path().join("parts/middle.md");
    let leaf = directory.path().join("parts/deep/leaf.md");
    write(&middle, "# Middle\n\n:::include deep/leaf.md\n:::\n");
    write(&leaf, "---\ntitle: [broken\n---\n");
    let mut running =
        Loop::spawn(Request::html(&input, &output, Stop::Built(2)));
    assert!(matches!(
        running.next("include parse failure"),
        Status::Built { prepared: false, .. }
    ));
    let files = watching(running.next("include watch set"));
    for path in [&input, &middle, &leaf] {
        assert!(files.contains(&canonical(path)), "{files:?}");
    }
    write(&leaf, "# Recovered included leaf\n");
    changed(running.next("leaf repair"), ChangeKind::Content, &leaf);
    successful(running.next("recovered include build"), &[Target::Html]);
    running.done();
    assert!(
        fs::read_to_string(output).unwrap().contains("Recovered included leaf")
    );
}

#[test]
fn output_inside_recursive_theme_does_not_trigger_a_self_rebuild() {
    let (directory, input, _) = project("# Theme output\n");
    let (theme, css) = child_theme(directory.path());
    let output = theme.join("build/sub/document.html");
    let mut request = Request::html(&input, &output, Stop::Built(2));
    request.theme = Some(theme.to_string_lossy().into_owned());
    let mut running = Loop::spawn(request);
    successful(running.next("initial output within theme"), &[Target::Html]);
    watching(running.next("watch set"));
    running.quiet();
    assert!(output.is_file());
    write(&css, "p { --watch-marker: theme-edit; }\n");
    changed(running.next("theme CSS edit"), ChangeKind::Css, &css);
    successful(running.next("theme CSS rebuild"), &[Target::Html]);
    running.done();
    assert!(
        fs::read_to_string(output)
            .unwrap()
            .contains("--watch-marker: theme-edit")
    );
}

#[test]
fn site_asset_directories_and_new_descendants_are_filtered_inside_a_theme() {
    let (directory, input, _) = project("# Site output\n");
    let (theme, css) = child_theme(directory.path());
    let output = theme.join("build/site");
    let assets = output.join("assets");
    let mut request = Request::html(&input, &output, Stop::Built(2));
    request.theme = Some(theme.to_string_lossy().into_owned());
    request.standalone = Some(false);
    let mut running = Loop::spawn(request);
    let written =
        successful(running.next("initial site build"), &[Target::Html]);
    assert_eq!(written[0][0], output.join("index.html"));
    assert!(written[0].contains(&assets));
    watching(running.next("site watch set"));
    running.quiet();
    write(&assets.join("new/deep/generated.css"), "/* output descendant */\n");
    running.quiet();
    write(&css, "p { --watch-marker: site-edit; }\n");
    changed(running.next("real theme CSS edit"), ChangeKind::Css, &css);
    successful(running.next("site rebuild"), &[Target::Html]);
    running.done();
    assert!(
        fs::read_to_string(output.join("index.html"))
            .unwrap()
            .contains("assets/")
    );
    assert!(walkdir::WalkDir::new(&assets).into_iter().any(|entry| {
        let entry = entry.unwrap();
        entry.file_type().is_file()
            && fs::read_to_string(entry.path())
                .is_ok_and(|text| text.contains("--watch-marker: site-edit"))
    }));
}

#[test]
fn selective_html_rebuild_keeps_multi_target_output_directory_and_docx_root() {
    let (directory, input, _) = project("# Two outputs\n");
    let (theme, css) = child_theme(directory.path());
    let output = theme.join("build/sub");
    let html = output.join("document.html");
    let docx = output.join("document.docx");
    let mut request = Request::html(&input, &output, Stop::Built(3));
    request.targets = vec![Target::Docx, Target::Html, Target::Docx];
    request.theme = Some(theme.to_string_lossy().into_owned());
    let mut running = Loop::spawn(request);
    let written =
        successful(running.next("both targets"), &[Target::Docx, Target::Html]);
    assert_eq!(written[0][0], docx);
    assert_eq!(written[1][0], html);
    watching(running.next("multi-target watch set"));
    let original_docx = fs::read(&docx).unwrap();
    assert!(
        validate(&original_docx, &ValidateOptions { builtin_styles: true })
            .is_empty()
    );
    running.quiet();
    write(&css, "p { --watch-marker: html-subset; }\n");
    changed(running.next("CSS subset"), ChangeKind::Css, &css);
    let written =
        successful(running.next("HTML subset build"), &[Target::Html]);
    assert_eq!(written[0][0], html);
    assert!(output.is_dir());
    assert!(
        fs::read_to_string(&html)
            .unwrap()
            .contains("--watch-marker: html-subset")
    );
    assert_eq!(fs::read(&docx).unwrap(), original_docx);
    // A partial HTML rebuild must retain the earlier DOCX output exclusion.
    fs::write(&docx, &original_docx).unwrap();
    running.quiet();
    append(&input, "\nBoth targets after the subset rebuild.\n");
    changed(running.next("later content edit"), ChangeKind::Content, &input);
    successful(
        running.next("both targets after subset"),
        &[Target::Docx, Target::Html],
    );
    running.done();
    let final_docx = fs::read(docx).unwrap();
    assert!(
        validate(&final_docx, &ValidateOptions { builtin_styles: true })
            .is_empty()
    );
    assert!(
        fs::read_to_string(html)
            .unwrap()
            .contains("Both targets after the subset")
    );
}

#[test]
fn pages_only_changes_with_no_requested_target_are_ignored_before_reporting() {
    let (directory, input, output) = project("# HTML only\n");
    let template = directory.path().join("template.typ");
    write(&template, "// Initial pages template\n");
    let mut request = Request::html(&input, &output, Stop::Built(2));
    request.template = Some(template.clone());
    let mut running = Loop::spawn(request);
    successful(running.next("initial HTML build"), &[Target::Html]);
    let Status::Watching { files, pages, .. } = running.next("pages watch set")
    else {
        panic!("expected Watching");
    };
    assert!(files.contains(&canonical(&template)));
    assert!(pages.contains(&canonical(&template)));
    write(&template, "// Changed pages template\n");
    running.quiet();
    append(&input, "\nStill watching after an ignored pages batch.\n");
    changed(
        running.next("content after ignored pages batch"),
        ChangeKind::Content,
        &input,
    );
    successful(running.next("HTML content rebuild"), &[Target::Html]);
    running.done();
    assert!(
        fs::read_to_string(output)
            .unwrap()
            .contains("Still watching after an ignored")
    );
}

#[test]
fn template_target_failures_do_not_terminate_the_watch_loop() {
    let (directory, input, _) =
        project("# HTML beside invalid LaTeX template\n");
    let output = directory.path().join("out");
    let config = directory.path().join("writer.toml");
    write(&config, "[pdf]\nsystem_fonts=false\n[latex]\ntemplate='bad.tex'\n");
    write(&directory.path().join("bad.tex"), "\\VAR{body}\n");
    let mut request = Request::html(&input, &output, Stop::Built(2));
    request.config = Some(config);
    request.targets = vec![Target::Html, Target::Latex];
    let mut running = Loop::spawn(request);
    for round in 0..2 {
        let targets = built(
            running.next("mixed target results"),
            &[Target::Html, Target::Latex],
        );
        assert!(targets[0].written.is_some());
        assert!(targets[0].error.is_none());
        assert!(targets[1].written.is_none());
        assert!(
            targets[1]
                .error
                .as_ref()
                .is_some_and(|error| error.contains("has no \\VAR{preamble}"))
        );
        if round == 0 {
            watching(running.next("watch set after target failure"));
            append(&input, "\nUpdated despite template failure.\n");
            changed(running.next("source edit"), ChangeKind::Content, &input);
        }
    }
    running.done();
    assert!(
        fs::read_to_string(output.join("document.html"))
            .unwrap()
            .contains("Updated despite")
    );
    assert!(!output.join("document.tex").exists());
    assert!(!output.join("document_assets").exists());
}

#[test]
fn watch_always_resolves_in_build_mode() {
    let (_directory, input, output) = project("Unresolved @does-not-exist.\n");
    let mut request = Request::html(&input, &output, Stop::Built(1));
    request.check_mode = true;
    let mut running = Loop::spawn(request);
    let Status::Built { errors, diagnostics, .. } =
        running.next("build-mode resolution")
    else {
        panic!("expected Built");
    };
    assert!(
        diagnostics.iter().any(|code| code == "unresolved-reference"),
        "{diagnostics:?}"
    );
    assert!(errors, "watch must override the caller's lenient Check mode");
    running.done();
}

#[test]
fn watch_set_adapts_every_input_role_and_target_specific_resources() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let roles = [
        ("main.md", InputRole::Source),
        ("include.md", InputRole::Include),
        ("writer.toml", InputRole::Config),
        ("plot.png", InputRole::Asset),
        ("missing/deep.png", InputRole::MissingAsset),
        ("theme", InputRole::Theme),
        ("refs.bib", InputRole::Bibliography),
        ("template.typ", InputRole::PdfTemplate),
        ("reference.docx", InputRole::ReferenceDoc),
        ("template.tex", InputRole::LatexTemplate),
    ];
    let inputs: Vec<_> =
        roles.iter().map(|(name, role)| (root.join(name), *role)).collect();
    let accessed = root.join("compiler-read.typ");
    for targets in [
        None,
        Some(vec![Target::Html]),
        Some(vec![Target::Docx, Target::Latex]),
    ] {
        let set = watch_set(
            &inputs,
            &[accessed.clone(), accessed.clone()],
            targets.as_deref(),
        );
        for (path, role) in &inputs {
            let included = match role {
                InputRole::Theme => {
                    assert!(set.theme_dirs.contains(&canonical(path)));
                    assert!(set.dirs_recursive.contains(&canonical(path)));
                    continue;
                }
                InputRole::ReferenceDoc => targets
                    .as_ref()
                    .is_some_and(|targets| targets.contains(&Target::Docx)),
                InputRole::LatexTemplate => targets
                    .as_ref()
                    .is_some_and(|targets| targets.contains(&Target::Latex)),
                _ => true,
            };
            assert_eq!(
                set.files.contains(&canonical(path)),
                included,
                "{role:?}: {set:?}"
            );
            if included {
                assert!(
                    set.dirs_flat.contains(&canonical(path.parent().unwrap()))
                );
            }
        }
        assert!(set.files.contains(&canonical(&accessed)));
        assert!(
            set.pages_paths.contains(&canonical(&root.join("template.typ")))
        );
        for paths in [
            &set.files, &set.dirs_flat, &set.dirs_recursive, &set.theme_dirs,
            &set.pages_paths,
        ] {
            assert!(
                paths.windows(2).all(|pair| pair[0] < pair[1]),
                "{paths:?}"
            );
        }
    }
}

#[test]
fn before_build_follows_markdown_format_override_and_transitive_missing_include()
 {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let input = root.join("source.txt");
    let middle = root.join("parts/middle.md");
    let missing = root.join("parts/deep/missing.md");
    write(&input, ":::include parts/middle.md\n:::\n");
    write(&middle, ":::include deep/missing.md\n:::\n");
    let config = root.join("writer.toml");
    write(&config, "[invalid config\n");
    let mut options = BuildOptions::new(&input);
    options.format = Some(SourceKind::Markdown);
    let set = watch_set_before_build(&options);
    for path in [&input, &middle, &missing, &config] {
        assert!(
            set.files.contains(&canonical(path)),
            "missing {} in {set:?}",
            path.display()
        );
    }
    options.format = Some(SourceKind::Notebook);
    let notebook = watch_set_before_build(&options);
    assert!(!notebook.files.contains(&canonical(&middle)));
    assert!(!notebook.files.contains(&canonical(&missing)));
    assert!(notebook.files.contains(&canonical(&config)));
}

#[test]
fn before_build_retains_explicit_missing_config_and_cli_theme_paths() {
    let (directory, input, _) = project("# Before build\n");
    let missing_config = directory.path().join("config/missing.toml");
    let theme = directory.path().join("themes/new-theme");
    let mut options = BuildOptions::new(&input);
    options.config_path = Some(missing_config.clone());
    options.cli.theme = Some(theme.to_string_lossy().into_owned());
    let set = watch_set_before_build(&options);
    assert!(set.files.contains(&canonical(&missing_config)), "{set:?}");
    assert!(set.theme_dirs.contains(&canonical(&theme)), "{set:?}");
    assert!(set.dirs_recursive.contains(&canonical(&theme)), "{set:?}");
}
