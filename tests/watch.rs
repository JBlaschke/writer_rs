//! T20: the watcher against the real notify backend in isolated temp directories.
//!
//! Ported from presenter's backend regressions. Startup events are drained before
//! edits, and every receive or drain has a deadline. Platform-specific structural
//! events may arrive in separate batches, so those checks collect a quiet window.

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime};

use writer::serve::watch::{
    ChangeBatch, ChangeKind, DEBOUNCE, Debounce, WatchSet, Watcher, canonical,
};

/// How long a batch may take to arrive. It only has to beat a hang: under a
/// parallel runner every watcher start waits for fseventsd, which serialises
/// FSEvents stream setup across every process on a Mac, and a loaded one has
/// taken over ten seconds.
const BATCH_TIMEOUT: Duration = Duration::from_secs(60);
/// How long to wait to be sure that no (further) batch comes.
const QUIET: Duration = Duration::from_millis(1200);

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn append(path: &Path, text: &str) {
    let mut file = OpenOptions::new().append(true).open(path).unwrap();
    file.write_all(text.as_bytes()).unwrap();
}

/// Sets the modification time of `path` to now without writing to it, like
/// `touch`.
fn touch(path: &Path) {
    let file = OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now()).unwrap();
}

/// Longer than a batch takes to form, so that the next change is a batch of its
/// own.
fn pause() {
    std::thread::sleep(DEBOUNCE + Duration::from_millis(1000));
}

/// A document `a.md` and a theme `theme/` with `styles/x.css`, in a fresh
/// temporary directory.
fn document_and_theme() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let document = dir.path().join("a.md");
    let theme = dir.path().join("theme");
    let css = theme.join("styles/x.css");
    write(&document, "# A\n");
    write(&theme.join("theme.toml"), "name = \"t\"\n");
    write(&css, "body { color: red; }\n");
    (dir, document, theme, css)
}

/// Starts a watcher on `set` coalescing changes with the default window
/// ([`start_with`]).
fn start(set: &WatchSet) -> (Watcher, Receiver<ChangeBatch>) {
    start_with(set, Debounce::default())
}

/// Starts a watcher on `set` coalescing changes with `debounce`, and waits
/// until it is
/// surely running, discarding anything it reports for the files just created:
/// FSEvents
/// delivers the events of changes made shortly before its stream was created
/// (inotify does
/// not), the later the more loaded the machine is.
fn start_with(
    set: &WatchSet, debounce: Debounce,
) -> (Watcher, Receiver<ChangeBatch>) {
    let (tx, rx) = mpsc::channel();
    let watcher = Watcher::start_with(set, tx, debounce).unwrap();
    std::thread::sleep(Duration::from_millis(500));
    drain(&rx, debounce.quiet + Duration::from_millis(400));
    (watcher, rx)
}

/// The next batch; fails the test after [`BATCH_TIMEOUT`].
fn next(rx: &Receiver<ChangeBatch>, what: &str) -> ChangeBatch {
    match rx.recv_timeout(BATCH_TIMEOUT) {
        Ok(batch) => batch,
        Err(err) => {
            panic!("no batch after {what} within {BATCH_TIMEOUT:?}: {err}")
        }
    }
}

/// Every batch that arrives until none has arrived for `quiet`.
fn drain(rx: &Receiver<ChangeBatch>, quiet: Duration) -> Vec<ChangeBatch> {
    let mut batches = Vec::new();
    let deadline = Instant::now() + BATCH_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "watcher did not become quiet: {batches:?}"
        );
        match rx.recv_timeout(quiet.min(remaining)) {
            Ok(batch) => batches.push(batch),
            Err(RecvTimeoutError::Timeout) if remaining >= quiet => {
                return batches;
            }
            Err(RecvTimeoutError::Timeout) => {
                panic!("watcher did not become quiet: {batches:?}");
            }
            Err(RecvTimeoutError::Disconnected) => panic!("watcher stopped"),
        }
    }
}

