//! One headless Chrome page driven over the DevTools protocol.
//!
//! Lifted from presenter_rs for browser smoke tests; compiled only with the
//! `browser-tests` feature. This Unix pipe client creates a private throwaway
//! browser profile, enforces a deadline for each call, and collects page errors.
//!
//! [`Page::navigate`] waits for the navigation's document or its replacement,
//! using frame and loader IDs. [`Page::evaluate`] awaits promises and returns
//! JSON values. [`Page::errors`] includes uncaught exceptions, console errors,
//! failed-resource log entries and dismissed JavaScript dialogs. Events are
//! collected while calls run; `evaluate("0")` drains earlier pending events.
//!
//! Executable discovery follows [`find_chrome`]. For file pages, create files
//! under [`readable_temp_dir`] so snap browsers can read them. Dropping a page
//! closes Chrome, kills and reaps it if needed, then removes its profile; a
//! watchdog also handles the parent process exiting unexpectedly.

use std::collections::VecDeque;
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader, ErrorKind, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{
    Child, ChildStderr, ChildStdin, Command, ExitStatus, Stdio,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::error::{Error, Result};

/// How long Chrome may take to exit after `Browser.close` before it is killed.
const CLOSE_GRACE: Duration = Duration::from_secs(5);

/// Maximum outgoing DevTools message, below Chrome's 100 MiB pipe limit.
const MAX_MESSAGE: usize = 96 << 20;
const _: () = assert!(MAX_MESSAGE < 100 << 20);

/// macOS app bundles searched by [`find_chrome`], in order: bundle and executable name.
/// Each is looked for in `/Applications`, then `~/Applications`.
const MAC_APPS: &[(&str, &str)] = &[
    ("Google Chrome.app", "Google Chrome"),
    ("Chromium.app", "Chromium"),
    ("Microsoft Edge.app", "Microsoft Edge"),
    ("Brave Browser.app", "Brave Browser"),
    ("Arc.app", "Arc"),
];

/// Executable names searched on `PATH` by [`find_chrome`], in order.
const PATH_NAMES: &[&str] = &[
    "google-chrome", "google-chrome-stable", "chromium", "chromium-browser",
    "microsoft-edge",
];

/// Chrome's command line, before `--user-data-dir=<profile>` and `about:blank`: headless,
/// a quiet throwaway profile (no first-run UI, sync, extensions or keychain access) and the
/// DevTools protocol on descriptors 3 and 4.
const CHROME_FLAGS: &[&str] = &[
    "--headless=new",
    "--disable-gpu",
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-extensions",
    "--disable-background-networking",
    "--disable-component-update",
    "--disable-sync",
    "--disable-default-apps",
    "--disable-domain-reliability",
    "--disable-client-side-phishing-detection",
    "--no-service-autorun",
    "--password-store=basic",
    "--use-mock-keychain",
    "--hide-scrollbars",
    "--mute-audio",
    "--remote-debugging-pipe",
];

/// Runs Chrome (`$0`, with the arguments `$@`) with the shell's stdin and stdout pipes moved
/// to descriptors 3 and 4, where `--remote-debugging-pipe` expects them. `exec` keeps the
/// process id, so writer's child is Chrome itself.
const LAUNCH_SCRIPT: &str =
    r#"exec "$0" "$@" 3<&0 4>&1 0</dev/null 1>/dev/null"#;

/// The watchdog of one browser (module docs, cleanup); `$1` is the temporary directory. Its
/// stdin carries `chrome <pid>` and `done` lines. Without `done`, it waits up to 3 s for
/// Chrome to exit, then kills it (checking it every second, for at most 10 s), and removes
/// the directory (retrying while Chrome's helper processes finish writing into it). A
/// process counts as Chrome only while its command line names the directory (Chrome's
/// `--user-data-dir`), so a reused process id is never killed. Without `ps`,
/// ownership cannot be verified and the watchdog leaves the process alone.
const JANITOR_SCRIPT: &str = r#"trap '' INT TERM HUP
dir=$1
chrome=
while read -r word arg; do
  case $word in
    chrome) chrome=$arg ;;
    done) exit 0 ;;
  esac
done
running() {
  [ -n "$chrome" ] || return 1
  if command -v ps >/dev/null 2>&1; then
    ps -ww -p "$chrome" -o args= 2>/dev/null | grep -qF -- "$dir"
  else
    # An unverified process id may now belong to somebody else.
    return 1
  fi
}
n=0
while [ "$n" -lt 10 ] && running; do
  if [ "$n" -ge 3 ]; then kill -9 "$chrome" 2>/dev/null; fi
  sleep 1
  n=$((n + 1))
done
n=0
while [ -e "$dir" ] && [ "$n" -lt 10 ]; do
  rm -rf "$dir" 2>/dev/null || sleep 1
  n=$((n + 1))
done
"#;

/// How many of Chrome's last stderr lines an error shows.
const STDERR_TAIL: usize = 30;

/// How long an error report waits for Chrome's last stderr lines to arrive.
const STDERR_DRAIN: Duration = Duration::from_millis(500);

/// How often a wait for Chrome's next message checks whether Chrome is still running.
const POLL: Duration = Duration::from_millis(100);

/// At most this much of an invalid DevTools message is shown in an error.
const SHOWN_MESSAGE: usize = 200;

/// A headless Chrome with one open page (module docs). Dropping it stops Chrome and
/// removes its profile.
pub struct Page {
    /// The browser as given to [`Page::launch`], for error messages.
    chrome: PathBuf,
    /// The running browser. Declared before `profile`: fields are dropped in order, and
    /// Chrome must be gone before its profile directory is removed.
    browser: Browser,
    /// The temporary directory with Chrome's profile (and its watchdog), removed on drop.
    profile: TempDir,
    /// The page's flat DevTools session.
    session: String,
    /// The last DevTools request id used.
    last_id: u64,
    /// How long each call may take.
    timeout: Duration,
    /// The page's errors collected so far.
    errors: Vec<String>,
}

/// Why a call to the page failed.
enum CallError {
    /// Chrome is fine but refused the request or could not do it; the reason.
    Refused(String),
    /// Driving Chrome failed: it exited, closed its pipe, did not answer in time, or the
    /// page crashed or was closed.
    Browser(Failure),
}

impl From<Failure> for CallError {
    fn from(failure: Failure) -> CallError {
        CallError::Browser(failure)
    }
}

/// A directory that `chrome` can read files from, for pages to open as `file://` URLs: the
/// system's temporary directory (`$TMPDIR`, `/tmp` when unset or empty), or for a
/// snap browser its `~/snap/<name>/common`. `None`
/// for a snap without that directory (one that has never run): it can read no temporary
/// directory at all; running it once creates the directory. The returned base
/// is absolute, and canonical when it exists, so file URLs preserve symlink/..
/// semantics and relative TMPDIR values. Bare executable names resolve on PATH
/// before snap placement is detected.
pub fn readable_temp_dir(chrome: &Path) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let path_var = std::env::var_os("PATH");
    readable_temp_dir_in(
        chrome,
        home.as_deref(),
        std::env::temp_dir(),
        path_var.as_deref(),
    )
}

/// The readable base with injected environment paths for portable tests.
fn readable_temp_dir_in(
    chrome: &Path, home: Option<&Path>, temp_dir: PathBuf,
    path_var: Option<&OsStr>,
) -> Option<PathBuf> {
    let resolved = resolve_executable(chrome, path_var);
    let executable = resolved.as_deref().unwrap_or(chrome);
    let base = match placement(executable, home) {
        (Some(dir), _) => dir,
        (None, FileAccess::Readable) => system_temp_dir(temp_dir),
        (None, FileAccess::Sandboxed) => return None,
    };
    Some(std::fs::canonicalize(&base).unwrap_or_else(|_| absolute(&base)))
}

impl Page {
    /// Starts `chrome` headless and opens a blank page. `chrome` is resolved
    /// as a direct executable, macOS app bundle or bare PATH name. Every call,
    /// including startup, must finish within `timeout`.
    ///
    /// Errors (all [`Error::Other`]): `chrome` is not an executable; the temporary profile
    /// cannot be created; Chrome cannot be started, exits or does not answer in time (with
    /// the browser's path and its last stderr lines); not macOS or Linux.
    pub fn launch(chrome: &Path, timeout: Duration) -> Result<Page> {
        if cfg!(not(unix)) {
            return Err(Error::Other(
                "driving Chrome needs macOS or Linux (Chrome is started through /bin/sh)".into(),
            ));
        }
        let path_var = std::env::var_os("PATH");
        let executable = resolve_executable(chrome, path_var.as_deref())
            .ok_or_else(|| {
                Error::Other(format!(
                    "cannot start {}: not an executable file",
                    chrome.display()
                ))
            })?;
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let (base, source) = placement(&executable, home.as_deref());
        let base =
            base.unwrap_or_else(|| system_temp_dir(std::env::temp_dir()));
        let mut profile = TempDir::new(&base, "writer-devtools")?;
        let scratch = chrome_scratch_dir(&profile, source)?;
        let browser = Browser::launch(
            &executable,
            &profile.path().join("profile"),
            scratch.as_deref(),
        )?;
        profile.watch(browser.pid());
        tracing::debug!("started {} for a DevTools page", executable.display());
        let mut page = Page {
            chrome: chrome.to_path_buf(),
            browser,
            profile,
            session: String::new(),
            last_id: 0,
            timeout,
            errors: Vec::new(),
        };
        page.session = page.run("opening a page", |cdp, _| open_page(cdp))?;
        Ok(page)
    }

