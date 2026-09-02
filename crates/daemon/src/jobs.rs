//! Long operations: a job keeps every line a command printed, streams them
//! over SSE, and carries a cancel flag the command polls.

use crate::lock;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::IntoResponse;
use ops::Progress;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::sync::{broadcast, mpsc, watch};
use tokio_stream::wrappers::ReceiverStream;

/// Which stream a line went to in the CLI.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    Out,
    Err,
}

/// One line a command printed, in order.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Line {
    pub stream: Stream,
    pub text: String,
}

/// What a command produced: its lines and the files it wrote.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Outcome {
    pub lines: Vec<Line>,
    pub files: Vec<String>,
}

/// A gated command's outcome: the plan when `committed` is false.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GatedOutcome {
    #[serde(flatten)]
    pub outcome: Outcome,
    pub committed: bool,
}

/// What a finished job produced.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(untagged)]
pub enum JobResult {
    Gated(GatedOutcome),
    Plain(Outcome),
}

/// The long operations, named as the API names them.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum JobKind {
    #[serde(rename = "provision")]
    Provision,
    #[serde(rename = "firmware/install")]
    FirmwareInstall,
    #[serde(rename = "flash/snapshot")]
    FlashSnapshot,
    #[serde(rename = "flash/restore")]
    FlashRestore,
    #[serde(rename = "show/video")]
    ShowVideo,
    #[serde(rename = "show/hold")]
    ShowHold,
}

impl JobKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Provision => "provision",
            Self::FirmwareInstall => "firmware/install",
            Self::FlashSnapshot => "flash/snapshot",
            Self::FlashRestore => "flash/restore",
            Self::ShowVideo => "show/video",
            Self::ShowHold => "show/hold",
        }
    }
}

impl fmt::Display for JobKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Running,
    Done,
    Failed,
    Cancelled,
}

/// A job as the API shows it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Job {
    pub id: String,
    pub kind: JobKind,
    pub state: JobState,
    pub started: String,
    pub finished: Option<String>,
    pub lines: Vec<Line>,
    pub error: Option<String>,
    pub result: Option<JobResult>,
}

/// What an SSE subscriber sees: a line with its index in `lines`, or the end.
#[derive(Clone, Debug)]
enum Event {
    Line(usize, Line),
    End,
}

/// A running or finished job and the channels around it.
pub struct Handle {
    job: Mutex<Job>,
    events: broadcast::Sender<Event>,
    done: watch::Sender<bool>,
    cancel: AtomicBool,
}

impl Handle {
    fn new(id: String, kind: JobKind) -> Self {
        Self {
            job: Mutex::new(Job {
                id,
                kind,
                state: JobState::Running,
                started: rfc3339_now(),
                finished: None,
                lines: Vec::new(),
                error: None,
                result: None,
            }),
            events: broadcast::channel(1024).0,
            done: watch::channel(false).0,
            cancel: AtomicBool::new(false),
        }
    }

    pub fn snapshot(&self) -> Job {
        lock(&self.job).clone()
    }

    pub fn id(&self) -> String {
        lock(&self.job).id.clone()
    }

    pub fn kind(&self) -> JobKind {
        lock(&self.job).kind
    }

    pub fn is_running(&self) -> bool {
        lock(&self.job).state == JobState::Running
    }

    /// Ask the command to stop at its next poll.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Wait until the worker has stopped.
    pub async fn wait(&self) {
        let mut rx = self.done.subscribe();
        let _ = rx.wait_for(|d| *d).await;
    }

    fn push(&self, stream: Stream, text: &str) {
        let line = Line {
            stream,
            text: text.to_owned(),
        };
        let index = {
            let mut job = lock(&self.job);
            job.lines.push(line.clone());
            job.lines.len() - 1
        };
        let _ = self.events.send(Event::Line(index, line));
    }

    /// Record the worker's result and wake everyone waiting on it.
    /// `result` is `Ok(files, committed)` or the rendered error.
    fn finish(&self, result: Result<(Vec<String>, Option<bool>), String>) {
        {
            let mut job = lock(&self.job);
            job.finished = Some(rfc3339_now());
            if self.cancel.load(Ordering::Relaxed) {
                job.state = JobState::Cancelled;
            } else {
                match result {
                    Ok((files, committed)) => {
                        job.state = JobState::Done;
                        let outcome = Outcome {
                            lines: job.lines.clone(),
                            files,
                        };
                        job.result = Some(match committed {
                            Some(committed) => JobResult::Gated(GatedOutcome { outcome, committed }),
                            None => JobResult::Plain(outcome),
                        });
                    }
                    Err(e) => {
                        job.state = JobState::Failed;
                        job.error = Some(e);
                    }
                }
            }
        }
        let _ = self.events.send(Event::End);
        let _ = self.done.send(true);
    }