#[test]
fn edits_produce_classified_batches() {
    let (_dir, document, theme, css) = document_and_theme();
    let set = WatchSet::new([document.clone()], [theme]);
    let (_watcher, rx) = start(&set);

    append(&document, "\nmore\n");
    let batch = next(&rx, "appending to a.md");
    assert_eq!(batch.kind, ChangeKind::Content);
    assert_eq!(batch.paths, vec![canonical(&document)]);
    assert_eq!(drain(&rx, QUIET), vec![]);

    write(&css, "body { color: blue; }\n");
    let batch = next(&rx, "writing x.css");
    assert_eq!(batch.kind, ChangeKind::Css);
    assert_eq!(batch.paths, vec![canonical(&css)]);
    assert_eq!(drain(&rx, QUIET), vec![]);

    // `touch`: only the modification time changes (FSEvents: inode metadata;
    // inotify:
    // IN_ATTRIB).
    touch(&css);
    let batch = next(&rx, "touching x.css");
    assert_eq!(batch.kind, ChangeKind::Css);
    assert_eq!(batch.paths, vec![canonical(&css)]);
    assert_eq!(drain(&rx, QUIET), vec![]);
}

/// Changes made together are one batch. A loaded machine may deliver the events
/// of two
/// changes made 30 ms apart further apart than the 150 ms window (FSEvents on a
/// busy CI
/// runner did), which would make them two batches: the watcher coalesces with a
/// wider
/// window here, so that the check does not depend on how promptly the OS
/// delivers.
#[test]
fn a_burst_of_edits_is_one_batch() {
    let (_dir, document, theme, css) = document_and_theme();
    let set = WatchSet::new([document.clone()], [theme]);
    let debounce =
        Debounce { quiet: Duration::from_secs(1), ..Debounce::default() };
    let (_watcher, rx) = start_with(&set, debounce);

    // Both within 50 ms: one batch, content (one path is not a stylesheet).
    append(&document, "\neven more\n");
    std::thread::sleep(Duration::from_millis(30));
    write(&css, "body { color: green; }\n");
    let batch = next(&rx, "writing both files");
    assert_eq!(batch.kind, ChangeKind::Content);
    assert_eq!(batch.paths, vec![canonical(&document), canonical(&css)]);
    assert_eq!(
        drain(&rx, QUIET + debounce.quiet),
        vec![],
        "a second batch for the burst"
    );
}

#[test]
fn rename_replace_is_seen_and_later_edits_too() {
    let (dir, document, theme, _css) = document_and_theme();
    let set = WatchSet::new([document.clone()], [theme]);
    let (_watcher, rx) = start(&set);
    let tmp = dir.path().join("a.md.tmp");

    for round in 1..=2 {
        // How vim and VS Code ("safe write") save: a new file renamed over the
        // old one.
        write(&tmp, &format!("# A\n\nround {round}\n"));
        std::fs::rename(&tmp, &document).unwrap();
        let batches = drain_at_least_one(&rx, "rename-replacing a.md");
        for batch in &batches {
            assert_eq!(
                batch.paths,
                vec![canonical(&document)],
                "round {round}"
            );
            assert_eq!(batch.kind, ChangeKind::Content);
        }

        append(&document, "\nedited after the rename\n");
        let batch = next(&rx, "editing a.md after the rename");
        assert_eq!(batch.paths, vec![canonical(&document)]);
        assert_eq!(batch.kind, ChangeKind::Content);
        assert_eq!(drain(&rx, QUIET), vec![]);
    }
}

/// The next batch and any that follow it within [`QUIET`].
fn drain_at_least_one(
    rx: &Receiver<ChangeBatch>, what: &str,
) -> Vec<ChangeBatch> {
    let mut batches = vec![next(rx, what)];
    batches.extend(drain(rx, QUIET));
    batches
}