    /// Sets the timeout of every DevTools call from now on (`launch`'s applies until then):
    /// a test can give Chrome's start-up the time a loaded machine needs and still expect
    /// one call to return quickly.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Loads `url` in the page and waits for the `load` event of the document it creates,
    /// or of the document that replaced it before its `load` (module docs). When `url`
    /// differs from the current page's only in its fragment (`#/2`), Chrome stays on the
    /// page and fires no `load` event: then this returns as soon as Chrome has accepted the
    /// navigation.
    ///
    /// Errors: Chrome cannot load it (`net::ERR_…`; for a `file://` URL with a hint about
    /// [`readable_temp_dir`]), the page crashes, Chrome exits or the timeout passes.
    pub fn navigate(&mut self, url: &str) -> Result<()> {
        self.run("loading a page", |cdp, session| {
            // Every lifecycle event from the request on, in order: those that arrive before
            // the reply are judged once the reply names the loader.
            let mut events = Vec::new();
            let params = json!({ "url": url });
            let navigation =
                call(cdp, session, "Page.navigate", params, &mut |message| {
                    events.extend(Lifecycle::of(message));
                })?;
            if let Some(error) =
                navigation.get("errorText").and_then(Value::as_str)
            {
                return Err(CallError::Refused(load_error(url, error)));
            }
            // A same-document navigation has no loader of its own.
            let Some(loader) =
                navigation.get("loaderId").and_then(Value::as_str)
            else {
                return Ok(());
            };
            let frame = navigation.get("frameId").and_then(Value::as_str);
            let mut awaited = LoadWait::new(frame, loader);
            let mut judged = 0;
            loop {
                for event in &events[judged..] {
                    if awaited.loaded(event) {
                        return Ok(());
                    }
                }
                judged = events.len();
                let message = cdp.next_message()?;
                events.extend(Lifecycle::of(&message));
                cdp.handle_event(&message)?;
            }
        })
    }

    /// Evaluates the JavaScript `expression` in the page, awaits its value when it is a
    /// promise, and returns that value as JSON. `undefined` is `null`; the numbers JSON
    /// cannot hold, `NaN`, `Infinity`, `-Infinity` and `-0`, and BigInts are strings in
    /// JavaScript's spelling (`"NaN"`, `"-0"`, `"1n"`); objects come back as Chrome
    /// serializes them by value (functions, DOM nodes and `Map`s as `{}`; inside an array
    /// or object, `NaN` and the like as `null`, as with `JSON.stringify`).
    ///
    /// Errors: the expression throws or its promise rejects (`JavaScript threw: ` and
    /// Chrome's description of the exception, for an `Error` the first line of its stack,
    /// for an object its class, like `Object`; for a thrown string, `null` or `undefined`,
    /// Chrome's text and the value, like `Uncaught boom`), or Chrome fails as for
    /// [`Page::navigate`].
    pub fn evaluate(&mut self, expression: &str) -> Result<Value> {
        let evaluated = self.run("evaluating JavaScript", |cdp, session| {
            let params = json!({
                "expression": expression,
                "awaitPromise": true,
                "returnByValue": true,
            });
            call(cdp, session, "Runtime.evaluate", params, &mut |_| {})
        })?;
        if let Some(details) = evaluated.get("exceptionDetails") {
            return Err(Error::Other(format!(
                "JavaScript threw: {}",
                exception_text(details)
            )));
        }
        Ok(result_value(&evaluated["result"]))
    }

    /// The page's errors so far (module docs), oldest first: `console.error: …` for a
    /// console message, the first line of an uncaught exception, a `Log` entry's text with
    /// its URL, `dismissed a JavaScript …` for a dialog.
    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    /// Runs `step` on a DevTools connection to the page's browser, with the page's session
    /// and timeout, and keeps the errors the page reported meanwhile. `stage` names what is
    /// going on in error messages.
    fn run<T>(
        &mut self, stage: &'static str,
        step: impl FnOnce(&mut Cdp<'_>, &str) -> std::result::Result<T, CallError>,
    ) -> Result<T> {
        let deadline = Instant::now() + self.timeout;
        let mut cdp = Cdp::new(&mut self.browser, deadline);
        cdp.next_id = self.last_id;
        cdp.stage = stage;
        let result = step(&mut cdp, &self.session);
        self.last_id = cdp.next_id;
        self.errors.append(&mut cdp.page_errors);
        result.map_err(|error| match error {
            CallError::Refused(reason) => {
                Error::Other(format!("Chrome (DevTools): {reason}"))
            }
            CallError::Browser(failure) => self.error(failure),
        })
    }

    /// The error when Chrome failed: the reason, the browser and Chrome's last stderr
    /// lines.
    fn error(&self, failure: Failure) -> Error {
        let mut message = format!(
            "Chrome (DevTools) failed: {}\n  browser: {}",
            failure.describe(self.timeout),
            self.chrome.display()
        );
        let stderr = self.browser.stderr_tail();
        if stderr.is_empty() {
            message.push_str("\n  Chrome wrote nothing on stderr");
        } else {
            message.push_str(&format!(
                "\n  Chrome's stderr (last {} lines):",
                stderr.len()
            ));
            for line in stderr {
                message.push_str("\n    ");
                message.push_str(&line);
            }
        }
        Error::Other(message)
    }
}

impl std::fmt::Debug for Page {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Page")
            .field("chrome", &self.chrome)
            .field("pid", &self.browser.pid())
            .field("profile", &self.profile.path())
            .field("timeout", &self.timeout)
            .field("errors", &self.errors)
            .finish_non_exhaustive()
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        {
            let deadline = Instant::now() + CLOSE_GRACE;
            let mut cdp = Cdp::new(&mut self.browser, deadline);
            cdp.next_id = self.last_id;
            cdp.close_browser();
        }
        self.browser.stop(CLOSE_GRACE);
        // Then `browser` (already stopped) and `profile` (removed) are dropped, in order.
    }
}

/// Opens a blank page with a flat session, enables the domains whose events [`Cdp`]
/// handles and the page's lifecycle events; returns the session id.
fn open_page(cdp: &mut Cdp<'_>) -> std::result::Result<String, CallError> {
    let target =
        cdp.call(None, "Target.createTarget", json!({ "url": "about:blank" }))?;
    let target_id = string_field(&target, "targetId")?;
    let attach = json!({ "targetId": target_id, "flatten": true });
    let attached = cdp.call(None, "Target.attachToTarget", attach)?;
    let session = string_field(&attached, "sessionId")?;
    for enable in
        ["Page.enable", "Runtime.enable", "Inspector.enable", "Log.enable"]
    {
        cdp.call(Some(&session), enable, json!({}))?;
    }
    let lifecycle = json!({ "enabled": true });
    cdp.call(Some(&session), "Page.setLifecycleEventsEnabled", lifecycle)?;
    Ok(session)
}

/// Sends `method` to the page `session` and returns its result, like [`Cdp::call`]: every
/// other message that arrives meanwhile is shown to `watch` and then handled by
/// [`Cdp::handle_event`]. An error reply is [`CallError::Refused`].
fn call(
    cdp: &mut Cdp<'_>, session: &str, method: &str, params: Value,
    watch: &mut dyn FnMut(&Value),
) -> std::result::Result<Value, CallError> {
    let id = cdp.send(Some(session), method, params)?;
    loop {
        let mut message = cdp.next_message()?;
        if message.get("id").and_then(Value::as_u64) != Some(id) {
            watch(&message);
            cdp.handle_event(&message)?;
            continue;
        }
        if let Some(error) = message.get("error") {
            let text = error.get("message").and_then(Value::as_str);
            let text = text.unwrap_or("unknown error");
            return Err(CallError::Refused(format!("{method}: {text}")));
        }
        return Ok(message["result"].take());
    }
}

/// One `Page.lifecycleEvent`: a document (`loader`) of a frame reached a stage (`name`:
/// `init` when it starts, `DOMContentLoaded`, `load`, …).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Lifecycle {
    /// The frame's id.
    frame: String,
    /// The document's loader id.
    loader: String,
    /// The stage.
    name: String,
}

impl Lifecycle {
    /// The lifecycle event `message` reports, if it is one.
    fn of(message: &Value) -> Option<Lifecycle> {
        if message["method"] != "Page.lifecycleEvent" {
            return None;
        }
        let params = &message["params"];
        let field = |name: &str| params[name].as_str().map(str::to_string);
        Some(Lifecycle {
            frame: field("frameId")?,
            loader: field("loaderId")?,
            name: field("name")?,
        })
    }
}

/// The document [`Page::navigate`] waits for (module docs): at first the one its navigation
/// created; when a newer document of the same frame starts after that one did, that one.
#[derive(Debug)]
struct LoadWait {
    /// The navigated frame, if Chrome named it (else every frame counts).
    frame: Option<String>,
    /// The awaited document's loader.
    loader: String,
    /// Whether the awaited document has started (its `init` event came).
    started: bool,
}

impl LoadWait {
    /// Waits for the document `loader` of `frame`.
    fn new(frame: Option<&str>, loader: &str) -> LoadWait {
        LoadWait {
            frame: frame.map(str::to_string),
            loader: loader.to_string(),
            started: false,
        }
    }

    /// Takes the page's next lifecycle event (in the order Chrome sent them); true once
    /// the awaited document has fired `load`. Events of a document that started before
    /// the awaited one (a navigation the page made by itself earlier) do not count.
    fn loaded(&mut self, event: &Lifecycle) -> bool {
        if self.frame.as_ref().is_some_and(|frame| *frame != event.frame) {
            return false;
        }
        match event.name.as_str() {
            "init" if event.loader == self.loader => self.started = true,
            "init" if self.started => self.loader.clone_from(&event.loader),
            "load" => return event.loader == self.loader,
            _ => {}
        }
        false
    }
}

/// The reason when Chrome cannot load `url` (`error`, e.g. `net::ERR_FILE_NOT_FOUND`); for
/// a `file://` URL with a hint about sandboxed browsers.
fn load_error(url: &str, error: &str) -> String {
    let mut reason = format!("cannot load {url}: {error}");
    if url.starts_with("file:") {
        reason.push_str(
            "\n  if the file exists, a sandboxed browser (a snap) may be unable to read its \
             directory: put it into `writer::pdf::devtools::readable_temp_dir`",
        );
    }
    reason
}

