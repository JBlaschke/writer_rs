//! Dev server routes, per-view last-good state, SSE, and bounded shutdown.
//! Lifted from presenter_rs; the two-view protocol is live-reload.md §§1–5.

use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::future::{Future, IntoFuture};
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::task::{Context, Poll};
use std::time::Duration;

use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Json, Router};
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, watch};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::{Stream, StreamExt};

use crate::assets::emit::SERVE_ASSET_PREFIX;
use crate::assets::registry::{AssetData, AssetRegistry};
use crate::config::ServeView;
use crate::error::{Error, Result};
use crate::html::{
    SERVE_HELPER_PREFIX, SERVE_THEME_CSS, normalize_helper_path,
    resolve_helper_path, serve_stylesheet,
};
use crate::pipeline::ChangeKind;
use crate::serve::events::{
    BuildEvent, Changed, DiagnosticJson, ErrorPayload, Hello, PagesView,
    Status, ViewStatus, WebView,
};
use crate::serve::files::{bytes_response, file_response, whole_response};
use crate::serve::pages::{PageSet, shell_html};
use crate::theme::Theme;

/// Maximum queued events per browser; lagging receivers skip the oldest.
pub const EVENT_CAPACITY: usize = 64;
/// Maximum time to finish open requests after shutdown begins.
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
const KEEP_ALIVE: Duration = Duration::from_secs(15);
const NO_STORE: &str = "no-store";
const VENDOR_CACHE: &str = "max-age=3600";
const PAGE_CACHE: &str = "public, max-age=31536000, immutable";
const SERVE_LIVE_RELOAD_JS: &str = "/__writer/live-reload.js";
const LIVE_RELOAD_KEY: &str = "writer/live-reload.js";

/// A successful web build, kept together so assets and HTML agree.
#[derive(Clone)]
pub struct WebBuild {
    /// Complete serve-mode document.
    pub html: String,
    /// Compiled stylesheet.
    pub theme_css: String,
    /// Registered document assets, including rendered equations.
    pub assets: AssetRegistry,
    /// Resolved theme chain for helper assets.
    pub theme: Theme,
}

/// The web view's last good build and latest attempt's diagnostics.
#[derive(Default)]
pub struct WebSlot {
    /// Latest successful build, retained after failures.
    pub good: Option<WebBuild>,
    /// Current error, cleared by a successful web build.
    pub error: Option<ErrorPayload>,
    /// Diagnostics from the latest web stage that ran.
    pub warnings: Vec<DiagnosticJson>,
}

/// The current and previous successful page sets and latest diagnostics.
#[derive(Default)]
pub struct PagesSlot {
    /// Latest successful page set.
    pub current: Option<Arc<PageSet>>,
    /// Preceding successful set, available for in-flight versioned requests.
    pub previous: Option<Arc<PageSet>>,
    /// Current error, cleared by a successful pages build.
    pub error: Option<ErrorPayload>,
    /// Diagnostics from the latest pages stage that ran.
    pub warnings: Vec<DiagnosticJson>,
}

/// State shared by blocking rebuilds and asynchronous request handlers.
pub struct ServeState {
    /// Latest attempt, including failures; zero before the first build.
    pub build_id: u64,
    /// View used when the root query does not select one explicitly.
    pub default_view: ServeView,
    /// Latest prepared title, or the input file name.
    pub title: String,
    /// Web build state.
    pub web: WebSlot,
    /// Pages build state.
    pub pages: PagesSlot,
    /// Diagnostics from the latest preparation stage that ran.
    pub prepare_warnings: Vec<DiagnosticJson>,
}

impl ServeState {
    /// Empty state before the first build.
    pub fn new(default_view: ServeView, title: String) -> Self {
        Self {
            build_id: 0,
            default_view,
            title,
            web: WebSlot::default(),
            pages: PagesSlot::default(),
            prepare_warnings: Vec::new(),
        }
    }

    /// Current state for a reconnect or `/__writer/state`; never skipped.
    pub fn hello(&self) -> Hello {
        Hello {
            build_id: self.build_id,
            status: self.status(),
            web: WebView {
                status: slot_status(self.web.error.as_ref()),
                error: self.web.error.clone(),
            },
            pages: self.pages_view(slot_status(self.pages.error.as_ref())),
            warnings: self.warnings(),
        }
    }

    /// Preparation, web, then pages diagnostics, with exact duplicates removed.
    pub fn warnings(&self) -> Vec<DiagnosticJson> {
        let mut result = Vec::new();
        for diagnostic in self
            .prepare_warnings
            .iter()
            .chain(&self.web.warnings)
            .chain(&self.pages.warnings)
        {
            if !result.contains(diagnostic) {
                result.push(diagnostic.clone());
            }
        }
        result
    }

    fn status(&self) -> Status {
        if self.web.error.is_some() || self.pages.error.is_some() {
            Status::Error
        } else {
            Status::Ok
        }
    }

    fn pages_view(&self, status: ViewStatus) -> PagesView {
        let current = self.pages.current.as_deref();
        PagesView {
            status,
            error: self.pages.error.clone(),
            count: current.map_or(0, |set| set.pages.len()),
            hashes: current.map_or_else(Vec::new, PageSet::hashes),
            size_pt: current.map_or_else(Vec::new, PageSet::sizes),
        }
    }
}

fn slot_status(error: Option<&ErrorPayload>) -> ViewStatus {
    if error.is_some() {
        ViewStatus::Error
    } else {
        ViewStatus::Ok
    }
}

/// A short-held standard lock, never held across I/O or `.await`.
pub type Shared = Arc<RwLock<ServeState>>;

/// Result of one view in a build attempt.
pub enum ViewOutcome<T> {
    /// Replace the last good value and clear the view's error.
    Built(T),
    /// Store the error while retaining the last good value.
    Failed(ErrorPayload),
    /// Keep the view's previous value and error.
    Skipped,
}

