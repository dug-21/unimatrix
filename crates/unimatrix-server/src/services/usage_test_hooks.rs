//! `#[cfg(test)]` counters for `UsageService` (vnc-050 Test-Only Seams).
//!
//! `enqueued` counts `record_access` calls that passed the `learning_allowed`
//! gate; `in_flight` tracks spawned recording tasks so tests can `flush()`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[derive(Debug, Default)]
pub(crate) struct UsageTestHooks {
    pub(crate) enqueued: AtomicUsize,
    pub(crate) in_flight: AtomicUsize,
    pub(crate) idle: tokio::sync::Notify,
}

/// Decrements `in_flight` on drop (completion or panic).
#[derive(Debug)]
pub(crate) struct InFlightGuard(Arc<UsageTestHooks>);

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.0.in_flight.fetch_sub(1, Ordering::SeqCst);
        self.0.idle.notify_waiters();
    }
}

impl UsageTestHooks {
    pub(crate) fn track(self: &Arc<Self>) -> InFlightGuard {
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        InFlightGuard(Arc::clone(self))
    }

    /// Wait until no recording task is in flight; panics after 5 s.
    pub(crate) async fn flush(&self) {
        let wait = async {
            loop {
                let notified = self.idle.notified();
                if self.in_flight.load(Ordering::SeqCst) == 0 {
                    return;
                }
                notified.await;
            }
        };
        tokio::time::timeout(Duration::from_secs(5), wait)
            .await
            .expect("UsageService::flush timed out after 5 s");
    }
}