#[test]
fn unrelated_and_ignored_paths_do_not_count() {
    let (dir, document, theme, _css) = document_and_theme();
    // Creating a directory inside a theme is a change; `templates/` exists
    // beforehand.
    std::fs::create_dir(theme.join("templates")).unwrap();
    let set = WatchSet::new([document.clone()], [theme.clone()]);
    let (_watcher, rx) = start(&set);

    // Next to the document but not an input; inside the theme but on the ignore
    // list.
    write(&dir.path().join("notes.txt"), "x");
    write(&dir.path().join("out.html"), "x");
    for name in [
        "x.css~", ".x.css.swp", ".x.css.swo", ".x.css.swa", ".#x.css",
        "#x.css#", "x.swx", "4913", ".DS_Store",
    ] {
        write(&theme.join("styles").join(name), "x");
    }
    write(&theme.join(".git/HEAD"), "x");
    assert_eq!(drain(&rx, QUIET), vec![]);

    write(&theme.join("templates/document.html"), "{{ x }}");
    write(&dir.path().join("notes.txt"), "y");
    let batch = next(&rx, "writing a template");
    assert_eq!(
        batch.paths,
        vec![canonical(&theme.join("templates/document.html"))]
    );
    assert_eq!(batch.kind, ChangeKind::Content);
    assert_eq!(drain(&rx, QUIET), vec![]);
}

/// FSEvents reports changes made shortly before its stream was created
/// ([`start_with`]
/// drains them for the other tests; inotify and Windows report none). On macOS
/// the watcher
/// drops them, by the change time: the build that follows a start reads the
/// files anyway.
/// So a watcher started right after the files were written, as `writer serve`
/// starts
/// its own, reports nothing, and a change made afterwards still counts.
///
/// FSEvents lags a few milliseconds on an idle Mac and more on a loaded one;
/// `Watcher::start`
/// takes about as long before its stream exists, so without the filter this
/// fails on a
/// loaded machine (the CI runners) and passes by timing on an idle one. The
/// unit test
/// `changes_from_before_the_last_batch_are_skipped` covers the filter itself.
#[test]
fn changes_from_before_the_watcher_started_are_not_reported() {
    let (_dir, document, theme, css) = document_and_theme();
    let set = WatchSet::new([document], [theme]);
    let (tx, rx) = mpsc::channel();
    let _watcher = Watcher::start(&set, tx).unwrap();
    assert_eq!(drain(&rx, QUIET), vec![]);

    write(&css, "body { color: blue; }\n");
    let batch = next(&rx, "editing the stylesheet");
    assert_eq!(batch.paths, vec![canonical(&css)]);
    assert_eq!(batch.kind, ChangeKind::Css);
    assert_eq!(drain(&rx, QUIET), vec![]);
}

/// A theme may keep inputs in directories named `dist` or `target` (a bundled
/// script entry, a vendored library's stylesheet), at any depth; editing them
/// counts like
/// any other theme file. Only `.git/` and editor files are ignored.
#[test]
fn theme_files_in_dist_and_target_directories_count() {
    let (_dir, document, theme, _css) = document_and_theme();
    let lib = theme.join("scripts/dist/lib.js");
    let vendored = theme.join("styles/target/x.css");
    let top = theme.join("dist/extra.css");
    write(&lib, "window.LIB = 1;\n");
    write(&vendored, "a { b: c; }\n");
    write(&top, "a { b: c; }\n");
    let set = WatchSet::new([document], [theme.clone()]);
    let (_watcher, rx) = start(&set);

    write(&lib, "window.LIB = 2;\n");
    let batch = next(&rx, "editing scripts/dist/lib.js");
    assert_eq!(batch.paths, vec![canonical(&lib)]);
    assert_eq!(batch.kind, ChangeKind::Content);
    assert_eq!(drain(&rx, QUIET), vec![]);

    write(&vendored, "a { b: d; }\n");
    let batch = next(&rx, "editing styles/target/x.css");
    assert_eq!(batch.paths, vec![canonical(&vendored)]);
    assert_eq!(batch.kind, ChangeKind::Css);
    assert_eq!(drain(&rx, QUIET), vec![]);

    write(&top, "a { b: d; }\n");
    let batch = next(&rx, "editing dist/extra.css");
    assert_eq!(batch.paths, vec![canonical(&top)]);
    assert_eq!(drain(&rx, QUIET), vec![]);
}