    /// `event: line` for every line so far and then as they arrive, then one
    /// `event: end` with the whole job; a `: keepalive` comment every 15 s.
    pub fn events(self: &Arc<Self>) -> impl IntoResponse {
        let (tx, rx) = mpsc::channel::<Result<SseEvent, std::convert::Infallible>>(64);
        let handle = self.clone();
        tokio::spawn(async move {
            let mut sub = handle.events.subscribe();
            let (replay, running) = {
                let job = lock(&handle.job);
                (job.lines.clone(), job.state == JobState::Running)
            };
            for line in &replay {
                if tx.send(Ok(line_event(line))).await.is_err() {
                    return;
                }
            }
            if running {
                follow(&mut sub, replay.len(), &tx).await;
            }
            let _ = tx.send(Ok(end_event(&handle.snapshot()))).await;
        });
        Sse::new(ReceiverStream::new(rx)).keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keepalive"),
        )
    }
}

/// Forward lines from index `seen` on until the job ends or the client goes.
async fn follow(
    sub: &mut broadcast::Receiver<Event>,
    seen: usize,
    tx: &mpsc::Sender<Result<SseEvent, std::convert::Infallible>>,
) {
    loop {
        match sub.recv().await {
            Ok(Event::Line(i, line)) if i >= seen => {
                if tx.send(Ok(line_event(&line))).await.is_err() {
                    return;
                }
            }
            Ok(Event::Line(..)) => {}
            Ok(Event::End) | Err(broadcast::error::RecvError::Closed) => return,
            // A slow reader lost lines; the end event carries them all.
            Err(broadcast::error::RecvError::Lagged(_)) => {}
        }
    }
}

fn line_event(line: &Line) -> SseEvent {
    SseEvent::default()
        .event("line")
        .data(serde_json::to_string(line).unwrap_or_default())
}

fn end_event(job: &Job) -> SseEvent {
    SseEvent::default()
        .event("end")
        .data(serde_json::to_string(job).unwrap_or_default())
}

/// The job's `Progress`: lines into the job, cancellation from it.
pub struct Sink(Arc<Handle>);

impl Progress for Sink {
    fn out(&mut self, line: &str) {
        self.0.push(Stream::Out, line);
    }

    fn err(&mut self, line: &str) {
        self.0.push(Stream::Err, line);
    }

    fn cancelled(&self) -> bool {
        self.0.cancel.load(Ordering::Relaxed)
    }
}

/// A sink for synchronous commands: the lines, nothing else.
#[derive(Default)]
pub struct Lines(pub Vec<Line>);

impl Progress for Lines {
    fn out(&mut self, line: &str) {
        self.0.push(Line {
            stream: Stream::Out,
            text: line.to_owned(),
        });
    }

    fn err(&mut self, line: &str) {
        self.0.push(Line {
            stream: Stream::Err,
            text: line.to_owned(),
        });
    }
}

/// Every job of the daemon's lifetime, newest last.
#[derive(Default)]
pub struct Jobs {
    next: AtomicU64,
    all: Mutex<Vec<Arc<Handle>>>,
}

/// How many jobs `GET /jobs` lists.
const LISTED: usize = 50;

impl Jobs {
    /// The next id, `j1`, `j2`, ...
    pub fn next_id(&self) -> String {
        format!("j{}", self.next.fetch_add(1, Ordering::Relaxed) + 1)
    }

    /// Register a job under an id from [`Jobs::next_id`].
    pub fn create(&self, id: String, kind: JobKind) -> Arc<Handle> {
        let handle = Arc::new(Handle::new(id, kind));
        lock(&self.all).push(handle.clone());
        handle
    }

    pub fn get(&self, id: &str) -> Option<Arc<Handle>> {
        lock(&self.all).iter().find(|h| h.id() == id).cloned()
    }

    /// The last 50 jobs, newest first.
    pub fn list(&self) -> Vec<Job> {
        lock(&self.all)
            .iter()
            .rev()
            .take(LISTED)
            .map(|h| h.snapshot())
            .collect()
    }
}

/// Run `f` on the blocking pool as job `handle`; `after` runs when it ends,
/// before the job is marked finished (the link is released there).
pub fn spawn<F, A>(handle: Arc<Handle>, subject: String, files: Vec<String>, f: F, after: A)
where
    F: FnOnce(&mut dyn Progress) -> anyhow::Result<Option<bool>> + Send + 'static,
    A: FnOnce() + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let mut sink = Sink(handle.clone());
        let result = f(&mut sink);
        after();
        handle.finish(
            result
                .map(|committed| (files, committed))
                .map_err(|e| format!("{subject}: {e:#}")),
        );
    });
}

/// The current time as `YYYY-MM-DDTHH:MM:SSZ`.
pub fn rfc3339_now() -> String {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days as i64);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's
/// `civil_from_days`).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates_match_known_days() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(20_697), (2026, 9, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    #[test]
    fn ids_count_up_and_listing_is_newest_first() {
        let jobs = Jobs::default();
        let a = jobs.next_id();
        let b = jobs.next_id();
        assert_eq!((a.as_str(), b.as_str()), ("j1", "j2"));
        jobs.create(a, JobKind::Provision);
        jobs.create(b, JobKind::FlashSnapshot);
        let ids: Vec<String> = jobs.list().into_iter().map(|j| j.id).collect();
        assert_eq!(ids, ["j2", "j1"]);
        assert!(jobs.get("j1").is_some());
        assert!(jobs.get("j9").is_none());
    }
}