/// The JSON value of a `Runtime.RemoteObject` returned by value (see [`Page::evaluate`]).
fn result_value(result: &Value) -> Value {
    if let Some(value) = result.get("value") {
        return value.clone();
    }
    match result.get("unserializableValue") {
        Some(Value::String(spelling)) => Value::String(spelling.clone()),
        _ => Value::Null,
    }
}

/// Finds an executable browser: explicit path, CHROME_PATH, macOS app bundles,
/// then known Chrome/Chromium/Edge names on PATH, in that order.
///
/// An explicit path must resolve; an invalid CHROME_PATH warns and falls back.
/// Names, executable paths and macOS `.app` bundles are accepted.
pub fn find_chrome(explicit: Option<&Path>) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let bundles = if cfg!(target_os = "macos") {
        mac_bundles(home.as_deref())
    } else {
        Vec::new()
    };
    find_chrome_in(explicit, &|name| std::env::var_os(name), &bundles)
}

/// [`find_chrome`] with the environment (`var`) and the app bundle candidates given.
fn find_chrome_in(
    explicit: Option<&Path>, var: &dyn Fn(&str) -> Option<OsString>,
    bundles: &[PathBuf],
) -> Option<PathBuf> {
    let path_var = var("PATH");
    let path_var = path_var.as_deref();
    if let Some(explicit) = explicit {
        return resolve_executable(explicit, path_var);
    }
    if let Some(env) = var("CHROME_PATH").filter(|value| !value.is_empty()) {
        match resolve_executable(Path::new(&env), path_var) {
            Some(found) => return Some(found),
            None => tracing::warn!(
                "CHROME_PATH={} is not an executable file; looking for Chrome elsewhere",
                Path::new(&env).display()
            ),
        }
    }
    if let Some(bundle) = bundles.iter().find(|bundle| is_executable(bundle)) {
        return Some(bundle.clone());
    }
    PATH_NAMES.iter().find_map(|name| which(Path::new(name), path_var?))
}

/// The macOS app bundle executables of [`MAC_APPS`]: each app in `/Applications`, then in
/// `<home>/Applications`.
fn mac_bundles(home: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/Applications")];
    roots.extend(home.map(|home| home.join("Applications")));
    MAC_APPS
        .iter()
        .flat_map(|(app, exe)| {
            roots.iter().map(move |root| {
                root.join(app).join("Contents/MacOS").join(exe)
            })
        })
        .collect()
}

/// `path` as an executable: itself, the executable of a `.app` bundle, or a bare name
/// looked up on `path_var`.
fn resolve_executable(
    path: &Path, path_var: Option<&OsStr>,
) -> Option<PathBuf> {
    if path.extension().is_some_and(|ext| ext == "app") && path.is_dir() {
        let exe = path.join("Contents/MacOS").join(path.file_stem()?);
        return is_executable(&exe).then(|| absolute(&exe));
    }
    if is_executable(path) {
        return Some(absolute(path));
    }
    if is_bare_name(path) {
        which(path, path_var?)
    } else {
        None
    }
}

/// True for a name without any directory part (`chromium`, not `./chromium`).
fn is_bare_name(path: &Path) -> bool {
    path.components().count() == 1 && path.parent() == Some(Path::new(""))
}

/// The first executable `name` in the directories of `path_var` (a `PATH` value).
fn which(name: &Path, path_var: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path_var)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
        .map(|found| absolute(&found))
}

/// `path` made absolute against the current directory (unchanged if that fails).
fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// True for a regular file (after symlinks) that is executable (any `x` bit on Unix).
fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

fn system_temp_dir(temp_dir: PathBuf) -> PathBuf {
    if temp_dir.as_os_str().is_empty() {
        PathBuf::from("/tmp")
    } else {
        temp_dir
    }
}

/// Whether Chrome can read a local scratch directory for this placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileAccess {
    /// Ordinary browser, or a snap with an existing common directory.
    Readable,
    /// A snap whose private temporary directory is not readable by Writer.
    Sandboxed,
}

fn chrome_scratch_dir(
    dir: &TempDir, source: FileAccess,
) -> Result<Option<PathBuf>> {
    if source != FileAccess::Readable {
        return Ok(None);
    }
    let scratch = dir.path().join("tmp");
    std::fs::create_dir(&scratch).map_err(|e| {
        Error::Other(format!("cannot create {}: {e}", scratch.display()))
    })?;
    Ok(Some(scratch))
}

fn placement(
    chrome: &Path, home: Option<&Path>,
) -> (Option<PathBuf>, FileAccess) {
    let Some(name) = snap_name(chrome) else {
        return (None, FileAccess::Readable);
    };
    let common = home.map(|home| home.join("snap").join(name).join("common"));
    match common.filter(|dir| dir.is_dir()) {
        Some(dir) => (Some(dir), FileAccess::Readable),
        None => (None, FileAccess::Sandboxed),
    }
}

/// The snap a browser belongs to: `/snap/bin/<name>[.<app>]`, `/snap/<name>/…`, or a small
/// wrapper script that runs `/snap/bin/<name>` (like Ubuntu's `chromium-browser`).
fn snap_name(chrome: &Path) -> Option<String> {
    if let Ok(rest) = chrome.strip_prefix("/snap") {
        let mut parts = rest.iter().map(OsStr::to_str);
        let first = parts.next()??;
        let name = match (first, parts.next()) {
            ("bin", Some(Some(app))) => app.split('.').next()?,
            ("bin", _) => return None,
            (name, _) => name,
        };
        return Some(name.to_string());
    }
    let mut head = Vec::new();
    std::fs::File::open(chrome)
        .and_then(|file| file.take(4096).read_to_end(&mut head))
        .ok()?;
    if !head.starts_with(b"#!") {
        return None;
    }
    let text = String::from_utf8_lossy(&head);
    let start = text.find("/snap/bin/")? + "/snap/bin/".len();
    let name: String = text[start..]
        .chars()
        .take_while(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-'
        })
        .collect();
    (!name.is_empty()).then_some(name)
}

fn string_field(
    result: &Value, field: &str,
) -> std::result::Result<String, Failure> {
    result.get(field).and_then(Value::as_str).map(str::to_string).ok_or_else(
        || Failure::Other(format!("DevTools reply without {field}: {result}")),
    )
}

/// A `Runtime.ExceptionDetails` as one line: the exception's description (for an `Error`,
/// the first line of its stack: `TypeError: …`), or for a thrown value without one, like
/// `throw 'boom'`, Chrome's text and the value (`Uncaught boom`), else Chrome's text.
fn exception_text(details: &Value) -> String {
    let exception = &details["exception"];
    let text = details["text"].as_str();
    let text = match (exception["description"].as_str(), exception.get("value"))
    {
        (Some(description), _) => description.to_string(),
        (None, Some(value)) => {
            let value = match value {
                Value::String(value) => value.clone(),
                other => other.to_string(),
            };
            format!("{} {value}", text.unwrap_or("Uncaught"))
        }
        (None, None) => text.unwrap_or("unknown error").to_string(),
    };
    text.lines().next().unwrap_or("").to_string()
}

/// The time left until `deadline`; `None` when it has passed.
fn remaining(deadline: Instant) -> Option<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|left| !left.is_zero())
}

/// Why driving Chrome failed.
#[derive(Debug)]
enum Failure {
    /// The deadline passed during this stage.
    Timeout(&'static str),
    /// Anything else.
    Other(String),
}

impl Failure {
    /// The reason as shown to the user.
    fn describe(self, timeout: Duration) -> String {
        match self {
            Failure::Timeout(stage) => {
                format!("timed out after {} s while {stage}", timeout.as_secs())
            }
            Failure::Other(message) => message,
        }
    }
}

/// A DevTools protocol connection to a launched browser (its pipes, flat sessions).
struct Cdp<'a> {
    /// The browser.
    browser: &'a mut Browser,
    /// Deadline of the current public call.
    deadline: Instant,
    /// The last request id used.
    next_id: u64,
    /// What is going on, for error messages.
    stage: &'static str,
    /// The page's uncaught exceptions, `console.error` messages, dismissed dialogs and
    /// `error` entries of the `Log` domain (only `devtools::Page` enables that domain).
    page_errors: Vec<String>,
}

impl<'a> Cdp<'a> {
    /// A connection to `browser` for one public call, ending by `deadline`.
    fn new(browser: &'a mut Browser, deadline: Instant) -> Cdp<'a> {
        Cdp {
            browser,
            deadline,
            next_id: 0,
            stage: "starting Chrome",
            page_errors: Vec::new(),
        }
    }

    /// Sends `method` (to the page `session`, or to the browser) and returns its result,
    /// handling the events that arrive meanwhile.
    fn call(
        &mut self, session: Option<&str>, method: &str, params: Value,
    ) -> std::result::Result<Value, CallError> {
        let id = self.send(session, method, params)?;
        loop {
            let mut message = self.next_message()?;
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                self.handle_event(&message)?;
                continue;
            }
            if let Some(error) = message.get("error") {
                let text =
                    error.get("message").and_then(Value::as_str).unwrap_or("");
                return Err(CallError::Refused(format!("{method}: {text}")));
            }
            return Ok(message["result"].take());
        }
    }