#[test]
fn update_watches_the_new_set() {
    let (dir, document, theme, css) = document_and_theme();
    let other = dir.path().join("b.md");
    write(&other, "# B\n");
    let (mut watcher, rx) = start(&WatchSet::new([document.clone()], []));
    let root = canonical(dir.path());
    assert_eq!(watcher.watched_dirs(), vec![(root.clone(), false)]);

    write(&css, "body { color: blue; }\n");
    assert_eq!(drain(&rx, QUIET), vec![], "the theme is not watched yet");

    watcher.update(&WatchSet::new([other.clone()], [theme.clone()])).unwrap();
    assert_eq!(
        watcher.watched_dirs(),
        vec![(root.clone(), false), (root.join("theme"), true)]
    );
    // The update sends what changed since the last batch (here: since the
    // start) in the
    // inputs it now watches: the stylesheet written while the theme was not
    // watched.
    let batch = next(&rx, "updating the watch set");
    assert_eq!(batch.paths, vec![canonical(&css)]);
    assert_eq!(batch.kind, ChangeKind::Css);
    std::thread::sleep(Duration::from_millis(300));

    append(&document, "\nno longer an input\n");
    write(&css, "body { color: green; }\n");
    let batch = next(&rx, "touching x.css after the update");
    assert_eq!(batch.paths, vec![canonical(&css)]);
    assert_eq!(batch.kind, ChangeKind::Css);
    assert_eq!(drain(&rx, QUIET), vec![]);

    append(&other, "\nnow an input\n");
    let batch = next(&rx, "appending to b.md");
    assert_eq!(batch.paths, vec![canonical(&other)]);
    assert_eq!(drain(&rx, QUIET), vec![]);

    // Unwatching the theme again.
    watcher.update(&WatchSet::new([other], [])).unwrap();
    assert_eq!(watcher.watched_dirs(), vec![(root, false)]);
}

#[test]
fn a_dropped_watcher_closes_the_channel() {
    let (_dir, document, _theme, _css) = document_and_theme();
    let (watcher, rx) = start(&WatchSet::new([document], []));
    drop(watcher);
    match rx.recv_timeout(BATCH_TIMEOUT) {
        Err(RecvTimeoutError::Disconnected) => {}
        other => panic!("expected the channel to close, got {other:?}"),
    }
}

/// Whether some batch of `batches` lists `path`.
fn mentions(batches: &[ChangeBatch], path: &Path) -> bool {
    batches.iter().any(|batch| batch.paths.iter().any(|p| p == path))
}

#[test]
fn a_recreated_directory_is_watched_again() {
    let dir = tempfile::tempdir().unwrap();
    let root = canonical(dir.path());
    let document = root.join("document.md");
    let figs = root.join("figs");
    let png = figs.join("plot.png");
    write(&document, "# A\n");
    write(&png, "v0");
    let set = WatchSet::new([document.clone(), png.clone()], []);
    let (mut watcher, rx) = start(&set);
    assert_eq!(
        watcher.watched_dirs(),
        vec![(root.clone(), false), (figs.clone(), false)]
    );

    std::fs::remove_dir_all(&figs).unwrap();
    let batches = drain_at_least_one(&rx, "removing figs/");
    assert!(
        mentions(&batches, &figs) || mentions(&batches, &png),
        "{batches:?}"
    );
    watcher.update(&set).unwrap();
    // `figs/` is gone: the document's directory (watched anyway) reports its
    // return.
    assert_eq!(watcher.watched_dirs(), vec![(root.clone(), false)]);

    pause();
    write(&png, "v1");
    let batches = drain_at_least_one(&rx, "recreating figs/plot.png");
    assert!(mentions(&batches, &figs), "{batches:?}");
    watcher.update(&set).unwrap();
    assert_eq!(
        watcher.watched_dirs(),
        vec![(root.clone(), false), (figs.clone(), false)]
    );
    // plot.png was written before that batch was sent: the rebuild for it reads
    // it.
    assert_eq!(drain(&rx, QUIET), vec![]);

    append(&png, "v2");
    let batch = next(&rx, "editing the recreated figs/plot.png");
    assert_eq!(batch.paths, vec![png]);
    assert_eq!(drain(&rx, QUIET), vec![]);
}

