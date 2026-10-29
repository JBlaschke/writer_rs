//! T21 dev-server integration against real HTTP and SSE on a free local port.
//! Adapted from presenter's server regressions to Writer's two-view protocol.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use serde_json::{Value, json};
use tokio::sync::{broadcast, oneshot};
use writer::assets::registry::AssetRegistry;
use writer::config::ServeView;
use writer::error::Error;
use writer::pipeline::{self, BuildOptions, Mode, WorldSlot};
use writer::serve::events::{
    BuildEvent, Changed, DiagnosticJson, ErrorPayload,
};
use writer::serve::pages::{PageSet, PageSvg, page_hash, shell_html};
use writer::serve::server::{
    self, AllowedHosts, Outcome, ServeState, Shared, ViewOutcome, WebBuild,
};
use writer::serve::watch::ChangeKind;

const WAIT: Duration = Duration::from_secs(10);
const NO_STORE: &str = "no-store";
const IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// A dedicated runtime, bounded requests, and shutdown cleanup even on assertion failure.
struct TestServer {
    addr: SocketAddr,
    state: Shared,
    events: broadcast::Sender<BuildEvent>,
    shutdown: Option<oneshot::Sender<()>>,
    done: mpsc::Receiver<writer::Result<()>>,
    thread: Option<JoinHandle<()>>,
    stopped: bool,
}

impl TestServer {
    fn start(state: Shared) -> Self {
        Self::start_with(state, None, false)
    }

    fn start_with(
        state: Shared, name: Option<&str>, extra_route: bool,
    ) -> Self {
        let events = server::event_channel();
        let app = server::router(state.clone(), events.clone());
        let app = if extra_route {
            app.merge(
                axum::Router::new()
                    .route("/extra", axum::routing::get(|| async { "extra" })),
            )
        } else {
            app
        };
        let name = name.map(str::to_owned);
        let (shutdown, shutdown_rx) = oneshot::channel();
        let (addr_tx, addr_rx) = mpsc::channel();
        let (done_tx, done) = mpsc::channel();
        let thread = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap();
            let result = runtime.block_on(async move {
                let listener =
                    tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let addr = listener.local_addr().unwrap();
                addr_tx.send(addr).unwrap();
                let shutdown = async move {
                    let _ = shutdown_rx.await;
                };
                match name {
                    Some(name) => {
                        let hosts = AllowedHosts::new(addr).with_name(&name);
                        server::serve_until_allowing(
                            listener, app, shutdown, hosts,
                        )
                        .await
                    }
                    None => server::serve_until(listener, app, shutdown).await,
                }
            });
            let _ = done_tx.send(result);
            runtime.shutdown_timeout(Duration::from_secs(1));
        });
        let addr = addr_rx.recv_timeout(WAIT).unwrap();
        Self {
            addr,
            state,
            events,
            shutdown: Some(shutdown),
            done,
            thread: Some(thread),
            stopped: false,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    fn get(&self, path: &str) -> Response {
        self.get_with(path, &[])
    }

    fn get_with(&self, path: &str, headers: &[(&str, &str)]) -> Response {
        let mut request = agent(WAIT).get(&self.url(path));
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let mut response = request
            .call()
            .unwrap_or_else(|error| panic!("GET {path}: {error}"));
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
        assert_eq!(response.status, 200, "GET {path}");
        String::from_utf8(response.body).unwrap()
    }

    fn json(&self) -> Value {
        serde_json::from_str(&self.text("/__writer/state")).unwrap()
    }

    fn wait_stopped(&mut self, timeout: Duration) {
        let result = self.done.recv_timeout(timeout).expect("server shutdown");
        self.stopped = true;
        assert!(result.is_ok(), "{result:?}");
        self.thread.take().unwrap().join().unwrap();
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        drop(self.shutdown.take());
        if !self.stopped {
            let result = self.done.recv_timeout(Duration::from_secs(5));
            if result.is_ok()
                && let Some(thread) = self.thread.take()
            {
                let _ = thread.join();
            }
            if !thread::panicking() {
                assert!(matches!(result, Ok(Ok(()))), "shutdown: {result:?}");
            }
        }
    }
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

    fn assert_headers(&self, content_type: &str, cache: &str) {
        assert_eq!(self.header("content-type"), Some(content_type));
        assert_eq!(self.header("cache-control"), Some(cache));
    }
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .proxy(None)
        .timeout_global(Some(timeout))
        .build()
        .into()
}

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(path)
}

fn empty_state(view: ServeView, title: &str) -> Shared {
    Arc::new(RwLock::new(ServeState::new(view, title.into())))
}

fn outcome(
    web: ViewOutcome<WebBuild>, pages: ViewOutcome<Arc<PageSet>>,
) -> Outcome {
    Outcome {
        kind: ChangeKind::Content,
        duration_ms: 12,
        changed: None,
        title: None,
        web,
        pages,
        prepare_warnings: Some(Vec::new()),
        web_warnings: Some(Vec::new()),
        pages_warnings: Some(Vec::new()),
    }
}