/// All state produced by one attempt; `None` diagnostics retain the stage's list.
pub struct Outcome {
    /// Effective batch classification.
    pub kind: ChangeKind,
    /// Elapsed rebuild wall time.
    pub duration_ms: u64,
    /// First changed source position, when applicable.
    pub changed: Option<Changed>,
    /// Prepared title, when preparation succeeded.
    pub title: Option<String>,
    /// Web stage result.
    pub web: ViewOutcome<WebBuild>,
    /// Pages stage result.
    pub pages: ViewOutcome<Arc<PageSet>>,
    /// Preparation diagnostics, when that stage ran.
    pub prepare_warnings: Option<Vec<DiagnosticJson>>,
    /// Web diagnostics, when that stage ran.
    pub web_warnings: Option<Vec<DiagnosticJson>>,
    /// Pages diagnostics, when that stage ran.
    pub pages_warnings: Option<Vec<DiagnosticJson>>,
}

/// A bounded channel; publishing without connected browsers is harmless.
pub fn event_channel() -> broadcast::Sender<BuildEvent> {
    broadcast::channel(EVENT_CAPACITY).0
}

/// Publish a completed attempt and return its receiver count.
pub fn publish(
    events: &broadcast::Sender<BuildEvent>, event: BuildEvent,
) -> usize {
    let build_id = event.build_id;
    let receivers = events.send(event).unwrap_or(0);
    tracing::debug!("build {build_id}: event sent to {receivers} browser(s)");
    receivers
}

/// ID to render into the next attempt's serve-mode resource URLs.
pub fn next_build_id(state: &Shared) -> u64 {
    read(state).build_id + 1
}

/// Store an attempt atomically, preserving last-good values for failed/skipped views.
pub fn apply_outcome(state: &Shared, outcome: Outcome) -> BuildEvent {
    let mut state = write(state);
    let web_status = apply_web(&mut state.web, outcome.web);
    let pages_status = apply_pages(&mut state.pages, outcome.pages);
    state.build_id += 1;
    if let Some(title) = outcome.title {
        state.title = title;
    }
    if let Some(warnings) = outcome.prepare_warnings {
        state.prepare_warnings = warnings;
    }
    if let Some(warnings) = outcome.web_warnings {
        state.web.warnings = warnings;
    }
    if let Some(warnings) = outcome.pages_warnings {
        state.pages.warnings = warnings;
    }
    BuildEvent {
        build_id: state.build_id,
        status: state.status(),
        kind: outcome.kind,
        duration_ms: outcome.duration_ms,
        changed: outcome.changed,
        web: WebView { status: web_status, error: state.web.error.clone() },
        pages: state.pages_view(pages_status),
        warnings: state.warnings(),
    }
}

fn apply_web(slot: &mut WebSlot, outcome: ViewOutcome<WebBuild>) -> ViewStatus {
    match outcome {
        ViewOutcome::Built(build) => {
            slot.good = Some(build);
            slot.error = None;
            ViewStatus::Ok
        }
        ViewOutcome::Failed(error) => {
            slot.error = Some(error);
            ViewStatus::Error
        }
        ViewOutcome::Skipped => ViewStatus::Skipped,
    }
}

fn apply_pages(
    slot: &mut PagesSlot, outcome: ViewOutcome<Arc<PageSet>>,
) -> ViewStatus {
    match outcome {
        ViewOutcome::Built(pages) => {
            slot.previous = slot.current.take();
            slot.current = Some(pages);
            slot.error = None;
            ViewStatus::Ok
        }
        ViewOutcome::Failed(error) => {
            slot.error = Some(error);
            ViewStatus::Error
        }
        ViewOutcome::Skipped => ViewStatus::Skipped,
    }
}

#[derive(Clone)]
struct App {
    state: Shared,
    events: broadcast::Sender<BuildEvent>,
}

/// The complete route table; Host checks and shutdown are installed by `serve_until`.
pub fn router(state: Shared, events: broadcast::Sender<BuildEvent>) -> Router {
    Router::new()
        .route("/", get(index))
        .route(SERVE_THEME_CSS, get(theme_css))
        .route(&format!("{SERVE_HELPER_PREFIX}{{*path}}"), get(helper_asset))
        .route(&format!("{SERVE_ASSET_PREFIX}{{id}}"), get(doc_asset))
        .route("/__writer/events", get(build_events))
        .route(SERVE_LIVE_RELOAD_JS, get(live_reload_js))
        .route("/__writer/pages.js", get(pages_js))
        .route("/__writer/pages", get(pages_shell))
        .route("/__writer/pages/{file}", get(page_svg))
        .route("/__writer/state", get(state_json))
        .route("/favicon.ico", get(|| async { StatusCode::NO_CONTENT }))
        .with_state(App { state, events })
}

/// Serves `router` on `listener` until `shutdown` completes, then shuts down gracefully:
/// no new connections are accepted, every open event stream ends, and the call returns once
/// the remaining requests have finished (at most [`SHUTDOWN_GRACE`] later; a client that
/// never completes its request would otherwise hold the server).
///
/// Only requests for the listener's address pass the Host check (module docs): exactly
/// [`serve_until_allowing`] with [`AllowedHosts::new`] of that address.
///
/// `router` is normally [`router`]`(…)`, possibly merged with more routes. Errors: the
/// listener's address cannot be read, or an I/O error of the server ([`Error::Io`]); axum
/// 0.8 handles accept errors itself and never returns one, so in practice this returns
/// `Ok(())` after the shutdown.
pub async fn serve_until<F>(
    listener: TcpListener, router: Router, shutdown: F,
) -> Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let hosts = AllowedHosts::new(listener.local_addr()?);
    serve_until_allowing(listener, router, shutdown, hosts).await
}