    /// Sends `method` without waiting for its reply and returns its request id.
    fn send(
        &mut self, session: Option<&str>, method: &str, params: Value,
    ) -> std::result::Result<u64, Failure> {
        self.next_id += 1;
        let mut request =
            json!({ "id": self.next_id, "method": method, "params": params });
        if let Some(session) = session {
            request["sessionId"] = Value::from(session);
        }
        tracing::trace!("CDP -> {method}");
        let bytes = request.to_string().into_bytes();
        if bytes.len() >= MAX_MESSAGE {
            return Err(Failure::Other(format!(
                "{method}: the DevTools message is {} bytes, more than Chrome accepts \
                 ({MAX_MESSAGE} bytes)",
                bytes.len()
            )));
        }
        let commands = self.browser.commands.as_ref();
        if commands.is_some_and(|commands| commands.send(bytes).is_ok()) {
            Ok(self.next_id)
        } else {
            Err(self.gone())
        }
    }

    /// The next message from Chrome, waiting at most until the deadline and failing as soon
    /// as Chrome has exited or either pipe has closed.
    fn next_message(&mut self) -> std::result::Result<Value, Failure> {
        loop {
            let left =
                remaining(self.deadline).ok_or(Failure::Timeout(self.stage))?;
            match self.browser.replies.recv_timeout(left.min(POLL)) {
                Ok(Incoming::Message(bytes)) => {
                    return serde_json::from_slice(&bytes).map_err(|e| {
                        let text = String::from_utf8_lossy(&bytes);
                        let shown: String =
                            text.chars().take(SHOWN_MESSAGE).collect();
                        Failure::Other(format!(
                            "invalid DevTools message ({e}): {shown}"
                        ))
                    });
                }
                Ok(Incoming::WriteFailed(error)) => {
                    return Err(self.unwritable(&error));
                }
                Err(RecvTimeoutError::Timeout) => {
                    if let Some(status) = self.browser.try_wait() {
                        return Err(self.exited(status));
                    }
                }
                Ok(Incoming::Closed) | Err(RecvTimeoutError::Disconnected) => {
                    return Err(self.gone());
                }
            }
        }
    }

    /// The failure when a command could not be written (`error`): Chrome exited, or closed
    /// its command pipe.
    fn unwritable(&mut self, error: &std::io::Error) -> Failure {
        match self.browser.wait_exit(Duration::from_secs(1)) {
            Some(status) => self.exited(status),
            None => Failure::Other(format!(
                "Chrome closed its DevTools command pipe ({error}) while {}",
                self.stage
            )),
        }
    }

    /// The failure when Chrome exited with `status`.
    fn exited(&self, status: ExitStatus) -> Failure {
        Failure::Other(format!("Chrome exited ({status}) while {}", self.stage))
    }

    /// The failure when Chrome's pipe broke: Chrome exited, or closed it.
    fn gone(&mut self) -> Failure {
        match self.browser.wait_exit(Duration::from_secs(1)) {
            Some(status) => self.exited(status),
            None => Failure::Other(format!(
                "Chrome closed its DevTools pipe while {}",
                self.stage
            )),
        }
    }

    /// Handles an event (or a reply nobody waits for): records page errors and
    /// dismisses dialogs; a crashed or closed page is an error.
    fn handle_event(
        &mut self, message: &Value,
    ) -> std::result::Result<(), Failure> {
        let params = &message["params"];
        let session = message.get("sessionId").and_then(Value::as_str);
        match message.get("method").and_then(Value::as_str) {
            None => {
                if let Some(error) = message.get("error") {
                    tracing::debug!(
                        "DevTools request {} failed: {error}",
                        message["id"]
                    );
                }
            }
            Some("Page.javascriptDialogOpening") => {
                self.dismiss_dialog(session, params)?
            }
            Some("Runtime.exceptionThrown") => {
                let text = exception_text(&params["exceptionDetails"]);
                tracing::debug!("page exception: {text}");
                self.page_errors.push(text);
            }
            Some("Runtime.consoleAPICalled") => {
                let kind = params["type"].as_str().unwrap_or("log");
                let text = console_text(params);
                tracing::debug!("page console.{kind}: {text}");
                if kind == "error" {
                    self.page_errors.push(format!("console.error: {text}"));
                }
            }
            // `Page::launch` enables the Log domain for failed-resource reports.
            Some("Log.entryAdded") => {
                let entry = &params["entry"];
                let text = entry["text"].as_str().unwrap_or("");
                tracing::debug!("page log ({}): {text}", entry["level"]);
                if entry["level"] == "error" {
                    self.page_errors.push(match entry["url"].as_str() {
                        Some(url) if !url.is_empty() => {
                            format!("{text} ({url})")
                        }
                        _ => text.to_string(),
                    });
                }
            }
            Some("Inspector.targetCrashed" | "Target.targetCrashed") => {
                return Err(Failure::Other(format!(
                    "the page crashed while {}",
                    self.stage
                )));
            }
            Some("Target.detachedFromTarget") => {
                return Err(Failure::Other(format!(
                    "the page was closed while {}",
                    self.stage
                )));
            }
            _ => {}
        }
        Ok(())
    }

    /// Dismisses a JavaScript dialog (it would block the page) and records it.
    fn dismiss_dialog(
        &mut self, session: Option<&str>, params: &Value,
    ) -> std::result::Result<(), Failure> {
        let kind = params["type"].as_str().unwrap_or("dialog");
        let text = params["message"].as_str().unwrap_or("");
        tracing::warn!(
            "the page opened a JavaScript {kind} ({text:?}); dismissed it"
        );
        self.page_errors.push(format!("dismissed a JavaScript {kind}: {text}"));
        let params = json!({ "accept": false });
        self.send(session, "Page.handleJavaScriptDialog", params).map(drop)
    }

    /// Asks the browser to exit (best effort; [`Browser::stop`] makes sure).
    fn close_browser(&mut self) {
        if let Err(failure) = self.send(None, "Browser.close", json!({})) {
            tracing::debug!("Browser.close: {failure:?}");
        }
    }
}

/// The arguments of a `Runtime.consoleAPICalled` event, space-separated.
fn console_text(params: &Value) -> String {
    let args = params["args"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    args.iter()
        .map(|arg| match (&arg["value"], arg["description"].as_str()) {
            (Value::String(text), _) => text.clone(),
            (Value::Null, Some(description)) => description.to_string(),
            (value, _) => value.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A running Chrome and the threads that talk to it. Dropping it kills Chrome if it is
/// still running.
struct Browser {
    /// The Chrome process.
    child: Child,
    /// Messages for Chrome, written to its descriptor 3 by a writer thread. `None` once
    /// closed; the writer then closes the pipe, which makes Chrome exit.
    commands: Option<Sender<Vec<u8>>>,
    /// Chrome's messages from its descriptor 4, read by a reader thread, and the writer
    /// thread's failure.
    replies: Receiver<Incoming>,
    /// Its last [`STDERR_TAIL`] stderr lines, filled by a reader thread.
    stderr: Arc<Mutex<VecDeque<String>>>,
    /// Disconnected when that thread reaches the end of Chrome's stderr.
    stderr_closed: Receiver<()>,
}

impl Browser {
    /// Starts `chrome` (the module docs) with its profile in `profile`, its own
    /// temporary files in `scratch` when given (`TMPDIR` in its environment), and the
    /// threads for its pipes.
    fn launch(
        chrome: &Path, profile: &Path, scratch: Option<&Path>,
    ) -> Result<Browser> {
        let mut user_data_dir = OsString::from("--user-data-dir=");
        user_data_dir.push(profile);
        tracing::debug!(
            "starting {} {}",
            chrome.display(),
            CHROME_FLAGS.join(" ")
        );
        let mut command = Command::new("/bin/sh");
        if let Some(scratch) = scratch {
            command.env("TMPDIR", scratch);
        }
        command.arg("-c").arg(LAUNCH_SCRIPT).arg(chrome).args(CHROME_FLAGS);
        if running_as_root() {
            // Chrome refuses to start as root (containers, CI) with its sandbox on.
            command.arg("--no-sandbox");
        }
        let mut child = command
            .arg(user_data_dir)
            .arg("about:blank")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                Error::Other(format!("cannot start {}: {e}", chrome.display()))
            })?;
        let pipes =
            (child.stdin.take(), child.stdout.take(), child.stderr.take());
        let (commands, to_chrome) = mpsc::channel();
        let (from_chrome, replies) = mpsc::channel();
        let write_failures = from_chrome.clone();
        let (stderr_open, stderr_closed) = mpsc::channel::<()>();
        let stderr = Arc::new(Mutex::new(VecDeque::new()));
        let log = Arc::clone(&stderr);
        // From here on, an early return drops `browser`, which kills Chrome.
        let browser = Browser {
            child,
            commands: Some(commands),
            replies,
            stderr,
            stderr_closed,
        };
        let (Some(input), Some(output), Some(errors)) = pipes else {
            return Err(Error::Other(
                "Chrome was started without its pipes".into(),
            ));
        };
        spawn_thread("chrome-commands", move || {
            write_messages(input, &to_chrome, &write_failures);
        })?;
        spawn_thread("chrome-replies", move || {
            read_messages(output, &from_chrome)
        })?;
        spawn_thread("chrome-stderr", move || {
            read_stderr(errors, &log);
            drop(stderr_open);
        })?;
        Ok(browser)
    }

    /// Chrome's process id.
    fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Chrome's exit status, if it has exited.
    fn try_wait(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().ok().flatten()
    }

    /// Chrome's last stderr lines, after giving the last ones [`STDERR_DRAIN`] to arrive.
    fn stderr_tail(&self) -> Vec<String> {
        let _ = self.stderr_closed.recv_timeout(STDERR_DRAIN);
        let lines = self.stderr.lock().unwrap_or_else(PoisonError::into_inner);
        lines.iter().cloned().collect()
    }

    /// Waits up to `grace` for Chrome to exit and returns its status.
    fn wait_exit(&mut self, grace: Duration) -> Option<ExitStatus> {
        let until = Instant::now() + grace;
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) if Instant::now() < until => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                _ => return None,
            }
        }
    }

    /// Closes Chrome's command pipe, gives Chrome `grace` to exit, then kills it, and reaps
    /// it.
    fn stop(&mut self, grace: Duration) {
        self.commands = None;
        if self.wait_exit(grace).is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        self.stop(Duration::ZERO);
    }
}