fn web_build(input: &Path, build_id: u64) -> WebBuild {
    let mut options = BuildOptions::new(input);
    options.mode = Mode::Serve { build_id };
    let mut prepared = pipeline::prepare(&options).unwrap();
    prepared.config.pdf.system_fonts = false;
    let mut worlds = WorldSlot::new();
    let world = worlds.get(Some(&prepared), &options).unwrap();
    let page = pipeline::web_page(&prepared, &options, world).unwrap();
    WebBuild {
        html: page.html,
        theme_css: page.theme_css,
        assets: page.assets,
        theme: page.theme,
    }
}

fn state_with(input: &Path) -> (Shared, WebBuild) {
    let state = empty_state(ServeView::Web, "Document");
    let build = web_build(input, server::next_build_id(&state));
    server::apply_outcome(
        &state,
        outcome(ViewOutcome::Built(build.clone()), ViewOutcome::Skipped),
    );
    (state, build)
}

fn pages(build_id: u64, labels: &[&str]) -> Arc<PageSet> {
    Arc::new(PageSet {
        build_id,
        pages: labels
            .iter()
            .enumerate()
            .map(|(index, label)| {
                let svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\"><text>{label}</text></svg>");
                PageSvg {
                    hash: page_hash(svg.as_bytes()), svg: Bytes::from(svg),
                    size_pt: [595.28, 841.89], key: index as u128,
                }
            })
            .collect(),
    })
}