/// [`serve_until`] answering the requests whose `Host` header `hosts` allows (module docs);
/// every other request gets `403 Forbidden`.
pub async fn serve_until_allowing<F>(
    listener: TcpListener, router: Router, shutdown: F, hosts: AllowedHosts,
) -> Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let (down_tx, down_rx) = watch::channel(false);
    let app = router
        .layer(Extension(ShutdownSignal(down_rx.clone())))
        .layer(middleware::from_fn_with_state(Arc::new(hosts), check_host));
    let signal = async move {
        shutdown.await;
        tracing::info!("dev server shutting down");
        // Ends every event stream, so that open browser tabs do not hold the server.
        let _ = down_tx.send(true);
    };
    let server =
        axum::serve(listener, app).with_graceful_shutdown(signal).into_future();
    let deadline = async move {
        ShutdownSignal(down_rx).wait().await;
        tokio::time::sleep(SHUTDOWN_GRACE).await;
    };
    tokio::select! {
        result = server => result?,
        () = deadline => tracing::warn!(
            "dev server: connections still open {SHUTDOWN_GRACE:?} after shutdown; closing anyway"
        ),
    }
    Ok(())
}

/// The `Host` headers the dev server answers (module docs, "Host check"): the address it
/// listens on (any IP address when that is unspecified), `localhost`, `127.0.0.1`,
/// `[::1]` and the names added with [`AllowedHosts::with_name`], each with the port the
/// server listens on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowedHosts {
    /// The port the server listens on.
    port: u16,
    /// The host names allowed, lowercase.
    names: BTreeSet<String>,
    /// The IP addresses allowed (canonical: an IPv4-mapped IPv6 address is the IPv4 one).
    addresses: BTreeSet<IpAddr>,
    /// True when the server listens on every address: any IP address is allowed.
    any_address: bool,
}

impl AllowedHosts {
    /// The hosts of a server listening on `addr`: `addr`'s address, `localhost`,
    /// `127.0.0.1` and `[::1]`, with `addr`'s port (the bound one: not 0).
    pub fn new(addr: SocketAddr) -> Self {
        let loopback =
            [IpAddr::from([127, 0, 0, 1]), IpAddr::from(Ipv6Addr::LOCALHOST)];
        let mut hosts = AllowedHosts {
            port: addr.port(),
            names: BTreeSet::from(["localhost".to_string()]),
            addresses: BTreeSet::from(loopback),
            any_address: false,
        };
        hosts.add_address(addr.ip());
        hosts
    }

    /// The same hosts and `name` (the `--host` the server was started with: a name,
    /// compared ignoring ASCII case, or an IP address, `[…]` for IPv6).
    pub fn with_name(mut self, name: &str) -> Self {
        let name = name.trim().to_ascii_lowercase();
        let bare = name.strip_prefix('[').and_then(|n| n.strip_suffix(']'));
        match bare.unwrap_or(&name).parse::<IpAddr>() {
            Ok(ip) => self.add_address(ip),
            Err(_) if !name.is_empty() => {
                self.names.insert(name);
            }
            Err(_) => {}
        }
        self
    }

    /// Allows the address `ip`: every address if it is unspecified (`0.0.0.0`, `::`).
    fn add_address(&mut self, ip: IpAddr) {
        match ip.is_unspecified() {
            true => self.any_address = true,
            false => {
                self.addresses.insert(ip.to_canonical());
            }
        }
    }

    /// True if a request whose `Host` header is `host` (`None`: it has none) is for this
    /// server: an allowed name or IP address (IPv6 in brackets), with the server's port,
    /// or without a port when that is 80.
    pub fn allows(&self, host: Option<&str>) -> bool {
        let Some(host) = host else {
            return true;
        };
        let Some((name, port)) = split_host(host) else {
            return false;
        };
        let allowed = match name.parse::<IpAddr>() {
            Ok(ip) => {
                self.any_address || self.addresses.contains(&ip.to_canonical())
            }
            Err(_) => self.names.contains(&name.to_ascii_lowercase()),
        };
        allowed && port.unwrap_or(80) == self.port
    }
}

/// The name and port of a `Host` header value (`name`, `name:port`, `[v6]`, `[v6]:port`),
/// without the brackets of an IPv6 address; `None` when it is malformed.
fn split_host(host: &str) -> Option<(&str, Option<u16>)> {
    let (name, port) = match host.strip_prefix('[') {
        Some(rest) => {
            let (address, after) = rest.split_once(']')?;
            address.parse::<std::net::Ipv6Addr>().ok()?;
            match after {
                "" => (address, None),
                _ => (address, Some(after.strip_prefix(':')?)),
            }
        }
        None => match host.rsplit_once(':') {
            Some((name, port)) => (name, Some(port)),
            None => (host, None),
        },
    };
    let port = match port {
        Some(port) => Some(port.parse::<u16>().ok()?),
        None => None,
    };
    (!name.is_empty()).then_some((name, port))
}

/// The Host check (module docs): passes a request that `hosts` allows to `next`, answers
/// any other with `403 Forbidden`.
async fn check_host(
    State(hosts): State<Arc<AllowedHosts>>, request: Request, next: Next,
) -> Response {
    let host = match request.headers().get(header::HOST) {
        // A value that is not visible ASCII is no host this server has.
        Some(value) => Some(value.to_str().unwrap_or_default()),
        None => request.uri().authority().map(|authority| authority.as_str()),
    };
    if hosts.allows(host) {
        return next.run(request).await;
    }
    let host = host.unwrap_or_default().to_string();
    tracing::info!("refused a request for host {host:?} (not this server)");
    let message = format!(
        "writer serve answers requests for the address it listens on, localhost or its \
         --host name on port {}, not for host {host:?} (a protection against DNS \
         rebinding): open the address it printed\n",
        hosts.port
    );
    (StatusCode::FORBIDDEN, message).into_response()
}

