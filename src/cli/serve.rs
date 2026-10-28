//! `writer serve`: bind after the first build, watch changes, and stop on Ctrl-C.
//! The retained two-view session lives in the private child module.

mod session;

use std::future::Future;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::Context as _;
use tokio::net::TcpListener;
use tokio::sync::mpsc as tokio_mpsc;

use super::{
    GlobalArgs, ServeArgs, display_path, input_kind, print_stdout,
    report_diagnostics,
};
use crate::config::{CliOverrides, Config, ServeConfig};
use crate::error::{Diagnostic, Diagnostics};
use crate::pipeline::{self, BuildOptions, Mode};
use crate::serve::server::{self, AllowedHosts};
use crate::serve::watch::{ChangeBatch, Watcher, watch_set_before_build};
use session::Session;

const BUILD_STACK_SIZE: usize = 8 << 20;
const SERVE_CONFIG_CODE: &str = "serve-config";

/// Runs the server until interruption, or returns a startup/usage failure.
pub fn run(args: &ServeArgs, global: &GlobalArgs) -> anyhow::Result<ExitCode> {
    let opts = serve_options(args, global)?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("writer-serve")
        .thread_stack_size(BUILD_STACK_SIZE)
        .build()
        .context("cannot start the server runtime")?;
    let served = runtime.block_on(start(opts, global));
    // Serve writes nothing; an in-flight blocking build may be abandoned.
    runtime.shutdown_background();
    served?;
    Ok(ExitCode::SUCCESS)
}

async fn start(opts: BuildOptions, global: &GlobalArgs) -> anyhow::Result<()> {
    let mut shutdown =
        Box::pin(interrupt().context("cannot listen for Ctrl-C")?);
    let config = serve_config(&opts, global);
    let (tx, batches) = mpsc::channel();
    let watcher = Watcher::start(&watch_set_before_build(&opts), tx)?;
    tracing::info!("first build of {}", display_path(&opts.input).display());
    let mut session = Session::new(opts, global.clone(), watcher, &config);
    let first = tokio::task::spawn_blocking(move || {
        session.rebuild(Vec::new());
        session
    });
    let session = tokio::select! {
        biased;
        () = &mut shutdown => {
            tracing::info!("interrupted during the first build");
            return Ok(());
        }
        session = first => session.context("the first build stopped")?,
    };
    serve(session, batches, &config, shutdown).await
}

fn serve_options(
    args: &ServeArgs, global: &GlobalArgs,
) -> anyhow::Result<BuildOptions> {
    Ok(BuildOptions {
        format: Some(input_kind(&args.input)?),
        config_path: global.config.clone(),
        cli: CliOverrides {
            theme: args.theme.clone(),
            template: args.template.clone(),
            port: args.port,
            host: args.host.clone(),
            open: args.open.then_some(true),
            view: args.view.map(Into::into),
            allow_packages: args.allow_packages.then_some(true),
            ..CliOverrides::default()
        },
        mode: Mode::Serve { build_id: 1 },
        ..BuildOptions::new(&args.input)
    })
}

/// The `[serve]` settings with the command line applied. When `writer.toml` cannot be
/// loaded, the defaults with the command line applied, and a warning saying so (the first
/// build reports the error itself).
fn serve_config(opts: &BuildOptions, global: &GlobalArgs) -> ServeConfig {
    match pipeline::load_config(opts) {
        Ok((config, _)) => config.serve,
        Err(err) => {
            tracing::debug!("cannot load the configuration: {err}");
            let mut config = Config::default();
            config.apply_cli(&opts.cli);
            let note = format!(
                "the [serve] settings cannot be read (the build error below says why), so \
                 the server listens on {} until it is restarted",
                display_address(&config.serve.host, config.serve.port)
            );
            let mut diagnostics = Diagnostics::new();
            diagnostics.push(Diagnostic::warning(SERVE_CONFIG_CODE, note));
            report_diagnostics(&diagnostics, global);
            config.serve
        }
    }
}