fn error(message: &str) -> ErrorPayload {
    ErrorPayload::new(&Error::Other(message.into()), None, None)
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn writer_urls(html: &str) -> Vec<String> {
    let mut urls: Vec<_> = ["src=\"", "href=\""]
        .iter()
        .flat_map(|attribute| html.split(attribute).skip(1))
        .filter_map(|rest| rest.split('"').next())
        .filter(|url| url.starts_with("/__writer/"))
        .map(str::to_string)
        .collect();
    urls.sort();
    urls.dedup();
    urls
}

fn css_urls(css: &str) -> Vec<String> {
    css.split("url(")
        .skip(1)
        .filter_map(|rest| rest.split(')').next())
        .map(|url| url.trim_matches(['"', '\'']))
        .filter(|url| !url.starts_with("data:") && !url.contains("://"))
        .map(|url| url.split(['?', '#']).next().unwrap().to_string())
        .collect()
}

#[test]
fn serves_the_document_stylesheet_assets_and_current_state() {
    let (state, build) = state_with(&fixture("md/minimal.md"));
    let server = TestServer::start(state);
    let index = server.get("/");
    assert_eq!(index.status, 200);
    index.assert_headers("text/html; charset=utf-8", NO_STORE);
    assert_eq!(index.body, build.html.as_bytes());
    assert!(build.html.contains("data-writer-document"));
    assert!(build.html.contains("/__writer/theme.css?v=1"));
    assert!(build.html.contains("/__writer/live-reload.js"));

    let css = server.get("/__writer/theme.css?v=999");
    css.assert_headers("text/css; charset=utf-8", NO_STORE);
    assert_eq!(css.body, build.theme_css.as_bytes());
    let state = server.get("/__writer/state");
    state.assert_headers("application/json", NO_STORE);
    let json: Value = serde_json::from_slice(&state.body).unwrap();
    assert_eq!(
        json,
        json!({
            "build_id": 1, "status": "ok", "web": {"status": "ok"},
            "pages": {"status": "ok", "count": 0, "hashes": [], "size_pt": []},
            "warnings": [],
        })
    );
    assert_eq!(server.get("/favicon.ico").status, 204);
    assert_eq!(server.get("/missing").status, 404);
}

#[test]
fn every_generated_document_and_stylesheet_url_resolves() {
    let (state, build) = state_with(&fixture("md/math.md"));
    let server = TestServer::start(state);
    let urls = writer_urls(&build.html);
    assert!(urls.iter().any(|url| url.contains("/vendor/katex/")));
    let style_urls = css_urls(&build.theme_css);
    for url in
        urls.iter().chain(style_urls.iter().filter(|url| url.starts_with('/')))
    {
        let response = server.get(url);
        assert_eq!(response.status, 200, "GET {url}");
        assert!(!response.body.is_empty(), "GET {url}");
    }
    let katex =
        urls.iter().find(|url| url.ends_with("/katex.min.css")).unwrap();
    let base = katex.rsplit_once('/').unwrap().0;
    let fonts: Vec<_> = css_urls(&server.text(katex))
        .into_iter()
        .filter(|url| url.ends_with(".woff2"))
        .collect();
    assert!(!fonts.is_empty());
    for font in fonts {
        let url = format!("{base}/{font}");
        let response = server.get(&url);
        assert_eq!(response.status, 200, "GET {url}");
        assert_eq!(response.header("content-type"), Some("font/woff2"));
        assert_eq!(response.header("cache-control"), Some("max-age=3600"));
    }
}

#[test]
fn serves_the_exact_morphdom_wrapper_and_pages_client() {
    let server = TestServer::start(empty_state(ServeView::Web, "Document"));
    let client = server.get("/__writer/live-reload.js");
    assert_eq!(client.status, 200);
    client.assert_headers("text/javascript; charset=utf-8", NO_STORE);
    let mut expected = b"/* morphdom 2.7.8 (MIT), vendored; see THIRD-PARTY-LICENSES */\n(function (define, module, exports) {\n".to_vec();
    expected.extend_from_slice(include_bytes!(
        "../assets/vendor/morphdom/morphdom-umd.min.js"
    ));
    expected.extend_from_slice(b"\n}).call(window);\n");
    expected
        .extend_from_slice(include_bytes!("../assets/writer/live-reload.js"));
    assert_eq!(client.body, expected);
    let pages = server.get("/__writer/pages.js");
    assert_eq!(pages.status, 200);
    pages.assert_headers("text/javascript; charset=utf-8", NO_STORE);
    assert_eq!(pages.body, include_bytes!("../assets/writer/pages.js"));
}

#[test]
fn theme_helpers_resolve_all_path_forms_and_version_relative_urls() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    write(
        &root.join("mytheme/theme.toml"),
        "name = 'mytheme'\nextends = 'default'\n[styles]\nentries = ['styles/extra.css']\n",
    );
    write(
        &root.join("mytheme/styles/extra.css"),
        ".a { background: url(logo.png); }\n.b { background: url(img/b.png); }\n",
    );
    write(
        &root.join("mytheme/styles/linked.css"),
        ".b { background: url(img/b.png); }\n",
    );
    write(&root.join("mytheme/styles/img/b.png"), "b-bytes");
    write(&root.join("mytheme/assets/logo.png"), "logo-bytes");
    write(&root.join("mytheme/assets/a b.png"), "space-bytes");
    write(&root.join("secret.txt"), "secret");
    let input = root.join("document.md");
    write(&input, "---\ntheme: ./mytheme\n---\n\n# Themed\n");
    let (state, _) = state_with(&input);
    let server = TestServer::start(state);
    let versioned = |path: &str, bytes: &[u8]| {
        let hash = blake3::hash(bytes);
        let version =
            u64::from_le_bytes(hash.as_bytes()[..8].try_into().unwrap());
        format!("/__writer/asset/{path}?v={version}")
    };
    let logo = versioned("@mytheme/assets/logo.png", b"logo-bytes");
    let image = versioned("@mytheme/styles/img/b.png", b"b-bytes");
    let css = server.text("/__writer/theme.css");
    assert!(css.contains(&format!("url({logo})")), "{css}");
    assert!(css.contains(&format!("url({image})")), "{css}");
    for (url, bytes) in [
        (logo.as_str(), b"logo-bytes".as_slice()),
        (image.as_str(), b"b-bytes".as_slice()),
        ("/__writer/asset/logo.png", b"logo-bytes".as_slice()),
        ("/__writer/asset/a%20b.png", b"space-bytes".as_slice()),
    ] {
        let response = server.get(url);
        assert_eq!(response.status, 200, "GET {url}");
        assert_eq!(response.body, bytes, "GET {url}");
        response.assert_headers("image/png", NO_STORE);
    }
    // Linked helpers resolve next to themselves, unlike compiled theme entries
    // which also search assets/. The first file has one unresolved neighbour,
    // so the route preserves its original bytes; the second can be rewritten.
    let extra = server.text("/__writer/asset/@mytheme/styles/extra.css");
    assert!(extra.contains("url(logo.png)"), "{extra}");
    let helper = server.text("/__writer/asset/@mytheme/styles/linked.css");
    assert!(helper.contains(&format!("url({image})")), "{helper}");
    assert_eq!(server.get("/__writer/asset/@default/theme.toml").status, 200);
    assert_eq!(server.get("/__writer/asset/writer/live-reload.js").status, 200);
    for url in [
        "/__writer/asset/%2E%2E/secret.txt",
        "/__writer/asset/@mytheme/%2E%2E/secret.txt",
        "/__writer/asset/@mytheme/..%2F..%2Fsecret.txt",
        "/__writer/asset/@nope/theme.toml",
        "/__writer/asset/%2Fetc%2Fpasswd",
        "/__writer/asset/vendor/%2E%2E/%2E%2E/Cargo.toml",
        "/__writer/asset/mytheme/theme.toml",
        "/__writer/asset/vendor/does-not-exist.js",
    ] {
        assert_eq!(server.get(url).status, 404, "GET {url}");
    }
}