/// The server's shutdown flag, given to the handlers by [`serve_until`].
#[derive(Clone)]
struct ShutdownSignal(watch::Receiver<bool>);

impl ShutdownSignal {
    /// Completes once the server shuts down (or the flag's sender is gone).
    async fn wait(mut self) {
        let _ = self.0.wait_for(|&down| down).await;
    }
}

/// A stream that ends when the server shuts down (an endless SSE response would otherwise
/// keep the graceful shutdown waiting).
struct UntilShutdown<S> {
    inner: S,
    /// `None` without a signal (a router served by something other than [`serve_until`]).
    shutdown: Option<Pin<Box<dyn Future<Output = ()> + Send>>>,
    ended: bool,
}

impl<S> UntilShutdown<S> {
    fn new(inner: S, signal: Option<ShutdownSignal>) -> Self {
        UntilShutdown {
            inner,
            shutdown: signal.map(
                |signal| -> Pin<Box<dyn Future<Output = ()> + Send>> {
                    Box::pin(signal.wait())
                },
            ),
            ended: false,
        }
    }
}

impl<S: Stream + Unpin> Stream for UntilShutdown<S> {
    type Item = S::Item;

    fn poll_next(
        mut self: Pin<&mut Self>, cx: &mut Context<'_>,
    ) -> Poll<Option<S::Item>> {
        if self.ended {
            return Poll::Ready(None);
        }
        if let Some(shutdown) = self.shutdown.as_mut()
            && shutdown.as_mut().poll(cx).is_ready()
        {
            self.ended = true;
            return Poll::Ready(None);
        }
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

/// Root document or pages shell selected by the query or session default.
async fn index(
    State(app): State<App>, Query(query): Query<BTreeMap<String, String>>,
) -> Response {
    let html = {
        let state = read(&app.state);
        let view = match query.get("view").map(String::as_str) {
            Some("web") => ServeView::Web,
            Some("pages") => ServeView::Pages,
            _ => state.default_view,
        };
        match view {
            ServeView::Pages => shell_html(&state.title),
            ServeView::Web => state
                .web
                .good
                .as_ref()
                .map(|build| build.html.clone())
                .unwrap_or_else(|| fallback_page(&state)),
        }
    };
    ([(header::CACHE_CONTROL, NO_STORE)], Html(html)).into_response()
}

async fn theme_css(State(app): State<App>) -> Response {
    let css =
        read(&app.state).web.good.as_ref().map(|build| build.theme_css.clone());
    match css {
        Some(css) => whole_response(css.into_bytes(), "text/css", NO_STORE),
        None => not_found(SERVE_THEME_CSS),
    }
}

async fn helper_asset(
    State(app): State<App>, Path(path): Path<String>, request: HeaderMap,
) -> Response {
    let theme =
        read(&app.state).web.good.as_ref().map(|build| build.theme.clone());
    blocking(move || helper_asset_response(theme.as_ref(), &path, &request))
        .await
}

async fn doc_asset(
    State(app): State<App>, Path(id): Path<String>, request: HeaderMap,
) -> Response {
    let asset = read(&app.state)
        .web
        .good
        .as_ref()
        .and_then(|build| build.assets.get(&id))
        .cloned();
    let Some(asset) = asset else {
        return not_found(&format!("{SERVE_ASSET_PREFIX}{id}"));
    };
    match asset.data {
        AssetData::Bytes(bytes) => {
            bytes_response(bytes, &asset.mime, NO_STORE, &request)
        }
        AssetData::File(path) => {
            match file_response(path.clone(), &asset.mime, NO_STORE, &request)
                .await
            {
                Ok(response) => response,
                Err(err) if is_missing(&err) => not_found(
                    &Error::MissingAsset { path, span: None }.to_string(),
                ),
                Err(err) => server_error(&format!("{}: {err}", path.display())),
            }
        }
    }
}

fn is_missing(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
    )
}

async fn live_reload_js() -> Response {
    blocking(|| {
        let keys = ["vendor/morphdom/morphdom-umd.min.js", LIVE_RELOAD_KEY];
        let mut files = Vec::new();
        for key in keys {
            match crate::vendor::get(key) {
                Some(bytes) => files.push(bytes.into_owned()),
                None => return server_error(&format!("embedded {key} is missing")),
            }
        }
        let mut bytes = b"/* morphdom 2.7.8 (MIT), vendored; see THIRD-PARTY-LICENSES */\n(function (define, module, exports) {\n".to_vec();
        bytes.extend_from_slice(&files[0]);
        bytes.extend_from_slice(b"\n}).call(window);\n");
        bytes.extend_from_slice(&files[1]);
        whole_response(bytes, "text/javascript", NO_STORE)
    }).await
}

async fn pages_js() -> Response {
    blocking(|| match crate::vendor::get("writer/pages.js") {
        Some(bytes) => {
            whole_response(bytes.into_owned(), "text/javascript", NO_STORE)
        }
        None => server_error("embedded writer/pages.js is missing"),
    })
    .await
}

async fn pages_shell(State(app): State<App>) -> Response {
    let html = shell_html(&read(&app.state).title);
    ([(header::CACHE_CONTROL, NO_STORE)], Html(html)).into_response()
}

async fn page_svg(
    State(app): State<App>, Path(file): Path<String>,
    Query(query): Query<BTreeMap<String, String>>,
) -> Response {
    let Some(index) = page_index(&file) else {
        return not_found(&file);
    };
    let page = {
        let state = read(&app.state);
        let pages = &state.pages;
        let versioned = query.get("v").and_then(|hash| {
            pages.current.as_deref().and_then(|set| set.find(hash)).or_else(
                || pages.previous.as_deref().and_then(|set| set.find(hash)),
            )
        });
        versioned.map(|page| (page.svg.clone(), PAGE_CACHE)).or_else(|| {
            pages
                .current
                .as_ref()
                .and_then(|set| set.pages.get(index))
                .map(|page| (page.svg.clone(), NO_STORE))
        })
    };
    match page {
        Some((svg, cache)) => (
            [(header::CONTENT_TYPE, "image/svg+xml"),
             (header::CACHE_CONTROL, cache),
             (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
             (header::CONTENT_SECURITY_POLICY, "default-src 'none'; img-src data:; style-src 'unsafe-inline'")],
            svg,
        ).into_response(),
        None => not_found(&file),
    }
}

fn page_index(file: &str) -> Option<usize> {
    let number = file.strip_suffix(".svg")?;
    if !matches!(number.as_bytes().first(), Some(b'1'..=b'9'))
        || !number.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    number.parse::<usize>().ok()?.checked_sub(1)
}

async fn state_json(State(app): State<App>) -> Response {
    let hello = read(&app.state).hello();
    ([(header::CACHE_CONTROL, NO_STORE)], Json(hello)).into_response()
}

/// `GET /__writer/events`: [`event_stream`] with a keep-alive every 15 s.
async fn build_events(
    State(app): State<App>, shutdown: Option<Extension<ShutdownSignal>>,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    event_stream(&app, shutdown.map(|Extension(signal)| signal), KEEP_ALIVE)
}

/// `hello` with the current build, then one `build` event per [`publish`]ed [`BuildEvent`]
/// ([`build_stream`]), and a keep-alive comment after `keep_alive` without an event. The
/// stream ends when `shutdown` fires.
fn event_stream(
    app: &App, shutdown: Option<ShutdownSignal>, keep_alive: Duration,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>> + use<>> {
    // Subscribe before reading the state, so that no build stored after `hello` is missed.
    // A build stored in between is reported twice (in `hello` and as an event); the client
    // then refreshes the page it already shows, which changes nothing.
    let receiver = app.events.subscribe();
    let hello = read(&app.state).hello();
    let builds =
        build_stream(receiver).filter_map(|event| sse_event("build", &event));
    let stream =
        tokio_stream::iter(sse_event("hello", &hello)).chain(builds).map(Ok);
    let stream = UntilShutdown::new(stream, shutdown);
    Sse::new(stream).keep_alive(KeepAlive::new().interval(keep_alive))
}

/// The build events `receiver` gets. A receiver that fell more than [`EVENT_CAPACITY`]
/// events behind skips the oldest ones (logged) and goes on with the newest.
fn build_stream(
    receiver: broadcast::Receiver<BuildEvent>,
) -> impl Stream<Item = BuildEvent> + Send + Unpin + 'static {
    BroadcastStream::new(receiver).filter_map(|item| match item {
        Ok(event) => Some(event),
        Err(BroadcastStreamRecvError::Lagged(skipped)) => {
            tracing::debug!(
                "event stream lagged behind; {skipped} build event(s) skipped"
            );
            None
        }
    })
}

/// An SSE event named `name` with `data` as JSON; `None` (logged) if it cannot be encoded.
fn sse_event<T: Serialize>(name: &str, data: &T) -> Option<Event> {
    match Event::default().event(name).json_data(data) {
        Ok(event) => Some(event),
        Err(err) => {
            tracing::warn!("cannot encode the '{name}' event: {err}");
            None
        }
    }
}

/// The response for helper path `path` to a request with `request` headers: the file (or
/// the requested range of it) with its MIME type, 404 if there is none. Before any build
/// succeeded there is no theme, and only `vendor/…` and `writer/…` files exist.
fn helper_asset_response(
    theme: Option<&Theme>, path: &str, request: &HeaderMap,
) -> Response {
    let found = match theme {
        Some(theme) => resolve_helper_path(theme, path),
        None => Ok(builtin_file(path)),
    };
    match found {
        Ok(Some((bytes, mime))) => {
            let key = normalize_helper_path(path).unwrap_or_default();
            let vendor = key.starts_with("vendor/");
            let cache = if vendor { VENDOR_CACHE } else { NO_STORE };
            let bytes = match theme {
                Some(theme) if mime == "text/css" && !is_builtin_key(&key) => {
                    theme_stylesheet(theme, &key, bytes)
                }
                _ => bytes,
            };
            bytes_response(bytes, &mime, cache, request)
        }
        Ok(None) => not_found(&format!("{SERVE_HELPER_PREFIX}{path}")),
        Err(err) => server_error(&err.to_string()),
    }
}

/// True for a normalized helper path of the embedded tree (`vendor/…`, `writer/…`).
fn is_builtin_key(key: &str) -> bool {
    key.starts_with("vendor/") || key.starts_with("writer/")
}

/// The stylesheet at helper path `key` of `theme`'s chain (`bytes`) as served: its relative
/// `url()`s rewritten to versioned URLs ([`serve_stylesheet`], module docs); as it is when
/// it is not UTF-8 or cannot be rewritten (the browser then resolves it as written).
fn theme_stylesheet(theme: &Theme, key: &str, bytes: Vec<u8>) -> Vec<u8> {
    let Ok(css) = std::str::from_utf8(&bytes) else {
        return bytes;
    };
    match serve_stylesheet(theme, key, css) {
        Ok(css) => css.into_bytes(),
        Err(err) => {
            tracing::debug!("serving {key} as written: {err}");
            bytes
        }
    }
}

/// A `vendor/…` or `writer/…` helper path's bytes and MIME type.
fn builtin_file(path: &str) -> Option<(Vec<u8>, String)> {
    let key = normalize_helper_path(path)?;
    if !is_builtin_key(&key) {
        return None;
    }
    let bytes = crate::vendor::get(&key)?.into_owned();
    let mime = mime_guess::from_path(&key).first_or_octet_stream();
    Some((bytes, mime.essence_str().to_string()))
}

/// Runs `f` (file reads) on the blocking pool.
async fn blocking<F>(f: F) -> Response
where
    F: FnOnce() -> Response + Send + 'static,
{
    tokio::task::spawn_blocking(f).await.unwrap_or_else(|err| {
        server_error(&format!("request handler failed: {err}"))
    })
}

/// `404 Not Found` naming `what`.
fn not_found(what: &str) -> Response {
    (StatusCode::NOT_FOUND, format!("not found: {what}\n")).into_response()
}

/// `500 Internal Server Error` with `message` (also logged).
fn server_error(message: &str) -> Response {
    tracing::warn!("dev server: {message}");
    (StatusCode::INTERNAL_SERVER_ERROR, format!("{message}\n")).into_response()
}

/// The fallback uses the same grouped, escaped error text as the live client.
fn fallback_page(state: &ServeState) -> String {
    let mut groups: Vec<(Vec<&str>, &ErrorPayload)> = Vec::new();
    for (view, error) in
        [("web", &state.web.error), ("pages", &state.pages.error)]
    {
        if let Some(error) = error {
            match groups.iter_mut().find(|(_, previous)| *previous == error) {
                Some((views, _)) => views.push(view),
                None => groups.push((vec![view], error)),
            }
        }
    }
    let mut bodies = Vec::new();
    for (views, error) in groups {
        let heading = if error.kind.is_empty() {
            format!("[{}] build error", views.join(", "))
        } else {
            format!("[{}] build error ({})", views.join(", "), error.kind)
        };
        let mut text = format!(
            "{heading}\n{}",
            if error.message.is_empty() {
                "unknown error"
            } else {
                &error.message
            }
        );
        if let Some(file) =
            error.file.as_deref().filter(|file| !file.is_empty())
        {
            text.push('\n');
            text.push_str(file);
            if let Some(cell) = error.cell {
                text.push_str(&format!(" cell {cell}"));
            }
            if let Some(line) = error.line {
                text.push_str(&format!(":{line}"));
            }
        }
        let mut html = html_escape::encode_text(&text).into_owned();
        if let Some(excerpt) =
            error.excerpt.as_deref().filter(|s| !s.is_empty())
        {
            html.push_str(&format!(
                "<pre style=\"margin:.5em 0;color:#fc6\">{}</pre>",
                html_escape::encode_text(excerpt)
            ));
        }
        for hint in &error.hints {
            html.push_str(&format!(
                "\nhint: {}",
                html_escape::encode_text(hint)
            ));
        }
        bodies.push(html);
    }
    let overlay = bodies.join("\n\n");
    format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<title>writer: build error</title>\n<style>body{{margin:0;padding:2em 1em;font:16px/1.5 system-ui,sans-serif;background:#fafafa;color:#222}}</style></head>\n<body>\n\
<p>The document could not be built. Fix the error and save: this page reloads by itself.</p>\n\
<div id=\"writer-overlay\" style=\"position:fixed;top:1em;right:1em;box-sizing:border-box;max-width:50%;max-height:80vh;overflow:auto;z-index:10000;background:#222;color:#eee;border-left:4px solid #e55;padding:1em;font:13px monospace;white-space:pre-wrap;cursor:pointer\" onclick=\"this.style.display='none'\">{overlay}\n(click to dismiss)</div>\n\
<script src=\"{SERVE_LIVE_RELOAD_JS}\"></script>\n</body>\n</html>\n"
    )
}

/// Read access to the state. A poisoned lock is used as is: writers only assign fields.
fn read(state: &Shared) -> RwLockReadGuard<'_, ServeState> {
    state.read().unwrap_or_else(PoisonError::into_inner)
}

/// Write access to the state (see [`read`]).
fn write(state: &Shared) -> RwLockWriteGuard<'_, ServeState> {
    state.write().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serve::pages::{PageSvg, page_hash};

    fn shared() -> Shared {
        Arc::new(RwLock::new(ServeState::new(
            ServeView::Web,
            "input.md".into(),
        )))
    }

    fn payload(message: &str) -> ErrorPayload {
        ErrorPayload::new(&Error::Config(message.into()), None, None)
    }

    fn web(id: u64) -> WebBuild {
        WebBuild {
            html: format!("<p>{id}</p>"),
            theme_css: format!("/* {id} */"),
            assets: AssetRegistry::new(),
            theme: crate::theme::resolve("default", &[]).unwrap(),
        }
    }

    fn pages(id: u64) -> Arc<PageSet> {
        let svg = format!("<svg>{id}</svg>").into_bytes();
        Arc::new(PageSet {
            build_id: id,
            pages: vec![PageSvg {
                hash: page_hash(&svg),
                svg: svg.into(),
                size_pt: [595.28, 841.89],
                key: u128::from(id),
            }],
        })
    }

    fn outcome(
        web: ViewOutcome<WebBuild>, pages: ViewOutcome<Arc<PageSet>>,
    ) -> Outcome {
        Outcome {
            kind: ChangeKind::Content,
            duration_ms: 3,
            changed: None,
            title: None,
            web,
            pages,
            prepare_warnings: None,
            web_warnings: None,
            pages_warnings: None,
        }
    }

    fn ok_event(id: u64) -> BuildEvent {
        let state = shared();
        let mut event = apply_outcome(
            &state,
            outcome(ViewOutcome::Built(web(id)), ViewOutcome::Built(pages(id))),
        );
        event.build_id = id;
        event
    }

    #[test]
    fn all_view_outcomes_preserve_good_values_and_report_attempt_status() {
        for web_result in 0..3 {
            for pages_result in 0..3 {
                let state = shared();
                let initial = read(&state).hello();
                assert_eq!(initial.build_id, 0);
                assert_eq!(initial.status, Status::Ok);
                assert_eq!(initial.pages.count, 0);
                apply_outcome(
                    &state,
                    outcome(
                        ViewOutcome::Built(web(1)),
                        ViewOutcome::Built(pages(1)),
                    ),
                );
                apply_outcome(
                    &state,
                    outcome(
                        ViewOutcome::Failed(payload("old web")),
                        ViewOutcome::Failed(payload("old pages")),
                    ),
                );
                let web_outcome = match web_result {
                    0 => ViewOutcome::Built(web(3)),
                    1 => ViewOutcome::Failed(payload("new web")),
                    _ => ViewOutcome::Skipped,
                };
                let pages_outcome = match pages_result {
                    0 => ViewOutcome::Built(pages(3)),
                    1 => ViewOutcome::Failed(payload("new pages")),
                    _ => ViewOutcome::Skipped,
                };
                assert_eq!(next_build_id(&state), 3);
                let event =
                    apply_outcome(&state, outcome(web_outcome, pages_outcome));
                let statuses =
                    [ViewStatus::Ok, ViewStatus::Error, ViewStatus::Skipped];
                assert_eq!(event.build_id, 3);
                assert_eq!(event.web.status, statuses[web_result]);
                assert_eq!(event.pages.status, statuses[pages_result]);
                let expected = if web_result == 0 && pages_result == 0 {
                    Status::Ok
                } else {
                    Status::Error
                };
                assert_eq!(event.status, expected);
                let s = read(&state);
                assert_eq!(s.hello().status, expected);
                assert_ne!(s.hello().web.status, ViewStatus::Skipped);
                assert_ne!(s.hello().pages.status, ViewStatus::Skipped);
                assert_eq!(
                    s.web.good.as_ref().unwrap().html,
                    if web_result == 0 {
                        "<p>3</p>"
                    } else {
                        "<p>1</p>"
                    }
                );
                assert_eq!(
                    s.pages.current.as_ref().unwrap().build_id,
                    if pages_result == 0 { 3 } else { 1 }
                );
                assert_eq!(
                    s.pages.previous.as_ref().map(|set| set.build_id),
                    if pages_result == 0 { Some(1) } else { None }
                );
                assert_eq!(
                    event.pages.hashes,
                    s.pages.current.as_ref().unwrap().hashes()
                );
                assert_eq!(event.pages.size_pt, vec![[595.28, 841.89]]);
                let expected_web = match web_result {
                    0 => None,
                    1 => Some(payload("new web")),
                    _ => Some(payload("old web")),
                };
                let expected_pages = match pages_result {
                    0 => None,
                    1 => Some(payload("new pages")),
                    _ => Some(payload("old pages")),
                };
                assert_eq!(event.web.error, expected_web);
                assert_eq!(event.pages.error, expected_pages);
            }
        }
    }

    fn warning(code: &str) -> DiagnosticJson {
        DiagnosticJson {
            level: "warning",
            code: code.into(),
            message: "message".into(),
            file: Some("input.md".into()),
            line: Some(1),
            cell: None,
        }
    }

    #[test]
    fn warnings_replace_only_stages_that_ran_and_deduplicate_all_fields() {
        let state = shared();
        let a = warning("a");
        let b = warning("b");
        let mut c = a.clone();
        c.cell = Some(0);
        let mut first = outcome(ViewOutcome::Skipped, ViewOutcome::Skipped);
        first.title = Some("Prepared title".into());
        first.prepare_warnings = Some(vec![a.clone(), b.clone()]);
        first.web_warnings = Some(vec![b.clone(), c.clone()]);
        first.pages_warnings = Some(vec![a.clone()]);
        let event = apply_outcome(&state, first);
        assert_eq!(event.warnings, vec![a.clone(), b, c.clone()]);
        assert_eq!(read(&state).title, "Prepared title");
        let mut next = outcome(
            ViewOutcome::Failed(payload("failed")),
            ViewOutcome::Skipped,
        );
        next.prepare_warnings = Some(Vec::new());
        next.web_warnings = Some(vec![c.clone()]);
        let event = apply_outcome(&state, next);
        assert_eq!(event.warnings, vec![c, a]);
        assert_eq!(read(&state).title, "Prepared title");
    }

    #[test]
    fn fallback_groups_and_escapes_errors_like_the_client() {
        let state = shared();
        let mut error = payload("<bad> & error");
        error.file = Some("a<b.md".into());
        error.cell = Some(0);
        error.line = Some(2);
        error.excerpt = Some("<foo>".into());
        error.hints = vec!["try <bar>".into()];
        apply_outcome(
            &state,
            outcome(
                ViewOutcome::Failed(error.clone()),
                ViewOutcome::Failed(error),
            ),
        );
        let page = fallback_page(&read(&state));
        assert!(page.contains("<title>writer: build error</title>"));
        assert!(page.contains("[web, pages] build error (config)"));
        assert!(page.contains("a&lt;b.md cell 0:2"));
        assert!(page.contains("&lt;foo&gt;</pre>"));
        assert!(page.contains("hint: try &lt;bar&gt;"));
        assert!(page.contains("/__writer/live-reload.js"));
        assert!(!page.contains("data-writer-document"));
        assert!(!page.contains("<bad>"));
    }

    #[test]
    fn page_numbers_require_the_exact_positive_decimal_grammar() {
        assert_eq!(page_index("1.svg"), Some(0));
        assert_eq!(page_index("42.svg"), Some(41));
        for file in [
            "0.svg",
            "01.svg",
            "+1.svg",
            "-1.svg",
            "1.SVG",
            "1.svg.bak",
            "1.0.svg",
            "١.svg",
            "999999999999999999999999999999.svg",
        ] {
            assert_eq!(page_index(file), None, "{file}");
        }
    }

    #[test]
    fn publishing_is_harmless_without_receivers() {
        let events = event_channel();
        let event = ok_event(1);
        assert_eq!(publish(&events, event.clone()), 0);
        let mut receiver = events.subscribe();
        assert_eq!(publish(&events, event.clone()), 1);
        assert_eq!(receiver.try_recv().unwrap(), event);
    }

    #[tokio::test]
    async fn lagged_browsers_skip_old_events_and_keep_receiving() {
        let events = event_channel();
        let mut stream = build_stream(events.subscribe());
        let total = EVENT_CAPACITY as u64 + 6;
        let template = ok_event(1);
        for id in 1..=total {
            let mut event = template.clone();
            event.build_id = id;
            publish(&events, event);
        }
        for id in 7..=total {
            let event =
                tokio::time::timeout(Duration::from_secs(1), stream.next())
                    .await;
            assert_eq!(event.unwrap().map(|event| event.build_id), Some(id));
        }
        publish(&events, ok_event(total + 1));
        let event =
            tokio::time::timeout(Duration::from_secs(1), stream.next()).await;
        assert_eq!(event.unwrap().unwrap().build_id, total + 1);
    }

    #[tokio::test]
    async fn streams_end_on_shutdown_and_stay_ended() {
        let (tx, rx) = watch::channel(false);
        let mut stream = UntilShutdown::new(
            tokio_stream::pending::<u8>(),
            Some(ShutdownSignal(rx)),
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(30), stream.next())
                .await
                .is_err()
        );
        tx.send(true).unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), stream.next())
                .await
                .unwrap(),
            None
        );
        assert_eq!(stream.next().await, None);
    }

    #[tokio::test]
    async fn event_stream_sends_compact_hello_keep_alive_and_build() {
        let app = App { state: shared(), events: event_channel() };
        let sse =
            event_stream(&app, None, Duration::from_millis(30)).into_response();
        let mut body = sse.into_body().into_data_stream();
        let mut next = async || {
            let chunk =
                tokio::time::timeout(Duration::from_secs(2), body.next()).await;
            String::from_utf8(chunk.unwrap().unwrap().unwrap().to_vec())
                .unwrap()
        };
        let hello = next().await;
        assert_eq!(
            hello,
            format!(
                "event: hello\ndata: {}\n\n",
                serde_json::to_string(&read(&app.state).hello()).unwrap()
            )
        );
        assert_eq!(next().await, ":\n\n");
        let event = ok_event(2);
        publish(&app.events, event.clone());
        let expected = format!(
            "event: build\ndata: {}\n\n",
            serde_json::to_string(&event).unwrap()
        );
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            let chunk =
                tokio::time::timeout_at(deadline, next()).await.unwrap();
            if chunk == ":\n\n" {
                continue;
            }
            assert_eq!(chunk, expected);
            break;
        }
    }
    #[test]
    fn hosts_are_this_server_by_name_or_address_and_port() {
        let hosts = AllowedHosts::new(SocketAddr::from(([127, 0, 0, 1], 4321)));
        for host in [
            "localhost:4321", "LocalHost:4321", "127.0.0.1:4321", "[::1]:4321",
            "[::ffff:127.0.0.1]:4321",
        ] {
            assert!(hosts.allows(Some(host)), "{host}");
        }
        // No Host header at all (HTTP/1.0): browsers always send one.
        assert!(hosts.allows(None));
        for host in [
            // Addresses this server does not listen on (audit review: the contract's list).
            "192.168.1.5:4321", "[fe80::1]:4321", "127.0.0.2:4321",
            "0.0.0.0:4321", "evil.example:4321", "evil.example",
            "localhost.evil.example:4321", "127.0.0.1.nip.io:4321",
            "localhost:80", "localhost:3000", "localhost", "127.0.0.1",
            "localhost:", "localhost:x", ":4321", "[::1", "[evil]:4321",
            "[::1]4321", "",
        ] {
            assert!(!hosts.allows(Some(host)), "{host}");
        }
        // A configured name; port 80 may be left out.
        let named = AllowedHosts::new(SocketAddr::from(([192, 168, 1, 5], 80)))
            .with_name("MyMac.local");
        for host in [
            "mymac.local", "MYMAC.LOCAL:80", "localhost", "192.168.1.5",
            "[::1]",
        ] {
            assert!(named.allows(Some(host)), "{host}");
        }
        for host in ["other.local", "mymac.local:8080", "10.0.0.2", "[fe80::1]"]
        {
            assert!(!named.allows(Some(host)), "{host}");
        }
        // Every address (`--host 0.0.0.0`): any IP address is one of the machine's; names
        // stay limited.
        let every = AllowedHosts::new(SocketAddr::from(([0, 0, 0, 0], 3000)))
            .with_name("0.0.0.0");
        for host in [
            "10.0.0.2:3000", "[fe80::1]:3000", "localhost:3000",
            "127.0.0.1:3000",
        ] {
            assert!(every.allows(Some(host)), "{host}");
        }
        for host in ["evil.example:3000", "10.0.0.2:3001"] {
            assert!(!every.allows(Some(host)), "{host}");
        }
        // An IPv6 `--host`, written in brackets or not.
        let v6 = AllowedHosts::new(SocketAddr::from(([127, 0, 0, 1], 3000)))
            .with_name("[2001:db8::1]")
            .with_name("2001:db8::2");
        for host in ["[2001:db8::1]:3000", "[2001:DB8::2]:3000"] {
            assert!(v6.allows(Some(host)), "{host}");
        }
        assert!(!v6.allows(Some("[2001:db8::3]:3000")));
    }
}
