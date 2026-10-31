//! Watch CLI recovery, reporting, selective rebuilds and child cleanup.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long a watching build may take to report, write or exit. It only has to
/// beat a hang: under a parallel runner every watcher start waits for
/// fseventsd, which serialises FSEvents stream setup across every process on a
/// Mac, and a loaded one has taken over ten seconds.
const TIMEOUT: Duration = Duration::from_secs(60);
const PNG: &[u8] = include_bytes!("fixtures/md/images/plot.png");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stream {
    Out,
    Err,
}

struct Running {
    child: Child,
    rx: Receiver<(Stream, String)>,
    readers: Vec<JoinHandle<()>>,
    lines: Vec<(Stream, String)>,
}

impl Running {
    fn start(directory: &Path, extra: &[&str]) -> Self {
        Self::start_targets(directory, "html", extra)
    }

    fn start_targets(directory: &Path, targets: &str, extra: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_writer"))
            .current_dir(directory)
            .args(["build", "document.md", "--watch", "--to", targets])
            .args(extra)
            .env_remove("RUST_LOG")
            .env_remove("CLICOLOR_FORCE")
            .env("SOURCE_DATE_EPOCH", "0")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let (tx, rx) = mpsc::channel();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let out_tx = tx.clone();
        let out = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if out_tx.send((Stream::Out, line)).is_err() {
                    break;
                }
            }
        });
        let err = std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if tx.send((Stream::Err, line)).is_err() {
                    break;
                }
            }
        });
        Self { child, rx, readers: vec![out, err], lines: Vec::new() }
    }

    fn wait_line(&mut self, since: usize, stream: Stream, needle: &str) {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if self.lines[since..]
                .iter()
                .any(|(found, line)| *found == stream && line.contains(needle))
            {
                return;
            }
            let wait = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(wait) {
                Ok(line) => self.lines.push(line),
                Err(error) => panic!(
                    "missing {stream:?} {needle:?}: {error}; {:?}",
                    self.lines
                ),
            }
        }
    }

    fn drain(&mut self) {
        self.lines.extend(self.rx.try_iter());
    }

    fn mark(&mut self) -> usize {
        self.drain();
        self.lines.len()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }
}

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn project(source: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    write(&directory.path().join("document.md"), source);
    write(
        &directory.path().join("writer.toml"),
        "[pdf]\nsystem_fonts = false\ntemplate = 'template.typ'\n",
    );
    write(&directory.path().join("template.typ"), "// ignored by HTML\n");
    write(
        &directory.path().join("theme/theme.toml"),
        "name = 'watch-test'\nextends = 'default'\n[styles]\nentries = ['styles/x.css']\n",
    );
    write(
        &directory.path().join("theme/styles/x.css"),
        "body { color: #112233; }\n",
    );
    directory
}

fn themed(directory: &Path, quiet: bool) -> Running {
    let theme = directory.join("theme");
    let theme = theme.to_str().unwrap();
    let mut extra = vec!["--theme", theme];
    if quiet {
        extra.push("--quiet");
    }
    Running::start(directory, &extra)
}

fn output(directory: &Path) -> PathBuf {
    directory.join("dist/document.html")
}

fn wait_file(path: &Path, needle: &str) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if std::fs::read_to_string(path).is_ok_and(|text| text.contains(needle))
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{} never contained {needle:?}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn css_and_content_rebuild_but_pages_changes_leave_html_alone() {
    let directory = project("# Initial document\n");
    let root = directory.path();
    let mut running = themed(root, false);
    running.wait_line(0, Stream::Out, "Wrote ");
    running.wait_line(0, Stream::Err, "Watching for changes (Ctrl-C to stop)");
    let mark = running.mark();
    write(&root.join("theme/styles/x.css"), "body { color: #445566; }\n");
    running.wait_line(mark, Stream::Err, "Rebuilding (css):");
    running.wait_line(mark, Stream::Out, "Wrote ");
    wait_file(&output(root), "#445566");
    let mark = running.mark();
    write(&root.join("document.md"), "# Changed document\n");
    running.wait_line(mark, Stream::Err, "Rebuilding (content):");
    running.wait_line(mark, Stream::Out, "Wrote ");
    wait_file(&output(root), "Changed document");
    let modified = output(root).metadata().unwrap().modified().unwrap();
    let mark = running.mark();
    write(&root.join("template.typ"), "// pages-only edit\n");
    std::thread::sleep(Duration::from_millis(1600));
    running.drain();
    assert_eq!(output(root).metadata().unwrap().modified().unwrap(), modified);
    assert!(
        !running.lines[mark..].iter().any(|(_, line)| {
            line.starts_with("Wrote ")
                || line.starts_with("Rebuilding (pages):")
        }),
        "{:?}",
        running.lines
    );
}

