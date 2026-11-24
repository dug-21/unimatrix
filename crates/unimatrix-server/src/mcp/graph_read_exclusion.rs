//! Read-time excluded-node set for context_graph traversal modes (#1009).
//!
//! Quarantine exclusion previously relied on background-tick side effects (orphaned-edge
//! compaction + `TypedGraphState` rebuild), so a freshly quarantined entry stayed
//! traversable until the next tick, and seed/chain hydration never filtered at all.
//! [`ExcludedNodes`] is loaded once per request by `handle_graph` (neighbors / path /
//! subgraph / chain only) and consulted by every traversal loop, independent of the tick.
//!
//! Written as a generic "excluded node set" so a future node-property traversal
//! constraint (PL-11) can generalize it instead of replacing per-site checks.
//!
//! **Denylist, not Active allowlist (deliberate):** the set holds `status = 3`
//! (Quarantined) only. Deprecated entries must stay traversable and resolvable so
//! `follow_to_current` can walk them to their active terminal; an Active-only filter
//! would sever supersession resolution. Deprecated behavior is unchanged by #1009.

use std::collections::HashSet;

use rmcp::model::ErrorData;
use unimatrix_core::Store;

use crate::error::ERROR_INTERNAL;

use super::graph_read_neighbors::follow_to_current;

/// Entry IDs that must never be emitted, traversed, or used as an anchor/seed.
#[derive(Debug, Default, Clone)]
pub(crate) struct ExcludedNodes {
    ids: HashSet<u64>,
}

impl ExcludedNodes {
    /// Load the quarantined ID set (`idx_entries_status`-covered; small by nature —
    /// quarantine is an admin-only action).
    pub(crate) async fn load_quarantined(store: &Store) -> Result<Self, ErrorData> {
        let rows: Vec<i64> =
            sqlx::query_scalar::<_, i64>("SELECT id FROM entries WHERE status = 3")
                .fetch_all(store.read_pool_server())
                .await
                .map_err(|e| {
                    tracing::error!(error = %e, "load quarantined id set failed");
                    ErrorData::new(ERROR_INTERNAL, format!("graph query failed: {e}"), None)
                })?;
        Ok(Self {
            ids: rows.into_iter().map(|id| id as u64).collect(),
        })
    }

    pub(crate) fn contains(&self, id: u64) -> bool {
        self.ids.contains(&id)
    }
}

/// Admit a raw traversal node: `None` when it must be skipped, else the effective ID.
///
/// The raw ID is checked BEFORE supersession resolution, so a quarantined node can never be
/// re-admitted via the `follow_to_current(..) == None -> raw` fallback. When resolution
/// returns a terminal, that terminal is Active by construction but is re-checked anyway.
/// A `None` from resolution on a non-excluded raw ID (deprecated orphan / cycle / 50-hop
/// cap) keeps the raw ID — unchanged deprecated behavior.
pub(crate) async fn admit(
    store: &Store,
    excluded: &ExcludedNodes,
    raw_id: u64,
    resolve: bool,
) -> Option<u64> {
    if excluded.contains(raw_id) {
        return None;
    }
    if !resolve {
        return Some(raw_id);
    }
    match follow_to_current(store, raw_id).await {
        Some(terminal) if excluded.contains(terminal) => None,
        Some(terminal) => Some(terminal),
        None => Some(raw_id),
    }
}