#[test]
fn document_assets_use_latest_good_registry_and_ignore_query_versions() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("document.md");
    let image = directory.path().join("plot.png");
    let bytes = include_bytes!("fixtures/md/images/plot.png");
    std::fs::write(&image, bytes).unwrap();
    write(&input, "# Figure\n\n![A plot](plot.png)\n");
    let (state, build) = state_with(&input);
    let server = TestServer::start(state);
    let urls: Vec<_> = writer_urls(&build.html)
        .into_iter()
        .filter(|url| url.starts_with("/__writer/doc-asset/"))
        .collect();
    assert_eq!(urls.len(), 1);
    let response = server.get(&urls[0]);
    response.assert_headers("image/png", NO_STORE);
    assert_eq!(response.body, bytes);
    let path = urls[0].split('?').next().unwrap();
    assert_eq!(server.get(&format!("{path}?v=wrong")).body, bytes);
    server::apply_outcome(
        &server.state,
        outcome(ViewOutcome::Failed(error("broken")), ViewOutcome::Skipped),
    );
    assert_eq!(server.get(path).body, bytes);
    assert_eq!(server.text("/"), build.html);
    assert_eq!(server.json()["status"], "error");
    assert_eq!(server.get("/__writer/doc-asset/ffffffffffff").status, 404);
    std::fs::remove_file(&image).unwrap();
    assert_eq!(server.get(path).status, 404);
    assert_eq!(server.get_with(path, &[("Range", "bytes=0-1")]).status, 404);
}

fn asset_server(registry: AssetRegistry) -> TestServer {
    let (state, mut build) = state_with(&fixture("md/minimal.md"));
    build.assets = registry;
    server::apply_outcome(
        &state,
        outcome(ViewOutcome::Built(build), ViewOutcome::Skipped),
    );
    TestServer::start(state)
}

fn assert_ranges(server: &TestServer, url: &str, bytes: &[u8], cache: &str) {
    let length = bytes.len();
    let whole = server.get(url);
    assert_eq!(whole.status, 200);
    assert_eq!(whole.header("accept-ranges"), Some("bytes"));
    assert_eq!(whole.header("content-range"), None);
    assert_eq!(
        whole.header("content-length"),
        Some(length.to_string().as_str())
    );
    assert_eq!(whole.body, bytes);
    for (spec, start, end) in [
        ("bytes=0-1".to_string(), 0, 1),
        ("bytes=1-3".to_string(), 1, 3),
        (format!("bytes={}-", length - 4), length - 4, length - 1),
        ("bytes=-3".to_string(), length - 3, length - 1),
        ("bytes=0-".to_string(), 0, length - 1),
        (format!("bytes=0-{}", length + 100), 0, length - 1),
    ] {
        let part = server.get_with(url, &[("Range", &spec)]);
        assert_eq!(part.status, 206, "{spec}");
        assert_eq!(part.header("cache-control"), Some(cache));
        assert_eq!(part.header("accept-ranges"), Some("bytes"));
        assert_eq!(
            part.header("content-range"),
            Some(format!("bytes {start}-{end}/{length}").as_str()),
        );
        assert_eq!(part.body, bytes[start..=end], "{spec}");
        assert_eq!(
            part.header("content-length"),
            Some((end - start + 1).to_string().as_str()),
        );
    }
    for spec in [format!("bytes={length}-"), "bytes=-0".into()] {
        let part = server.get_with(url, &[("Range", &spec)]);
        assert_eq!(part.status, 416, "{spec}");
        assert_eq!(
            part.header("content-range"),
            Some(format!("bytes */{length}").as_str())
        );
    }
    for spec in
        ["bytes=0-1,4-5", "items=0-1", "bytes=abc", "bytes=--", "bytes=9-1"]
    {
        let part = server.get_with(url, &[("Range", spec)]);
        assert_eq!(part.status, 200, "{spec}");
        assert_eq!(part.body, bytes, "{spec}");
    }
    let conditional = server
        .get_with(url, &[("Range", "bytes=0-1"), ("If-Range", "\"old\"")]);
    assert_eq!(conditional.status, 200);
    assert_eq!(conditional.body, bytes);
}

#[test]
fn file_assets_stream_single_ranges_and_deleted_or_directory_assets_are_404() {
    let directory = tempfile::tempdir().unwrap();
    let bytes: Vec<_> = (0..300_000).map(|index| (index % 251) as u8).collect();
    let path = directory.path().join("clip.mp4");
    std::fs::write(&path, &bytes).unwrap();
    let mut registry = AssetRegistry::new();
    let id = registry.register_file(&path);
    let directory_id = registry.register_file(directory.path());
    let server = asset_server(registry);
    let url = format!("/__writer/doc-asset/{id}");
    assert_eq!(server.get(&url).header("content-type"), Some("video/mp4"));
    assert_ranges(&server, &url, &bytes, NO_STORE);
    let directory_url = format!("/__writer/doc-asset/{directory_id}");
    assert_eq!(server.get(&directory_url).status, 404);
    assert_eq!(
        server.get_with(&directory_url, &[("Range", "bytes=0-1")]).status,
        404
    );
    std::fs::remove_file(path).unwrap();
    assert_eq!(server.get(&url).status, 404);
}

