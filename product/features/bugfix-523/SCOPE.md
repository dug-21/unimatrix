# Hardening Batch: NLI Tick Gate + Log Downgrade + NaN Guards + Session Sanitization

## Problem Statement

Four independent hardening defects were discovered across `unimatrix-server` in the areas of background tick control flow, operational observability noise, config validation completeness, and UDS security guard coverage. Each is narrow and non-breaking, but left unaddressed they represent operational risk (tick congestion under NLI load), log signal degradation (warn spam obscures real errors), silent NaN propagation into scoring pipelines, and a session injection gap in the rework-candidate dispatch arm. GH #523 consolidates all four into a single deliverable with full design + test methodology as requested.

## Goals

1. Gate Path B (NLI Supports, Phases 6/7/8) behind an explicit `config.nli_enabled` check inside `run_graph_inference_tick`, so the async `get_provider()` call and rayon dispatch are skipped entirely when `nli_enabled = false` (the production default), eliminating the observed 353-second background tick.
2. Downgrade the `tracing::warn!` to `tracing::debug!` in Path C's `run_cosine_supports_path` when a candidate entry is absent from `category_map` (deprecated mid-tick), eliminating 40–50 noisy warn lines per tick that obscure real signal.
3. Add `!v.is_finite()` prefix guards to all float fields in `InferenceConfig::validate()` that currently use comparison-only guards — including the 6 fusion weight fields and 2 phase weight fields — so NaN and ±Inf are caught at server startup rather than propagating silently into scoring formulas (19 fields total).
4. Add `sanitize_session_id` guard at the top of the `post_tool_use_rework_candidate` arm in `dispatch_request`, matching the pattern applied to every other UDS arm that consumes `session_id`.

## Non-Goals

