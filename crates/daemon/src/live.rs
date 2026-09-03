//! The live state.
//!
//! What is on the panel, the brightness, the discovered cards and the newest
//! job. `GET /state` returns it; `GET /state/events` pushes the whole object
//! over the broadcast machinery `jobs.rs` already carries, on every change
//! and on nothing else.

use crate::api::{Card, JobBrief, Show, ShowKind, State};
use crate::jobs::{broadcast_sse, rfc3339_now, Handle};
use crate::lock;
use axum::response::IntoResponse;
use std::sync::Mutex;
use tokio::sync::broadcast;
use wall::Canvas;

/// How many changes a slow subscriber may fall behind before it is sent the
/// current state instead of the ones it missed.
const BACKLOG: usize = 64;

pub struct Live {
    state: Mutex<State>,
    tx: broadcast::Sender<State>,
}

impl Live {
    pub fn new(brightness: u8) -> Self {
        Self {
            state: Mutex::new(State {
                show: None,
                brightness,
                cards: Vec::new(),
                job: None,
            }),
            tx: broadcast::channel(BACKLOG).0,
        }
    }

    pub fn snapshot(&self) -> State {
        lock(&self.state).clone()
    }

    /// Change the state and push the whole of it to every subscriber.
    fn publish(&self, f: impl FnOnce(&mut State)) {
        let next = {
            let mut s = lock(&self.state);
            f(&mut s);
            s.clone()
        };
        let _ = self.tx.send(next);
    }

    pub fn set_brightness(&self, value: u8) {
        self.publish(|s| s.brightness = value);
    }

    pub fn set_cards(&self, cards: Vec<Card>) {
        self.publish(|s| s.cards = cards);
    }

    /// A show went up: it replaces whatever was on the panel.
    pub fn show(
        &self,
        kind: ShowKind,
        source: String,
        fps: Option<u32>,
        canvas: &Canvas,
        job: Option<String>,
    ) {
        let show = Show {
            kind,
            source,
            fps,
            layout: layout_name(canvas),
            started: rfc3339_now(),
            job,
        };
        self.publish(|s| s.show = Some(show));
    }

    /// A job started or finished: its line, and the end of the show it kept
    /// on the panel.
    pub fn job(&self, handle: &Handle) {
        let job = handle.snapshot();
        let brief = JobBrief {
            id: job.id,
            kind: job.kind,
            state: job.state,
            started: job.started,
        };
        let running = brief.state == crate::jobs::JobState::Running;
        self.publish(|s| {
            if !running && s.show.as_ref().and_then(|w| w.job.as_deref()) == Some(brief.id.as_str())
            {
                s.show = None;
            }
            s.job = Some(brief);
        });
    }

    pub fn events(&self) -> impl IntoResponse {
        broadcast_sse("state", self.snapshot(), self.tx.subscribe())
    }
}

/// The wall a show is drawn on, as the status bar names it.
fn layout_name(c: &Canvas) -> String {
    let n = c.receivers.len();
    format!(
        "{}x{}, {n} card{}",
        c.width,
        c.height,
        if n == 1 { "" } else { "s" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{JobKind, Jobs};

    #[test]
    fn a_show_is_cleared_when_the_job_that_held_it_ends() {
        let live = Live::new(255);
        let jobs = Jobs::default();
        let handle = jobs.create(jobs.next_id(), JobKind::ShowHold);
        let canvas = Canvas::single(128, 64);
        live.show(
            ShowKind::Pattern,
            "rgb".to_owned(),
            None,
            &canvas,
            Some(handle.id()),
        );
        live.job(&handle);
        let s = live.snapshot();
        assert_eq!(s.show.as_ref().unwrap().layout, "128x64, 1 card");
        assert_eq!(s.job.as_ref().unwrap().id, "j1");

        handle.cancel();
        handle.finish_for_test();
        live.job(&handle);
        let s = live.snapshot();
        assert!(s.show.is_none(), "the show left with its job");
        assert_eq!(s.job.unwrap().state, crate::jobs::JobState::Cancelled);
    }

    #[test]
    fn brightness_and_cards_leave_the_show_alone() {
        let live = Live::new(200);
        let canvas = Canvas::single(64, 32);
        live.show(ShowKind::Blank, "blank".to_owned(), None, &canvas, None);
        live.set_brightness(40);
        live.set_cards(Vec::new());
        let s = live.snapshot();
        assert_eq!(s.brightness, 40);
        assert_eq!(s.show.unwrap().kind, ShowKind::Blank);
    }
}