#[test]
fn in_memory_and_vendored_helper_assets_support_ranges() {
    let bytes = b"in-memory document image bytes";
    let mut registry = AssetRegistry::new();
    let id = registry.register_bytes(bytes.to_vec(), "image/png", "image.png");
    let server = asset_server(registry);
    assert_ranges(
        &server,
        &format!("/__writer/doc-asset/{id}"),
        bytes,
        NO_STORE,
    );
    let url = "/__writer/asset/vendor/morphdom/morphdom-umd.min.js";
    let bytes = server.get(url).body;
    assert_ranges(&server, url, &bytes, "max-age=3600");
}

#[test]
fn empty_document_assets_have_full_and_unsatisfiable_range_responses() {
    let mut registry = AssetRegistry::new();
    let id = registry.register_bytes(
        Vec::new(),
        "application/octet-stream",
        "empty.bin",
    );
    let server = asset_server(registry);
    let url = format!("/__writer/doc-asset/{id}");
    let whole = server.get(&url);
    assert_eq!(whole.status, 200);
    assert!(whole.body.is_empty());
    let response = server.get_with(&url, &[("Range", "bytes=0-")]);
    assert_eq!(response.status, 416);
    assert_eq!(response.header("content-range"), Some("bytes */0"));
    let suffix = server.get_with(&url, &[("Range", "bytes=-1")]);
    assert_eq!(suffix.status, 200);
    assert!(suffix.body.is_empty());
}

#[test]
fn both_view_defaults_and_explicit_queries_select_the_correct_document() {
    for default in [ServeView::Web, ServeView::Pages] {
        let (state, _) = state_with(&fixture("md/minimal.md"));
        {
            let mut current = state.write().unwrap();
            current.default_view = default;
            current.title = "A <title> & \"quote\"".into();
        }
        let server = TestServer::start(state);
        let web = server.text("/?view=web");
        assert!(web.contains("data-writer-document"));
        assert!(!web.contains("data-writer-pages"));
        let shell = server.get("/__writer/pages");
        shell.assert_headers("text/html; charset=utf-8", NO_STORE);
        let shell = String::from_utf8(shell.body).unwrap();
        assert_eq!(shell, shell_html("A <title> & \"quote\""));
        assert!(shell.contains("A &lt;title&gt; &amp; \"quote\" · pages"));
        assert!(shell.contains("data-writer-pages"));
        assert!(!shell.contains("data-writer-document"));
        assert!(
            shell.find("/__writer/pages.js").unwrap()
                < shell.find("/__writer/live-reload.js").unwrap()
        );
        assert_eq!(server.text("/?view=pages"), shell);
        let selected = if default == ServeView::Web {
            &web
        } else {
            &shell
        };
        assert_eq!(&server.text("/"), selected);
        assert_eq!(&server.text("/?view=unknown"), selected);
    }
}

#[test]
fn page_resources_use_current_or_previous_content_hash_and_security_headers() {
    let state = empty_state(ServeView::Pages, "Pages");
    let old = pages(1, &["old first", "old second"]);
    let current = pages(2, &["new first"]);
    server::apply_outcome(
        &state,
        outcome(ViewOutcome::Skipped, ViewOutcome::Built(old.clone())),
    );
    server::apply_outcome(
        &state,
        outcome(ViewOutcome::Skipped, ViewOutcome::Built(current.clone())),
    );
    let server = TestServer::start(state);
    let cases = [
        (
            "/__writer/pages/1.svg".to_string(),
            current.pages[0].svg.clone(),
            NO_STORE,
        ),
        (
            "/__writer/pages/1.svg?v=unknown".to_string(),
            current.pages[0].svg.clone(),
            NO_STORE,
        ),
        (
            format!("/__writer/pages/1.svg?v={}", old.pages[1].hash),
            old.pages[1].svg.clone(),
            IMMUTABLE,
        ),
        (
            format!("/__writer/pages/999.svg?v={}", current.pages[0].hash),
            current.pages[0].svg.clone(),
            IMMUTABLE,
        ),
    ];
    for (url, expected, cache) in cases {
        let response = server.get(&url);
        assert_eq!(response.status, 200, "GET {url}");
        response.assert_headers("image/svg+xml", cache);
        assert_eq!(response.header("x-content-type-options"), Some("nosniff"));
        assert_eq!(
            response.header("content-security-policy"),
            Some(
                "default-src 'none'; img-src data:; style-src 'unsafe-inline'"
            )
        );
        assert_eq!(response.body, expected, "GET {url}");
    }
    assert_eq!(server.get("/__writer/pages/2.svg").status, 404);
    assert_eq!(server.get("/__writer/pages/2.svg?v=unknown").status, 404);
    server::apply_outcome(
        &server.state,
        outcome(
            ViewOutcome::Skipped,
            ViewOutcome::Failed(error("compile failed")),
        ),
    );
    let retained = server.get("/__writer/pages/1.svg");
    assert_eq!(retained.body, current.pages[0].svg);
    assert_eq!(server.json()["pages"]["count"], 1);
    assert_eq!(server.json()["pages"]["status"], "error");
    let next = pages(4, &["next"]);
    server::apply_outcome(
        &server.state,
        outcome(ViewOutcome::Skipped, ViewOutcome::Built(next)),
    );
    // Old versions expire after the next successful set rotates the previous slot.
    let expired =
        server.get(&format!("/__writer/pages/1.svg?v={}", old.pages[0].hash));
    assert_eq!(expired.header("cache-control"), Some(NO_STORE));
    assert_ne!(expired.body, old.pages[0].svg);
}