#[test]
fn the_documents_directory_can_be_removed_and_created_again() {
    // `git checkout` of a branch without it, or a generator doing `rm -rf gen
    // && …`.
    let dir = tempfile::tempdir().unwrap();
    let root = canonical(dir.path());
    let gen_dir = root.join("gen");
    let document = gen_dir.join("document.md");
    write(&document, "# Generated\n");
    let set = WatchSet::new([document.clone()], []);
    let (mut watcher, rx) = start(&set);

    std::fs::remove_dir_all(&gen_dir).unwrap();
    drain_at_least_one(&rx, "removing gen/");
    watcher.update(&set).unwrap();
    assert_eq!(watcher.watched_dirs(), vec![(root.clone(), false)]);

    // Longer than the batch window: the removal and the creation are separate
    // batches.
    pause();
    write(&document, "# Generated again\n");
    let batches = drain_at_least_one(&rx, "creating gen/ again");
    assert!(mentions(&batches, &gen_dir), "{batches:?}");
    watcher.update(&set).unwrap();
    assert_eq!(watcher.watched_dirs(), vec![(gen_dir, false)]);

    append(&document, "\nmore\n");
    let batch = next(&rx, "editing the regenerated document");
    assert_eq!(batch.paths, vec![document]);
    assert_eq!(drain(&rx, QUIET), vec![]);
}

#[test]
fn a_missing_file_two_directories_down_is_seen_when_created() {
    let dir = tempfile::tempdir().unwrap();
    let root = canonical(dir.path());
    let document = root.join("document.md");
    let png = root.join("figs/sub/plot.png");
    write(&document, "# A\n");
    let set = WatchSet::new([document.clone(), png.clone()], []);
    let (mut watcher, rx) = start(&set);
    assert_eq!(watcher.watched_dirs(), vec![(root.clone(), false)]);

    // One directory at a time: each is a change, and each update watches one
    // level deeper.
    std::fs::create_dir(root.join("figs")).unwrap();
    let batches = drain_at_least_one(&rx, "creating figs/");
    assert!(mentions(&batches, &root.join("figs")), "{batches:?}");
    watcher.update(&set).unwrap();
    assert_eq!(
        watcher.watched_dirs(),
        vec![(root.clone(), false), (root.join("figs"), false)]
    );

    std::fs::create_dir(root.join("figs/sub")).unwrap();
    let batches = drain_at_least_one(&rx, "creating figs/sub/");
    assert!(mentions(&batches, &root.join("figs/sub")), "{batches:?}");
    watcher.update(&set).unwrap();
    assert_eq!(
        watcher.watched_dirs(),
        vec![(root.clone(), false), (root.join("figs/sub"), false)]
    );

    write(&png, "png");
    let batch = next(&rx, "creating figs/sub/plot.png");
    assert_eq!(batch.paths, vec![png.clone()]);
    assert_eq!(drain(&rx, QUIET), vec![]);

    // All at once (`mkdir -p figs/sub && cp …`): one batch for the top
    // directory.
    std::fs::remove_dir_all(root.join("figs")).unwrap();
    drain_at_least_one(&rx, "removing figs/");
    watcher.update(&set).unwrap();
    assert_eq!(watcher.watched_dirs(), vec![(root.clone(), false)]);
    pause();
    write(&png, "png again");
    let batches = drain_at_least_one(&rx, "creating figs/sub/plot.png at once");
    assert!(mentions(&batches, &root.join("figs")), "{batches:?}");
}

