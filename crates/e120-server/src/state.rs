//! What every handler shares: the settings, the wall, the last discovery,
//! the jobs, and the one owner of the raw link.

use crate::error::{ApiError, ApiResult};
use crate::jobs::{self, Handle, Jobs, Line, Lines};
use crate::{lock, store};
use anyhow::{Context, Result};
use e120_canvas::Canvas;
use e120_commands::{Ctx, Progress};
use e120_proto::DiscoveryInfo;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// The interface when neither the command line nor the settings name one.
const DEFAULT_IFACE: &str = "en24";

/// `settings.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub iface: String,
    pub brightness: u8,
}

/// Who holds the link: a job, or a synchronous command by its subject.
struct Holder {
    describe: String,
    job: Option<Arc<Handle>>,
}

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub wall: Mutex<Canvas>,
    pub cards: Mutex<Vec<DiscoveryInfo>>,
    pub jobs: Jobs,
    holder: Mutex<Option<Holder>>,
    pub data_dir: PathBuf,
    pub token: Option<String>,
}

/// Held while a command or job owns the link; dropping it frees the link.
pub struct Guard(Arc<AppState>);

impl Drop for Guard {
    fn drop(&mut self) {
        *lock(&self.0.holder) = None;
    }
}

/// The chip libraries built into the daemon, so a spec from the browser
/// resolves `config/chips/...` without a checkout.
static CHIPS: include_dir::Dir<'static> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/../../config/chips");

/// The daemon's chip-library loader: the embedded set by repository path,
/// then the filesystem relative to the working directory.
///
/// # Errors
/// Fails when the path is in neither.
pub fn load_library(path: &str) -> Result<String> {
    if let Some(f) = path
        .strip_prefix("config/chips/")
        .and_then(|rel| CHIPS.get_file(rel))
    {
        return f
            .contents_utf8()
            .map(str::to_owned)
            .with_context(|| format!("{path}: not utf-8"));
    }
    e120_commands::read_library(path)
}

impl AppState {
    /// Load the settings and wall from `data_dir`; `iface` from the command
    /// line overrides the saved one.
    ///
    /// # Errors
    /// Fails if a stored file exists but does not parse.
    pub fn new(
        data_dir: PathBuf,
        token: Option<String>,
        iface: Option<String>,
    ) -> Result<Arc<Self>> {
        let mut settings: Settings =
            store::load(&data_dir.join("settings.json"))?.unwrap_or_else(|| Settings {
                iface: DEFAULT_IFACE.to_owned(),
                brightness: 255,
            });
        if let Some(i) = iface {
            settings.iface = i;
        }
        let wall =
            store::load(&data_dir.join("wall.json"))?.unwrap_or_else(|| Canvas::single(128, 64));
        Ok(Arc::new(Self {
            settings: Mutex::new(settings),
            wall: Mutex::new(wall),
            cards: Mutex::new(Vec::new()),
            jobs: Jobs::default(),
            holder: Mutex::new(None),
            data_dir,
            token,
        }))
    }

    pub fn settings(&self) -> Settings {
        lock(&self.settings).clone()
    }

    /// Replace and persist the settings.
    ///
    /// # Errors
    /// Fails if `settings.json` cannot be written.
    pub fn set_settings(&self, s: Settings) -> Result<()> {
        store::save(&self.data_dir.join("settings.json"), &s)?;
        *lock(&self.settings) = s;
        Ok(())
    }

    pub fn wall(&self) -> Canvas {
        lock(&self.wall).clone()
    }

    /// Replace and persist the wall.
    ///
    /// # Errors
    /// Fails if `wall.json` cannot be written.
    pub fn set_wall(&self, c: Canvas) -> Result<()> {
        store::save(&self.data_dir.join("wall.json"), &c)?;
        *lock(&self.wall) = c;
        Ok(())
    }

