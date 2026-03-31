# Gate Bugfix Report: bugfix-batch (#337, #345, #346, #378, #379, #380)

> Gate: Bugfix Validation
> Date: 2026-03-30
> Result: PASS (after recheck 2026-03-30)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| #337 fix — validate_config after merge_configs | PASS | Line 1487: `validate_config(&merged, &global_path)?` present |
| #345 fix — all 4 saturating_add | PASS | session.rs (×3) and search.rs (×1) confirmed |
| #346 fix — sanitize_session_id in CompactPayload | PASS | Line 1096: guard present before handle_compact_payload |
| #378/#379 fix — escape helpers defined | PASS | escape_md_cell and escape_md_text at lines 184 and 194 |
| #378/#379 fix — escape applied at all expected sites | PASS | goal, phase_timeline, sessions, phase_outliers, baseline_outliers, cross_cycle_table, knowledge_reuse (intra + cross) |
| #378/#379 fix — leading-# escape only on starts_with('#') | PASS | trim_start().starts_with('#') guard; no global replace |
| #380 fix — i64::try_from + unwrap_or(i64::MAX) | PASS | Line 3038; replaces `obs.ts as i64` |
| #380 fix — comment accurately describes type cast | PASS | "type cast (u64→i64, same unit), not a unit conversion" |
| No todo!/unimplemented!/TODO/FIXME in changed files | PASS | Zero matches across all 6 files |
| No new unsafe code | PASS | "unsafe" appears only as prose in listener.rs comment |
| Fix is minimal — no unrelated changes | PASS | Diff confined to bug sites + tests |
| Workspace builds | PASS | cargo build --workspace: 0 errors, 14 pre-existing warnings |
| 8 regression tests pass | PASS | All 8/8 confirmed by direct run |
| Full unit suite | PASS | 2526 passed, 0 failed (per verifier report) |
| Smoke tests (22/22) | PASS | Per verifier report |
| New tests would have caught original bugs | PASS | Tests directly exercise overflow boundary, invalid session_id, pipe/newline escaping, heading escaping, i64::MAX saturation |
| Clippy — no new warnings in changed files | PASS | Pre-existing errors in unimatrix-engine and unimatrix-observe only |
| xpass test_search_multihop_injects_terminal_active | WARN | Not caused by these 6 fixes (search.rs diff only adds saturating_add + formatting); GH#406 remains open; xfail marker should stay |
| Knowledge stewardship — rust-dev report | PASS | Fix report present; Queried: and Stored:/Declined: entries confirmed (recheck 2026-03-30) |

---

## Detailed Findings

### Fix #337: validate_config after merge_configs

**Status**: PASS

**Evidence**: `config.rs` line 1483-1487:
```
let merged = merge_configs(global_config, project_config);

// Step 4: post-merge validation — catches constraint violations that only appear
// when two individually-valid configs are combined (e.g., fusion weight sum > 1.0).
validate_config(&merged, &global_path)?;
```
Regression test `test_merge_configs_post_merge_fusion_weight_sum_exceeded` confirmed PASS. Test constructs global (w_sim=0.5) and project (w_nli=0.6) configs that are each individually valid but produce sum=1.1 after merge; verifies ConfigError::CustomWeightSumInvariant is returned.

### Fix #345: saturating_add — all 4 instances

**Status**: PASS

**Evidence**:
- `session.rs` line 267: `*count = count.saturating_add(1)` — histogram category counter
- `session.rs` line 435: `tally.count = tally.count.saturating_add(1)` — TopicTally counter
- `session.rs` line 410: `.fold(0u32, |acc, v| acc.saturating_add(v))` — topic_signals sum fold
- `search.rs` line 1043: `.fold(0u32, |acc, v| acc.saturating_add(v))` — histogram_total fold

Regression test `test_category_counter_saturates_at_u32_max` confirmed PASS.

### Fix #346: sanitize_session_id in CompactPayload

**Status**: PASS

