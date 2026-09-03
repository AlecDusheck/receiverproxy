//! What every handler shares: the settings, the wall, the last discovery,
//! the jobs, and the one owner of the raw link.

use crate::error::{ApiError, ApiResult};
use crate::api::{Card, Settings, ShowKind};
use crate::jobs::{self, Handle, JobKind, Jobs, Line, Lines};
use crate::live::Live;
use crate::{lock, store};
use anyhow::Result;
use wall::{Canvas, Frame};
use ops::{Ctx, Progress};
use colorlight::DiscoveryInfo;
use sources::raw::Header;
use std::path::PathBuf;
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The interface when neither the command line nor the settings name one.
const DEFAULT_IFACE: &str = "en24";

/// Who holds the link: a job, or a synchronous command by its subject.
struct Holder {
    describe: String,
    job: Option<Arc<Handle>>,
}

/// The `show/stream` job a `POST /show/frame` feeds: the channel its worker
/// reads and the frame size the pushing client announced.
struct Pushing {
    tx: SyncSender<Frame>,
    header: Header,
    job: Arc<Handle>,
}

/// Frames buffered between the route and the worker; a client that runs
/// ahead of the panel waits instead of piling up latency.
const PUSH_QUEUE: usize = 2;

/// A pushed stream ends when this long passes with no frame.
const PUSH_IDLE: Duration = Duration::from_secs(5);

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub wall: Mutex<Canvas>,
    pub cards: Mutex<Vec<DiscoveryInfo>>,
    pub jobs: Jobs,
    /// What is on the panel, for `GET /state` and its event stream.
    pub live: Live,
    holder: Mutex<Option<Holder>>,
    pushing: Mutex<Option<Pushing>>,
    pub data_dir: PathBuf,
    /// The credential every API request but `GET /health` must present.
    pub token: String,
}

/// Held while a command or job owns the link; dropping it frees the link.
pub struct Guard(Arc<AppState>);

impl Drop for Guard {
    fn drop(&mut self) {
        *lock(&self.0.holder) = None;
    }
}

/// The daemon's chip-library loader: the embedded set by repository path,
/// then the filesystem relative to the working directory.
///
/// # Errors
/// Fails when the path is in neither.
pub fn load_library(path: &str) -> Result<String> {
    // The embedded set, so a spec from the browser resolves `config/chips/...`
    // without a checkout.
    if let Some(text) = ops::panelspec::embedded::chip(path) {
        return Ok(text.to_owned());
    }
    ops::read_library(path)
}

impl AppState {
    /// Load the settings and wall from `data_dir`; `iface` from the command
    /// line overrides the saved one.
    ///
    /// # Errors
    /// Fails if a stored file exists but does not parse.
    pub fn new(data_dir: PathBuf, token: String, iface: Option<String>) -> Result<Arc<Self>> {
        let mut settings: Settings =
            store::load(&data_dir.join("settings.json"))?.unwrap_or_else(|| Settings {
                iface: DEFAULT_IFACE.to_owned(),
                brightness: 255,
                card: None,
            });
        if let Some(i) = iface {
            settings.iface = i;
        }
        let wall =
            store::load(&data_dir.join("wall.json"))?.unwrap_or_else(|| Canvas::single(128, 64));
        Ok(Arc::new(Self {
            live: Live::new(settings.brightness),
            settings: Mutex::new(settings),
            wall: Mutex::new(wall),
            cards: Mutex::new(Vec::new()),
            jobs: Jobs::default(),
            holder: Mutex::new(None),
            pushing: Mutex::new(None),
            data_dir,
            token,
        }))
    }

    /// Record the discovery result and publish it.
    pub fn set_cards(&self, cards: Vec<DiscoveryInfo>) {
        self.live.set_cards(cards.iter().map(Card::from).collect());
        *lock(&self.cards) = cards;
    }