/// Bind and announce once, then serve while rebuilding sequentially off the async pool.
async fn serve(
    session: Session, batches: mpsc::Receiver<ChangeBatch>,
    config: &ServeConfig, shutdown: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    let mut shutdown = Box::pin(shutdown);
    // Name resolution and address probes must remain interruptible too.
    let listener = tokio::select! {
        biased;
        () = &mut shutdown => return Ok(()),
        listener = bind(&config.host, config.port) => listener?,
    };
    let addr =
        listener.local_addr().context("cannot read the server's address")?;
    print_stdout(&format!("Serving {}", url_for(addr)))?;
    if config.open {
        open_browser(url_for(browser_addr(addr)));
    }
    let router = session.router();
    let (tx, rx) = tokio_mpsc::unbounded_channel();
    std::thread::Builder::new()
        .name("writer-batches".into())
        .spawn(move || {
            for batch in batches {
                if tx.send(batch).is_err() {
                    break;
                }
            }
        })
        .context("cannot start the rebuild thread")?;
    let rebuilds = tokio::spawn(rebuild_loop(session, rx));
    let hosts = AllowedHosts::new(addr).with_name(&config.host);
    let served =
        server::serve_until_allowing(listener, router, shutdown, hosts).await;
    rebuilds.abort();
    tracing::info!("dev server stopped");
    Ok(served?)
}

/// How long [`bind`] waits for a server that may already answer on the address.
const PROBE_TIMEOUT: Duration = Duration::from_millis(300);

/// Binds `host:port` (each address `host` resolves to, in order, until one binds).
///
/// A port that another server already answers on is in use (live-reload.md §4: an error
/// with the hint to use `--port 0`), also when the bind itself succeeds: the listener
/// sets `SO_REUSEADDR` (so that a restart is not refused while connections of the previous
/// run linger in `TIME_WAIT`), and on macOS and the BSDs that lets it bind `127.0.0.1:P`
/// although another server listens on `0.0.0.0:P` or a dual-stack `[::]:P`. So, unless the
/// port is 0, each address is first probed with a connection (an unspecified address at
/// its loopback address). Errors name the address and hint at the fix ([`bind_error`]).
async fn bind(host: &str, port: u16) -> anyhow::Result<TcpListener> {
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|err| {
            let err = io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                format!("cannot resolve {host}: {err}"),
            );
            bind_error(host, port, &err)
        })?
        .collect();
    let taken = if port == 0 {
        None
    } else {
        answering(&addrs).await
    };
    let listener = TcpListener::bind(addrs.as_slice())
        .await
        .map_err(|err| bind_error(host, port, &err))?;
    match taken {
        Some(addr) => {
            let err = io::Error::new(
                io::ErrorKind::AddrInUse,
                format!("another server already answers on {addr}"),
            );
            Err(bind_error(host, port, &err))
        }
        None => Ok(listener),
    }
}

/// The first of `addrs` (an unspecified address at its loopback address) where a server
/// accepts a connection within [`PROBE_TIMEOUT`].
async fn answering(addrs: &[SocketAddr]) -> Option<SocketAddr> {
    for addr in addrs.iter().map(|addr| browser_addr(*addr)) {
        let connect = tokio::net::TcpStream::connect(addr);
        if let Ok(Ok(_)) = tokio::time::timeout(PROBE_TIMEOUT, connect).await {
            return Some(addr);
        }
    }
    None
}

/// The error for a failed bind of `host:port`: names the address, then a hint by the
/// kind of `err`: `--port 0` for a port in use (or one that needs privileges), the host
/// settings for an address that cannot be used here (or resolved).
fn bind_error(host: &str, port: u16, err: &io::Error) -> anyhow::Error {
    let address = display_address(host, port);
    let hint = match err.kind() {
        io::ErrorKind::AddrInUse => Some("use --port 0 to pick a free port"),
        io::ErrorKind::PermissionDenied if port != 0 => {
            Some("use --port 0 to pick a free port")
        }
        io::ErrorKind::AddrNotAvailable => {
            Some("check --host, or host in [serve] of writer.toml")
        }
        _ => None,
    };
    match hint {
        Some(hint) => {
            anyhow::anyhow!("cannot listen on {address}: {err} ({hint})")
        }
        None => anyhow::anyhow!("cannot listen on {address}: {err}"),
    }
}

/// `host:port` for messages (an IPv6 address in brackets).
fn display_address(host: &str, port: u16) -> String {
    match host.parse::<IpAddr>() {
        Ok(ip) => SocketAddr::new(ip, port).to_string(),
        Err(_) => format!("{host}:{port}"),
    }
}

/// A future that completes on Ctrl-C (SIGINT). The handler is installed by this call, not
/// when the future is first polled, so no Ctrl-C after it is missed.
#[cfg(unix)]
fn interrupt() -> std::io::Result<impl Future<Output = ()> + Send + 'static> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut sigint = signal(SignalKind::interrupt())?;
    Ok(async move {
        sigint.recv().await;
    })
}

