# Risk Coverage Report: bugfix-523 — Server Hardening Batch

**AC-04 and AC-05 log-level assertions are behavioral-only per ADR-001(c) (Unimatrix entry #4143). Log level verified by code review. No `tracing-test` harness used.**

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Path A or Path C accidentally gated by Item 1 insertion | `test_nli_gate_path_a_informs_edges_still_written_nli_disabled`, `test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled` | PASS | Full |
| R-02 | Gate at wrong structural boundary — ADR-001 violation | Same as R-01 + code review of `// === PATH B entry gate ===` landmark | PASS | Full |
| R-03 | All 19 NaN fields not individually tested | `test_nan_guard_*` (19 tests, AC-06..AC-24) + `test_inf_guard_*` (AC-25, AC-26) | PASS | Full |
| R-04 | `sanitize_session_id` guard inserted after `event.session_id` first use | `test_dispatch_rework_candidate_invalid_session_id_rejected` (AC-28) + code inspection | PASS | Full |
| R-05 | Wrong `warn!` site downgraded in Item 2 | `test_cosine_supports_path_skips_missing_category_map_src`, `test_cosine_supports_path_nonfinite_cosine_handled` + code review | PASS | Full |
| R-06 | Entire test module absent at Gate 3b | All 30 named test functions present by inspection; count verified | PASS | Full |
| R-07 | NaN tests use wrong field name string — vacuous pass | Field name strings for AC-17..AC-24 verified against `fusion_weight_checks` / `phase_weight_checks` array | PASS | Full |
| R-08 | AC-29 regression — valid rework-candidate events rejected | `test_dispatch_rework_candidate_valid_session_id_succeeds` (AC-29) | PASS | Full |
| R-09 | AC-03 regression — NLI-enabled path broken | `test_nli_gate_nli_enabled_path_not_regressed` (AC-03) | PASS | Full |
| R-10 | AC-27 regression — existing boundary-value tests break | All 336 `infra::config` tests pass; `w_sim` boundary values confirmed | PASS | Full |
| R-11 | AC-04/AC-05 log-level coverage unacknowledged at gate | Explicit statement included in this report (see header and AC-04/AC-05 entries) | PASS | Full |
| R-12 | Cross-field invariant NaN pass-through | Covered upstream by AC-07 + AC-08 per-field guards; no additional test required | PASS | Full |

---

## Test Results

### Unit Tests

- **Total workspace**: 4530 passed; 0 failed; 0 ignored
- **Run**: `cargo test --workspace 2>&1 | tail -30`
- **Outcome**: CLEAN — no failures

#### Feature-Specific Module Results

| Module | Tests Run | Passed | Failed |
|--------|-----------|--------|--------|
| `services::nli_detection_tick` | 76 | 76 | 0 |
| `infra::config` | 336 | 336 | 0 |
| `uds::listener` | 161 | 161 | 0 |

#### New Feature Tests (30 total)

**Items 1 + 2 — `nli_detection_tick.rs` (7 tests)**

| Test Function | AC | Result |
|---|---|---|
| `test_nli_gate_path_b_skipped_nli_disabled` | AC-01 | PASS |
| `test_nli_gate_path_a_informs_edges_still_written_nli_disabled` | AC-02 (Path A) | PASS |
| `test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled` | AC-02 (Path C) | PASS |
| `test_nli_gate_nli_enabled_path_not_regressed` | AC-03 | PASS |
| `test_cosine_supports_path_skips_missing_category_map_src` | AC-04 (src branch) | PASS |
| `test_cosine_supports_path_skips_missing_category_map_tgt` | AC-04 (tgt branch) | PASS |
| `test_cosine_supports_path_nonfinite_cosine_handled` | AC-05 | PASS |

**Item 3 — `config.rs` NaN/Inf guards (21 tests)**

| Test Function | AC | Result |
|---|---|---|
| `test_nan_guard_nli_entailment_threshold` | AC-06 | PASS |
| `test_nan_guard_nli_contradiction_threshold` | AC-07 | PASS |
| `test_nan_guard_nli_auto_quarantine_threshold` | AC-08 | PASS |
| `test_nan_guard_supports_candidate_threshold` | AC-09 | PASS |
| `test_nan_guard_supports_edge_threshold` | AC-10 | PASS |
| `test_nan_guard_ppr_alpha` | AC-11 | PASS |
| `test_nan_guard_ppr_inclusion_threshold` | AC-12 | PASS |
| `test_nan_guard_ppr_blend_weight` | AC-13 | PASS |
| `test_nan_guard_nli_informs_cosine_floor` | AC-14 | PASS |
| `test_nan_guard_nli_informs_ppr_weight` | AC-15 | PASS |
| `test_nan_guard_supports_cosine_threshold` | AC-16 | PASS |
| `test_nan_guard_w_sim` | AC-17 | PASS |
| `test_nan_guard_w_nli` | AC-18 | PASS |
| `test_nan_guard_w_conf` | AC-19 | PASS |
| `test_nan_guard_w_coac` | AC-20 | PASS |
| `test_nan_guard_w_util` | AC-21 | PASS |
| `test_nan_guard_w_prov` | AC-22 | PASS |
| `test_nan_guard_w_phase_histogram` | AC-23 | PASS |
| `test_nan_guard_w_phase_explicit` | AC-24 | PASS |
| `test_inf_guard_nli_entailment_threshold_f32` | AC-25 | PASS |
| `test_inf_guard_ppr_alpha_f64` | AC-26 | PASS |

**Item 4 — `listener.rs` dispatch-arm tests (2 tests)**

| Test Function | AC | Note |
|---|---|---|
| `test_dispatch_rework_candidate_invalid_session_id_rejected` | AC-28 | PASS |
| `test_dispatch_rework_candidate_valid_session_id_succeeds` | AC-29 | PASS. Name deviation from spec: `valid_session_id_succeeds` vs `valid_path_not_regressed` — semantically equivalent, coverage complete. |

### Clippy

- **Run**: `cargo clippy --workspace -- -D warnings 2>&1 | head -30`
- **Result**: Pre-existing warning in `crates/unimatrix-engine/src/auth.rs:113` (`collapsible_if`). This file is **not modified by bugfix-523** (confirmed via `git diff 642f7439..HEAD -- crates/unimatrix-engine/src/auth.rs` — no output). Pre-existing, not caused by this batch.
- **Action**: No fix required in this PR. Filed as pre-existing. Not blocking.

### Integration Tests

- **Smoke tests (mandatory gate)**: `pytest -m smoke`
  - **Total**: 22
  - **Passed**: 22
  - **Failed**: 0
  - **Result**: PASS
  - **Run time**: 191s
- **Additional suites**: Per test-plan/OVERVIEW.md, no additional suites apply — all four items are internal server changes with no MCP-visible behavior (no new tools, no schema changes, UDS not exercised by infra-001 harness). Smoke is the correct and sufficient integration gate.

---

## Code Review Findings

### R-02 — Structural Landmark Verification (Non-Negotiable)

Confirmed via source inspection of `crates/unimatrix-server/src/services/nli_detection_tick.rs`:

- The `// === PATH B entry gate ===` comment block is present at line 546.
- The `if !config.nli_enabled { ... return; }` gate is inserted immediately after `candidate_pairs.is_empty()` fast-exit and before `nli_handle.get_provider().await`.
- The exact debug message text is `"graph inference tick: NLI disabled by config; Path B skipped"` — matches the OQ-02 prescription verbatim and is distinct from the `get_provider()` Err message `"graph inference tick: NLI provider not ready; Supports path skipped"`.
- `background.rs` is **unchanged** (confirmed: `git diff 642f7439..HEAD -- .../background.rs` produces no output). C-01 satisfied.

### R-05 / AC-05 — Non-Finite Cosine `warn!` Site (Code Review Only)

Confirmed via source inspection of `run_cosine_supports_path` (line 776):

- The non-finite cosine guard (`!cosine.is_finite()`) uses `tracing::warn!` — **unchanged**.
- Exactly two `warn!` → `debug!` changes are present in `run_cosine_supports_path`:
  1. The `category_map.get(src_id)` None arm: `"Path C: source entry not found in category_map (deprecated mid-tick?) — skipping"`
  2. The `category_map.get(tgt_id)` None arm: `"Path C: target entry not found in category_map (deprecated mid-tick?) — skipping"`
- No other sites in `run_cosine_supports_path` were modified.

**Log level for non-finite cosine site verified by code review to be unchanged. Exactly two `warn!` → `debug!` changes in `run_cosine_supports_path`.**

### R-04 — Item 4 Insertion Order Verification

Confirmed via source inspection of `dispatch_request` in `listener.rs` for the `post_tool_use_rework_candidate` arm:

1. Capability check (`uds_has_capability(Capability::SessionWrite)`) — existing, line ~660
2. `sanitize_session_id(&event.session_id)` guard — **NEW (bugfix-523)**
3. `event.payload.get("tool_name")` extraction — existing
4. `session_registry.record_rework_event(...)` — existing

**No use of `event.session_id` appears between the capability check (step 1) and the guard (step 2). `ERR_INVALID_PAYLOAD` is the error code. Warn message contains `"(rework_candidate)"`. Item 4 insertion order verified by code inspection.**

### R-07 — Field Name Spot-Check for Loop-Group Fields (AC-17..AC-24)

Confirmed via source inspection of `InferenceConfig::validate()`:

`fusion_weight_checks` array `&'static str` entries (exact): `"w_sim"`, `"w_nli"`, `"w_conf"`, `"w_coac"`, `"w_util"`, `"w_prov"`.

`phase_weight_checks` array `&'static str` entries (exact): `"w_phase_histogram"`, `"w_phase_explicit"`.

Test strings in AC-17..AC-24 match these exactly. The `!value.is_finite()` guard is present in both loop bodies. `value` is type `f64` (not `&f64`) per the array declaration `&[(&'static str, f64)]` — auto-deref not required. Guard form is correct.

**Field name strings for AC-17..AC-24 verified against `fusion_weight_checks` / `phase_weight_checks` array entries. No vacuous pass risk.**

### Edge Cases Noted

- `f32::NEG_INFINITY` and `f64::NEG_INFINITY` are also caught by `!v.is_finite()`. The two representative Inf tests (AC-25, AC-26) use positive infinity; no separate negative-infinity tests are required as `is_finite()` is sign-agnostic.
- The `nli_entailment_threshold` test verifies a field in Group A (inline guard pattern). The `w_sim` test verifies a field in Group B (loop guard pattern). The `w_phase_histogram` test verifies a field in Group C (phase loop guard pattern). All three guard forms are independently exercised.

---

## Gaps

**None.** All 12 risks from RISK-TEST-STRATEGY.md have full test coverage or code-review coverage (as specified for log-level risks per ADR-001(c)).

R-12 (cross-field invariant NaN pass-through) is mitigated upstream by AC-07 and AC-08 per-field guards, exactly as documented in RISK-TEST-STRATEGY.md. No additional test is required.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_nli_gate_path_b_skipped_nli_disabled` — non-empty candidate_pairs used; no NLI Supports edges written when `nli_enabled=false`. |
| AC-02 | PASS | `test_nli_gate_path_a_informs_edges_still_written_nli_disabled` (Path A) AND `test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled` (Path C) — both required, both pass. |
| AC-03 | PASS | `test_nli_gate_nli_enabled_path_not_regressed` — gate condition is `if !nli_enabled`; test with `nli_enabled=true` confirms gate does not fire and provider path executes. |
| AC-04 | PASS (behavioral-only) | `test_cosine_supports_path_skips_missing_category_map_src` and `test_cosine_supports_path_skips_missing_category_map_tgt` — pair skipped, no panic. Log level (`debug!`) verified by code review only per ADR-001(c) (entry #4143). |
| AC-05 | PASS (behavioral-only) | `test_cosine_supports_path_nonfinite_cosine_handled` — pair skipped, no panic. Non-finite cosine site remains `tracing::warn!` — verified by code review. Per ADR-001(c) (entry #4143). |
| AC-06 | PASS | `test_nan_guard_nli_entailment_threshold` |
| AC-07 | PASS | `test_nan_guard_nli_contradiction_threshold` |
| AC-08 | PASS | `test_nan_guard_nli_auto_quarantine_threshold` |
| AC-09 | PASS | `test_nan_guard_supports_candidate_threshold` |
| AC-10 | PASS | `test_nan_guard_supports_edge_threshold` |
| AC-11 | PASS | `test_nan_guard_ppr_alpha` |
| AC-12 | PASS | `test_nan_guard_ppr_inclusion_threshold` |
| AC-13 | PASS | `test_nan_guard_ppr_blend_weight` |
| AC-14 | PASS | `test_nan_guard_nli_informs_cosine_floor` |
| AC-15 | PASS | `test_nan_guard_nli_informs_ppr_weight` |
| AC-16 | PASS | `test_nan_guard_supports_cosine_threshold` |
| AC-17 | PASS | `test_nan_guard_w_sim` — field name `"w_sim"` verified against `fusion_weight_checks` array. |
| AC-18 | PASS | `test_nan_guard_w_nli` |
| AC-19 | PASS | `test_nan_guard_w_conf` |
| AC-20 | PASS | `test_nan_guard_w_coac` |
| AC-21 | PASS | `test_nan_guard_w_util` |
| AC-22 | PASS | `test_nan_guard_w_prov` |
| AC-23 | PASS | `test_nan_guard_w_phase_histogram` — field name `"w_phase_histogram"` verified against `phase_weight_checks` array. |
| AC-24 | PASS | `test_nan_guard_w_phase_explicit` |
| AC-25 | PASS | `test_inf_guard_nli_entailment_threshold_f32` — representative f32 Inf test. `NEG_INFINITY` also caught by `!v.is_finite()`; no additional test required. |
| AC-26 | PASS | `test_inf_guard_ppr_alpha_f64` — representative f64 Inf test. |
| AC-27 | PASS | All 336 `infra::config` tests pass; `w_sim` boundary-value tests (0.0, 0.5 valid; -0.1, 1.1 invalid) confirmed passing. Loop-body dereference form correct. |
| AC-28 | PASS | `test_dispatch_rework_candidate_invalid_session_id_rejected` — `"../../etc/passwd"` rejected with `ERR_INVALID_PAYLOAD`; registry call not reached. Code inspection confirms guard placement. |
| AC-29 | PASS | `test_dispatch_rework_candidate_valid_session_id_succeeds` — valid `"session-abc123"` proceeds to `record_rework_event`. (Note: implemented as `valid_session_id_succeeds` vs specified name `valid_path_not_regressed`; semantically equivalent — coverage complete.) |

---

## Test Count Summary

| Category | New | Pre-existing | Total | Pass | Fail |
|----------|-----|-------------|-------|------|------|
| Items 1+2 unit tests (nli_detection_tick) | 7 | 69 | 76 | 76 | 0 |
| Item 3 unit tests (config NaN/Inf) | 21 | 315 | 336 | 336 | 0 |
| Item 4 unit tests (listener dispatch) | 2 | 159 | 161 | 161 | 0 |
| Integration smoke | 0 | 22 | 22 | 22 | 0 |
| **Workspace total** | **30** | **4500** | **4530+22** | **4552** | **0** |

---

## GH Issues Filed

None. No integration test failures were encountered. No pre-existing failures were exposed by this batch.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entry #4143 (bugfix-523 ADR-001, authoritative behavioral-only log-level decision), #3766 (InferenceConfig NaN lesson from bugfix-444), #238 (test infrastructure is cumulative convention), #3918 (lifecycle integration XPASS lesson), #3927 (missing agent reports pattern). All directly applicable.
- Stored: nothing novel to store — the behavioral-only log-level test pattern is captured in entry #4143 and #3935. The NaN guard test pattern is in #4133. The dispatch-arm guard insertion pattern is in #3921/#4141. No new cross-feature patterns emerged from this execution.
