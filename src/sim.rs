//! Simulation plumbing: a deferred-event scheduler standing in for the
//! design's `setTimeout` calls, plus a tiny PRNG so we don't need a crate.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    ServicesLoaded,
    CountriesLoaded,
    OffersLoaded,
    RequestResolved { id: u32 },
    CodeArrives { id: u32 },
    CancelResolved { id: u32 },
    ConnectResolved { provider: String },
    SnackExpire { token: u64 },
    CopiedExpire { token: u64 },
}

#[derive(Default)]
pub struct Scheduler {
    queue: Vec<(Instant, Event)>,
}

impl Scheduler {
    pub fn schedule(&mut self, delay: Duration, ev: Event) {
        self.queue.push((Instant::now() + delay, ev));
    }

    /// Pops every event whose deadline has passed, in deadline order.
    pub fn drain_due(&mut self, now: Instant) -> Vec<Event> {
        self.queue.sort_by_key(|(t, _)| *t);
        let split = self.queue.partition_point(|(t, _)| *t <= now);
        self.queue.drain(..split).map(|(_, e)| e).collect()
    }

    pub fn next_due(&self) -> Option<Instant> {
        self.queue.iter().map(|(t, _)| *t).min()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

/// Timing knobs exposed as editor props in the design.
#[derive(Clone, Debug)]
pub struct SimConfig {
    pub code_delay: Duration,
    pub fail_rate_pct: f64,
    pub invert_received: bool,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            code_delay: Duration::from_secs(6),
            fail_rate_pct: 15.0,
            invert_received: true,
        }
    }
}

/// xorshift64* — plenty for a mock-up.
pub struct Rng(u64);

impl Rng {
    pub fn from_time() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9);
        Self::seeded(nanos | 1)
    }

    pub fn seeded(seed: u64) -> Self {
        Self(if seed == 0 {
            0x2545_F491_4F6C_DD1D
        } else {
            seed
        })
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduler_pops_only_due_events_in_order() {
        let mut s = Scheduler::default();
        s.schedule(Duration::from_millis(50), Event::OffersLoaded);
        s.schedule(Duration::ZERO, Event::ServicesLoaded);
        s.schedule(Duration::from_secs(60), Event::CountriesLoaded);
        let due = s.drain_due(Instant::now() + Duration::from_millis(100));
        assert_eq!(due, vec![Event::ServicesLoaded, Event::OffersLoaded]);
        assert!(!s.is_empty());
        assert!(s.next_due().is_some());
    }

    #[test]
    fn rng_is_in_unit_range_and_varies() {
        let mut r = Rng::seeded(42);
        let a = r.unit();
        let b = r.unit();
        assert!((0.0..1.0).contains(&a) && (0.0..1.0).contains(&b));
        assert_ne!(a, b);
        assert_ne!(Rng::seeded(0).next_u64(), 0);
    }
}
