# Gate 3c Report: bugfix-523

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-04-05
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 12 risks traced to passing tests in RISK-COVERAGE-REPORT.md |
| Test coverage completeness | PASS | 30 new tests; all risk-to-scenario mappings exercised |
| Specification compliance | PASS | All 29 ACs verified (AC-04/AC-05 behavioral-only per ADR-001(c), entry #4143) |
| Architecture compliance | PASS | Gate placement, background.rs unconditional call, and log-level semantics all confirmed |
| Integration smoke tests | PASS | 22/22 smoke tests passed; no xfail markers; no deletions |
| Knowledge stewardship | PASS | Tester report contains Queried + Stored entries |
| nli_informs_ppr_weight type (f32 vs f64 in spec) | WARN | Spec/architecture document this as f64; actual struct is f32. Test uses f32::NAN and is correct relative to implementation. Guard works correctly. Documentation error in spec only. |

---

## Detailed Findings

### 1. Risk Mitigation Proof

**Status**: PASS

**Evidence**: RISK-COVERAGE-REPORT.md maps all 12 risks (R-01 through R-12) to passing tests or acknowledged code-review coverage:

- R-01/R-02 (gate position): `test_nli_gate_path_a_informs_edges_still_written_nli_disabled` and `test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled` pass, confirming Path A and Path C are unconditional when `nli_enabled=false`. Structural landmark `// === PATH B entry gate ===` at line 546 confirmed by code review.
- R-03 (19-field NaN coverage): All 19 `test_nan_guard_*` tests present and passing. 2 representative Inf tests (AC-25, AC-26) also present.
- R-04 (guard placement): `test_dispatch_rework_candidate_invalid_session_id_rejected` passes. Code inspection confirms guard at step 2 (after capability check, before payload extraction). No use of `event.session_id` between capability check and guard.
- R-05 (wrong warn site): Exactly two `warn!` → `debug!` changes in `run_cosine_supports_path` (both `category_map` miss sites). Non-finite cosine guard at line 776 remains `warn!`. Confirmed by code review and behavioral test `test_cosine_supports_path_nonfinite_cosine_handled`.
- R-06 (missing tests): 30 new tests present. Count verified: 7 (NLI tick + log), 21 (NaN/Inf), 2 (session sanitization).
- R-07 (vacuous pass): `fusion_weight_checks` array entries (`"w_sim"`, `"w_nli"`, `"w_conf"`, `"w_coac"`, `"w_util"`, `"w_prov"`) and `phase_weight_checks` entries (`"w_phase_histogram"`, `"w_phase_explicit"`) verified against test strings. Exact match confirmed.
- R-08 (AC-29 regression): `test_dispatch_rework_candidate_valid_session_id_succeeds` passes.
- R-09 (AC-03 regression): `test_nli_gate_nli_enabled_path_not_regressed` passes.
- R-10 (AC-27 regression): All 336 `infra::config` tests pass; `w_sim` boundary values confirmed.
- R-11 (log-level acknowledgment): RISK-COVERAGE-REPORT header and AC-04/AC-05 entries both cite ADR-001(c) (entry #4143) explicitly.
- R-12 (cross-field NaN): Mitigated upstream by AC-07 + AC-08 per-field guards, as documented.

### 2. Test Coverage Completeness

**Status**: PASS

**Evidence**: All risk-to-scenario mappings from RISK-TEST-STRATEGY.md are exercised:

- Items 1+2 (nli_detection_tick): 7 new tests + 69 pre-existing = 76 total, all passing.
- Item 3 (config NaN/Inf): 21 new tests + 315 pre-existing = 336 total, all passing.
- Item 4 (listener dispatch): 2 new tests + 159 pre-existing = 161 total, all passing.
- Integration smoke: 22/22 passed in 191s. No xfail markers. No integration tests deleted or commented out.
- No additional integration suites required — all four items are internal server changes with no MCP-visible behavior (no new tools, no schema changes, UDS not exercised by infra-001 harness).
- Workspace total: 4530 passed, 0 failed (confirmed by running `cargo test --workspace`).

**Test naming deviation noted (non-blocking)**: AC-29 test delivered as `test_dispatch_rework_candidate_valid_session_id_succeeds` vs. specified name `test_dispatch_rework_candidate_valid_path_not_regressed`. Semantically equivalent; coverage complete; documented in RISK-COVERAGE-REPORT.md.

### 3. Specification Compliance

**Status**: PASS

**Evidence**: All 29 ACs verified:

- AC-01 through AC-03 (NLI gate behavior): PASS — gate is correctly placed after `candidate_pairs.is_empty()` fast-exit and before `get_provider().await` (confirmed at lines 552/561/571 of `nli_detection_tick.rs`). Debug message text matches FR-01 prescription verbatim.
- AC-04/AC-05 (log level): PASS (behavioral-only) per ADR-001(c) (entry #4143). Log level verified by code review. `tracing-test` harness not used, as documented in IMPLEMENTATION-BRIEF.
- AC-06 through AC-24 (all 19 NaN guards): PASS — all 19 `test_nan_guard_*` functions present and passing.
- AC-25/AC-26 (Inf guards): PASS — `test_inf_guard_nli_entailment_threshold_f32` and `test_inf_guard_ppr_alpha_f64` present and passing.
- AC-27 (no regression on existing tests): PASS — 336 `infra::config` tests all pass.
- AC-28 (invalid session_id rejected): PASS — `test_dispatch_rework_candidate_invalid_session_id_rejected` uses `"../../etc/passwd"` and asserts `ERR_INVALID_PAYLOAD` with no registry call.
- AC-29 (valid session_id succeeds): PASS — `test_dispatch_rework_candidate_valid_session_id_succeeds` passes.

**FR compliance**:
- FR-01 (NLI gate): Guard inserted. Debug message prescribed text confirmed. C-01 (background.rs unconditional) verified — `background.rs` is unchanged in git diff.
- FR-02 (log downgrade): Exactly two `warn!` → `debug!` changes in `run_cosine_supports_path`. Non-finite cosine guard unchanged as `warn!`.
- FR-03 (NaN guards): All 19 fields updated with `!v.is_finite()` prefix. Loop guard patterns (Group B, Group C) and inline guards (Group A) all verified.
- FR-04 (sanitize_session_id guard): Guard inserted after capability check (line 666 area) and before `event.payload.get("tool_name")`. `ERR_INVALID_PAYLOAD` used. Warn message contains `"(rework_candidate)"`.

**NFR compliance**:
- NFR-01 (no Path A/C regression): Verified by AC-02 tests.
- NFR-02 (NaN caught before first use): validate() called at startup, 19 fields all guarded.
- NFR-03 (synchronous O(1)): sanitize_session_id is unchanged, existing contract.
- NFR-04 (no new dependencies): git diff confirms no Cargo.toml changes.
- NFR-05 (no regression): 4530 tests pass, 0 failures.

### 4. Architecture Compliance

**Status**: PASS

**Evidence**:

- Item 1 gate position: The `// === PATH B entry gate ===` landmark is at line 546. Ordering is: (1) `candidate_pairs.is_empty()` fast-exit, (2) `!config.nli_enabled` explicit gate, (3) `get_provider().await`. This is exactly the sequence prescribed by the architecture.
- ADR-001 compliance: Phase A (Informs) and Path C (cosine Supports) execute before the gate. Both are unconditional. `background.rs` outer call is unconditional (verified via git diff producing no output for that file).
- Item 2 log level semantics: The architecture's log level contract (ADR entry #3467 — operational anomalies use `warn!`; expected degraded-mode behavior uses `debug!`) is preserved. Category_map misses (expected) are now `debug!`. Non-finite cosine (structural anomaly) remains `warn!`.
- Item 3 no new error variants: `ConfigError::NliFieldOutOfRange` is the sole variant for all 19 field errors, per C-04.
- Item 4 insertion order: Matches the architecture's prescribed sequence (capability check → sanitize_session_id → payload extraction → registry call).
- Component isolation: All four items remain in their designated single files. No changes outside the three target files.
- No architecture drift: No new abstractions, no schema changes, no new MCP tools, no new dependencies.

### 5. Integration Smoke Tests

**Status**: PASS

**Evidence**: From RISK-COVERAGE-REPORT.md and tester agent report:
- Command: `pytest -m smoke`
- 22/22 passed in 191s, 0 failed
- No xfail markers introduced
- No integration tests deleted or commented out
- No additional suites required for this feature (all changes are internal, no MCP-visible behavior changes)

### 6. Knowledge Stewardship (Tester Agent)

**Status**: PASS

**Evidence**: `bugfix-523-agent-6-tester-report.md` contains a `## Knowledge Stewardship` section with:
- Queried: `mcp__unimatrix__context_briefing` — returned entries #4143, #3766, #238, #3918, #3927. All applicable.
- Stored: "nothing novel to store — behavioral-only log-level pattern (#4143/#3935), NaN guard pattern (#4133), dispatch-arm guard pattern (#3921/#4141) are all already captured. No new cross-feature patterns from this execution." Reason for not storing is specified.

### 7. nli_informs_ppr_weight Type Discrepancy (WARN)

**Status**: WARN

**Evidence**: The specification (SPECIFICATION.md line 125, field checklist row 10) and the architecture (ARCHITECTURE.md line 136, Group A field list) both describe `nli_informs_ppr_weight` as `f64`. The actual struct definition in `config.rs` declares it as `f32`:

```rust
pub nli_informs_ppr_weight: f32,
```

The test `test_nan_guard_nli_informs_ppr_weight` correctly uses `f32::NAN`, matching the actual field type. The `!v.is_finite()` guard in `validate()` is applied to the `f32` value and functions correctly. The NaN test passes and provides real coverage.

**This is a documentation error in the spec/architecture, not a code defect.** The implementation and test are internally consistent. The guard works correctly. No runtime risk.

**Non-blocking**: The behavior produced is correct. The spec/architecture documentation should be corrected in a future pass but does not block this PR.

---

## Integration Test Notes

- AC-04/AC-05 log-level assertions are behavioral-only per ADR-001(c) (Unimatrix entry #4143). Log level verified by code review. No `tracing-test` harness used. This is an explicit architectural decision documented in the IMPLEMENTATION-BRIEF and RISK-COVERAGE-REPORT.
- Non-finite cosine site at `run_cosine_supports_path` line 776 verified by code review to remain `warn!`. Per R-05 / RISK-TEST-STRATEGY.md, code review is the only available mechanism for this verification.
- R-12 (cross-field invariant NaN pass-through) is mitigated upstream by AC-07 + AC-08 per-field guards. No additional test required per RISK-TEST-STRATEGY.md.
- `background.rs` unchanged — confirmed via `git diff 642f7439..HEAD -- crates/unimatrix-server/src/background.rs` producing no output.

---

## Gaps

None. All 12 risks have full coverage. All 29 ACs are verified. All 30 new tests present and passing. 22/22 smoke tests passed.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` prior to gate — patterns for NaN guard validation, behavioral-only log-level test documentation, and session sanitization guard confirmed in existing entries #4143, #4133, #3921.
- Stored: nothing novel to store — all gate patterns observed here (behavioral-only log-level coverage, NaN guard test structure, dispatch-arm insertion verification) are already captured in Unimatrix. The spec type documentation error (f32 vs f64) is a one-off feature-specific finding, not a recurring pattern. No new cross-feature lesson warranted.
