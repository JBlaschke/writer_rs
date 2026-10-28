//! Opt-in headless-browser checks for standalone HTML and the DevTools client.
//!
//! Run with `cargo test --features browser-tests --test browser`. Chrome is
//! discovered through the same helper used by later serve/browser tests. A
//! missing browser skips with an explanation unless WRITER_REQUIRE_CHROME is
//! nonempty. Fixtures are local files in a directory the selected browser can
//! read; the standalone example embeds its assets and needs no network.
//!
//! Client regression fixtures are lifted from presenter_rs/tests/browser.rs.
//! Serve/live-reload browser tests belong to T23 and page rendering to T24.

#![cfg(all(feature = "browser-tests", unix))]

use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use writer::config::Target;
use writer::pdf::devtools::{Page, find_chrome, readable_temp_dir};
use writer::pipeline::{BuildOptions, WorldSlot, build_targets};

/// A loaded machine gets time to start Chrome; each later call has its own bound.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);
const CALL_TIMEOUT: Duration = Duration::from_secs(30);
const NAVIGATION_TIMEOUT: Duration = Duration::from_secs(8);

/// Wait for fonts, images and KaTeX, then let two frames complete. Every awaited
/// promise shares the same deadline, including document.fonts.ready and frames.
const DOCUMENT_READY: &str = r#"(async () => {
    const deadline = performance.now() + 20000;
    const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
    const bounded = (promise) => Promise.race([
        promise.then(() => true),
        sleep(Math.max(0, deadline - performance.now())).then(() => false)
    ]);
    if (!await bounded(document.fonts.ready)) return { timeout: 'fonts' };
    const ready = () =>
        [...document.images].every((img) => img.complete && img.naturalWidth > 0) &&
        [...document.querySelectorAll('[data-math-style]')].every((el) =>
            el.dataset.rendered === '1' && el.querySelector('.katex'));
    while (!ready()) {
        if (performance.now() >= deadline) return { timeout: 'images or math' };
        await sleep(25);
    }
    const frames = new Promise((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(resolve)));
    if (!await bounded(frames)) return { timeout: 'animation frames' };
    return true;
})()"#;

/// Inspect the finished document once, so failures show useful browser facts.
const DOCUMENT_FACTS: &str = r#"(() => {
    const roots = document.querySelectorAll('[data-writer-document]');
    const images = [...document.images];
    const math = [...document.querySelectorAll('[data-math-style]')];
    return {
        roots: roots.length,
        content: roots[0]?.textContent.trim().length ?? 0,
        highlighted: document.querySelectorAll('pre.hl-code').length,
        highlightSpans: document.querySelectorAll('pre.hl-code span[class*="hl-"]').length,
        math: math.length,
        renderedMath: math.filter((el) => el.dataset.rendered === '1').length,
        katex: document.querySelectorAll('.katex').length,
        images: images.length,
        completeImages: images.filter((img) => img.complete && img.naturalWidth > 0).length,
        remoteResources: performance.getEntriesByType('resource')
            .map((entry) => entry.name).filter((url) => /^https?:/i.test(url))
    };
})()"#;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn chrome_required(value: Option<&OsStr>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}

/// A browser, or a visible skip; CI can turn missing Chrome into a failure.
fn chrome() -> Option<PathBuf> {
    let found = find_chrome(None);
    if found.is_none() {
        let required = std::env::var_os("WRITER_REQUIRE_CHROME");
        assert!(
            !chrome_required(required.as_deref()),
            "no chrome found, but WRITER_REQUIRE_CHROME is set"
        );
        eprintln!("skipping: no chrome found");
    }
    found
}

/// All actual browser launches take turns, including after a failed assertion.
fn serial() -> MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    TURN.lock().unwrap_or_else(PoisonError::into_inner)
}

fn scratch(chrome: &Path) -> tempfile::TempDir {
    let base = readable_temp_dir(chrome).unwrap_or_else(|| {
        panic!(
            "{} is a snap that has never run, so it can read no temporary directory: \
             run it once (that creates ~/snap/<name>/common), or set CHROME_PATH to \
             another browser",
            chrome.display()
        )
    });
    tempfile::tempdir_in(&base)
        .unwrap_or_else(|error| panic!("{}: {error}", base.display()))
}

/// Copy source fixtures without carrying an earlier example build into the test.
fn copy_example(chrome: &Path, name: &str) -> tempfile::TempDir {
    let dir = scratch(chrome);
    copy_dir(&root().join("examples").join(name), dir.path());
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name() == "dist" {
            continue;
        }
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            fs::create_dir(&target).unwrap();
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Percent-encode path bytes, including non-UTF-8 Unix names, for a local URL.
fn file_url(path: &Path) -> String {
    assert!(path.is_absolute(), "file URL needs an absolute path");
    let mut url = String::from("file://");
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            url.push(char::from(byte));
        } else {
            url.push_str(&format!("%{byte:02X}"));
        }
    }
    url
}

