# Design Review Report: 358-design-reviewer

## Assessment: APPROVED WITH NOTES

The proposed fix approach is architecturally sound and follows established patterns.
One non-blocking note regarding the neighbor lookup approach.

---

## Findings

### 1. Hot-path risk — NONE (non-blocking)

The pre-fetch of `active_entries` before entering rayon happens in the async Tokio
context at tick time, not on the MCP serving path. The contradiction scan already
runs only every `CONTRADICTION_SCAN_INTERVAL_TICKS` ticks (roughly 60-minute
intervals). The `query_by_status(Status::Active)` call is already used throughout
the codebase and is not a new DB pattern. No hot-path concern.

For `check_entry_contradiction` in the quality gate (background.rs ~1585): this
also runs in the background tick, not on the MCP serving path. Pre-fetching active
entries before the quality gate rayon spawn is acceptable. Note that active entries
are fetched once and shared across all entries in the `accepted` loop — this is
more efficient than the current (broken) per-entry store.get() pattern.

For `check_embedding_consistency` in status.rs: this is behind the opt-in
`check_embeddings` flag and already has a `MCP_HANDLER_TIMEOUT` guard. The
async pre-fetch adds one DB read before the rayon spawn. Acceptable.

### 2. Blast radius — LOW (non-blocking)

Three public function signatures change. All callers are within the same crate:
- `background.rs` line ~586: `scan_contradictions`
- `background.rs` line ~1613: `check_entry_contradiction`
- `status.rs` line ~566: `check_embedding_consistency`

No external crate callers (verified: functions are `pub` within `infra::contradiction`
but the `infra` module is not re-exported from the crate's public API). All callers
are in the same crate.

### 3. Architectural fit — APPROVED

The pattern "pre-fetch data async before rayon spawn, pass Vec to sync helper" is
documented as the correct pattern in Unimatrix entry #1758 ("Extract spawn_blocking
body into named sync helper for unit testability"). The proposed fix aligns with this.

The comment on `read_active_entries` states it is "Called from `spawn_blocking`
closures where the tokio handle is available" — this is incorrect and was the
source of confusion. Removing `read_active_entries` eliminates the misleading doc
comment entirely.

ADR-004 (entry #61): "Synchronous API with spawn_blocking Delegation" — the rayon
pool pattern is the equivalent for CPU-bound ML inference (crt-022). The fix respects
this boundary by keeping the I/O in the async layer and the CPU work in rayon.

### 4. Neighbor lookup approach — NOTE (non-blocking)

The investigator proposes replacing `store.get(neighbor.entry_id)` inside the rayon
closure with a HashMap lookup against pre-fetched `active_entries`.

**This is correct for `scan_contradictions`**: The function already filters out
non-Active neighbors (`if neighbor_entry.status != Status::Active { continue }`).
Since `active_entries` contains only Active entries, a miss in the HashMap means
the neighbor is non-Active and should be skipped — behaviorally identical.

**For `check_entry_contradiction`**: Same analysis applies. The `store.get()` result
is immediately checked for `status != Status::Active`. A HashMap miss against
pre-fetched active entries is semantically equivalent to `continue`.

**Note**: Build the HashMap outside the per-entry loop (once per function call),
not inside it. The implementer should do:
```rust
let entries_by_id: std::collections::HashMap<u64, &EntryRecord> =
    active_entries.iter().map(|e| (e.id, e)).collect();
```

### 5. Missing constraint — NOTE (non-blocking)

The `Store` import in `contradiction.rs` will become unused after removing
`read_active_entries`. The implementer must remove the `use unimatrix_core::Store;`
import at line 12 to avoid a compiler warning (which becomes an error under
`-D warnings`). Clippy will catch this.

### 6. Security surface — CLEAN

No new trust boundaries. No privilege changes. The pre-fetched `Vec<EntryRecord>`
is the same data that was already being read. No new input validation gaps.

---

## Revised Fix Approach

The investigator's proposal is correct. Implementing as described:

1. `scan_contradictions(active_entries: Vec<EntryRecord>, ...)` — remove `store` param
2. `check_embedding_consistency(active_entries: Vec<EntryRecord>, ...)` — remove `store` param
3. `check_entry_contradiction(content, title, active_entries: &[EntryRecord], ...)` — remove `store` param
4. Remove `read_active_entries` function entirely
5. Remove `use unimatrix_core::Store;` import (will be unused)
6. `background.rs` scan site: `let active_entries = store.query_by_status(Status::Active).await?;` before rayon spawn
7. `background.rs` quality gate site: same pre-fetch before rayon spawn
8. `status.rs` embed consistency site: same pre-fetch before rayon spawn

Build the `entries_by_id: HashMap<u64, &EntryRecord>` once at the top of
`scan_contradictions` and `check_entry_contradiction` (not per-iteration).

---

## Knowledge Stewardship

- Queried: context_search "async sync boundary rayon spawn pre-fetch store reads background tick" — found #1758 (extract sync helper pattern), #1560 (background-tick state cache pattern), #61 (ADR-004)
- Queried: context_search "ADR ml_inference_pool rayon pool pattern B background task" — found ADRs for other features
- Stored: No new ADR needed for this bugfix — the fix restores correct behavior per existing ADR-004. The lesson (rayon workers have no Tokio runtime) will be stored by bugfix leader.
- Declined: No new architectural decisions introduced by this fix.