    /// A show went up on the daemon's wall.
    pub fn showing(&self, kind: ShowKind, source: String, fps: Option<u32>, job: Option<String>) {
        self.live.show(kind, source, fps, &self.wall(), job);
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
    /// without a layout, and every daemon command passes the wall. The card
    /// model is the `card` setting, else the first discovered card's.
    pub fn ctx(&self) -> Ctx {
        let s = self.settings();
        let model = match &s.card {
            Some(name) => receivers::by_name(name),
            None => lock(&self.cards).first().and_then(|c| receivers::by_id(c.card_id)),
        };
        Ctx {
            iface: s.iface,
            width: 128,
            height: 64,
            order: colorlight::ColorOrder::Bgr,
            brightness: s.brightness,
            model,
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
        lock(&self.holder).as_ref().and_then(|h| h.job.clone()).filter(|j| {
            matches!(
                j.kind(),
                JobKind::ShowVideo | JobKind::ShowHold | JobKind::ShowStream
            )
        })
    }

    /// A `show/*` request replaces a running show job: cancel it and wait.
    pub async fn cancel_show(&self) {
        // Dropping the sender ends a pushed stream at its next read.
        *lock(&self.pushing) = None;
        if let Some(job) = self.show_job() {
            job.cancel();
            job.wait().await;
        }
    }

    /// Hand a frame to the running pushed stream: its job id when it took
    /// it or was too busy for it (a mirror drops a frame rather than fall
    /// behind), the frame back when there is no such stream.
    fn offer(&self, header: Header, frame: Frame) -> Result<String, Frame> {
        use std::sync::mpsc::TrySendError;
        let pushing = lock(&self.pushing);
        let Some(p) = &*pushing else {
            return Err(frame);
        };
        if p.header != header || !p.job.is_running() {
            return Err(frame);
        }
        match p.tx.try_send(frame) {
            Ok(()) | Err(TrySendError::Full(_)) => Ok(p.job.id()),
            Err(TrySendError::Disconnected(f)) => Err(f),
        }
    }

    /// Draw one pushed frame, starting the `show/stream` job that owns the
    /// link on the first frame and whenever the announced size changes.
    /// Returns the job's id.
    ///
    /// # Errors
    /// 409 while another job holds the link.
    pub async fn push_frame(
        self: &Arc<Self>,
        header: Header,
        frame: Frame,
        source: String,
        fit: sources::Fit,
    ) -> ApiResult<String> {
        // The frame comes back when the running stream cannot take it.
        let frame = match self.offer(header, frame) {
            Ok(id) => return Ok(id),
            Err(f) => f,
        };
        self.cancel_show().await;
        let (tx, rx) = sync_channel::<Frame>(PUSH_QUEUE);
        let canvas = self.wall();
        let id = self.start_job(JobKind::ShowStream, "show frame", Vec::new(), move |ctx, p| {
            ops::ingest::stream_channel(ctx, canvas, &rx, fit, PUSH_IDLE, p).map(|()| None)
        })?;
        let job = self
            .jobs
            .get(&id)
            .ok_or_else(|| ApiError::command("show frame", &anyhow::anyhow!("job {id} vanished")))?;
        let _ = tx.send(frame);
        *lock(&self.pushing) = Some(Pushing {
            tx,
            header,
            job,
        });
        self.showing(
            ShowKind::Stream,
            source,
            Some(u32::from(header.fps)),
            Some(id.clone()),
        );
        Ok(id)
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
        kind: JobKind,
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
        self.live.job(&handle);
        // The live state's `finished` comes from the handle, so it is read
        // after `finish`, not in the `after` hook that releases the link.
        let watch = (self.clone(), handle.clone());
        tokio::spawn(async move {
            watch.1.wait().await;
            watch.0.live.job(&watch.1);
        });
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
                ops::capture::discover(ctx, 3, p)
            })
            .await
        {
            Ok((cards, _)) => self.set_cards(cards),
            Err(e) => eprintln!("rxp ui: {}", e.message),
        }
    }
}