/// Starts a named thread.
fn spawn_thread(
    name: &str, body: impl FnOnce() + Send + 'static,
) -> Result<()> {
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(body)
        .map(drop)
        .map_err(|e| {
            Error::Other(format!(
                "cannot start a thread to talk to Chrome: {e}"
            ))
        })
}

/// What the threads on Chrome's DevTools pipes report to [`Cdp::next_message`].
#[derive(Debug)]
enum Incoming {
    /// A message from Chrome, without its NUL terminator.
    Message(Vec<u8>),
    /// Chrome's reply pipe reached its end (or failed).
    Closed,
    /// A command could not be written to Chrome's command pipe.
    WriteFailed(std::io::Error),
}

/// The body of the writer thread: writes each message to Chrome's command pipe with its
/// NUL terminator, until the channel closes (then the pipe is closed) or a write fails,
/// which it reports to `failures` (so that a wait for a reply ends at once).
fn write_messages(
    mut input: impl std::io::Write, messages: &Receiver<Vec<u8>>,
    failures: &Sender<Incoming>,
) {
    for mut message in messages {
        message.push(0);
        if let Err(e) = input.write_all(&message).and_then(|()| input.flush()) {
            tracing::debug!("cannot write to Chrome's DevTools pipe: {e}");
            let _ = failures.send(Incoming::WriteFailed(e));
            return;
        }
    }
}

/// The body of the reader thread: sends each NUL-terminated message from Chrome to
/// `messages`, then [`Incoming::Closed`] when the pipe closes; stops early when nobody
/// listens any more.
fn read_messages(output: impl std::io::Read, messages: &Sender<Incoming>) {
    let mut reader = BufReader::new(output);
    loop {
        let mut message = Vec::new();
        match reader.read_until(0, &mut message) {
            Ok(0) | Err(_) => {
                let _ = messages.send(Incoming::Closed);
                return;
            }
            Ok(_) => {}
        }
        if message.last() == Some(&0) {
            message.pop();
        }
        if messages.send(Incoming::Message(message)).is_err() {
            return;
        }
    }
}

/// The body of the stderr reader thread: keeps the last [`STDERR_TAIL`] lines in `log`.
fn read_stderr(pipe: ChildStderr, log: &Mutex<VecDeque<String>>) {
    let mut reader = BufReader::new(pipe);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let text = String::from_utf8_lossy(&line).trim_end().to_string();
        if text.is_empty() {
            continue;
        }
        tracing::trace!("chrome: {text}");
        let mut lines = log.lock().unwrap_or_else(PoisonError::into_inner);
        if lines.len() == STDERR_TAIL {
            lines.pop_front();
        }
        lines.push_back(text);
    }
}

/// True when this process runs as root on Linux (effective uid 0 in `/proc/self/status`),
/// where Chrome needs `--no-sandbox`.
fn running_as_root() -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }
    std::fs::read_to_string("/proc/self/status")
        .is_ok_and(|status| status_is_root(&status))
}

/// Whether a `/proc/<pid>/status` text has effective uid 0 (`Uid: real effective …`).
fn status_is_root(status: &str) -> bool {
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|uids| uids.split_whitespace().nth(1))
        == Some("0")
}

/// A private temporary directory, removed (with its contents) on drop, and the watchdog
/// that removes it if writer dies first (module docs, cleanup).
struct TempDir {
    /// The directory.
    path: PathBuf,
    /// Its watchdog; dropped (told `done`) after the directory is removed.
    janitor: Janitor,
}

impl TempDir {
    /// Creates `<base>/<prefix>-<pid>-<n>` (mode 0700 on Unix) and starts its watchdog.
    ///
    /// The directory is then named by its real path ([`std::fs::canonicalize`]; made
    /// absolute if that fails): that path becomes the page's `file://` URL and Chrome's
    /// `--user-data-dir`. A relative `base` (`TMPDIR=tmp`; [`std::env::temp_dir`] returns
    /// `$TMPDIR` as set) would make them depend on Chrome's current directory, and a `..`
    /// after a symbolic link (`TMPDIR=lnk/../t`) would name another directory in the URL:
    /// the system follows `lnk` before `..`, Chrome drops `lnk/..` from the URL.
    fn new(base: &Path, prefix: &str) -> Result<TempDir> {
        let created = create_private_dir(base, prefix)?;
        let path = std::fs::canonicalize(&created)
            .unwrap_or_else(|_| absolute(&created));
        match Janitor::start(&path) {
            Ok(janitor) => Ok(TempDir { path, janitor }),
            Err(e) => {
                remove_dir_patiently(&path);
                Err(Error::Other(format!(
                    "cannot start the cleanup watchdog (/bin/sh): {e}"
                )))
            }
        }
    }

    /// The directory.
    fn path(&self) -> &Path {
        &self.path
    }

    /// Tells the watchdog which Chrome process uses the directory.
    fn watch(&mut self, chrome: u32) {
        self.janitor.tell(&format!("chrome {chrome}"));
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        remove_dir_patiently(&self.path);
    }
}

/// Creates `<base>/<prefix>-<pid>-<n>` with mode 0700 (on Unix), for the first free `n`.
fn create_private_dir(base: &Path, prefix: &str) -> Result<PathBuf> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let pid = std::process::id();
    // Only Unix sets the mode, so the binding is never mutated elsewhere.
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    let mut attempts = 0;
    loop {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = base.join(format!("{prefix}-{pid}-{n}"));
        attempts += 1;
        match builder.create(&path) {
            Ok(()) => return Ok(path),
            // Left over by an earlier process with the same id.
            Err(e)
                if e.kind() == ErrorKind::AlreadyExists && attempts < 1_000 => {
            }
            Err(e) => {
                return Err(Error::Other(format!(
                    "cannot create a temporary directory in {}: {e}",
                    base.display()
                )));
            }
        }
    }
}

