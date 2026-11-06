//! Token buckets for the deadlock-api limit scopes (doc 04 "Client rules", FR-DATA-004, TST-013).
//!
//! Each bucket is a `governor` GCRA limiter with a small burst and a slower steady rate, chosen so
//! that no window of the bucket's length ever holds more than the configured count. A plain
//! `per_minute(160)` quota would allow 160 at once and then 160 more over the next minute, which
//! breaks the published limit it is meant to respect.
//!
//! The clock is injectable so tests can queue hundreds of calls without waiting a real minute.

use std::{
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, Instant},
};

use governor::{
    clock::{Clock, FakeRelativeClock},
    middleware::NoOpMiddleware,
    nanos::Nanos,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};

/// Time source for buckets and cooldowns.
#[derive(Clone, Debug)]
pub(crate) enum PaceClock {
    Real(Arc<Instant>),
    /// Tests: time moves only when a limiter would have to wait, or when a test advances it.
    Fake(FakeRelativeClock),
}

impl PaceClock {
    pub(crate) fn real() -> Self {
        Self::Real(Arc::new(Instant::now()))
    }

    /// Wait out `d`: really sleep, or jump the fake clock forward.
    pub(crate) async fn sleep(&self, d: Duration) {
        match self {
            Self::Real(_) => tokio::time::sleep(d).await,
            Self::Fake(c) => c.advance(d),
        }
    }
}

impl Clock for PaceClock {
    type Instant = Nanos;

    fn now(&self) -> Nanos {
        match self {
            Self::Real(start) => start.elapsed().into(),
            Self::Fake(c) => c.now(),
        }
    }
}

/// The quota that keeps any window of length `window` (closed at both ends) to at most `max`
/// events: a burst of one tenth of `max`, then one event per `window / (max - burst - 1)`.
fn quota(max: u32, window: Duration) -> Quota {
    let max = max.max(1);
    let burst = (max / 10).max(1);
    let steady = max.saturating_sub(burst).saturating_sub(1).max(1);
    let period_ns = window.as_nanos().div_ceil(u128::from(steady)).max(1);
    let period = Duration::from_nanos(u64::try_from(period_ns).unwrap_or(u64::MAX));
    Quota::with_period(period)
        .expect("period is non-zero")
        .allow_burst(NonZeroU32::new(burst).expect("burst is at least 1"))
}

pub(crate) struct Bucket {
    limiter: RateLimiter<NotKeyed, InMemoryState, PaceClock, NoOpMiddleware<Nanos>>,
    clock: PaceClock,
}

impl Bucket {
    pub(crate) fn new(max: u32, window: Duration, clock: PaceClock) -> Self {
        Self {
            limiter: RateLimiter::direct_with_clock(quota(max, window), clock.clone()),
            clock,
        }
    }

    /// Take a slot if one is free now. Otherwise say how long until one is.
    pub(crate) fn try_acquire(&self) -> Result<(), Duration> {
        self.limiter
            .check()
            .map_err(|n| n.wait_time_from(self.clock.now()))
    }

    /// Wait until one more request is allowed.
    pub(crate) async fn acquire(&self) {
        loop {
            match self.limiter.check() {
                Ok(()) => return,
                Err(not_until) => {
                    let wait = not_until.wait_time_from(self.clock.now());
                    self.clock.sleep(wait.max(Duration::from_millis(1))).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Send times of `n` sequential acquires, in fake time.
    async fn times(max: u32, window: Duration, n: usize) -> Vec<Duration> {
        let clock = FakeRelativeClock::default();
        let pace = PaceClock::Fake(clock);
        let b = Bucket::new(max, window, pace.clone());
        let mut out = Vec::new();
        for _ in 0..n {
            b.acquire().await;
            out.push(Duration::from(pace.now()));
        }
        out
    }

    #[tokio::test]
    async fn no_window_holds_more_than_max() {
        for (max, window) in [(160, 60), (24, 60), (16, 60), (60, 60), (80, 1)] {
            let window = Duration::from_secs(window);
            let t = times(max, window, max as usize * 5).await;
            for (i, start) in t.iter().enumerate() {
                let in_window = t[i..].iter().filter(|x| **x - *start <= window).count();
                assert!(
                    in_window <= max as usize,
                    "{max} per {window:?}: {in_window} in one window"
                );
            }
        }
    }

    #[tokio::test]
    async fn a_burst_goes_out_at_once() {
        let t = times(160, Duration::from_secs(60), 16).await;
        assert!(t.iter().all(|x| x.is_zero()), "first calls are not delayed");
    }
}
