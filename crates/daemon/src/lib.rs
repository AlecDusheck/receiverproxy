//! The daemon behind `rxp ui`: an HTTP server that holds the raw Ethernet
//! link and runs the `ops` functions, as documented in `docs/ui.md`. Static
//! files come from `web/build-static` when it existed at build time; the JSON API
//! lives under `/api/v1` and every route but `GET /health` needs the token.

pub mod api;
mod assets;
mod error;
mod ifaces;
pub mod jobs;
mod routes;
pub mod state;
mod store;

pub use routes::router;
pub use state::AppState;

use anyhow::{Context, Result};
use base64::prelude::{Engine, BASE64_URL_SAFE_NO_PAD};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::Arc;

/// What `rxp ui` passes.
#[derive(Clone, Debug)]
pub struct Options {
    pub port: u16,
    /// The address to bind. Loopback keeps the daemon on this machine;
    /// anything else exposes it to that network, with the token as the only
    /// credential.
    pub listen: Ipv4Addr,
    /// Open the browser on the URL once listening.
    pub open: bool,
    /// The credential every API request must carry; a random one when absent.
    pub token: Option<String>,
    /// Interface given on the command line; beats the saved setting.
    pub iface: Option<String>,
    /// Where settings, the wall, backups and snapshots live; the OS config
    /// directory plus `rxp` when absent.
    pub data_dir: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            port: 7120,
            listen: Ipv4Addr::LOCALHOST,
            open: false,
            token: None,
            iface: None,
            data_dir: None,
        }
    }
}

/// Bind, discover once, print the URL, serve until the process ends.
///
/// # Errors
/// Fails if there is no data directory, or the port cannot be bound.
pub fn run(opts: Options) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("start the async runtime")?;
    rt.block_on(serve(opts))
}

async fn serve(opts: Options) -> Result<()> {
    let data_dir = match opts.data_dir {
        Some(d) => d,
        None => dirs::config_dir()
            .context("no configuration directory for this user; pass --data-dir")?
            .join("receiverproxy"),
    };
    let token = match opts.token {
        Some(t) => t,
        None => new_token()?,
    };
    let state = AppState::new(data_dir, token.clone(), opts.iface)?;
    let listener = tokio::net::TcpListener::bind((opts.listen, opts.port))
        .await
        .with_context(|| format!("bind {}:{}", opts.listen, opts.port))?;
    // On 0.0.0.0 the printed URL names an address a browser can reach.
    let host = if opts.listen.is_unspecified() {
        ifaces::first_non_loopback_v4().unwrap_or(opts.listen)
    } else {
        opts.listen
    };
    let url = format!("http://{host}:{}/#token={token}", opts.port);
    println!("rxp ui: {url}");

    state.discover_at_start().await;
    if opts.open {
        open_browser(&url);
    }
    axum::serve(listener, router(state)).await.context("serve")
}

/// 32 random bytes as base64url, the credential for one run of the daemon.
fn new_token() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).context("generate the token")?;
    Ok(BASE64_URL_SAFE_NO_PAD.encode(bytes))
}

/// Hand the URL to the desktop; a failure only means the user opens it by hand.
fn open_browser(url: &str) {
    let cmd = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(cmd)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// Shared by the routes and the runner: the state behind every handler.
pub type Shared = Arc<AppState>;

/// A poisoned lock still holds usable data: the panic that poisoned it was
/// another request's.
pub(crate) fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
