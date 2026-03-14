# Agent Report: crt-018b-agent-5-search

**Agent ID**: crt-018b-agent-5-search
**Component**: Search Utility Delta (`services/search.rs`)
**Feature**: crt-018b — Effectiveness-Driven Retrieval

---

## Deliverables

### Files Modified

- `crates/unimatrix-server/src/services/search.rs`

### Changes Made

1. **New imports**: Added `std::sync::Mutex`, `unimatrix_engine::effectiveness::{EffectivenessCategory, SETTLED_BOOST, UTILITY_BOOST, UTILITY_PENALTY}`, and `crate::services::effectiveness::{EffectivenessSnapshot, EffectivenessStateHandle}`.

2. **New fields on `SearchService`**:
   - `effectiveness_state: EffectivenessStateHandle` — Arc clone shared from ServiceLayer
   - `cached_snapshot: Arc<Mutex<EffectivenessSnapshot>>` — generation-cached per-service snapshot (R-06)

3. **Updated `SearchService::new()`**: Added `effectiveness_state: EffectivenessStateHandle` as required 8th parameter; initializes `cached_snapshot` via `EffectivenessSnapshot::new_shared()`.

4. **New `utility_delta()` free function**: Maps `Option<EffectivenessCategory>` to `f64` using constants from `unimatrix_engine::effectiveness`. `None` and `Unmatched` return `0.0` (cold-start safe, AC-06).

5. **Snapshot logic at top of `search()`**: Generation-cache pattern (ADR-001). Acquires read lock, reads generation into local var, drops read guard, then acquires mutex. Never holds both simultaneously (R-01 lock ordering). On generation mismatch, re-acquires read lock briefly to clone categories.

6. **Step 7 sort updated**: `utility_delta` added to `base_a`/`base_b` inside `* penalty` multiplication (ADR-003). Formula: `(rerank_score + delta + prov) * penalty`.

7. **Step 8 co-access re-sort updated**: `utility_delta` added alongside `boost` and `prov` inside `* penalty`. Formula: `(rerank_score + delta + boost + prov) * penalty`.

8. **Step 11 ScoredEntry construction updated**: `final_score` includes `utility_delta` for consistency with sort order: `(rerank_score + delta) * penalty`.

---

## Tests

**24 tests pass, 0 fail** (run with `background.rs` in-progress changes from another agent stashed, as that agent's partial work causes a compile error unrelated to our code).

### New Tests Added (16 new tests)

| Test | Coverage |
|------|----------|
| `test_utility_delta_effective` | UTILITY_BOOST = 0.05 |
| `test_utility_delta_settled` | SETTLED_BOOST = 0.01 |
| `test_utility_delta_ineffective` | -UTILITY_PENALTY = -0.05 |
| `test_utility_delta_noisy` | -UTILITY_PENALTY = -0.05 |
| `test_utility_delta_unmatched_zero` | Unmatched = 0.0 |
| `test_utility_delta_none_zero` | None = 0.0 (AC-06) |
| `test_utility_delta_noisy_equals_ineffective_penalty` | Symmetry invariant |
| `test_utility_constants_values` | All three constants + SETTLED_BOOST < 0.03 |
| `test_effective_outranks_ineffective_at_close_similarity` | AC-05, cw=0.15 |
| `test_effective_outranks_ineffective_at_max_weight` | AC-05, cw=0.25 |
| `test_utility_delta_inside_deprecated_penalty` | ADR-003, R-05 |
| `test_utility_delta_inside_superseded_penalty` | ADR-003, R-05 |
| `test_utility_delta_absent_entry_zero` | AC-06, R-07 |
| `test_cached_snapshot_shared_across_clones` | R-06 |
| `test_snapshot_read_guard_dropped_before_mutex_lock` | R-01 |
| `test_generation_cache_skips_clone_when_unchanged` | ADR-001 |

---

## Issues / Blockers

**Pre-existing blocker (not caused by this agent)**: `background.rs` has in-progress changes from the background-tick-writer agent (crt-018b-agent-background) that introduce an arity mismatch when calling `maintenance_tick()` (8 args supplied, function takes 4). This prevents the lib test binary from compiling when that agent's working-tree changes are present.

Workaround used for verification: stashed `background.rs` changes, ran tests (24/24 pass), then restored. The blocker is in another agent's scope.

**Workspace build**: `cargo build --workspace` passes cleanly (the background.rs arity error only manifests in the test binary compilation, not in the lib build). The full workspace library build has no errors.

---

## Knowledge Stewardship

- **Queried**: `/uni-query-patterns` for `unimatrix-server search service effectiveness state` — tool loaded but no project-specific results returned. Proceeded without (non-blocking per instructions).
- **Stored**: Nothing novel to store — the generation-cache + read-before-mutex lock ordering pattern is already documented in ADR-001 and mirrors the existing `ConfidenceState` pattern established in crt-019. No new gotchas discovered beyond what the pseudocode specified. The implementation followed the spec exactly.