#[test]
fn an_update_reports_what_changed_while_the_watches_changed() {
    let dir = tempfile::tempdir().unwrap();
    let root = canonical(dir.path());
    let document = root.join("document.md");
    let png = root.join("other/x.png");
    write(&document, "# A\n");
    write(&png, "v0");
    let (mut watcher, rx) = start(&WatchSet::new([document.clone()], []));
    // `other/` is not watched yet; a build reads x.png, which changes before
    // the update
    // that starts watching it.
    write(&png, "v1");
    assert_eq!(drain(&rx, QUIET), vec![]);
    watcher
        .update(&WatchSet::new([document.clone(), png.clone()], []))
        .unwrap();
    let batch = next(&rx, "updating to a set with a changed file");
    assert_eq!(batch.paths, vec![png.clone()]);
    assert_eq!(drain(&rx, QUIET), vec![]);

    // An update that changes no watch reports nothing.
    watcher
        .update(&WatchSet::new([document.clone(), png.clone()], []))
        .unwrap();
    assert_eq!(drain(&rx, QUIET), vec![]);

    // Nor do updates that change watches: x.png's change was reported already.
    watcher.update(&WatchSet::new([document.clone()], [])).unwrap();
    assert_eq!(watcher.watched_dirs(), vec![(root.clone(), false)]);
    watcher
        .update(&WatchSet::new([document.clone(), png.clone()], []))
        .unwrap();
    assert_eq!(watcher.watched_dirs().len(), 2);
    assert_eq!(drain(&rx, QUIET), vec![]);

    // A change while `other/` is unwatched is reported by the update that
    // watches it.
    watcher.update(&WatchSet::new([document.clone()], [])).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    write(&png, "v2");
    assert_eq!(drain(&rx, QUIET), vec![]);
    watcher.update(&WatchSet::new([document, png.clone()], [])).unwrap();
    let batch = next(&rx, "watching other/ again");
    assert_eq!(batch.paths, vec![png]);
    assert_eq!(drain(&rx, QUIET), vec![]);
}

#[test]
fn an_explicit_input_takes_precedence_over_editor_and_git_ignores() {
    let (_dir, document, theme, _css) = document_and_theme();
    let backup = theme.join("styles/x.css~");
    let git_head = theme.join(".git/HEAD");
    write(&backup, "old stylesheet\n");
    write(&git_head, "old reference\n");
    let set =
        WatchSet::new([document, backup.clone(), git_head.clone()], [theme]);
    let (_watcher, rx) = start(&set);

    append(&backup, "new stylesheet\n");
    let batch = next(&rx, "editing an explicitly listed backup file");
    assert_eq!(batch.paths, vec![canonical(&backup)]);
    assert_eq!(batch.kind, ChangeKind::Css);
    assert_eq!(drain(&rx, QUIET), vec![]);

    append(&git_head, "new reference\n");
    let batch = next(&rx, "editing an explicitly listed .git file");
    assert_eq!(batch.paths, vec![canonical(&git_head)]);
    assert_eq!(batch.kind, ChangeKind::Content);
    assert_eq!(drain(&rx, QUIET), vec![]);
}

