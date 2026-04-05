# Agent Report: bugfix-523-agent-3-nli-tick

**Feature**: bugfix-523  
**Agent ID**: bugfix-523-agent-3-nli-tick  
**Items**: 1 (NLI tick gate) + 2 (log downgrade)  
**File**: `crates/unimatrix-server/src/services/nli_detection_tick.rs`

---

## Changes Made

### Item 1 — NLI Tick Gate

Inserted the explicit `nli_enabled` gate at the PATH B entry boundary, after the `candidate_pairs.is_empty()` fast-exit and before `nli_handle.get_provider().await`:

```rust
if !config.nli_enabled {
    tracing::debug!("graph inference tick: NLI disabled by config; Path B skipped");
    return;
}
```

Also updated the comment at the `get_provider()` call site to remove "Expected when nli_enabled=false (production default)" language. The comment now reads "Transient: provider not yet initialized or temporarily unavailable" and the pre-comment clarifies the nli_enabled=false case is handled by the explicit gate above.

Structural invariants confirmed:
- Gate is AFTER `run_cosine_supports_path(...)` call at line 544
- Gate is AFTER `candidate_pairs.is_empty()` fast-exit at line 552
- Gate is BEFORE `nli_handle.get_provider().await` at line 571
- Path A (Informs) and Path C (cosine Supports) are unconditional
- `background.rs` call site not modified

### Item 2 — Log Downgrade

Changed `tracing::warn!` to `tracing::debug!` at exactly two sites:

1. `category_map.get(src_id)` None arm — message: "Path C: source entry not found in category_map (deprecated mid-tick?) — skipping"
2. `category_map.get(tgt_id)` None arm — message: "Path C: target entry not found in category_map (deprecated mid-tick?) — skipping"

The non-finite cosine `warn!` at the `!cosine.is_finite()` guard site remains `tracing::warn!` unchanged — verified by code review.

---

## Tests Added

All 7 new tests in `services::nli_detection_tick::tests`:

| Test | AC | Result |
|------|----|--------|
| `test_nli_gate_path_b_skipped_nli_disabled` | AC-01 | PASS |
| `test_nli_gate_path_a_informs_edges_still_written_nli_disabled` | AC-02 (Path A) | PASS |
| `test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled` | AC-02 (Path C) | PASS |
| `test_nli_gate_nli_enabled_path_not_regressed` | AC-03 | PASS |
| `test_cosine_supports_path_skips_missing_category_map_src` | AC-04 src branch | PASS |
| `test_cosine_supports_path_skips_missing_category_map_tgt` | AC-04 tgt branch | PASS |
| `test_cosine_supports_path_nonfinite_cosine_handled` | AC-05 | PASS |

AC-04 and AC-05 log-level assertions are behavioral-only per ADR-001(c) (Unimatrix entry #4143). Log level verified by code review. No `tracing-test` harness used.

Non-finite cosine `warn!` site verified by code review to be unchanged. Exactly two `warn!` to `debug!` changes in `run_cosine_supports_path`.

---

## Test Results

- Pre-existing suite: 69 tests — all PASS (no regressions)
- New tests: 7 PASS
- Full nli_detection_tick suite: **76 passed, 0 failed**
- Full workspace: **all pass, 0 failed**

---

## Implementation Notes

**T-03 pair direction fix**: `test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled` initially failed intermittently. The HNSW source candidates are shuffle-selected, so the pair (lesson-learned entry, decision entry) may appear as either ordering in `candidate_pairs`. Path C's category filter is directional (`pair[0] == src_cat && pair[1] == tgt_cat`). Fix: override `informs_category_pairs` in the test config to include both orderings so either HNSW direction produces a valid edge.

---

## Gate Checklist

- [x] `cargo build --workspace` — zero errors
- [x] `cargo test --workspace` — all pass, no new failures
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, `HACK` in non-test code
- [x] All changes within scope (nli_detection_tick.rs only)
- [x] No `.unwrap()` in non-test code added
- [x] Gate message text matches prescribed text exactly: "graph inference tick: NLI disabled by config; Path B skipped"
- [x] Exactly two `warn!` → `debug!` changes in `run_cosine_supports_path`
- [x] Non-finite cosine `warn!` site unchanged (code review confirmed)
- [x] background.rs unchanged (C-01 constraint honored)
- [x] nli_detection_tick.rs line count: 3680 lines — exceeds 500-line guidance but this is pre-existing; no new module split was needed for this batch (surgical changes only)

---

## Knowledge Stewardship

- **Queried**: `mcp__unimatrix__context_briefing` — returned ADR #4143 (bugfix-523 decisions), pattern #3675 (tick gate patterns), ADR #4017 (crt-039 structural invariant). All applied.
- **Stored**: entry #4145 "HNSW pair direction in run_graph_inference_tick tests is non-deterministic" via `/uni-store-pattern`. The shuffle in `select_source_candidates` makes pair direction non-deterministic; directional category pair filters cause silent test flaps. Fix: include both pair orderings in test config.
