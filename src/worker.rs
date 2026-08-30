//! Background execution for blocking provider calls, plus wall-clock timers.
//!
//! Every provider call runs on its own thread; the result comes back as an event through an
//! `mpsc` channel that the app drains once per frame. The thread also asks egui to repaint so the
//! result shows up without waiting for the next input event.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

pub struct Worker<E> {
    tx: Sender<E>,
    ctx: Option<egui::Context>,
    inline: bool,
}

impl<E: Send + 'static> Worker<E> {
    /// Jobs run on background threads; `ctx` is repainted when a job finishes.
    pub fn threaded(ctx: Option<egui::Context>) -> (Self, Receiver<E>) {
        let (tx, rx) = channel();
        (
            Self {
                tx,
                ctx,
                inline: false,
            },
            rx,
        )
    }

    /// Jobs run synchronously on the calling thread — deterministic for tests.
    #[cfg(test)]
    pub fn inline() -> (Self, Receiver<E>) {
        let (tx, rx) = channel();
        (
            Self {
                tx,
                ctx: None,
                inline: true,
            },
            rx,
        )
    }

    pub fn run(&self, job: impl FnOnce() -> E + Send + 'static) {
        if self.inline {
            let _ = self.tx.send(job());
            return;
        }
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        thread::spawn(move || {
            let event = job();
            let _ = tx.send(event);
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });
    }
}

/// Deferred events keyed by wall-clock deadline (snackbar expiry, poll scheduling …).
pub struct Timers<E> {
    queue: Vec<(Instant, E)>,
}

impl<E> Default for Timers<E> {
    fn default() -> Self {
        Self { queue: Vec::new() }
    }
}

impl<E> Timers<E> {
    pub fn schedule(&mut self, delay: Duration, ev: E) {
        self.queue.push((Instant::now() + delay, ev));
    }

    /// Pops every event whose deadline has passed, in deadline order.
    pub fn drain_due(&mut self, now: Instant) -> Vec<E> {
        self.queue.sort_by_key(|(t, _)| *t);
        let split = self.queue.partition_point(|(t, _)| *t <= now);
        self.queue.drain(..split).map(|(_, e)| e).collect()
    }

    pub fn next_due(&self) -> Option<Instant> {
        self.queue.iter().map(|(t, _)| *t).min()
    }

    /// Drops queued events matching `pred` (e.g. polls for a number that was dismissed).
    pub fn retain(&mut self, mut pred: impl FnMut(&E) -> bool) {
        self.queue.retain(|(_, e)| pred(e));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_worker_delivers_synchronously() {
        let (w, rx) = Worker::<u32>::inline();
        w.run(|| 7);
        assert_eq!(rx.try_recv().unwrap(), 7);
    }

    #[test]
    fn threaded_worker_delivers_eventually() {
        let (w, rx) = Worker::<&'static str>::threaded(None);
        w.run(|| "done");
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), "done");
    }

    #[test]
    fn timers_pop_in_order_and_retain() {
        let mut t = Timers::default();
        t.schedule(Duration::from_millis(50), "b");
        t.schedule(Duration::ZERO, "a");
        t.schedule(Duration::from_secs(60), "c");
        assert_eq!(
            t.drain_due(Instant::now() + Duration::from_millis(100)),
            vec!["a", "b"]
        );
        t.retain(|e| *e != "c");
        assert!(t.next_due().is_none());
    }
}