#[test]
fn reading_watched_inputs_does_not_report_a_change() {
    let (_dir, document, theme, css) = document_and_theme();
    let set = WatchSet::new([document.clone()], [theme.clone()]);
    let (_watcher, rx) = start(&set);

    // Linux emits open/read/close events when a build reads its own inputs.
    for _ in 0..3 {
        assert_eq!(std::fs::read_to_string(&document).unwrap(), "# A\n");
        assert!(std::fs::read_to_string(&css).unwrap().contains("red"));
        let entries: Vec<_> = std::fs::read_dir(&theme).unwrap().collect();
        assert!(!entries.is_empty());
    }
    assert_eq!(drain(&rx, QUIET), vec![]);

    // Suppressing access events must still allow the next mutation through.
    append(&document, "\nA real edit\n");
    let batch = next(&rx, "a mutation after reading the inputs");
    assert_eq!(batch.paths, vec![canonical(&document)]);
    assert_eq!(batch.kind, ChangeKind::Content);
    assert_eq!(drain(&rx, QUIET), vec![]);
}

#[test]
fn removing_and_recreating_a_file_keeps_its_parent_watch() {
    let (_dir, document, _theme, _css) = document_and_theme();
    let set = WatchSet::new([document.clone()], []);
    let (_watcher, rx) = start(&set);
    let expected = canonical(&document);

    std::fs::remove_file(&document).unwrap();
    let batches = drain_at_least_one(&rx, "removing a listed file");
    assert!(mentions(&batches, &expected), "{batches:?}");
    assert!(batches.iter().all(|batch| batch.kind == ChangeKind::Content));

    write(&document, "# Recreated\n");
    let batches = drain_at_least_one(&rx, "recreating a listed file");
    assert!(mentions(&batches, &expected), "{batches:?}");

    append(&document, "\nEdited after recreation\n");
    let batch = next(&rx, "editing the recreated file");
    assert_eq!(batch.paths, vec![expected]);
    assert_eq!(batch.kind, ChangeKind::Content);
    assert_eq!(drain(&rx, QUIET), vec![]);
}

#[test]
fn a_recursive_theme_directory_can_be_removed_and_created_again() {
    let (dir, document, theme, css) = document_and_theme();
    let root = canonical(dir.path());
    let theme = canonical(&theme);
    let css = canonical(&css);
    let set = WatchSet::new([document], [theme.clone()]);
    let (mut watcher, rx) = start(&set);

    std::fs::remove_dir_all(&theme).unwrap();
    let batches = drain_at_least_one(&rx, "removing the recursive theme");
    assert!(mentions(&batches, &theme), "{batches:?}");
    watcher.update(&set).unwrap();
    assert_eq!(watcher.watched_dirs(), vec![(root.clone(), false)]);

    pause();
    write(&css, "body { color: blue; }\n");
    let batches = drain_at_least_one(&rx, "recreating the theme tree");
    assert!(mentions(&batches, &theme), "{batches:?}");
    watcher.update(&set).unwrap();
    assert_eq!(watcher.watched_dirs(), vec![(root, false), (theme, true)]);
    drain(&rx, QUIET);

    append(&css, "p { color: green; }\n");
    let batch = next(&rx, "editing a recreated recursive theme");
    assert_eq!(batch.paths, vec![css]);
    assert_eq!(batch.kind, ChangeKind::Css);
    assert_eq!(drain(&rx, QUIET), vec![]);
}

#[test]
fn a_missing_recursive_theme_is_watched_after_creation() {
    let dir = tempfile::tempdir().unwrap();
    let root = canonical(dir.path());
    let document = root.join("document.md");
    let theme = root.join("theme");
    let css = theme.join("styles/new.css");
    write(&document, "# A\n");
    let set = WatchSet::new([document], [theme.clone()]);
    let (mut watcher, rx) = start(&set);
    assert_eq!(watcher.watched_dirs(), vec![(root.clone(), false)]);

    write(&css, "body { color: blue; }\n");
    let batches = drain_at_least_one(&rx, "creating a missing recursive theme");
    assert!(mentions(&batches, &theme), "{batches:?}");
    watcher.update(&set).unwrap();
    assert_eq!(watcher.watched_dirs(), vec![(root, false), (theme, true)]);
    drain(&rx, QUIET);

    append(&css, "p { color: green; }\n");
    let batch = next(&rx, "editing the newly watched theme");
    assert_eq!(batch.paths, vec![css]);
    assert_eq!(batch.kind, ChangeKind::Css);
    assert_eq!(drain(&rx, QUIET), vec![]);
}

