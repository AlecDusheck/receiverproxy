//! The daemon behind `e120 ui`: an HTTP server on 127.0.0.1 that holds the
//! raw Ethernet link and runs the `e120-commands` functions, as documented in
//! `docs/ui.md`. Static files come from `web/dist` when it existed at build
//! time; the JSON API lives under `/api/v1`.

mod assets;
mod error;
pub mod jobs;
mod routes;
pub mod state;
mod store;

pub use routes::router;
pub use state::AppState;

use anyhow::{Context, Result};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::Arc;

/// What `e120 ui` passes.
#[derive(Clone, Debug, Default)]
pub struct Options {
    pub port: u16,
    /// Open the browser on the URL once listening.
    pub open: bool,
    /// Required in an `X-Token` header on every API request when set.
    pub token: Option<String>,
    /// Interface given on the command line; beats the saved setting.
    pub iface: Option<String>,
    /// Where settings, the wall, backups and snapshots live; the OS config
    /// directory plus `e120` when absent.
    pub data_dir: Option<PathBuf>,
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
            .join("e120"),
    };
    let state = AppState::new(data_dir, opts.token.clone(), opts.iface)?;
    let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, opts.port))
        .await
        .with_context(|| format!("bind 127.0.0.1:{}", opts.port))?;
    let mut url = format!("http://127.0.0.1:{}", opts.port);
    if let Some(t) = &opts.token {
        url.push_str("#token=");
        url.push_str(t);
    }
    println!("e120 ui: {url}");

    state.discover_at_start().await;
    if opts.open {
        open_browser(&url);
    }
    axum::serve(listener, router(state)).await.context("serve")
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