#[test]
fn svg_page_names_follow_the_positive_decimal_grammar() {
    let state = empty_state(ServeView::Pages, "Pages");
    let set = pages(1, &["one"]);
    let hash = set.pages[0].hash.clone();
    server::apply_outcome(
        &state,
        outcome(ViewOutcome::Skipped, ViewOutcome::Built(set)),
    );
    let server = TestServer::start(state);
    for file in [
        "0.svg",
        "00.svg",
        "01.svg",
        "-1.svg",
        "+1.svg",
        "1.SVG",
        "1.svg.svg",
        "one.svg",
        "1",
        "1.0.svg",
        "1%20.svg",
        "999999999999999999999999999999999.svg",
    ] {
        let url = format!("/__writer/pages/{file}?v={hash}");
        assert_eq!(server.get(&url).status, 404, "GET {url}");
    }
    let empty = TestServer::start(empty_state(ServeView::Pages, "None"));
    assert_eq!(empty.get("/__writer/pages/1.svg").status, 404);
}

#[test]
fn fallback_escapes_current_errors_and_builtin_routes_work_before_good_web_build()
 {
    let state = empty_state(ServeView::Web, "Broken");
    let payload = ErrorPayload {
        message: "bad <value> & value".into(),
        file: Some("chapter.md".into()),
        line: Some(7),
        cell: Some(2),
        kind: "parse".into(),
        excerpt: Some("<bad>".into()),
        hints: vec!["try <this>".into()],
    };
    server::apply_outcome(
        &state,
        outcome(
            ViewOutcome::Failed(payload.clone()),
            ViewOutcome::Failed(payload),
        ),
    );
    let server = TestServer::start(state);
    let html = server.text("/");
    for expected in [
        "writer: build error",
        "The document could not be built. Fix the error and save: this page reloads by itself.",
        "id=\"writer-overlay\"",
        "[web, pages] build error (parse)",
        "bad &lt;value&gt; &amp; value",
        "&lt;bad&gt;",
        "try &lt;this&gt;",
        "/__writer/live-reload.js",
    ] {
        assert!(html.contains(expected), "{expected:?} absent: {html}");
    }
    assert!(!html.contains("data-writer-document"));
    assert_eq!(server.get("/__writer/theme.css").status, 404);
    assert_eq!(server.get("/__writer/asset/logo.png").status, 404);
    let builtin =
        server.get("/__writer/asset/vendor/morphdom/morphdom-umd.min.js");
    assert_eq!(builtin.status, 200);
    assert_eq!(builtin.header("cache-control"), Some("max-age=3600"));
    assert_eq!(server.get("/__writer/asset/writer/live-reload.js").status, 200);
}

struct Frame {
    event: String,
    data: Value,
    data_lines: usize,
    raw_data: String,
}

struct Events {
    reader: BufReader<ureq::BodyReader<'static>>,
}

impl Events {
    fn open(server: &TestServer) -> Self {
        let response =
            agent(WAIT).get(&server.url("/__writer/events")).call().unwrap();
        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/event-stream"
        );
        assert_eq!(
            response.headers().get("cache-control").unwrap(),
            "no-cache"
        );
        Self { reader: BufReader::new(response.into_body().into_reader()) }
    }

    fn next(&mut self) -> Option<Frame> {
        let (mut event, mut data, mut data_lines) = (None, String::new(), 0);
        loop {
            let mut line = String::new();
            if self.reader.read_line(&mut line).unwrap() == 0 {
                return None;
            }
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                if let Some(event) = event.take() {
                    let value = serde_json::from_str(&data).unwrap();
                    return Some(Frame {
                        event,
                        data: value,
                        data_lines,
                        raw_data: data,
                    });
                }
            } else if let Some(name) = line.strip_prefix("event:") {
                event = Some(name.trim().to_string());
            } else if let Some(chunk) = line.strip_prefix("data:") {
                data_lines += 1;
                data.push_str(chunk.trim_start());
            }
        }
    }
}