    /// The command context from the settings; the panel size only matters
    /// without a layout, and every daemon command passes the wall.
    pub fn ctx(&self) -> Ctx {
        let s = self.settings();
        Ctx {
            iface: s.iface,
            width: 128,
            height: 64,
            order: e120_proto::ColorOrder::Bgr,
            brightness: s.brightness,
        }
    }

    /// Seconds since the epoch, for file names.
    pub fn unix_seconds() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    }

    /// Take the link for `describe`, or 409 naming who has it.
    fn acquire(self: &Arc<Self>, describe: String, job: Option<Arc<Handle>>) -> ApiResult<Guard> {
        let mut holder = lock(&self.holder);
        if let Some(h) = &*holder {
            return Err(ApiError::conflict(format!("{} is running", h.describe)));
        }
        *holder = Some(Holder { describe, job });
        Ok(Guard(self.clone()))
    }

    /// The running `show/video` or `show/hold` job, if one holds the link.
    fn show_job(&self) -> Option<Arc<Handle>> {
        lock(&self.holder)
            .as_ref()
            .and_then(|h| h.job.clone())
            .filter(|j| matches!(j.kind(), "show/video" | "show/hold"))
    }

    /// A `show/*` request replaces a running show job: cancel it and wait.
    pub async fn cancel_show(&self) {
        if let Some(job) = self.show_job() {
            job.cancel();
            job.wait().await;
        }
    }

    /// Run a synchronous command on the blocking pool with the link held.
    /// `subject` is the CLI's error prefix (`config read`).
    ///
    /// # Errors
    /// 409 while the link is held; 500 with the command's error.
    pub async fn command<T, F>(self: &Arc<Self>, subject: &str, f: F) -> ApiResult<(T, Vec<Line>)>
    where
        T: Send + 'static,
        F: FnOnce(&Ctx, &mut dyn Progress) -> Result<T> + Send + 'static,
    {
        let guard = self.acquire(subject.to_owned(), None)?;
        let ctx = self.ctx();
        let result = tokio::task::spawn_blocking(move || {
            let mut lines = Lines::default();
            let r = f(&ctx, &mut lines);
            drop(guard);
            (r, lines.0)
        })
        .await;
        match result {
            Ok((Ok(v), lines)) => Ok((v, lines)),
            Ok((Err(e), _)) => Err(ApiError::command(subject, &e)),
            Err(e) => Err(ApiError::command(subject, &anyhow::anyhow!("{e}"))),
        }
    }

    /// Start a job holding the link; returns its id. `f` returns `Some(committed)`
    /// for a gated command.
    ///
    /// # Errors
    /// 409 while the link is held.
    pub fn start_job<F>(
        self: &Arc<Self>,
        kind: &'static str,
        subject: &str,
        files: Vec<String>,
        f: F,
    ) -> ApiResult<String>
    where
        F: FnOnce(&Ctx, &mut dyn Progress) -> Result<Option<bool>> + Send + 'static,
    {
        // The id is taken only once the link is free, so a 409 burns none.
        let (guard, handle) = {
            let mut holder = lock(&self.holder);
            if let Some(h) = &*holder {
                return Err(ApiError::conflict(format!("{} is running", h.describe)));
            }
            let id = self.jobs.next_id();
            let handle = self.jobs.create(id.clone(), kind);
            *holder = Some(Holder {
                describe: format!("job {id} ({kind})"),
                job: Some(handle.clone()),
            });
            (Guard(self.clone()), handle)
        };
        let id = handle.id();
        let ctx = self.ctx();
        jobs::spawn(
            handle,
            subject.to_owned(),
            files,
            move |p| f(&ctx, p),
            move || drop(guard),
        );
        Ok(id)
    }

    /// The startup discovery: three seconds, the result into `cards`, a
    /// failure on stderr.
    pub async fn discover_at_start(self: &Arc<Self>) {
        match self
            .command("discover", |ctx, p| {
                e120_commands::capture::discover(ctx, 3, p)
            })
            .await
        {
            Ok((cards, _)) => *lock(&self.cards) = cards,
            Err(e) => eprintln!("e120 ui: {}", e.message),
        }
    }
}
