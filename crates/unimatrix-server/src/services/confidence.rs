//! ConfidenceService: batched fire-and-forget confidence recomputation.
//!
//! Replaces scattered compute_confidence + update_confidence blocks
//! throughout tools.rs (ADR-004).

use std::sync::Arc;

use unimatrix_core::Store;

/// Batched confidence recomputation service.
#[derive(Clone)]
pub(crate) struct ConfidenceService {
    store: Arc<Store>,
}

impl ConfidenceService {
    pub(crate) fn new(store: Arc<Store>) -> Self {
        ConfidenceService { store }
    }

    /// Recompute confidence for a batch of entries.
    ///
    /// Fire-and-forget via `spawn_blocking`. Single iteration over entry IDs
    /// within one blocking task. Per-entry failure is logged and skipped
    /// (consistent with existing fire-and-forget contract, ADR #53).
    ///
    /// Empty `entry_ids` is a no-op (FR-03.4).
    pub(crate) fn recompute(&self, entry_ids: &[u64]) {
        if entry_ids.is_empty() {
            return;
        }

        let store = Arc::clone(&self.store);
        let ids = entry_ids.to_vec();

        let _ = tokio::task::spawn_blocking(move || {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            for id in ids {
                match store.get(id) {
                    Ok(entry) => {
                        let conf =
                            unimatrix_engine::confidence::compute_confidence(&entry, now);
                        if let Err(e) = store.update_confidence(id, conf) {
                            tracing::warn!("confidence recompute failed for {id}: {e}");
                        }
                    }
                    Err(e) => {
                        tracing::warn!("confidence recompute: entry {id} not found: {e}");
                    }
                }
            }
        });
    }
}