#[test]
fn sse_sends_compact_hello_and_builds_with_per_view_state() {
    let (state, build) = state_with(&fixture("md/minimal.md"));
    let server = TestServer::start(state);
    let mut events = Events::open(&server);
    let hello = events.next().unwrap();
    assert_eq!(hello.event, "hello");
    assert_eq!(hello.data, server.json());
    assert_eq!(hello.data_lines, 1);
    assert_eq!(
        hello.raw_data,
        serde_json::to_string(&server.state.read().unwrap().hello()).unwrap(),
    );
    let mut attempt =
        outcome(ViewOutcome::Built(build.clone()), ViewOutcome::Skipped);
    attempt.kind = ChangeKind::Css;
    attempt.duration_ms = 42;
    attempt.web_warnings = Some(vec![DiagnosticJson {
        level: "warning",
        code: "test-warning".into(),
        message: "two\nlines".into(),
        file: None,
        line: None,
        cell: None,
    }]);
    let event = server::apply_outcome(&server.state, attempt);
    assert_eq!(server::publish(&server.events, event.clone()), 1);
    let frame = events.next().unwrap();
    assert_eq!(frame.event, "build");
    assert_eq!(frame.data, serde_json::to_value(&event).unwrap());
    assert_eq!(frame.data_lines, 1);
    assert_eq!(frame.data["web"]["status"], "ok");
    assert_eq!(frame.data["pages"]["status"], "skipped");
    assert_eq!(server.json()["pages"]["status"], "ok");

    let mut failure =
        outcome(ViewOutcome::Skipped, ViewOutcome::Failed(error("bad pages")));
    failure.kind = ChangeKind::Pages;
    let event = server::apply_outcome(&server.state, failure);
    server::publish(&server.events, event);
    let failed = events.next().unwrap();
    assert_eq!(failed.data["status"], "error");
    assert_eq!(failed.data["web"]["status"], "skipped");
    assert_eq!(failed.data["pages"]["status"], "error");
    assert_eq!(server.text("/"), build.html);
    let mut late = Events::open(&server);
    let reconnect = late.next().unwrap();
    assert_eq!(reconnect.event, "hello");
    assert_eq!(reconnect.data, server.json());
    assert_eq!(reconnect.data["web"]["status"], "ok");
    assert!(reconnect.data.get("kind").is_none());
}

#[test]
fn separate_sse_subscribers_survive_a_dropped_browser_and_new_builds() {
    let (state, _) = state_with(&fixture("md/minimal.md"));
    let server = TestServer::start(state);
    let mut first = Events::open(&server);
    let mut second = Events::open(&server);
    let mut gone = Events::open(&server);
    assert_eq!(first.next().unwrap().event, "hello");
    assert_eq!(second.next().unwrap().event, "hello");
    assert_eq!(gone.next().unwrap().event, "hello");
    drop(gone);
    let mut attempt = outcome(ViewOutcome::Skipped, ViewOutcome::Skipped);
    attempt.changed = Some(Changed {
        file: "chapters/method.md".into(),
        source: 2,
        line: Some(41),
        cell: None,
    });
    let event = server::apply_outcome(&server.state, attempt);
    server::publish(&server.events, event.clone());
    for stream in [&mut first, &mut second] {
        assert_eq!(
            stream.next().unwrap().data,
            serde_json::to_value(&event).unwrap()
        );
    }
    assert_eq!(server.json()["build_id"], event.build_id);
}

#[test]
fn publishing_without_receivers_is_harmless_and_burst_stream_reaches_latest() {
    let state = empty_state(ServeView::Web, "Burst");
    let event = server::apply_outcome(
        &state,
        outcome(ViewOutcome::Skipped, ViewOutcome::Skipped),
    );
    assert_eq!(server::publish(&server::event_channel(), event), 0);
    let server = TestServer::start(state);
    let mut events = Events::open(&server);
    assert_eq!(events.next().unwrap().event, "hello");
    let mut latest = 1;
    for _ in 0..server::EVENT_CAPACITY * 4 {
        let event = server::apply_outcome(
            &server.state,
            outcome(ViewOutcome::Skipped, ViewOutcome::Skipped),
        );
        latest = event.build_id;
        server::publish(&server.events, event);
    }
    let mut previous = 1;
    for _ in 0..server::EVENT_CAPACITY * 4 {
        let frame = events.next().unwrap();
        let id = frame.data["build_id"].as_u64().unwrap();
        assert!(id > previous);
        previous = id;
        if id == latest {
            return;
        }
    }
    panic!("stream did not reach build {latest}; last was {previous}");
}