#[test]
fn backend_batches_use_pages_paths_and_updated_classification() {
    let dir = tempfile::tempdir().unwrap();
    let template = dir.path().join("template.typ");
    write(&template, "// Initial template\n");
    let mut set = WatchSet::new([template.clone()], []);
    set.pages_paths.push(canonical(&template));
    let (mut watcher, rx) = start(&set);
    let watched = watcher.watched_dirs();

    append(&template, "// Pages change\n");
    let batch = next(&rx, "editing a pages-only template");
    assert_eq!(batch.paths, vec![canonical(&template)]);
    assert_eq!(batch.kind, ChangeKind::Pages);
    assert_eq!(drain(&rx, QUIET), vec![]);

    // The directory watches stay the same; classification must use the new set.
    set.pages_paths.clear();
    watcher.update(&set).unwrap();
    assert_eq!(watcher.watched_dirs(), watched);
    assert_eq!(drain(&rx, QUIET), vec![]);
    append(&template, "// Ordinary content now\n");
    let batch = next(&rx, "editing after only classification changes");
    assert_eq!(batch.paths, vec![canonical(&template)]);
    assert_eq!(batch.kind, ChangeKind::Content);
    assert_eq!(drain(&rx, QUIET), vec![]);
}

#[test]
fn a_watcher_can_be_sent_to_another_thread() {
    fn assert_send<T: Send>() {}
    assert_send::<Watcher>();

    let (_dir, document, _theme, _css) = document_and_theme();
    let (watcher, rx) = start(&WatchSet::new([document.clone()], []));
    let (done, finished) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        let batch = next(&rx, "editing after sending the watcher to a thread");
        drop(watcher);
        done.send(batch).unwrap();
    });
    append(&document, "\nEdited after moving the watcher\n");
    let batch = finished.recv_timeout(BATCH_TIMEOUT).unwrap();
    thread.join().unwrap();
    assert_eq!(batch.paths, vec![canonical(&document)]);
    assert_eq!(batch.kind, ChangeKind::Content);
}

#[cfg(unix)]
#[test]
fn a_missing_input_through_symlink_parent_tracks_the_actual_target() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let root = canonical(dir.path());
    let document = root.join("document.md");
    let target_dir = root.join("actual/nested");
    let link = root.join("link");
    write(&document, "# A\n");
    std::fs::create_dir_all(&target_dir).unwrap();
    symlink(&target_dir, &link).unwrap();

    // The OS resolves the symlink before `..`; the input does not exist yet.
    let written = link.join("../created.md");
    let actual = root.join("actual/created.md");
    let lexical_alias = root.join("created.md");
    let set = WatchSet::new([document, written.clone()], []);
    assert!(set.files.contains(&actual), "{set:?}");
    assert!(!set.files.contains(&lexical_alias), "{set:?}");
    let (_watcher, rx) = start(&set);

    write(&lexical_alias, "An unrelated file at the lexical alias\n");
    assert_eq!(drain(&rx, QUIET), vec![]);

    std::fs::write(&written, "Created through the symlink parent\n").unwrap();
    assert_eq!(
        std::fs::read_to_string(&actual).unwrap(),
        "Created through the symlink parent\n"
    );
    let batch = next(&rx, "creating a missing input through symlink/..");
    assert_eq!(batch.paths, vec![actual.clone()]);
    assert_eq!(batch.kind, ChangeKind::Content);
    assert_eq!(drain(&rx, QUIET), vec![]);

    append(&actual, "Edited at the actual path\n");
    let batch = next(&rx, "editing the resolved target after creation");
    assert_eq!(batch.paths, vec![actual]);
    assert_eq!(batch.kind, ChangeKind::Content);
    assert_eq!(drain(&rx, QUIET), vec![]);
}