/// Removes `path` and its contents, retrying for up to 1 s: Chrome's helper processes may
/// still be finishing writes into the profile.
fn remove_dir_patiently(path: &Path) {
    for attempt in 0..20 {
        match std::fs::remove_dir_all(path) {
            Ok(()) => return,
            Err(e) if e.kind() == ErrorKind::NotFound => return,
            Err(e) if attempt == 19 => {
                tracing::warn!("cannot remove {}: {e}", path.display());
            }
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

/// The watchdog process of one browser (module docs, cleanup; `JANITOR_SCRIPT`). Dropping it
/// tells it `done` and reaps it.
struct Janitor {
    /// The `/bin/sh` process.
    child: Child,
    /// Its stdin; closing it without `done` (writer died) starts the cleanup.
    input: Option<ChildStdin>,
}

impl Janitor {
    /// Starts the watchdog for the temporary directory `dir`, in its own process group (so
    /// that a Ctrl-C in the terminal does not reach it).
    fn start(dir: &Path) -> std::io::Result<Janitor> {
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg(JANITOR_SCRIPT)
            .arg("writer-devtools-janitor")
            .arg(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn()?;
        let input = child.stdin.take();
        Ok(Janitor { child, input })
    }

    /// Sends the watchdog one line (best effort).
    fn tell(&mut self, line: &str) {
        if let Some(input) = &mut self.input
            && let Err(e) = writeln!(input, "{line}")
        {
            tracing::debug!("cannot talk to the cleanup watchdog: {e}");
        }
    }
}

impl Drop for Janitor {
    fn drop(&mut self) {
        self.tell("done");
        self.input = None;
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_missing_executable_cannot_be_launched() {
        let err = Page::launch(
            Path::new("/nonexistent/chrome"),
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "cannot start /nonexistent/chrome: not an executable file"
        );
    }

    #[test]
    fn values_json_cannot_hold_keep_their_javascript_spelling() {
        let number =
            json!({ "type": "number", "value": 1.5, "description": "1.5" });
        assert_eq!(result_value(&number), json!(1.5));
        for spelling in ["NaN", "Infinity", "-Infinity", "-0", "12n"] {
            let remote =
                json!({ "type": "number", "unserializableValue": spelling });
            assert_eq!(result_value(&remote), json!(spelling));
        }
        assert_eq!(result_value(&json!({ "type": "undefined" })), Value::Null);
        let null =
            json!({ "type": "object", "subtype": "null", "value": null });
        assert_eq!(result_value(&null), Value::Null);
    }

    #[test]
    fn file_urls_that_cannot_be_loaded_mention_sandboxed_browsers() {
        assert_eq!(
            load_error("file:///tmp/x.html", "net::ERR_FILE_NOT_FOUND"),
            "cannot load file:///tmp/x.html: net::ERR_FILE_NOT_FOUND\n  if the file exists, \
             a sandboxed browser (a snap) may be unable to read its directory: put it into \
             `writer::pdf::devtools::readable_temp_dir`"
        );
        assert_eq!(
            load_error("http://127.0.0.1:1/", "net::ERR_CONNECTION_REFUSED"),
            "cannot load http://127.0.0.1:1/: net::ERR_CONNECTION_REFUSED"
        );
    }

    /// The lifecycle event `name` of document `loader` in frame `frame`, as Chrome sends it.
    fn event(frame: &str, loader: &str, name: &str) -> Value {
        json!({
            "method": "Page.lifecycleEvent",
            "params": { "frameId": frame, "loaderId": loader, "name": name, "timestamp": 1.0 },
        })
    }

    #[test]
    fn lifecycle_events_are_read_from_their_messages() {
        assert_eq!(
            Lifecycle::of(&event("F", "B", "load")),
            Some(Lifecycle {
                frame: "F".into(),
                loader: "B".into(),
                name: "load".into()
            })
        );
        let fired = json!({ "method": "Page.loadEventFired", "params": {} });
        assert_eq!(Lifecycle::of(&fired), None);
        assert_eq!(Lifecycle::of(&json!({ "id": 3, "result": {} })), None);
    }

    /// Runs `events` (frame, loader, name) through a [`LoadWait`] for loader `A` of frame
    /// `F`; returns the index of the event it finished on.
    fn finishes_at(events: &[(&str, &str, &str)]) -> Option<usize> {
        let mut wait = LoadWait::new(Some("F"), "A");
        events.iter().position(|(frame, loader, name)| {
            wait.loaded(&Lifecycle::of(&event(frame, loader, name)).unwrap())
        })
    }

    #[test]
    fn navigate_waits_for_its_document_or_the_one_that_replaced_it() {
        // Its own document.
        let own = [
            ("F", "A", "init"),
            ("F", "A", "DOMContentLoaded"),
            ("F", "A", "load"),
        ];
        assert_eq!(finishes_at(&own), Some(2));
        // The page navigated by itself before: those events do not count.
        let stale = [
            ("F", "S", "init"),
            ("F", "S", "load"),
            ("F", "A", "init"),
            ("F", "A", "load"),
        ];
        assert_eq!(finishes_at(&stale), Some(3));
        // A script in the new document navigates again before its load (L-T24-1): the
        // replacing document's load counts, then also a further replacement's.
        let redirect =
            [("F", "A", "init"), ("F", "B", "init"), ("F", "B", "load")];
        assert_eq!(finishes_at(&redirect), Some(2));
        let twice = [
            ("F", "A", "init"),
            ("F", "B", "init"),
            ("F", "C", "init"),
            ("F", "B", "load"),
            ("F", "C", "load"),
        ];
        assert_eq!(finishes_at(&twice), Some(4));
        // Another frame (an iframe) does not count.
        let iframe = [
            ("F", "A", "init"),
            ("I", "X", "init"),
            ("I", "X", "load"),
            ("F", "A", "load"),
        ];
        assert_eq!(finishes_at(&iframe), Some(3));
        assert_eq!(
            finishes_at(&[("F", "A", "init"), ("F", "A", "DOMContentLoaded")]),
            None
        );
        // Without a frame id from Chrome, every frame counts.
        let mut any = LoadWait::new(None, "A");
        assert!(any.loaded(&Lifecycle::of(&event("G", "A", "load")).unwrap()));
    }
    #[cfg(unix)]
    use std::collections::BTreeMap;

    #[cfg(unix)]
    fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .unwrap();
        path
    }

    #[cfg(unix)]
    fn env(
        vars: &[(&str, &OsString)],
    ) -> impl Fn(&str) -> Option<OsString> + use<> {
        let vars: BTreeMap<String, OsString> = vars
            .iter()
            .map(|(name, value)| (name.to_string(), (*value).clone()))
            .collect();
        move |name| vars.get(name).cloned()
    }

    #[cfg(unix)]
    fn serial() -> std::sync::MutexGuard<'static, ()> {
        static PROCESSES: Mutex<()> = Mutex::new(());
        PROCESSES.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[cfg(unix)]
    fn entries(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| {
                entry.unwrap().file_name().to_string_lossy().into_owned()
            })
            .collect()
    }

    #[cfg(unix)]
    #[test]
    fn explicit_paths_win_and_must_be_executable() {
        let dir = tempfile::tempdir().unwrap();
        let fake = script(dir.path(), "my-chrome", "exit 0");
        let other = script(dir.path(), "other-chrome", "exit 0");
        let chrome_path = other.clone().into_os_string();
        let vars = env(&[("CHROME_PATH", &chrome_path)]);
        let bundles = [other.clone()];
        assert_eq!(find_chrome_in(Some(&fake), &vars, &bundles), Some(fake));
        // An explicit path that is not an executable is not replaced by another browser.
        let missing = dir.path().join("missing");
        assert_eq!(find_chrome_in(Some(&missing), &vars, &bundles), None);
        let plain = dir.path().join("plain.txt");
        std::fs::write(&plain, "not a program").unwrap();
        assert_eq!(find_chrome_in(Some(&plain), &vars, &bundles), None);
        assert_eq!(find_chrome_in(Some(dir.path()), &vars, &bundles), None);
    }

    #[cfg(unix)]
    #[test]
    fn chrome_path_is_used_before_well_known_locations() {
        let dir = tempfile::tempdir().unwrap();
        let fake = script(dir.path(), "fake-chrome", "exit 0");
        let bundle = script(dir.path(), "bundle-chrome", "exit 0");
        let on_path = script(dir.path(), "chromium", "exit 0");
        let chrome_path = fake.clone().into_os_string();
        let path_var = dir.path().as_os_str().to_os_string();
        let bundles = [dir.path().join("absent"), bundle.clone()];

        let vars = env(&[("CHROME_PATH", &chrome_path), ("PATH", &path_var)]);
        assert_eq!(find_chrome_in(None, &vars, &bundles), Some(fake));
        // Without CHROME_PATH: the first existing bundle, then PATH.
        let vars = env(&[("PATH", &path_var)]);
        assert_eq!(find_chrome_in(None, &vars, &bundles), Some(bundle));
        assert_eq!(find_chrome_in(None, &vars, &[]), Some(on_path.clone()));
        // An empty or broken CHROME_PATH is skipped.
        let empty = OsString::new();
        let vars = env(&[("CHROME_PATH", &empty), ("PATH", &path_var)]);
        assert_eq!(find_chrome_in(None, &vars, &[]), Some(on_path.clone()));
        let broken = dir.path().join("nope").into_os_string();
        let vars = env(&[("CHROME_PATH", &broken), ("PATH", &path_var)]);
        assert_eq!(find_chrome_in(None, &vars, &[]), Some(on_path));
        // Nothing anywhere.
        assert_eq!(find_chrome_in(None, &env(&[]), &[]), None);
    }

    #[cfg(unix)]
    #[test]
    fn path_lookup_follows_the_documented_order() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        script(first.path(), "chromium-browser", "exit 0");
        let stable = script(second.path(), "google-chrome-stable", "exit 0");
        let path_var =
            std::env::join_paths([first.path(), second.path()]).unwrap();
        let vars = env(&[("PATH", &path_var)]);
        // google-chrome-stable comes before chromium-browser in PATH_NAMES.
        assert_eq!(find_chrome_in(None, &vars, &[]), Some(stable.clone()));
        // A bare --chrome name is looked up on PATH; a CHROME_PATH name too.
        let name = Path::new("chromium-browser");
        assert_eq!(
            find_chrome_in(Some(name), &vars, &[]),
            Some(first.path().join("chromium-browser"))
        );
        let chrome_path = OsString::from("google-chrome-stable");
        let vars = env(&[("PATH", &path_var), ("CHROME_PATH", &chrome_path)]);
        assert_eq!(find_chrome_in(None, &vars, &[]), Some(stable));
    }

    #[cfg(unix)]
    #[test]
    fn app_bundles_stand_for_their_executable() {
        let dir = tempfile::tempdir().unwrap();
        let macos = dir.path().join("Chromium.app/Contents/MacOS");
        std::fs::create_dir_all(&macos).unwrap();
        let exe = script(&macos, "Chromium", "exit 0");
        let bundle = dir.path().join("Chromium.app");
        assert_eq!(find_chrome_in(Some(&bundle), &env(&[]), &[]), Some(exe));
    }

    #[cfg(unix)]
    #[test]
    fn snap_browsers_keep_their_profile_where_writer_can_remove_it() {
        assert_eq!(
            snap_name(Path::new("/snap/bin/chromium")).as_deref(),
            Some("chromium")
        );
        assert_eq!(
            snap_name(Path::new("/snap/bin/chromium.chromedriver")).as_deref(),
            Some("chromium")
        );
        assert_eq!(
            snap_name(Path::new("/snap/brave/123/opt/brave.com/brave/brave"))
                .as_deref(),
            Some("brave")
        );
        assert_eq!(snap_name(Path::new("/snap/bin")), None);
        assert_eq!(snap_name(Path::new("/usr/bin/nonexistent-browser")), None);

        // Ubuntu's chromium-browser is a script that runs the snap.
        let dir = tempfile::tempdir().unwrap();
        let wrapper = script(
            dir.path(),
            "chromium-browser",
            "if ! [ -x /snap/bin/chromium ]; then exit 1; fi\nexec /snap/bin/chromium \"$@\"",
        );
        assert_eq!(snap_name(&wrapper).as_deref(), Some("chromium"));
        let plain = script(
            dir.path(),
            "google-chrome",
            "exec /opt/google/chrome/chrome",
        );
        assert_eq!(snap_name(&plain), None);

        // A snap can read file pages from its common directory when it exists.
        // Otherwise the client reports that no readable scratch base exists.
        let home = tempfile::tempdir().unwrap();
        let snap = Path::new("/snap/bin/chromium");
        let served = (None, FileAccess::Sandboxed);
        assert_eq!(placement(snap, Some(home.path())), served);
        let common = home.path().join("snap/chromium/common");
        std::fs::create_dir_all(&common).unwrap();
        assert_eq!(
            placement(snap, Some(home.path())),
            (Some(common), FileAccess::Readable)
        );
        assert_eq!(placement(snap, None), served);
        // Other browsers can read file pages from the system temp directory.
        let other = (None, FileAccess::Readable);
        assert_eq!(placement(&plain, Some(home.path())), other);
        assert_eq!(placement(Path::new("/usr/bin/chromium"), None), other);
    }

    #[cfg(unix)]
    #[test]
    fn temporary_directories_are_private_and_removed() {
        let _serial = serial();
        let base = tempfile::tempdir().unwrap();
        let dir = TempDir::new(base.path(), "writer-test").unwrap();
        let path = dir.path().to_path_buf();
        assert!(path.starts_with(std::fs::canonicalize(base.path()).unwrap()));
        std::fs::create_dir_all(path.join("profile/nested")).unwrap();
        std::fs::write(path.join("profile/nested/file"), "x").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        let other = TempDir::new(base.path(), "writer-test").unwrap();
        assert_ne!(other.path(), path);
        let started = Instant::now();
        drop(dir);
        drop(other);
        assert!(entries(base.path()).is_empty());
        // The watchdogs were told `done` and exited at once.
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn bare_names_have_no_directory_part() {
        assert!(is_bare_name(Path::new("chromium")));
        assert!(!is_bare_name(Path::new("./chromium")));
        assert!(!is_bare_name(Path::new("bin/chromium")));
        assert!(!is_bare_name(Path::new("/usr/bin/chromium")));
    }

    #[test]
    fn mac_bundles_list_every_app_in_both_application_folders() {
        let bundles = mac_bundles(Some(Path::new("/Users/me")));
        assert_eq!(bundles.len(), 2 * MAC_APPS.len());
        assert_eq!(
            bundles[0],
            Path::new(
                "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
            )
        );
        assert_eq!(
            bundles[1],
            Path::new(
                "/Users/me/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
            )
        );
        assert!(bundles.contains(&PathBuf::from(
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"
        )));
        assert_eq!(mac_bundles(None).len(), MAC_APPS.len());
    }

    #[test]
    fn root_is_read_from_the_effective_uid() {
        assert!(status_is_root(
            "Name:\tx\nUid:\t0\t0\t0\t0\nGid:\t0\t0\t0\t0\n"
        ));
        assert!(status_is_root("Uid:\t1000\t0\t0\t0\n"));
        assert!(!status_is_root("Uid:\t1000\t1000\t1000\t1000\n"));
        assert!(!status_is_root("Name:\tx\n"));
        if !cfg!(target_os = "linux") {
            assert!(!running_as_root());
        }
    }

    #[test]
    fn an_empty_tmpdir_means_tmp() {
        assert_eq!(system_temp_dir(PathBuf::new()), Path::new("/tmp"));
        assert_eq!(system_temp_dir("/var/tmp".into()), Path::new("/var/tmp"));
        assert_eq!(system_temp_dir("reltmp".into()), Path::new("reltmp"));
    }

    #[test]
    fn write_failures_and_closed_pipes_are_reported() {
        /// A pipe whose reader is gone.
        struct Broken;
        impl std::io::Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let (commands, to_chrome) = mpsc::channel();
        let (from_chrome, replies) = mpsc::channel();
        commands.send(b"{}".to_vec()).unwrap();
        write_messages(Broken, &to_chrome, &from_chrome);
        match replies.try_recv() {
            Ok(Incoming::WriteFailed(e)) => {
                assert_eq!(e.kind(), ErrorKind::BrokenPipe)
            }
            other => panic!("{other:?}"),
        }
        // Messages are delivered with their NUL terminator removed, then the end.
        let output: &[u8] = b"{\"id\":1}\0{\"id\":2}\0";
        read_messages(output, &from_chrome);
        let received: Vec<Incoming> = replies.try_iter().collect();
        assert!(
            matches!(
                received.as_slice(),
                [Incoming::Message(a), Incoming::Message(b), Incoming::Closed]
                    if a == b"{\"id\":1}" && b == b"{\"id\":2}"
            ),
            "{received:?}"
        );
    }

    #[test]
    fn exceptions_read_as_their_description_or_the_thrown_value() {
        let error = json!({
            "text": "Uncaught",
            "exception": {
                "type": "object", "subtype": "error", "className": "TypeError",
                "description": "TypeError: x is null\n    at <anonymous>:1:1",
            },
        });
        assert_eq!(exception_text(&error), "TypeError: x is null");
        let string = json!({
            "text": "Uncaught",
            "exception": { "type": "string", "value": "boom\nsecond line" },
        });
        assert_eq!(exception_text(&string), "Uncaught boom");
        let rejected = json!({
            "text": "Uncaught (in promise)",
            "exception": { "type": "object", "subtype": "null", "value": null },
        });
        assert_eq!(exception_text(&rejected), "Uncaught (in promise) null");
        let number = json!({
            "text": "Uncaught",
            "exception": { "type": "number", "value": 3, "description": "3" },
        });
        assert_eq!(exception_text(&number), "3");
        let undefined =
            json!({ "text": "Uncaught", "exception": { "type": "undefined" } });
        assert_eq!(exception_text(&undefined), "Uncaught");
        assert_eq!(exception_text(&json!({})), "unknown error");
    }

    #[cfg(not(unix))]
    #[test]
    fn launching_on_an_unsupported_platform_reports_the_platform_first() {
        let error = Page::launch(Path::new("chrome"), Duration::from_secs(1))
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "driving Chrome needs macOS or Linux (Chrome is started through /bin/sh)"
        );
    }

    #[test]
    fn command_messages_are_nul_terminated_and_reader_keeps_message_boundaries()
    {
        let (commands, received) = mpsc::channel();
        let (failures, errors) = mpsc::channel();
        commands.send(br#"{"id":1}"#.to_vec()).unwrap();
        commands.send(br#"{"id":2}"#.to_vec()).unwrap();
        drop(commands);
        let mut output = Vec::new();
        write_messages(&mut output, &received, &failures);
        assert_eq!(output, b"{\"id\":1}\0{\"id\":2}\0");
        assert!(errors.try_recv().is_err());

        let (messages, incoming) = mpsc::channel();
        read_messages(output.as_slice(), &messages);
        let all: Vec<_> = incoming.try_iter().collect();
        assert!(matches!(
            all.as_slice(),
            [Incoming::Message(first), Incoming::Message(second), Incoming::Closed]
                if first == br#"{"id":1}"# && second == br#"{"id":2}"#
        ));
    }

    #[cfg(unix)]
    struct Process(Child);

    #[cfg(unix)]
    impl Drop for Process {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[cfg(unix)]
    fn wait_for_janitor(janitor: &mut Janitor) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = janitor.child.try_wait().unwrap() {
                assert!(status.success());
                return;
            }
            if Instant::now() >= deadline {
                let _ = janitor.child.kill();
                let _ = janitor.child.wait();
                panic!("cleanup watchdog did not exit within 10 seconds");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(unix)]
    fn mock_chrome(dir: &Path, body: &str) -> PathBuf {
        let capture = r#"for arg in "$@"; do
  case "$arg" in
    --user-data-dir=*) printf '%s\n' "${arg#--user-data-dir=}" > "$0.profile" ;;
  esac
done
"#;
        script(dir, "chrome", &format!("{capture}{body}"))
    }

    #[cfg(unix)]
    fn mock_messages(messages: &[Value]) -> String {
        let mut body = String::from("printf '%s\\0'");
        for message in messages {
            let quoted = message.to_string().replace('\'', "'\\''");
            body.push_str(&format!(" '{quoted}'"));
        }
        body.push_str(" >&4\nexec cat <&3 >/dev/null\n");
        body
    }

    #[cfg(unix)]
    fn startup_messages() -> Vec<Value> {
        let mut messages = vec![
            json!({ "id": 1, "result": { "targetId": "target" } }),
            json!({ "id": 2, "result": { "sessionId": "session" } }),
        ];
        for id in 3..=7 {
            messages.push(json!({ "id": id, "result": {} }));
        }
        messages
    }

    #[cfg(unix)]
    fn assert_mock_profile_removed(chrome: &Path) {
        let mut marker = chrome.as_os_str().to_os_string();
        marker.push(".profile");
        let profile = std::fs::read_to_string(Path::new(&marker)).unwrap();
        let parent = Path::new(profile.trim_end()).parent().unwrap();
        assert!(!parent.exists(), "profile remains: {}", parent.display());
    }

    #[cfg(unix)]
    #[test]
    fn mock_page_keeps_ids_and_reports_a_refusal_without_browser_failure() {
        let _serial = serial();
        let dir = tempfile::tempdir().unwrap();
        let mut messages = startup_messages();
        messages.push(json!({
            "id": 8, "result": { "result": { "type": "number", "value": 42 } },
        }));
        messages.push(json!({
            "id": 9, "error": { "message": "refused by the mock" },
        }));
        let chrome = mock_chrome(dir.path(), &mock_messages(&messages));
        let mut page = Page::launch(&chrome, Duration::from_secs(2)).unwrap();
        assert_eq!(page.last_id, 7);
        page.set_timeout(Duration::from_secs(1));
        assert_eq!(page.timeout, Duration::from_secs(1));
        assert_eq!(page.evaluate("6 * 7").unwrap(), json!(42));
        assert_eq!(page.last_id, 8);
        let error = page.evaluate("refused").unwrap_err().to_string();
        assert_eq!(
            error,
            "Chrome (DevTools): Runtime.evaluate: refused by the mock"
        );
        assert_eq!(page.last_id, 9);
        assert!(page.errors().is_empty());
        drop(page);
        assert_mock_profile_removed(&chrome);
    }

    #[cfg(unix)]
    #[test]
    fn page_collects_error_events_and_dismisses_dialogs_while_awaiting_a_reply()
    {
        let _serial = serial();
        let dir = tempfile::tempdir().unwrap();
        let mut messages = startup_messages();
        messages.extend([
            json!({
                "method": "Runtime.exceptionThrown",
                "params": { "exceptionDetails": { "exception": {
                    "description": "Error: mock exception\n    at smoke:1",
                } } },
            }),
            json!({
                "method": "Runtime.consoleAPICalled",
                "params": { "type": "error", "args": [
                    { "value": "mock" }, { "value": 42 },
                ] },
            }),
            json!({
                "method": "Log.entryAdded", "params": { "entry": {
                    "level": "error", "text": "missing", "url": "file:///image.png",
                } },
            }),
            json!({
                "method": "Runtime.consoleAPICalled",
                "params": { "type": "warn", "args": [{ "value": "harmless" }] },
            }),
            json!({
                "method": "Page.javascriptDialogOpening", "sessionId": "session",
                "params": { "type": "alert", "message": "dismiss me" },
            }),
            json!({
                "id": 8, "result": { "result": { "type": "boolean", "value": true } },
            }),
        ]);
        let chrome = mock_chrome(dir.path(), &mock_messages(&messages));
        let mut page = Page::launch(&chrome, Duration::from_secs(2)).unwrap();
        assert_eq!(page.evaluate("true").unwrap(), json!(true));
        assert_eq!(page.last_id, 9, "dialog dismissal also used a request ID");
        assert_eq!(
            page.errors(),
            [
                "Error: mock exception",
                "console.error: mock 42",
                "missing (file:///image.png)",
                "dismissed a JavaScript alert: dismiss me",
            ]
        );
        drop(page);
        assert_mock_profile_removed(&chrome);
    }

    #[cfg(unix)]
    #[test]
    fn failed_browser_launches_report_the_cause_and_remove_the_profile() {
        let _serial = serial();
        let cases = [
            (
                "printf 'first line\\nlast line\\n' >&2\nexit 7\n",
                "Chrome exited",
            ),
            (
                "exec 4>&-\nexec cat <&3 >/dev/null\n",
                "Chrome closed its DevTools pipe",
            ),
            (
                "printf '%s\\0' 'not-json' >&4\nexec cat <&3 >/dev/null\n",
                "invalid DevTools message",
            ),
            ("exec cat <&3 >/dev/null\n", "timed out"),
        ];
        for (body, cause) in cases {
            let dir = tempfile::tempdir().unwrap();
            let chrome = mock_chrome(dir.path(), body);
            let started = Instant::now();
            let timeout = if cause == "timed out" {
                Duration::from_millis(200)
            } else {
                Duration::from_secs(2)
            };
            let error = Page::launch(&chrome, timeout).unwrap_err().to_string();
            assert!(error.starts_with("Chrome (DevTools) failed:"), "{error}");
            assert!(error.contains(cause), "{error}");
            assert!(
                error.contains(&format!("browser: {}", chrome.display())),
                "{error}"
            );
            if cause == "Chrome exited" {
                assert!(error.contains("first line\n    last line"), "{error}");
            }
            assert!(started.elapsed() < Duration::from_secs(10), "{error}");
            assert_mock_profile_removed(&chrome);
        }
    }

    #[cfg(unix)]
    #[test]
    fn stderr_tail_keeps_the_last_thirty_lines() {
        let _serial = serial();
        let dir = tempfile::tempdir().unwrap();
        let body = "n=0\nwhile [ \"$n\" -lt 40 ]; do\n  printf 'line-%s\\n' \"$n\" >&2\n  n=$((n + 1))\ndone\nexit 7\n";
        let chrome = mock_chrome(dir.path(), body);
        let error = Page::launch(&chrome, Duration::from_secs(2))
            .unwrap_err()
            .to_string();
        assert!(error.contains("Chrome's stderr (last 30 lines):"), "{error}");
        assert!(error.contains("    line-10\n"), "{error}");
        assert!(error.ends_with("    line-39"), "{error}");
        assert!(!error.contains("    line-9\n"), "{error}");
        assert_mock_profile_removed(&chrome);
    }

    #[cfg(unix)]
    #[test]
    fn temporary_profile_uses_real_paths_after_symlinks_and_relative_bases() {
        use std::os::unix::fs::symlink;
        use std::path::Component;

        let _serial = serial();
        let base = tempfile::tempdir().unwrap();
        let real = base.path().join("real");
        std::fs::create_dir_all(real.join("nested")).unwrap();
        std::fs::create_dir(real.join("cache")).unwrap();
        symlink(real.join("nested"), base.path().join("link")).unwrap();
        let spelling = base.path().join("link/../cache");
        let expected = std::fs::canonicalize(real.join("cache")).unwrap();
        let dir = TempDir::new(&spelling, "writer-test").unwrap();
        assert!(dir.path().starts_with(&expected));
        drop(dir);

        let cwd = std::env::current_dir().unwrap();
        let levels = cwd
            .components()
            .filter(|part| matches!(part, Component::Normal(_)))
            .count();
        let mut relative = PathBuf::new();
        for _ in 0..levels {
            relative.push("..");
        }
        relative.push(expected.strip_prefix("/").unwrap());
        let dir = TempDir::new(&relative, "writer-test").unwrap();
        assert!(dir.path().starts_with(&expected));
        assert!(dir.path().is_absolute());
        drop(dir);
        assert!(entries(&expected).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn watchdog_does_not_kill_a_reused_process_id() {
        let _serial = serial();
        let base = tempfile::tempdir().unwrap();
        let profile = base.path().join("writer-test-profile");
        std::fs::create_dir(&profile).unwrap();
        let mut other = Process(
            Command::new("/bin/sh")
                .args(["-c", "exec sleep 30"])
                .spawn()
                .unwrap(),
        );
        let mut janitor = Janitor::start(&profile).unwrap();
        janitor.tell(&format!("chrome {}", other.0.id()));
        janitor.input = None;
        wait_for_janitor(&mut janitor);
        assert!(!profile.exists());
        assert!(other.0.try_wait().unwrap().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn readable_bases_are_canonical_for_relative_tmpdir_and_symlink_dot_dot() {
        use std::os::unix::fs::symlink;
        use std::path::Component;

        let base = tempfile::tempdir().unwrap();
        let real = base.path().join("real");
        std::fs::create_dir_all(real.join("nested")).unwrap();
        std::fs::create_dir(real.join("cache")).unwrap();
        symlink(real.join("nested"), base.path().join("link")).unwrap();
        let spelling = base.path().join("link/../cache");
        let expected = std::fs::canonicalize(real.join("cache")).unwrap();
        let missing = Path::new("/nonexistent/chrome");
        assert_eq!(
            readable_temp_dir_in(missing, None, spelling, None),
            Some(expected.clone())
        );

        let cwd = std::env::current_dir().unwrap();
        let mut relative = PathBuf::new();
        for part in cwd.components() {
            if matches!(part, Component::Normal(_)) {
                relative.push("..");
            }
        }
        relative.push(expected.strip_prefix("/").unwrap());
        assert_eq!(
            readable_temp_dir_in(missing, None, relative, None),
            Some(expected)
        );
        assert_eq!(
            readable_temp_dir_in(missing, None, PathBuf::new(), None),
            Some(std::fs::canonicalize("/tmp").unwrap())
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_bare_snap_wrapper_uses_the_resolved_executable_for_placement() {
        use std::os::unix::fs::symlink;

        let home = tempfile::tempdir().unwrap();
        let bin = tempfile::tempdir().unwrap();
        let temp = tempfile::tempdir().unwrap();
        script(bin.path(), "mock-snap", "exec /snap/bin/chromium \"$@\"");
        let name = Path::new("mock-snap");
        let path_var = bin.path().as_os_str();
        assert_eq!(
            readable_temp_dir_in(
                name,
                Some(home.path()),
                temp.path().into(),
                Some(path_var)
            ),
            None
        );
        let readable = home.path().join("readable");
        std::fs::create_dir(&readable).unwrap();
        std::fs::create_dir_all(home.path().join("snap/chromium")).unwrap();
        symlink(&readable, home.path().join("snap/chromium/common")).unwrap();
        let expected = std::fs::canonicalize(&readable).unwrap();
        assert_eq!(
            readable_temp_dir_in(
                name,
                Some(home.path()),
                temp.path().into(),
                Some(path_var)
            ),
            Some(expected)
        );
        let executable = resolve_executable(name, Some(path_var)).unwrap();
        assert_eq!(
            placement(&executable, Some(home.path())).1,
            FileAccess::Readable
        );
    }

    #[cfg(unix)]
    #[test]
    fn startup_protocol_refusals_do_not_collect_browser_stderr() {
        let _serial = serial();
        let dir = tempfile::tempdir().unwrap();
        let replies = [json!({
            "id": 1, "error": { "message": "startup denied" },
        })];
        let chrome = mock_chrome(dir.path(), &mock_messages(&replies));
        let started = Instant::now();
        let error = Page::launch(&chrome, Duration::from_secs(2))
            .unwrap_err()
            .to_string();
        assert_eq!(
            error,
            "Chrome (DevTools): Target.createTarget: startup denied"
        );
        assert!(!error.contains("stderr"));
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_mock_profile_removed(&chrome);
    }

    #[cfg(unix)]
    #[test]
    fn watchdog_without_ps_leaves_unverified_processes_alive() {
        use std::os::unix::fs::symlink;

        let _serial = serial();
        let base = tempfile::tempdir().unwrap();
        let bin = tempfile::tempdir().unwrap();
        let profile = base.path().join("writer-test-profile");
        std::fs::create_dir(&profile).unwrap();
        let path_var = std::env::var_os("PATH").unwrap_or_default();
        let rm = which(Path::new("rm"), &path_var).unwrap();
        symlink(rm, bin.path().join("rm")).unwrap();
        // PATH contains rm for cleanup, and deliberately no ps for ownership.
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg(JANITOR_SCRIPT)
            .arg("writer-test-janitor")
            .arg(&profile)
            .env("PATH", bin.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut other = Process(
            Command::new("/bin/sh")
                .args(["-c", "exec sleep 30"])
                .spawn()
                .unwrap(),
        );
        let input = child.stdin.take();
        let mut janitor = Janitor { child, input };
        janitor.tell(&format!("chrome {}", other.0.id()));
        janitor.input = None;
        wait_for_janitor(&mut janitor);
        assert!(!profile.exists());
        assert!(other.0.try_wait().unwrap().is_none());
    }
}