#[test]
fn invalid_startup_config_is_watched_until_repaired() {
    let directory = project("# Recovered config\n");
    let root = directory.path();
    write(&root.join("writer.toml"), "[pdf\n");
    let mut running = themed(root, false);
    running.wait_line(0, Stream::Err, "error:");
    running.wait_line(0, Stream::Err, "Watching for changes");
    assert!(!output(root).exists());
    let mark = running.mark();
    write(&root.join("writer.toml"), "[pdf]\nsystem_fonts = false\n");
    running.wait_line(mark, Stream::Err, "Rebuilding (content):");
    running.wait_line(mark, Stream::Out, "Wrote ");
    wait_file(&output(root), "Recovered config");
    running.drain();
    assert_eq!(
        running
            .lines
            .iter()
            .filter(|(_, line)| { line.starts_with("Watching for changes") })
            .count(),
        1
    );
}

#[test]
fn missing_asset_recovers_and_reports_source_excerpt() {
    let directory = project("# Asset\n\n![Missing](plot.png)\n");
    let root = directory.path();
    let mut running = themed(root, false);
    running.wait_line(0, Stream::Err, "asset not found:");
    running.wait_line(0, Stream::Err, "![Missing](plot.png)");
    running.wait_line(0, Stream::Err, "Watching for changes");
    assert!(!output(root).exists());
    let mark = running.mark();
    std::fs::write(root.join("plot.png"), PNG).unwrap();
    running.wait_line(mark, Stream::Out, "Wrote ");
    wait_file(&output(root), "data:image/png");
}

#[test]
fn included_parse_failure_can_be_fixed_without_restarting() {
    let directory = project("# Root\n\n:::include child.md\n:::\n");
    let root = directory.path();
    write(&root.join("child.md"), "---\ninvalid: [\n---\n");
    let mut running = themed(root, false);
    running.wait_line(0, Stream::Err, "error:");
    running.wait_line(0, Stream::Err, "Watching for changes");
    let mark = running.mark();
    write(&root.join("child.md"), "## Repaired included section\n");
    running.wait_line(mark, Stream::Out, "Wrote ");
    wait_file(&output(root), "Repaired included section");
}

#[test]
fn quiet_hides_status_and_success_but_retains_errors() {
    let directory = project("![Missing](plot.png)\n");
    let root = directory.path();
    let mut running = themed(root, true);
    running.wait_line(0, Stream::Err, "asset not found:");
    std::fs::write(root.join("plot.png"), PNG).unwrap();
    wait_file(&output(root), "data:image/png");
    running.drain();
    assert!(
        running.lines.iter().all(|(stream, line)| {
            *stream != Stream::Out
                && !line.starts_with("Watching for changes")
                && !line.starts_with("Rebuilding (")
        }),
        "{:?}",
        running.lines
    );
}

#[test]
fn initial_clashes_and_usage_errors_exit_instead_of_waiting() {
    let directory = project("# Preserve this input\n");
    let root = directory.path();
    let before = std::fs::read(root.join("document.md")).unwrap();
    for args in [
        vec![
            "build", "document.md", "--watch", "--to", "html", "-o",
            "document.md",
        ],
        vec!["build", "unknown.txt", "--watch"],
    ] {
        let mut command =
            assert_cmd::Command::new(env!("CARGO_BIN_EXE_writer"));
        command.current_dir(root).timeout(TIMEOUT).args(args).assert().code(2);
    }
    assert_eq!(std::fs::read(root.join("document.md")).unwrap(), before);
    assert!(!output(root).exists());
}

#[test]
fn later_output_clash_is_reported_and_watching_continues() {
    let directory = project("# Initial output\n");
    let root = directory.path();
    let mut running = themed(root, false);
    running.wait_line(0, Stream::Err, "Watching for changes");
    let before = std::fs::read(output(root)).unwrap();
    let mark = running.mark();
    write(&root.join("document.md"), "![Protected](dist/document.html)\n");
    running.wait_line(mark, Stream::Err, "refusing to write");
    assert_eq!(std::fs::read(output(root)).unwrap(), before);
    let mark = running.mark();
    write(&root.join("document.md"), "# Repaired after clash\n");
    running.wait_line(mark, Stream::Out, "Wrote ");
    wait_file(&output(root), "Repaired after clash");
}