fn open_page(chrome: &Path) -> Page {
    let mut page = Page::launch(chrome, STARTUP_TIMEOUT)
        .unwrap_or_else(|error| panic!("{error}"));
    page.set_timeout(CALL_TIMEOUT);
    page
}

fn navigate(page: &mut Page, path: &Path) {
    page.navigate(&file_url(path)).unwrap_or_else(|error| panic!("{error}"));
}

fn evaluate(page: &mut Page, expression: &str) -> Value {
    page.evaluate(expression)
        .unwrap_or_else(|error| panic!("{error}\nexpression: {expression}"))
}

fn assert_no_page_errors(page: &mut Page) {
    evaluate(page, "0");
    assert!(
        page.errors().is_empty(),
        "the page reported errors:\n{}",
        page.errors().join("\n")
    );
}

/// Full build-mode rendering of the copied example, without a second process to
/// supervise. Output is explicitly standalone and stays in the browser's scratch.
fn build_standalone(dir: &Path) -> PathBuf {
    let output = dir.join("dist/document.html");
    let mut options = BuildOptions::new(dir.join("document.md"));
    options.cli.out = Some(output.clone());
    options.cli.standalone = Some(true);
    let report =
        build_targets(&options, &[Target::Html], &mut WorldSlot::new());
    assert!(report.prepared.is_ok(), "prepare failed: {:?}", report.prepared);
    assert_eq!(report.targets.len(), 1, "{report:?}");
    let written = report.targets[0]
        .written
        .as_ref()
        .unwrap_or_else(|error| panic!("standalone build failed: {error}"));
    assert!(written.contains(&output), "written outputs: {written:?}");
    assert!(output.is_file(), "{} was not written", output.display());
    output
}

#[test]
fn the_standalone_basic_example_renders_without_page_errors() {
    let _turn = serial();
    let Some(chrome) = chrome() else {
        return;
    };
    let dir = copy_example(&chrome, "basic");
    let html = build_standalone(dir.path());
    let mut page = open_page(&chrome);
    navigate(&mut page, &html);
    let ready = evaluate(&mut page, DOCUMENT_READY);
    assert_eq!(
        ready,
        true,
        "document did not become ready: {ready}; {:?}",
        page.errors()
    );
    let facts = evaluate(&mut page, DOCUMENT_FACTS);
    eprintln!("standalone basic example: {facts}");
    let count = |key: &str| {
        facts[key]
            .as_u64()
            .unwrap_or_else(|| panic!("missing count {key}: {facts}"))
    };
    assert_eq!(count("roots"), 1, "{facts}");
    assert!(count("content") > 500, "{facts}");
    assert!(count("highlighted") > 0, "{facts}");
    assert!(count("highlightSpans") > 0, "{facts}");
    assert!(count("math") >= 3, "{facts}");
    assert_eq!(count("renderedMath"), count("math"), "{facts}");
    assert_eq!(count("katex"), count("math"), "{facts}");
    assert!(count("images") > 0, "{facts}");
    assert_eq!(count("completeImages"), count("images"), "{facts}");
    assert_eq!(facts["remoteResources"], json!([]), "{facts}");
    assert_no_page_errors(&mut page);
}

/// Error collection, same-document navigation and return-by-value semantics are
/// prerequisites for every later browser test. Expected fixture errors stay local.
#[test]
fn the_devtools_page_reports_errors_fragments_values_and_refusals() {
    let _turn = serial();
    let Some(chrome) = chrome() else {
        return;
    };
    let dir = scratch(&chrome);
    let html = dir.path().join("errors.html");
    fs::write(
        &html,
        "<!doctype html><meta charset=\"utf-8\"><title>errors</title>\n\
         <img src=\"missing.png\" alt=\"\">\n\
         <script>console.error('smoke', 42); setTimeout(() => { throw new Error('later'); });\n\
         setTimeout(() => { throw 'a string'; });</script>\n",
    ).unwrap();
    let mut page = open_page(&chrome);
    navigate(&mut page, &html);
    evaluate(&mut page, "new Promise((resolve) => setTimeout(resolve, 200))");
    evaluate(&mut page, "0");
    let errors = page.errors().join("\n");
    eprintln!("expected fixture errors:\n{errors}");
    assert!(errors.contains("console.error: smoke 42"), "{errors}");
    assert!(errors.contains("Error: later"), "{errors}");
    assert!(errors.contains("Uncaught a string"), "{errors}");
    assert!(errors.contains("missing.png"), "{errors}");
    assert_eq!(page.errors().len(), 4, "{errors}");

    evaluate(&mut page, "window.sameDocument = true");
    page.navigate(&format!("{}#fragment", file_url(&html)))
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        evaluate(&mut page, "[location.hash, window.sameDocument === true]"),
        json!(["#fragment", true]),
    );
    let values =
        ["NaN", "Infinity", "-Infinity", "-0", "2n ** 70n", "undefined", "1.5"]
            .map(|expression| evaluate(&mut page, expression));
    assert_eq!(
        values,
        [
            json!("NaN"),
            json!("Infinity"),
            json!("-Infinity"),
            json!("-0"),
            json!("1180591620717411303424n"),
            Value::Null,
            json!(1.5),
        ]
    );
    assert_eq!(
        evaluate(
            &mut page,
            "new Promise((resolve) => setTimeout(() => resolve({answer: 42}), 20))"
        ),
        json!({"answer": 42}),
    );
    let thrown = page.evaluate("null.x").unwrap_err().to_string();
    assert!(thrown.starts_with("JavaScript threw: TypeError"), "{thrown}");
    let thrown = page.evaluate("throw 'boom'").unwrap_err().to_string();
    assert_eq!(thrown, "JavaScript threw: Uncaught boom");
    let rejected = page.evaluate("Promise.reject('nope')").unwrap_err();
    assert_eq!(
        rejected.to_string(),
        "JavaScript threw: Uncaught (in promise) nope"
    );
    let missing = file_url(&dir.path().join("nothing-here.html"));
    let failed = page.navigate(&missing).unwrap_err().to_string();
    assert!(failed.contains("nothing-here.html"), "{failed}");
    assert!(failed.contains("readable_temp_dir"), "{failed}");
    assert!(!failed.contains("stderr"), "{failed}");
}