/// A future that completes on Ctrl-C. The handler is installed by this call.
#[cfg(windows)]
fn interrupt() -> std::io::Result<impl Future<Output = ()> + Send + 'static> {
    let mut ctrl_c = tokio::signal::windows::ctrl_c()?;
    Ok(async move {
        ctrl_c.recv().await;
    })
}

/// `http://<addr>/` (an IPv6 address in brackets).
fn url_for(addr: SocketAddr) -> String {
    format!("http://{addr}/")
}

/// The address a browser on this machine reaches `addr` at: `addr`, or the loopback
/// address of its family when it is unspecified (`0.0.0.0`, `::`).
fn browser_addr(addr: SocketAddr) -> SocketAddr {
    match addr.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => {
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), addr.port())
        }
        IpAddr::V6(ip) if ip.is_unspecified() => SocketAddr::new(
            IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
            addr.port(),
        ),
        _ => addr,
    }
}

/// Opens `url` in the default browser, off the async threads (launchers can block). A
/// failure is a warning.
fn open_browser(url: String) {
    tokio::task::spawn_blocking(move || {
        if let Err(err) = open::that(&url) {
            tracing::warn!("cannot open {url} in a browser: {err}");
        }
    });
}

async fn rebuild_loop(
    mut session: Session,
    mut batches: tokio_mpsc::UnboundedReceiver<ChangeBatch>,
) {
    while let Some(first) = batches.recv().await {
        let mut queued = vec![first];
        while let Ok(more) = batches.try_recv() {
            queued.push(more);
        }
        let job = tokio::task::spawn_blocking(move || {
            session.rebuild(queued);
            session
        });
        session = match job.await {
            Ok(session) => session,
            Err(err) => {
                tracing::error!("live reload stopped: {err}");
                return;
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv6Addr;
    #[test]
    fn bind_errors_name_the_address_and_hint_by_kind() {
        let in_use = io::Error::from(io::ErrorKind::AddrInUse);
        let message = bind_error("127.0.0.1", 3000, &in_use).to_string();
        assert!(
            message.starts_with("cannot listen on 127.0.0.1:3000: "),
            "{message}"
        );
        assert!(
            message.ends_with("(use --port 0 to pick a free port)"),
            "{message}"
        );
        assert!(
            bind_error("::1", 80, &in_use).to_string().contains("[::1]:80")
        );
        assert!(
            bind_error("localhost", 80, &in_use)
                .to_string()
                .contains("localhost:80")
        );
        // A privileged port: --port 0 helps; with port 0 already, it does not.
        let denied = io::Error::from(io::ErrorKind::PermissionDenied);
        assert!(
            bind_error("127.0.0.1", 80, &denied)
                .to_string()
                .contains("--port 0")
        );
        assert!(
            !bind_error("127.0.0.1", 0, &denied)
                .to_string()
                .contains("--port 0")
        );
        // The host is the problem: point at the host settings, not at the port.
        let elsewhere = io::Error::from(io::ErrorKind::AddrNotAvailable);
        let message = bind_error("192.0.2.1", 0, &elsewhere).to_string();
        assert!(
            message
                .ends_with("(check --host, or host in [serve] of writer.toml)"),
            "{message}"
        );
        assert!(!message.contains("--port 0"), "{message}");
        let other = io::Error::other("boom");
        assert_eq!(
            bind_error("127.0.0.1", 3000, &other).to_string(),
            "cannot listen on 127.0.0.1:3000: boom"
        );
    }

    #[tokio::test]
    async fn a_port_in_use_is_a_bind_error() {
        let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = taken.local_addr().unwrap().port();
        let err = bind("127.0.0.1", port).await.unwrap_err();
        assert!(err.to_string().contains("--port 0"), "{err}");
        let free = bind("127.0.0.1", 0).await.unwrap();
        assert_ne!(free.local_addr().unwrap().port(), 0);
    }

    /// With `SO_REUSEADDR` (which the listener sets), macOS lets `127.0.0.1:P` be bound
    /// while another server listens on `0.0.0.0:P` or a dual-stack `[::]:P`; the probe in
    /// `bind` reports the port as in use anyway (on Linux the bind itself fails).
    #[tokio::test]
    async fn a_port_held_on_a_wildcard_address_is_in_use() {
        let v4 = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
        let port = v4.local_addr().unwrap().port();
        let err = bind("127.0.0.1", port).await.unwrap_err().to_string();
        assert!(
            err.starts_with(&format!("cannot listen on 127.0.0.1:{port}: ")),
            "{err}"
        );
        assert!(err.ends_with("(use --port 0 to pick a free port)"), "{err}");
        drop(v4);
        // A dual-stack [::] listener, where the system has IPv6 and makes it dual-stack.
        let Ok(v6) = std::net::TcpListener::bind("[::]:0") else {
            return;
        };
        let port = v6.local_addr().unwrap().port();
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            let err = bind("127.0.0.1", port).await.unwrap_err().to_string();
            assert!(err.contains("--port 0"), "{err}");
        }
    }

    /// The probe does not mistake a port whose previous server is gone, with connections
    /// left in `TIME_WAIT` on the server side, for one in use (a restart on the same port).
    #[tokio::test]
    async fn a_port_left_in_time_wait_can_be_bound_again() {
        let first = bind("127.0.0.1", 0).await.unwrap();
        let addr = first.local_addr().unwrap();
        let client = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (accepted, _) = first.accept().await.unwrap();
        // The server side closes first, so its end of the connection enters TIME_WAIT.
        drop(accepted);
        drop(first);
        drop(client);
        let again = bind("127.0.0.1", addr.port()).await.unwrap();
        assert_eq!(again.local_addr().unwrap(), addr);
    }

    #[tokio::test]
    async fn an_address_of_another_machine_is_a_host_error() {
        // TEST-NET-1 (RFC 5737) is no address of this machine; port 0 skips the probe.
        let err = bind("192.0.2.1", 0).await.unwrap_err().to_string();
        assert!(err.starts_with("cannot listen on 192.0.2.1:0: "), "{err}");
        assert!(!err.contains("--port 0"), "{err}");
    }

    #[test]
    fn urls_and_browser_addresses() {
        let v4 = SocketAddr::from(([127, 0, 0, 1], 3000));
        assert_eq!(url_for(v4), "http://127.0.0.1:3000/");
        let v6 = SocketAddr::from((Ipv6Addr::LOCALHOST, 8080));
        assert_eq!(url_for(v6), "http://[::1]:8080/");
        assert_eq!(browser_addr(SocketAddr::from(([0, 0, 0, 0], 3000))), v4);
        assert_eq!(
            browser_addr(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 8080))),
            v6
        );
        let lan = SocketAddr::from(([192, 168, 1, 5], 3000));
        assert_eq!(browser_addr(lan), lan);
    }

    #[test]
    fn options_keep_serve_flags_without_loading_configuration() {
        use crate::cli::{Cli, Command};
        use clap::Parser;
        let cli = Cli::try_parse_from([
            "writer", "serve", "input.ipynb", "--port", "0", "--host",
            "localhost", "--view", "pages", "--theme", "child", "--template",
            "custom.typ", "--allow-packages", "--config", "missing.toml",
        ])
        .unwrap();
        let Some(Command::Serve(args)) = cli.command else {
            panic!("serve command");
        };
        let opts = serve_options(&args, &cli.global).unwrap();
        assert!(matches!(opts.mode, Mode::Serve { build_id: 1 }));
        assert_eq!(opts.format, Some(crate::ir::SourceKind::Notebook));
        assert_eq!(opts.cli.port, Some(0));
        assert_eq!(opts.cli.host.as_deref(), Some("localhost"));
        assert_eq!(opts.cli.view, Some(crate::config::ServeView::Pages));
        assert_eq!(opts.cli.theme.as_deref(), Some("child"));
        assert_eq!(
            opts.cli.template.as_deref(),
            Some(std::path::Path::new("custom.typ"))
        );
        assert_eq!(opts.cli.allow_packages, Some(true));
        assert_eq!(
            opts.config_path.as_deref(),
            Some(std::path::Path::new("missing.toml"))
        );
    }

    #[test]
    fn invalid_startup_config_uses_cli_listening_defaults() {
        let root = tempfile::tempdir().unwrap();
        let mut opts = BuildOptions::new(root.path().join("document.md"));
        opts.config_path = Some(root.path().join("missing.toml"));
        opts.cli.port = Some(0);
        opts.cli.host = Some("localhost".into());
        opts.cli.view = Some(crate::config::ServeView::Pages);
        let config = serve_config(
            &opts,
            &GlobalArgs { quiet: true, ..GlobalArgs::default() },
        );
        assert_eq!(config.port, 0);
        assert_eq!(config.host, "localhost");
        assert_eq!(config.view, Some(crate::config::ServeView::Pages));
    }
}