#[test]
fn dropping_shutdown_sender_closes_open_sse_and_releases_port_promptly() {
    let server_state = empty_state(ServeView::Web, "Shutdown");
    let mut server = TestServer::start(server_state);
    let mut events = Events::open(&server);
    assert_eq!(events.next().unwrap().event, "hello");
    let started = Instant::now();
    drop(server.shutdown.take());
    server.wait_stopped(Duration::from_secs(2));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(events.next().is_none());
    assert!(
        agent(Duration::from_secs(1)).get(&server.url("/")).call().is_err()
    );
}

#[test]
fn stalled_request_cannot_hold_shutdown_beyond_the_grace_period() {
    let mut server = TestServer::start(empty_state(ServeView::Web, "Shutdown"));
    let mut stalled = TcpStream::connect(server.addr).unwrap();
    stalled.set_write_timeout(Some(WAIT)).unwrap();
    stalled.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n").unwrap();
    let mut idle = TcpStream::connect(server.addr).unwrap();
    idle.set_read_timeout(Some(WAIT)).unwrap();
    idle.set_write_timeout(Some(WAIT)).unwrap();
    let request = format!(
        "GET /__writer/state HTTP/1.1\r\nHost: {}\r\n\r\n",
        server.addr
    );
    idle.write_all(request.as_bytes()).unwrap();
    assert!(idle.read(&mut [0u8; 256]).unwrap() > 0);
    thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    drop(server.shutdown.take());
    server.wait_stopped(server::SHUTDOWN_GRACE + Duration::from_secs(2));
    assert!(
        started.elapsed() < server::SHUTDOWN_GRACE + Duration::from_secs(1)
    );
}

fn raw_status(addr: SocketAddr, request: &[u8]) -> u16 {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream.set_read_timeout(Some(WAIT)).unwrap();
    stream.set_write_timeout(Some(WAIT)).unwrap();
    stream.write_all(request).unwrap();
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).unwrap();
    line.split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("bad HTTP status: {line:?}"))
}

fn host_status(server: &TestServer, path: &str, host: &str) -> u16 {
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    );
    raw_status(server.addr, request.as_bytes())
}

#[test]
fn host_checks_cover_every_route_and_unknown_routes() {
    let (state, _) = state_with(&fixture("md/minimal.md"));
    let server = TestServer::start(state);
    let evil = format!("evil.example:{}", server.addr.port());
    let local = format!("localhost:{}", server.addr.port());
    for path in [
        "/",
        "/__writer/state",
        "/__writer/events",
        "/__writer/theme.css",
        "/__writer/asset/vendor/morphdom/morphdom-umd.min.js",
        "/__writer/doc-asset/ffffffffffff",
        "/__writer/live-reload.js",
        "/__writer/pages",
        "/__writer/pages/1.svg",
        "/__writer/pages.js",
        "/favicon.ico",
        "/not-a-route",
    ] {
        assert_eq!(host_status(&server, path, &evil), 403, "{path}");
        assert_ne!(host_status(&server, path, &local), 403, "{path}");
    }
    for host in
        [format!("192.168.1.5:{}", server.addr.port()), "localhost".into()]
    {
        assert_eq!(host_status(&server, "/", &host), 403, "{host}");
    }
    assert_eq!(server.get_with("/", &[("Host", &evil)]).status, 403);
    let forbidden = server.get_with("/", &[("Host", &evil)]);
    assert!(
        String::from_utf8(forbidden.body).unwrap().contains("DNS rebinding")
    );
    let mut invalid = b"GET / HTTP/1.1\r\nHost: ".to_vec();
    invalid.push(0xff);
    invalid.extend_from_slice(b"\r\nConnection: close\r\n\r\n");
    assert_eq!(raw_status(server.addr, &invalid), 403);
    assert_eq!(
        raw_status(server.addr, b"GET /__writer/state HTTP/1.0\r\n\r\n"),
        200
    );
    let absolute = format!("GET http://{evil}/__writer/state HTTP/1.0\r\n\r\n");
    assert_eq!(raw_status(server.addr, absolute.as_bytes()), 403);
}

#[test]
fn explicit_host_name_is_case_insensitive_and_middleware_covers_merged_routes()
{
    let server = TestServer::start_with(
        empty_state(ServeView::Web, "Host"),
        Some("Writer.Example"),
        true,
    );
    let named = format!("writer.example:{}", server.addr.port());
    assert_eq!(host_status(&server, "/extra", &named), 200);
    assert_eq!(host_status(&server, "/extra", &named.to_uppercase()), 200);
    let other = format!("other.example:{}", server.addr.port());
    assert_eq!(host_status(&server, "/extra", &other), 403);
    let mapped = format!("[::ffff:127.0.0.1]:{}", server.addr.port());
    assert_eq!(host_status(&server, "/extra", &mapped), 200);
}