- Removing Path B code. NLI Supports detection remains in the codebase for future reactivation when a domain-adapted model is available. This fix gates, not deletes.
- Re-introducing the outer `if inference_config.nli_enabled` gate removed from `background.rs` at line 775 (crt-039 FR-01, ADR-001). The caller stays unconditional; the gate moves inside `run_graph_inference_tick` to preserve Phase A (structural Informs) running on every tick.
- Adding `is_finite()` guards to `RetentionConfig`, `CoherenceConfig`, or any config struct other than `InferenceConfig`. Scope is `InferenceConfig::validate()` only.
- Changing the `NliServiceHandle::get_provider()` interface or the `NliServiceHandle` initialization path.
- Any schema change, new MCP tool, or API surface change.
- Fixing pre-existing open issues (#452, #303, #305).

## Background Research

### Item 1: NLI Tick Gate

**File**: `crates/unimatrix-server/src/services/nli_detection_tick.rs`, Path B entry gate (~line 546).

**Current state**: `run_graph_inference_tick` is called unconditionally from `background.rs` line 776 (comment at 775 explains crt-039 ADR-001 removed the outer gate). Inside the function, Path B is reached after Phase A and Path C complete. The current implicit gate is `nli_handle.get_provider().await`, which returns `Err(NliNotReady)` immediately when `nli_enabled = false`. Comment at line 563 confirms this. However: the function still enters, runs Phase 2 DB reads (three `await` calls), Phase 4b HNSW scan, Path A Informs write loop, Path C cosine Supports write loop, and only then calls `get_provider()`. Under NLI load (model loaded), a 353-second tick was observed due to rayon pool congestion.

**Fix location**: Add `if !config.nli_enabled { return; }` immediately before the `get_provider()` call at the PATH B entry gate (~line 546), after Path C completes. This short-circuits the async call and prevents rayon spawn when NLI is disabled. Path A (Informs) and Path C (cosine Supports) continue to run unconditionally — only the NLI rayon dispatch is skipped.

**ADR context**: crt-039 ADR-001 (entry #4017) explicitly removed the outer gate in `background.rs` to keep Phase 4b running unconditionally. The fix proposed here moves the gate inside `run_graph_inference_tick` at the Path B boundary, which is consistent with ADR-001's intent: Phase A and Path C are still unconditional; only Path B is gated.

**Nuance confirmed**: The issue description says "ignoring the `nli_enabled` config flag" — this is accurate in the sense that there is no explicit `nli_enabled` check; the current Err-return from `get_provider()` is an implicit gate. The fix makes it explicit and avoids the async call entirely.

### Item 2: Deprecated-Entry Log Downgrade

**File**: `crates/unimatrix-server/src/services/nli_detection_tick.rs`, function `run_cosine_supports_path`.

**Current state**: Lines 796–800 and 806–810 emit `tracing::warn!` when `category_map.get(src_id)` or `category_map.get(tgt_id)` returns `None`. The comment at line 792 explains: "If an entry was deprecated between Phase 2 DB read and this point, it will be absent." This is not an error — it is expected behavior in the HNSW-plus-DB architecture where HNSW is rebuilt on compaction cycles, not on every deprecation. Issue #508 reports 40–50 of these warn lines per tick from the same entry IDs (3909, 3947, 3960, 3961, etc.).

**Issue description vs. code**: The issue description says this warn is in `nli_detection_tick.rs` and attributes it to "cosine candidate not found in category_map". The code confirms this is in `run_cosine_supports_path` (Path C), not Path B. The issue description's reference to "Path B" in Item 1 and "cosine candidate" in Item 2 is internally consistent: Path B uses NLI; the warn fires in Path C (pure cosine). No discrepancy.

**Fix**: Change the two `tracing::warn!` calls at lines 796 and 806 to `tracing::debug!`. One-line change each. The finite-cosine warn at line 766 is a structural anomaly (NaN from HNSW), not an expected condition — that one stays as `warn!`.

### Item 3: InferenceConfig NaN Guards

**File**: `crates/unimatrix-server/src/infra/config.rs`, `InferenceConfig::validate()` (~lines 997–1414).

**Current state**: The three crt-046 fields added last (`goal_cluster_similarity_threshold`, `w_goal_cluster_conf`, `w_goal_boost`) already have `!v.is_finite()` guards (PR #516, lines 1382, 1393, 1404). Lesson #4132 documents this pattern and the NaN trap. All earlier float fields use comparison-only guards that silently pass NaN due to IEEE 754 behavior (`NaN <= 0.0` is false; `NaN >= 1.0` is false).

**Fields requiring `!v.is_finite()` prefix** (confirmed by reading lines 1026–1309):
- `nli_entailment_threshold` (f32, line 1028) — guard: `<= 0.0 || >= 1.0`
- `nli_contradiction_threshold` (f32, line 1037) — guard: `<= 0.0 || >= 1.0`
- `nli_auto_quarantine_threshold` (f32, line 1046) — guard: `<= 0.0 || >= 1.0`
- `supports_candidate_threshold` (f32, line 1089) — guard: `<= 0.0 || >= 1.0`
- `supports_edge_threshold` (f32, line 1099) — guard: `<= 0.0 || >= 1.0`
- `ppr_alpha` (f64, line 1221) — guard: `<= 0.0 || >= 1.0`
- `ppr_inclusion_threshold` (f64, line 1241) — guard: `<= 0.0 || >= 1.0`
- `ppr_blend_weight` (f64, line 1251) — guard: `< 0.0 || > 1.0`
- `nli_informs_cosine_floor` (f32, line 1282) — guard: `<= 0.0 || >= 1.0`
- `nli_informs_ppr_weight` (f64, line 1292) — guard: `< 0.0 || > 1.0`
- `supports_cosine_threshold` (f32, line 1302) — guard: `<= 0.0 || >= 1.0`

That is 11 fields. The issue says "8+" — confirmed in range.

**Additional fields — fusion and phase weights (in scope per OQ-01 resolution)**: The 6 fusion weight fields (`w_sim`, `w_nli`, `w_conf`, `w_coac`, `w_util`, `w_prov`, all f64, lines 1160–1168) and 2 phase weight fields (`w_phase_histogram`, `w_phase_explicit`, f64, lines 1178–1186) are included. Rationale: the existing sum check catches NaN indirectly but produces a misleading error ("weights don't sum to 1.0" instead of "w_sim is not finite"); a NaN fusion weight silently corrupts every search result until restart. All 8 fields get the same `!v.is_finite() || ` prefix treatment. **Total: 19 fields.**

**Cross-field invariants**: The cross-field invariant checks (`nli_auto_quarantine_threshold <= nli_contradiction_threshold`, `supports_candidate_threshold >= supports_edge_threshold`) are comparisons between two NaN fields; NaN comparisons return false so they would not fire even if both fields were NaN. Adding `is_finite()` to the individual per-field checks above is sufficient to catch NaN at the individual field level before reaching cross-field checks.

**Established pattern** (PR #516, lesson #4132): `!v.is_finite() || <existing comparison>`. No new error variant needed — use existing `ConfigError::NliFieldOutOfRange`.

### Item 4: UDS sanitize_session_id Gap

**File**: `crates/unimatrix-server/src/uds/listener.rs`, function `dispatch_request`.

**Current state**: The `post_tool_use_rework_candidate` arm (lines 656–718) extracts `session_id` from `event.session_id` at line 690 (`session_registry.record_rework_event(&event.session_id, ...)`) and line 694 (`session_registry.record_topic_signal(&event.session_id, ...)`) without a prior `sanitize_session_id` call.

**Confirmed guards in other arms**:
- `SessionRegister` arm: line 545
- `SessionClose` arm: line 629
- `RecordEvent` (general) arm: line 731 (added in PR #521, GH #519)
- `RecordEvents` batch arm: line 863 (added in PR #521)
- `ContextSearch` arm: line 998
- `CompactPayload` arm: line 1162

The `post_tool_use_rework_candidate` arm is indeed the last arm without this guard, as described in issue #523 and pattern entry #3921.

**Fix**: Add the guard immediately after the capability check at line 660, before any `event.session_id` is passed to registry methods. Return `HookResponse::Error { code: ERR_INVALID_PAYLOAD, message: e }` on failure, same as all other arms. The session_id referenced here comes from `event.session_id` (the `HookEvent` struct field), not a destructured local.

**Existing tests**: `sanitize_session_id` unit tests exist at lines 3833–3880 covering the function itself. No existing test covers the `post_tool_use_rework_candidate` dispatch arm receiving an invalid session_id — this is the coverage gap to fill.

### Test Coverage Analysis

| Item | Existing tests | Gap |
|------|---------------|-----|
| 1 (NLI gate) | `test_path_c_runs_unconditionally_nli_disabled` (TC-05, line 2762) covers Path C with `nli_enabled=false`. No test explicitly verifies Path B is skipped via the `nli_enabled` flag (vs. `get_provider()` Err). | Need: test that Path B is NOT entered when `nli_enabled=false` even when a provider would be available. |
| 2 (log downgrade) | No test for log level at the category_map miss site. | Need: unit test for `run_cosine_supports_path` with a candidate whose IDs are absent from `category_map`. At minimum confirm the skip behavior; log level is not easily unit-tested but the skip logic (continue) is. |
| 3 (NaN guards) | Tests exist for boundary values (0.0, 1.0, -0.1) for each affected field. NaN tests exist only for crt-046 fields (lines 8004–8094). | Need: NaN test case for each of the 11 affected fields following the `assert_validate_fails_with_field(c, "field_name")` pattern. |
| 4 (sanitize_session_id) | `sanitize_session_id` function unit tests exist (lines 3833–3880). No test covers the `post_tool_use_rework_candidate` arm with an invalid session_id. | Need: dispatch-level test for the rework arm with a malformed session_id returning `HookResponse::Error { code: ERR_INVALID_PAYLOAD }`. |

## Proposed Approach

All four fixes are surgical single-file changes. No new dependencies, no schema changes, no API changes.

**Item 1 — NLI tick gate**: In `run_graph_inference_tick`, insert `if !config.nli_enabled { return; }` at the PATH B entry gate (after Path C completes, before the `get_provider()` call at ~line 560). Emit `tracing::debug!("graph inference tick: NLI disabled by config; Path B skipped")` on early return — distinct from the existing `get_provider()` Err message ("NLI provider not ready; Supports path skipped") so operators can differentiate intentional-off from transient-not-ready. Update the comment at line 563 to reference the explicit check rather than the implicit one.

**Item 2 — Log downgrade**: Replace `tracing::warn!` with `tracing::debug!` at lines 796 and 806 in `run_cosine_supports_path`. The existing comment at line 792 already explains why this is expected. No other changes.

**Item 3 — NaN guards**: For each of the 19 fields (11 threshold fields + 6 fusion weights + 2 phase weights), prefix the existing guard condition with `!v.is_finite() || `. The `value.to_string()` in the error already represents NaN as "NaN", which is a valid debug string. No new error variants. Update the comment block at line 1026 to document the NaN guard requirement.

**Item 4 — sanitize_session_id**: In the `post_tool_use_rework_candidate` arm, after the capability check block, add:
```rust
if let Err(e) = sanitize_session_id(&event.session_id) {
    tracing::warn!(session_id = %event.session_id, error = %e, "UDS: RecordEvent (rework_candidate) rejected: invalid session_id");
    return HookResponse::Error {
        code: ERR_INVALID_PAYLOAD,
        message: e,
    };
}
```
This is the identical pattern used in the `RecordEvent` general arm (lines 731–738).

## Acceptance Criteria

- AC-01: When `inference_config.nli_enabled = false`, `run_graph_inference_tick` returns after Path C without calling `nli_handle.get_provider()` or spawning any rayon task.
- AC-02: When `inference_config.nli_enabled = false`, Path A (Informs writes) and Path C (cosine Supports writes) are NOT affected — they continue to run and write edges as normal.
- AC-03: When `inference_config.nli_enabled = true` and a provider is available, Phases 6/7/8 continue to execute (no regression in NLI-enabled path).
- AC-04: `run_cosine_supports_path` with a candidate pair whose source or target ID is absent from `category_map` emits `tracing::debug!` (not `warn!`) and skips the pair.
- AC-05: `run_cosine_supports_path` with a non-finite cosine value continues to emit `tracing::warn!` (the non-finite cosine guard at line 766 is unchanged).
- AC-06: `InferenceConfig::validate()` returns `Err(ConfigError::NliFieldOutOfRange { field: "nli_entailment_threshold", ... })` when `nli_entailment_threshold = f32::NAN`.
- AC-07: `InferenceConfig::validate()` returns `Err(ConfigError::NliFieldOutOfRange { field: "nli_contradiction_threshold", ... })` when `nli_contradiction_threshold = f32::NAN`.
- AC-08: `InferenceConfig::validate()` returns `Err(ConfigError::NliFieldOutOfRange { field: "nli_auto_quarantine_threshold", ... })` when `nli_auto_quarantine_threshold = f32::NAN`.
- AC-09: `InferenceConfig::validate()` returns `Err` when `supports_candidate_threshold = f32::NAN`.
- AC-10: `InferenceConfig::validate()` returns `Err` when `supports_edge_threshold = f32::NAN`.
- AC-11: `InferenceConfig::validate()` returns `Err` when `ppr_alpha = f64::NAN`.
- AC-12: `InferenceConfig::validate()` returns `Err` when `ppr_inclusion_threshold = f64::NAN`.
- AC-13: `InferenceConfig::validate()` returns `Err` when `ppr_blend_weight = f64::NAN`.
- AC-14: `InferenceConfig::validate()` returns `Err` when `nli_informs_cosine_floor = f32::NAN`.
- AC-15: `InferenceConfig::validate()` returns `Err` when `nli_informs_ppr_weight = f64::NAN`.
- AC-16: `InferenceConfig::validate()` returns `Err` when `supports_cosine_threshold = f32::NAN`.
- AC-17: `InferenceConfig::validate()` returns `Err` when `w_sim = f64::NAN`.
- AC-18: `InferenceConfig::validate()` returns `Err` when `w_nli = f64::NAN`.
- AC-19: `InferenceConfig::validate()` returns `Err` when `w_conf = f64::NAN`.
- AC-20: `InferenceConfig::validate()` returns `Err` when `w_coac = f64::NAN`.
- AC-21: `InferenceConfig::validate()` returns `Err` when `w_util = f64::NAN`.
- AC-22: `InferenceConfig::validate()` returns `Err` when `w_prov = f64::NAN`.
- AC-23: `InferenceConfig::validate()` returns `Err` when `w_phase_histogram = f64::NAN`.
- AC-24: `InferenceConfig::validate()` returns `Err` when `w_phase_explicit = f64::NAN`.
- AC-25: `InferenceConfig::validate()` returns `Err` when `nli_entailment_threshold = f32::INFINITY` (representative f32 Inf test).
- AC-26: `InferenceConfig::validate()` returns `Err` when `ppr_alpha = f64::INFINITY` (representative f64 Inf test).
- AC-27: All existing `InferenceConfig::validate()` tests continue to pass — no regression on boundary values.
- AC-28: `dispatch_request` with a `HookRequest::RecordEvent { event }` where `event.event_type == "post_tool_use_rework_candidate"` and `event.session_id` contains an invalid character returns `HookResponse::Error { code: ERR_INVALID_PAYLOAD, message: _ }` without calling `session_registry.record_rework_event`.
- AC-29: `dispatch_request` with a valid `post_tool_use_rework_candidate` event continues to call `session_registry.record_rework_event` (no regression on the normal path).

## Constraints

- **crt-039 ADR-001 (entry #4017)**: The outer `run_graph_inference_tick` call in `background.rs` must remain unconditional. Phase A and Path C must not be gated. The NLI gate must be inserted inside `run_graph_inference_tick` at the Path B boundary only.
- **W1-2 contract**: All `score_batch` calls must go through `rayon_pool.spawn()`. This is unaffected by the gate; the rayon spawn is skipped when `nli_enabled=false`.
- **`ConfigError::NliFieldOutOfRange` is the established variant** for `InferenceConfig` field errors (lesson #4132). No new error variants.
- **ERR_INVALID_PAYLOAD** is the established error code for session_id validation failures in UDS dispatch arms. Use it.
- **`assert_validate_fails_with_field` helper** (line 4615 in config.rs) checks `err.to_string().contains(field_name)`. All new NaN tests must use this helper with the exact field name string.
- **Log level semantic contract** (ADR entry #3467): operational anomalies use `warn!`; expected degraded-mode behavior uses `debug!`. The category_map miss is expected — it belongs at `debug!`.
- **Max 500 lines per file** (rust-workspace.md): config.rs is already a large file (~8000+ lines). New tests go in the existing `#[cfg(test)]` module within config.rs following the established test block pattern.
- **No `is_finite()` on integer fields**: `usize` and `u32` fields are not subject to IEEE 754 NaN. The 19 fields listed above are the complete target set.

## Open Questions

All open questions resolved:

- **OQ-01 RESOLVED**: Fusion and phase weight fields included. 19 total fields (11 threshold + 6 fusion weights + 2 phase weights). Rationale: NaN fusion weight silently corrupts every search result; misleading "weights don't sum to 1.0" error is worse than no error.
- **OQ-02 RESOLVED**: Emit distinct `debug!` message: "graph inference tick: NLI disabled by config; Path B skipped". Different from the `get_provider()` Err message so operators can distinguish intentional-off from transient-not-ready.
- **OQ-03 RESOLVED**: All 19 fields get a NaN test. One representative f32::INFINITY test (AC-25, `nli_entailment_threshold`) and one representative f64::INFINITY test (AC-26, `ppr_alpha`). No need to repeat for all 19.
- **OQ-04 RESOLVED**: PR #521 merged, #519 closed. RecordEvent (line 731) and RecordEvents (line 863) covered. Rework candidate arm gap is isolated to lines 656–718. No overlap.

## Tracking

GH Issue: #523