/// A stale load event from an earlier self-navigation must not complete the
/// next navigation; a destroyed execution context must fail without a timeout.
#[test]
fn navigate_waits_for_its_own_loader_and_evaluate_refusal_is_immediate() {
    let _turn = serial();
    let Some(chrome) = chrome() else {
        return;
    };
    let dir = scratch(&chrome);
    for (name, body) in [
        (
            "a.html",
            "<script>setTimeout(() => { location.href = 'b.html'; }, 50);</script>",
        ),
        ("b.html", ""),
        (
            "c.html",
            "<script>const end = Date.now() + 1500; while (Date.now() < end) {}</script>",
        ),
    ] {
        let title = name.trim_end_matches(".html");
        fs::write(
            dir.path().join(name),
            format!("<!doctype html><title>{title}</title>{body}\n"),
        )
        .unwrap();
    }
    let mut page = open_page(&chrome);
    page.set_timeout(NAVIGATION_TIMEOUT);
    navigate(&mut page, &dir.path().join("a.html"));
    // Let a.html replace itself while no call is draining Chrome's events.
    thread::sleep(Duration::from_millis(800));
    let start = Instant::now();
    navigate(&mut page, &dir.path().join("c.html"));
    let took = start.elapsed();
    assert!(took >= Duration::from_millis(1400), "returned after {took:?}");
    assert_eq!(
        evaluate(&mut page, "[document.title, document.readyState]"),
        json!(["c", "complete"])
    );

    let start = Instant::now();
    let refused = page
        .evaluate("location.reload(), new Promise(() => {})")
        .unwrap_err()
        .to_string();
    eprintln!("evaluate refused in {:?}: {refused}", start.elapsed());
    assert!(start.elapsed() < Duration::from_secs(5), "{refused}");
    assert!(
        refused.starts_with("Chrome (DevTools): Runtime.evaluate: "),
        "{refused}"
    );
    assert!(!refused.contains("stderr"), "{refused}");
    assert_no_page_errors(&mut page);
}

/// Inline navigation before load must follow the replacing loader, which is
/// the lifecycle used when the live-reload client reloads from its first hello.
#[test]
fn navigate_follows_a_page_that_replaces_itself_before_load() {
    let _turn = serial();
    let Some(chrome) = chrome() else {
        return;
    };
    let dir = scratch(&chrome);
    fs::write(
        dir.path().join("a.html"),
        "<!doctype html><title>a</title><script>location.href = 'b.html';</script>\n",
    ).unwrap();
    fs::write(dir.path().join("b.html"), "<!doctype html><title>b</title>\n")
        .unwrap();
    let mut page = open_page(&chrome);
    page.set_timeout(NAVIGATION_TIMEOUT);
    let start = Instant::now();
    navigate(&mut page, &dir.path().join("a.html"));
    let took = start.elapsed();
    assert!(took < Duration::from_secs(5), "returned after {took:?}");
    assert_eq!(
        evaluate(&mut page, "[document.title, document.readyState]"),
        json!(["b", "complete"])
    );
    assert_no_page_errors(&mut page);
}

#[test]
fn require_chrome_switch_is_nonempty_without_changing_process_environment() {
    assert!(!chrome_required(None));
    assert!(!chrome_required(Some(OsStr::new(""))));
    assert!(chrome_required(Some(OsStr::new("1"))));
    assert!(chrome_required(Some(OsStr::new("0"))));
}

#[test]
fn file_urls_encode_reserved_and_non_utf8_path_bytes() {
    let path = Path::new(OsStr::from_bytes(b"/tmp/a b#%?\xFF.html"));
    assert_eq!(file_url(path), "file:///tmp/a%20b%23%25%3F%FF.html");
}