**Evidence**: `listener.rs` lines 1096-1102:
```rust
if let Err(e) = sanitize_session_id(&session_id) {
    tracing::warn!(session_id, error = %e, "UDS: CompactPayload rejected: invalid session_id");
    return HookResponse::Error {
        code: ERR_INVALID_PAYLOAD,
        message: e,
    };
}
handle_compact_payload(...)
```
The guard is placed before `handle_compact_payload` is called. Regression test `dispatch_compact_payload_invalid_session_id_returns_error` confirmed PASS against empty string, 129-char string, and `"session/with/slash"`.

### Fix #378/#379: escape_md_cell and escape_md_text helpers

**Status**: PASS

**Evidence**:
- `escape_md_cell` (line 184): collapses newlines, escapes `|` as `\|`
- `escape_md_text` (line 194): same as escape_md_cell, plus escapes leading `#` via `trim_start().starts_with('#')` guard only — no global `#` replace

Application verified at all expected sites:
- goal field: line 208 `escape_md_text(goal)`
- sessions phase_timeline: lines 264, 270 (agent names), 279 (outcome)
- phase_timeline phase cell: line 341 `escape_md_cell(&ps.phase)`
- phase_timeline agent cells: lines 354
- phase_outliers rework section: lines 378 `escape_md_text(outcome_text)`, 393 `escape_md_cell(&ps.phase)`
- baseline_outliers: lines 596, 971-972
- cross_cycle_table: lines 1187-1188 `escape_md_cell(&c.phase)`, `escape_md_cell(&c.category)`
- knowledge_reuse intra-cycle: line 1020 `escape_md_cell(feature_cycle)`
- knowledge_reuse cross-feature: lines 1048 `escape_md_cell(&entry.title)`, 1054 `escape_md_cell(&entry.category)`, 1056 `escape_md_cell(&entry.feature_cycle)`

All 4 regression tests confirmed PASS.

### Fix #380: i64::try_from cast with unwrap_or(i64::MAX)

**Status**: PASS

**Evidence**: `tools.rs` lines 3033-3038:
```rust
// obs.ts is u64 epoch millis; clamp to i64 for comparison.
// Values above i64::MAX (year ~292 billion) saturate to i64::MAX rather than
// wrapping negative, which would silently exclude them from every phase window.
// This is a type cast (u64→i64, same unit), not a unit conversion — distinct
// from cycle_ts_to_obs_millis which converts seconds→millis.
let ts = i64::try_from(obs.ts).unwrap_or(i64::MAX);
```
Comment accurately describes the change as a type cast (not unit conversion), consistent with GH#380 root cause. Regression test `test_compute_phase_stats_obs_ts_u64_max_included_via_saturation` confirmed PASS; test directly verifies `i64::try_from(u64::MAX).unwrap_or(i64::MAX) == i64::MAX` (not -1 as the old `as i64` cast would produce).

### xpass: test_search_multihop_injects_terminal_active (GH#406)

**Status**: WARN

**Finding**: The verifier observed this test unexpectedly passing. Gate validation confirms this xpass is NOT caused by any of the 6 bug fixes:
- The only change to `search.rs` in this batch is adding `saturating_add` to the `histogram_total` fold and aligning comment indentation — no change to `find_terminal_active` traversal logic
- crt-035 (the preceding commit on this branch) also does not touch `search.rs`

GH#406 is still open. The xpass is environmental or caused by a pre-merge change in the base branch. The xfail marker at line 704 of `test_lifecycle.py` should remain. The test is non-strict xfail so it does not block the suite, but GH#406 should not be closed based on this observation alone.

### Knowledge Stewardship — rust-dev report

**Status**: PASS (recheck 2026-03-30)

**Evidence**: Fix report confirmed present at `product/features/bugfix-batch/agents/batch-337-345-346-378-379-380-agent-1-fix-report.md`.
- `## Knowledge Stewardship` block present
- `Queried:` entry: `context_briefing` results consulted (entries #3372, #3901–#3905)
- `Stored:` entry: Declined with explicit reason — standard Rust idioms (saturating_add, try_from) already codified in existing lesson entries

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — general bugfix validation procedures
- Stored: nothing novel to store — bugfix gate passes follow standard procedure; the missing-agent-report failure is noted for the protocol but is a one-off gap, not a novel systemic pattern