#[test]
fn missing_config_is_followed_through_new_ancestor_directories() {
    let directory = project("# Recovered nested config\n");
    let root = directory.path();
    let mut running = Running::start(root, &["--config", "new/a/writer.toml"]);
    running.wait_line(0, Stream::Err, "Watching for changes");
    for relative in ["new", "new/a"] {
        let mark = running.mark();
        std::fs::create_dir(root.join(relative)).unwrap();
        running.wait_line(mark, Stream::Err, "cannot read");
    }
    let mark = running.mark();
    write(&root.join("new/a/writer.toml"), "[pdf]\nsystem_fonts = false\n");
    running.wait_line(mark, Stream::Out, "Wrote ");
    wait_file(&output(root), "Recovered nested config");
}

#[test]
fn first_multi_target_clash_reports_other_target_results_once() {
    let directory = project("![Protected](document.docx)\n");
    let root = directory.path();
    std::fs::write(root.join("document.docx"), PNG).unwrap();
    write(&root.join("bad.tex"), "\\VAR{body}\n");
    let mut command = assert_cmd::Command::new(env!("CARGO_BIN_EXE_writer"));
    let result = command
        .current_dir(root)
        .timeout(TIMEOUT)
        .args([
            "build", "document.md", "--watch", "--to", "html,docx,latex", "-o",
            ".", "--latex-template", "bad.tex",
        ])
        .assert()
        .code(2)
        .get_output()
        .clone();
    let stdout = String::from_utf8(result.stdout).unwrap();
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert!(stdout.contains("Wrote "), "{stdout}");
    assert!(stdout.contains("document.html"), "{stdout}");
    assert_eq!(stderr.matches("refusing to write").count(), 1, "{stderr}");
    assert!(stderr.contains("has no \\VAR{preamble}"), "{stderr}");
    assert!(root.join("document.html").is_file());
    assert_eq!(std::fs::read(root.join("document.docx")).unwrap(), PNG);
    assert!(!root.join("document.tex").exists());
    assert!(!root.join("document_assets").exists());
}

#[test]
fn html_pdf_watch_keeps_pdf_on_css_and_rewrites_both_on_content() {
    let directory = project("# Initial dual output\n");
    let root = directory.path();
    write(
        &root.join("template.typ"),
        include_str!("../assets/typst/writer-doc/template.typ"),
    );
    let theme = root.join("theme");
    let mut running = Running::start_targets(
        root,
        "html,pdf",
        &["--theme", theme.to_str().unwrap()],
    );
    running.wait_line(0, Stream::Out, "document.html");
    running.wait_line(0, Stream::Out, "document.pdf");
    running.wait_line(0, Stream::Err, "Watching for changes");
    let html = output(root);
    let pdf = root.join("dist/document.pdf");
    let pdf_before = std::fs::read(&pdf).unwrap();
    assert!(pdf_before.starts_with(b"%PDF-"));
    // Set an old known timestamp so even coarse filesystem resolution can
    // distinguish a write, without a timing-dependent sleep between edits.
    let sentinel = std::time::UNIX_EPOCH + Duration::from_secs(86_400);
    for path in [&html, &pdf] {
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(sentinel)
            .unwrap();
    }
    let mark = running.mark();
    write(&root.join("theme/styles/x.css"), "body { color: #667788; }\n");
    running.wait_line(mark, Stream::Err, "Rebuilding (css):");
    running.wait_line(mark, Stream::Out, "document.html");
    wait_file(&html, "#667788");
    assert_ne!(html.metadata().unwrap().modified().unwrap(), sentinel);
    assert_eq!(pdf.metadata().unwrap().modified().unwrap(), sentinel);
    assert_eq!(std::fs::read(&pdf).unwrap(), pdf_before);
    running.drain();
    assert!(!running.lines[mark..].iter().any(|(stream, line)| {
        *stream == Stream::Out
            && line.starts_with("Wrote ")
            && line.contains("document.pdf")
    }));

    std::fs::OpenOptions::new()
        .write(true)
        .open(&html)
        .unwrap()
        .set_modified(sentinel)
        .unwrap();
    let mark = running.mark();
    write(&root.join("document.md"), "# Changed dual output\n");
    running.wait_line(mark, Stream::Err, "Rebuilding (content):");
    running.wait_line(mark, Stream::Out, "document.html");
    running.wait_line(mark, Stream::Out, "document.pdf");
    wait_file(&html, "Changed dual output");
    assert_ne!(html.metadata().unwrap().modified().unwrap(), sentinel);
    assert_ne!(pdf.metadata().unwrap().modified().unwrap(), sentinel);
    assert_ne!(std::fs::read(&pdf).unwrap(), pdf_before);
}
