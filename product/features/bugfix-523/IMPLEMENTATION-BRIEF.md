# Implementation Brief: bugfix-523 — Server Hardening Batch

NLI Tick Gate + Log Downgrade + NaN Guards + Session Sanitization

---

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/bugfix-523/SCOPE.md |
| Scope Risk Assessment | product/features/bugfix-523/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/bugfix-523/specification/SPECIFICATION.md |
| Architecture | product/features/bugfix-523/architecture/ARCHITECTURE.md |
| ADR-001 | product/features/bugfix-523/architecture/ADR-001-hardening-batch-523.md |
| Risk-Test Strategy | product/features/bugfix-523/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/bugfix-523/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Source File | Pseudocode | Test Plan |
|-----------|-------------|-----------|-----------|
| nli-tick-gate (Item 1) | `crates/unimatrix-server/src/services/nli_detection_tick.rs` | pseudocode/nli-tick-gate.md | test-plan/nli-tick-gate.md |
| log-downgrade (Item 2) | `crates/unimatrix-server/src/services/nli_detection_tick.rs` | pseudocode/log-downgrade.md | test-plan/log-downgrade.md |
| nan-guards (Item 3) | `crates/unimatrix-server/src/infra/config.rs` | pseudocode/nan-guards.md | test-plan/nan-guards.md |
| session-sanitization (Item 4) | `crates/unimatrix-server/src/uds/listener.rs` | pseudocode/session-sanitization.md | test-plan/session-sanitization.md |

Note: Items 1 and 2 share `nli_detection_tick.rs` and MUST be assigned to the same implementation agent (SR-06 / C-08 constraint).

### Cross-Cutting Artifacts

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode and test-plan files are produced in Session 2 Stage 3a. The Component Map
lists expected components from the architecture. Items 1 and 2 share `nli_detection_tick.rs`
and MUST be assigned to the same implementation agent (SR-06 / C-08 constraint).

---

## Goal

Four independent hardening defects in `unimatrix-server` are resolved as a single deliverable:
an explicit `nli_enabled` gate on Path B of the graph inference tick eliminates background tick
congestion (353-second observed tick); a log-level downgrade for expected deprecated-entry misses
in Path C removes warn spam that obscures real signal; `!v.is_finite()` prefix guards on all 19
float fields in `InferenceConfig::validate()` catch NaN/Inf at server startup before they
propagate into scoring pipelines; and a `sanitize_session_id` guard in the
`post_tool_use_rework_candidate` UDS dispatch arm closes the last session injection gap in
`listener.rs`.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| NLI gate placement (SR-01) | Insert `if !config.nli_enabled { return; }` at PATH B entry gate only — structurally after `run_cosine_supports_path` returns and after `candidate_pairs.is_empty()` fast-exit, before `get_provider().await`. Structural landmark: `// === PATH B entry gate ===` comment block. ADR #4017 structural invariant preserved: Phase A and Path C remain unconditional. | SCOPE.md, ARCHITECTURE.md | architecture/ADR-001-hardening-batch-523.md (Unimatrix entry #4143) |
| 19-field NaN guard scope (OQ-01) | All 19 float fields in `InferenceConfig::validate()`: 11 threshold fields (Group A, inline guards) + 6 fusion weight fields (Group B, loop guard) + 2 phase weight fields (Group C, loop guard). NaN fusion weights silently pass sum check under IEEE 754; per-field `!v.is_finite()` guard is strictly better than the misleading `FusionWeightSumExceeded` produced without it. | SCOPE.md, ARCHITECTURE.md | architecture/ADR-001-hardening-batch-523.md (Unimatrix entry #4143) |
| Log-level test strategy (SR-03 / WARN-1) | **Behavioral-only coverage. Log level is NOT asserted in tests for Items 1 and 2.** See Design Decisions section. | ADR-001(c), Unimatrix entry #4143 | architecture/ADR-001-hardening-batch-523.md (Unimatrix entry #4143) |
| Item 4 sanitize guard placement (SR-05) | Guard inserted after capability check block, before any use of `event.session_id`. Structural order: (1) capability check, (2) `sanitize_session_id` guard [new], (3) payload field extraction, (4) `record_rework_event` call. | SPECIFICATION.md C-07, ARCHITECTURE.md | architecture/ADR-001-hardening-batch-523.md (Unimatrix entry #4143) |
| Distinct debug! message text (OQ-02) | `"graph inference tick: NLI disabled by config; Path B skipped"` — intentionally distinct from the existing `get_provider()` Err message (`"graph inference tick: NLI provider not ready; Supports path skipped"`) to allow operator disambiguation of intentional-off vs. transient-not-ready. | SCOPE.md, SPECIFICATION.md | architecture/ADR-001-hardening-batch-523.md (Unimatrix entry #4143) |
| crt-039 outer gate remains absent | The `if inference_config.nli_enabled` check removed from `background.rs` line 775 in crt-039 must NOT be reintroduced. The gate moves inside `run_graph_inference_tick` at the Path B boundary, not back to the call site. | crt-039 ADR-001 (entry #4017), SCOPE.md Non-Goals | N/A (prior ADR) |

---

## Design Decisions

### WARN-1 Resolution: Behavioral-Only Log-Level Coverage (AC-04 / AC-05)

**This is the authoritative decision. It supersedes SPECIFICATION.md's "Option A preferred" statement.**

The architecture (ADR-001(c), Unimatrix entry #4143) commits to behavioral-only coverage for
the log-level ACs introduced by Items 1 and 2. Log level is NOT asserted in tests. This
resolves the WARN-1 variance identified in ALIGNMENT-REPORT.md, where SPECIFICATION.md offered
tracing-test as a preferred option while the architecture had already committed to behavioral-only.

**Rationale**: Lesson #3935 documents that `tracing-test` / `tracing_subscriber` harnesses in
this codebase cause subscriber state leakage and initialization conflicts across parallel tests,
leading to Gate 3b failures. Adding `tracing-test` as a dev-dependency for two assertions in one
batch is not justified by the risk reduction achieved.

**What IS tested**:
- Item 1 (AC-01/AC-02): Behavioral proxy — assert Supports edges are not written and Path A
  Informs edges ARE written when `nli_enabled=false`. This proves both sides of the gate.
- Item 2 (AC-04/AC-05): Behavioral proxy — assert `run_cosine_supports_path` skips pairs with
  absent `category_map` entries (no panic, pair not written, function returns). The non-finite
  cosine path is tested similarly.

**Gate instruction**: At Gate 3b, the tester MUST include the following statement in the gate
report when covering AC-04 and AC-05:

> "AC-04 and AC-05 log-level assertions are behavioral-only per ADR-001(c) (Unimatrix entry
> #4143). Log level verified by code review. No `tracing-test` harness used."

Reviewers must accept behavioral-only coverage for these two ACs. Any gate feedback requesting
log-level assertions must be escalated to the Bugfix Leader, not unilaterally resolved by adding
the `tracing-test` harness.

---

## Files to Create / Modify

| File | Change |
|------|--------|
| `crates/unimatrix-server/src/services/nli_detection_tick.rs` | Items 1 + 2: insert `nli_enabled` gate at PATH B entry; change two `warn!` to `debug!` in `run_cosine_supports_path`; update comment at `get_provider()` call site |
| `crates/unimatrix-server/src/infra/config.rs` | Item 3: add `!v.is_finite()` prefix to 19 float field guards in `InferenceConfig::validate()`; add 21 new NaN/Inf tests in the `#[cfg(test)]` module |
| `crates/unimatrix-server/src/uds/listener.rs` | Item 4: add `sanitize_session_id` guard in `post_tool_use_rework_candidate` arm; add 2 new dispatch-arm tests |

No new files. No schema migrations. No Cargo.toml changes.

---

## Item 1 — NLI Tick Gate

### File
`crates/unimatrix-server/src/services/nli_detection_tick.rs`

### Exact Change

Insert the following block at the PATH B entry gate — structurally after the
`run_cosine_supports_path(...)` call completes and after the
`if candidate_pairs.is_empty() { return; }` fast-exit, before the
`let provider = match nli_handle.get_provider().await` call:

```rust
if !config.nli_enabled {
    tracing::debug!("graph inference tick: NLI disabled by config; Path B skipped");
    return;
}
```

**Exact debug! message text** (prescribed, must not be altered):
`"graph inference tick: NLI disabled by config; Path B skipped"`

This message is intentionally distinct from the existing `get_provider()` Err message
(`"graph inference tick: NLI provider not ready; Supports path skipped"`) so operators can
distinguish intentional-off from transient-not-ready in logs.

Also update the comment at the `get_provider().await` call site to remove language stating
that the Err-return is "expected when nli_enabled=false" — the explicit gate now handles that
case before reaching `get_provider()`.

**Structural insertion sequence at PATH B entry gate**:
```
// [existing] if candidate_pairs.is_empty() { return; }
// [NEW]      if !config.nli_enabled { tracing::debug!("..."); return; }
// [existing] let provider = match nli_handle.get_provider().await { ... }
```

### Constraints
- The outer call in `background.rs` at line 776 must remain unconditional (crt-039 ADR-001,
  entry #4017). Do not add a gate there.
- Phase A (Informs write loop) and Path C (`run_cosine_supports_path`) execute before this
  gate and must not be gated.

### Test Requirements
- `test_nli_gate_path_a_informs_edges_still_written_nli_disabled`: run tick with
  `nli_enabled=false`, assert Informs edges are present in output (AC-02).
- `test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled`: run tick with
  `nli_enabled=false`, assert cosine Supports edges are written (AC-02).
- `test_nli_gate_path_b_skipped_nli_disabled`: run tick with `nli_enabled=false` and a
  mock-ready provider, assert no NLI Supports edges are written and no rayon dispatch occurs
  (AC-01). The mock-ready provider distinguishes this gate from the implicit `get_provider()`
  Err path. **This test must use non-empty `candidate_pairs`** — if the pair list is empty the
  `candidate_pairs.is_empty()` fast-exit fires before the `nli_enabled` check, so the new gate
  is never reached. Behavioral proxies (no rayon dispatch, no NLI edges) remain valid with empty
  candidates, but meaningful coverage of the explicit gate requires at least one candidate pair.
- `test_nli_gate_nli_enabled_path_not_regressed`: run tick with `nli_enabled=true` and a mock
  provider available, assert `get_provider()` is called and NLI path executes (AC-03).

---

## Item 2 — Log Downgrade in `run_cosine_supports_path`

### File
`crates/unimatrix-server/src/services/nli_detection_tick.rs`

### Exact Change

Change `tracing::warn!` to `tracing::debug!` at exactly two sites in `run_cosine_supports_path`:

1. The `category_map.get(src_id)` None arm (message currently contains
   `"Path C: source entry not found in category_map (deprecated mid-tick?) — skipping"`).
2. The `category_map.get(tgt_id)` None arm (message currently contains
   `"Path C: target entry not found in category_map (deprecated mid-tick?) — skipping"`).

**The non-finite cosine `warn!` at the `!cosine.is_finite()` guard site (currently
`"Path C: non-finite cosine for candidate pair — skipping"`) MUST remain `tracing::warn!`
unchanged.** That site is a structural anomaly (NaN from HNSW), not an expected race condition.
Log level semantic contract (entry #3467): operational anomalies use `warn!`; expected
degraded-mode behavior uses `debug!`.

No other changes to `run_cosine_supports_path`.

### Test Requirements (behavioral-only per ADR-001(c) — see Design Decisions)
- `test_cosine_supports_path_skips_missing_category_map_src`: call `run_cosine_supports_path`
  with a candidate pair where `src_id` is absent from `category_map`. Assert pair is skipped,
  function returns without panic (AC-04 behavioral proxy).
- `test_cosine_supports_path_skips_missing_category_map_tgt`: call `run_cosine_supports_path`
  with a candidate pair where `tgt_id` is absent from `category_map`. Assert pair is skipped,
  function returns without panic (AC-04 behavioral proxy, tgt branch).
- `test_cosine_supports_path_nonfinite_cosine_handled`: call with a pair yielding a non-finite
  cosine. Assert pair is skipped, no panic. Verify by code review (not test assertion) that
  the non-finite cosine site remains `warn!` (AC-05).

---

## Item 3 — `InferenceConfig::validate()` NaN Guards

### File
`crates/unimatrix-server/src/infra/config.rs`

### Exact Change

For each of the 19 fields below, prefix the existing comparison guard with `!v.is_finite() || `
(Group A) or `!value.is_finite() || ` (Groups B and C). Pattern from PR #516, lesson #4132:

**Group A — 11 individual f32/f64 threshold fields (inline guards)**:
```rust
// Before:
if self.<field> <= 0.0 || self.<field> >= 1.0 {

// After:
let v = self.<field>;
if !v.is_finite() || v <= 0.0 || v >= 1.0 {
```

**Groups B and C — 8 weight fields (loop-body guards)**:
```rust
// Before:
for (field, value) in <weight>_checks {
    if *value < 0.0 || *value > 1.0 {

// After:
for (field, value) in <weight>_checks {
    if !value.is_finite() || *value < 0.0 || *value > 1.0 {
```

Error variant: `ConfigError::NliFieldOutOfRange` — the established variant (lesson #4132).
No new error variants. `value.to_string()` for NaN produces `"NaN"` — valid as-is.

### Complete 19-Field Checklist (SR-02 requirement — all fields required, no sampling)

| # | Field Name | Type | Group | Current Guard (before prefix) | Valid Range |
|---|-----------|------|-------|-------------------------------|-------------|
| 1 | `nli_entailment_threshold` | f32 | A | `<= 0.0 \|\| >= 1.0` | (0.0, 1.0) exclusive |
| 2 | `nli_contradiction_threshold` | f32 | A | `<= 0.0 \|\| >= 1.0` | (0.0, 1.0) exclusive |
| 3 | `nli_auto_quarantine_threshold` | f32 | A | `<= 0.0 \|\| >= 1.0` | (0.0, 1.0) exclusive |
| 4 | `supports_candidate_threshold` | f32 | A | `<= 0.0 \|\| >= 1.0` | (0.0, 1.0) exclusive |
| 5 | `supports_edge_threshold` | f32 | A | `<= 0.0 \|\| >= 1.0` | (0.0, 1.0) exclusive |
| 6 | `ppr_alpha` | f64 | A | `<= 0.0 \|\| >= 1.0` | (0.0, 1.0) exclusive |
| 7 | `ppr_inclusion_threshold` | f64 | A | `<= 0.0 \|\| >= 1.0` | (0.0, 1.0) exclusive |
| 8 | `ppr_blend_weight` | f64 | A | `< 0.0 \|\| > 1.0` | [0.0, 1.0] inclusive |
| 9 | `nli_informs_cosine_floor` | f32 | A | `<= 0.0 \|\| >= 1.0` | (0.0, 1.0) exclusive |
| 10 | `nli_informs_ppr_weight` | f64 | A | `< 0.0 \|\| > 1.0` | [0.0, 1.0] inclusive |
| 11 | `supports_cosine_threshold` | f32 | A | `<= 0.0 \|\| >= 1.0` | (0.0, 1.0) exclusive |
| 12 | `w_sim` | f64 | B | `< 0.0 \|\| > 1.0` | [0.0, 1.0] inclusive |
| 13 | `w_nli` | f64 | B | `< 0.0 \|\| > 1.0` | [0.0, 1.0] inclusive |
| 14 | `w_conf` | f64 | B | `< 0.0 \|\| > 1.0` | [0.0, 1.0] inclusive |
| 15 | `w_coac` | f64 | B | `< 0.0 \|\| > 1.0` | [0.0, 1.0] inclusive |
| 16 | `w_util` | f64 | B | `< 0.0 \|\| > 1.0` | [0.0, 1.0] inclusive |
| 17 | `w_prov` | f64 | B | `< 0.0 \|\| > 1.0` | [0.0, 1.0] inclusive |
| 18 | `w_phase_histogram` | f64 | C | `< 0.0 \|\| > 1.0` | [0.0, 1.0] inclusive |
| 19 | `w_phase_explicit` | f64 | C | `< 0.0 \|\| > 1.0` | [0.0, 1.0] inclusive |

Group A fields 1–11: inline `let v = self.<field>;` pattern.
Group B fields 12–17: `fusion_weight_checks` loop body.
Group C fields 18–19: `phase_weight_checks` loop body.

**Implementation note for Groups B and C**: Depending on how the loop array is constructed,
the iterated `value` may be `&f64` rather than `f64`. Rust auto-deref makes `value.is_finite()`
call correctly in both cases, but verify the dereference form at the actual loop iteration site
to confirm the guard compiles cleanly. If the iterator yields `&&f64`, calling `.is_finite()`
directly works — auto-deref resolves both levels. The AC-27 boundary-value regression tests
(w_sim valid at 0.0 and 0.5; invalid at -0.1 and 1.1) are the structural regression check that
the guard form is correct: if `!value.is_finite()` is miscoded in a way that rejects valid values,
AC-27 will catch it.

Fields NOT in scope: `usize` and `u32` fields (`nli_top_k`, `max_contradicts_per_tick`,
`max_graph_inference_per_tick`, etc.) are not subject to IEEE 754 NaN. All three crt-046 fields
(`goal_cluster_similarity_threshold`, `w_goal_cluster_conf`, `w_goal_boost`) already have
`!v.is_finite()` guards from PR #516 and must not be double-modified.

### Test Requirements

All 19 NaN tests and 2 representative Inf tests are non-negotiable (R-03, R-06).
Use `assert_validate_fails_with_field(c, "field_name")` helper (~line 4615 in config.rs) for
every test. Exact field name strings must match the checklist above.

Test pattern (established for crt-046 fields, lines 8004–8094 in config.rs):
```rust
#[test]
fn test_nan_guard_nli_entailment_threshold() {
    let mut c = InferenceConfig::default();
    c.nli_entailment_threshold = f32::NAN;
    assert_validate_fails_with_field(&c, "nli_entailment_threshold");
}
```

Required test function names (Gate 3a must verify all 21 are present by name):
- `test_nan_guard_nli_entailment_threshold` (AC-06)
- `test_nan_guard_nli_contradiction_threshold` (AC-07)
- `test_nan_guard_nli_auto_quarantine_threshold` (AC-08)
- `test_nan_guard_supports_candidate_threshold` (AC-09)
- `test_nan_guard_supports_edge_threshold` (AC-10)
- `test_nan_guard_ppr_alpha` (AC-11)
- `test_nan_guard_ppr_inclusion_threshold` (AC-12)
- `test_nan_guard_ppr_blend_weight` (AC-13)
- `test_nan_guard_nli_informs_cosine_floor` (AC-14)
- `test_nan_guard_nli_informs_ppr_weight` (AC-15)
- `test_nan_guard_supports_cosine_threshold` (AC-16)
- `test_nan_guard_w_sim` (AC-17)
- `test_nan_guard_w_nli` (AC-18)
- `test_nan_guard_w_conf` (AC-19)
- `test_nan_guard_w_coac` (AC-20)
- `test_nan_guard_w_util` (AC-21)
- `test_nan_guard_w_prov` (AC-22)
- `test_nan_guard_w_phase_histogram` (AC-23)
- `test_nan_guard_w_phase_explicit` (AC-24)
- `test_inf_guard_nli_entailment_threshold_f32` (AC-25, representative f32 Inf)
- `test_inf_guard_ppr_alpha_f64` (AC-26, representative f64 Inf)

Also required: `cargo test` must pass all pre-existing `InferenceConfig::validate()` tests
(AC-27). Verify boundary tests for `w_sim` (0.0, 0.5 valid; -0.1, 1.1 invalid) specifically.

---

## Item 4 — `sanitize_session_id` Guard in `post_tool_use_rework_candidate` Arm

### File
`crates/unimatrix-server/src/uds/listener.rs`

### Exact Change

In `dispatch_request`, within the `post_tool_use_rework_candidate` arm, insert the following
guard immediately after the capability check block (after the `uds_has_capability(Capability::SessionWrite)`
return at lines 660–665) and before the `event.payload.get("tool_name")` extraction:

```rust
if let Err(e) = sanitize_session_id(&event.session_id) {
    tracing::warn!(
        session_id = %event.session_id,
        error = %e,
        "UDS: RecordEvent (rework_candidate) rejected: invalid session_id"
    );
    return HookResponse::Error {
        code: ERR_INVALID_PAYLOAD,
        message: e,
    };
}
```

This is the identical pattern used in the `RecordEvent` general arm (lines 731–738). The warn
message qualifier `(rework_candidate)` identifies the arm in logs.

**Structural insertion order** (SR-05 / C-07):
1. Capability check (existing — unchanged)
2. `sanitize_session_id` guard (new — this change)
3. `event.payload.get("tool_name")` extraction (existing — unchanged)
4. `session_registry.record_rework_event(&event.session_id, ...)` (existing — unchanged)

No `event.session_id` value may appear between steps 1 and 2.

### Test Requirements

- `test_dispatch_rework_candidate_invalid_session_id_rejected` (AC-28): dispatch
  `post_tool_use_rework_candidate` event with `event.session_id = "../../etc/passwd"`. Assert
  `HookResponse::Error { code: ERR_INVALID_PAYLOAD }` is returned and `record_rework_event` is
  never called.
- `test_dispatch_rework_candidate_valid_path_not_regressed` (AC-29): dispatch with a valid
  session_id (e.g., `"session-abc123"`). Assert `record_rework_event` is called and a success
  response is returned.

---

## Data Structures

No new data structures. Existing structures used as-is:

| Structure | Usage |
|-----------|-------|
| `InferenceConfig` | Field validation target for Item 3; `nli_enabled: bool` field used as gate condition in Item 1 |
| `ConfigError::NliFieldOutOfRange { path, field, value, reason }` | Returned by all 19 new guards in Item 3 |
| `HookResponse::Error { code: i64, message: String }` | Returned by Item 4 guard on invalid session_id |
| `HookEvent` | Source of `event.session_id` (field, not destructured local) in Item 4 |
| `ERR_INVALID_PAYLOAD` (i64 constant) | Error code for session_id validation failures |

---

## Function Signatures (Unchanged)

All four fixes call existing functions with existing signatures:

| Function | Signature | Item |
|----------|-----------|------|
| `sanitize_session_id` | `fn(&str) -> Result<(), String>` | 4 |
| `NliServiceHandle::get_provider` | `async fn(&self) -> Result<Arc<dyn NliProvider>, NliError>` | 1 (avoided when gate fires) |
| `run_cosine_supports_path` | `async fn(store, config, pairs, existing, category_map, ts)` | 2 (internal change only) |
| `InferenceConfig::validate` | `fn(&self) -> Result<(), ConfigError>` | 3 (internal change only) |
| `assert_validate_fails_with_field` | `fn(c: &InferenceConfig, field: &str)` (~line 4615) | 3 (tests) |

---

## Constraints

| ID | Constraint |
|----|-----------|
| C-01 | The `run_graph_inference_tick` caller in `background.rs` must remain unconditional (crt-039 ADR-001, entry #4017). |
| C-02 | Path A (Informs) and Path C (cosine Supports) must not be gated by `nli_enabled`. |
| C-03 | NLI gate must be inserted inside `run_graph_inference_tick` at Path B boundary only — after Path C completes. |
| C-04 | `ConfigError::NliFieldOutOfRange` is the only error variant for `InferenceConfig` float field errors. No new error variants. |
| C-05 | `ERR_INVALID_PAYLOAD` is the established error code for session_id validation failures in all UDS dispatch arms. |
| C-06 | All 21 NaN/Inf tests must use `assert_validate_fails_with_field(c, "field_name")` with exact field name strings. |
| C-07 | `sanitize_session_id` guard must be placed after capability check, before any use of `event.session_id`. |
| C-08 | Items 1 and 2 both modify `nli_detection_tick.rs` and MUST be assigned to the same implementation agent/wave. |
| C-09 | `!v.is_finite()` guards must not be applied to `usize` or `u32` fields in `InferenceConfig`. |
| C-10 | No changes to `RetentionConfig`, `CoherenceConfig`, or any struct other than `InferenceConfig`. |

---

## Dependencies

All existing — no new crates or external services:

| Component | Source | Item |
|-----------|--------|------|
| `tracing::debug!` / `tracing::warn!` | `tracing` crate (existing dep) | 1, 2 |
| `ConfigError::NliFieldOutOfRange` | `infra/config.rs` | 3 |
| `sanitize_session_id` | `uds/listener.rs` (local fn) | 4 |
| `ERR_INVALID_PAYLOAD` | `uds/listener.rs` (local const) | 4 |
| `assert_validate_fails_with_field` | `infra/config.rs` `#[cfg(test)]` (~line 4615) | 3 tests |

No schema migrations. No new Cargo.toml entries. No MCP API changes.

---

## NOT in Scope

- Removing Path B code (NLI Supports detection remains for future reactivation).
- Re-introducing the outer `if inference_config.nli_enabled` gate in `background.rs`.
- Adding `is_finite()` guards to `RetentionConfig`, `CoherenceConfig`, or any struct other than
  `InferenceConfig`.
- Adding `is_finite()` guards to integer fields (`usize`, `u32`) in `InferenceConfig`.
- Adding guards to crt-046 fields already guarded in PR #516
  (`goal_cluster_similarity_threshold`, `w_goal_cluster_conf`, `w_goal_boost`).
- Changing the `NliServiceHandle::get_provider()` interface or initialization path.
- Any schema change, new MCP tool, or API surface change.
- Fixing pre-existing open issues (#452, #303, #305).
- Any log level change other than the two category_map miss sites in `run_cosine_supports_path`.

---

## Top 3 Must-Not-Skip Tester Scenarios

From RISK-TEST-STRATEGY.md, the three highest-priority scenarios the tester must not skip:

**1. R-01/R-02 — Path A and Path C continue to run when `nli_enabled=false` (AC-02)**

Passing AC-01 (Path B skipped) is insufficient without passing AC-02 (Path A and Path C
continue). The gate check is only correct if both sides of the predicate are verified. Tests:
`test_nli_gate_path_a_informs_edges_still_written_nli_disabled` and
`test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled` are both non-negotiable.
If the gate is placed before `run_cosine_supports_path`, Path C stops accumulating cosine
Supports edges in production — a silent data absence failure with no crash and no error log.

**2. R-03 — All 19 NaN fields individually tested (AC-06 through AC-24)**

The tester must verify the count of NaN tests is exactly 19 and each test uses the exact field
name string from the checklist above. A count mismatch is the most likely source of a NaN test
gap. Passing a sample (e.g., testing only Group A threshold fields) and assuming Group B/C loop
fields work by analogy is not acceptable — the loop-body guard dereference form is different and
must be verified independently. Spot-check AC-17 through AC-24 (loop-group fields) by verifying
the `err.to_string().contains(field_name)` assertion would fail with a wrong field name string.

**3. R-04 — sanitize_session_id guard insertion order verified by code review (AC-28)**

AC-28 runtime test confirms the guard fires. Code inspection must additionally confirm that no
use of `event.session_id` appears between the capability check and the guard block. A guard
placed after even one line that touches `event.session_id` violates the structural contract and
creates a maintenance trap. Both the runtime test and the code review are required; the test
alone is insufficient.

---

## Alignment Status

Source: ALIGNMENT-REPORT.md (reviewed 2026-04-05).

| Check | Status |
|-------|--------|
| Vision Alignment | PASS — all four items directly support NLI pipeline reliability, config integrity, security surface, and observability goals. |
| Milestone Fit | PASS — Wave 1A / W1-4 maintenance hardening; no Wave 2 or Wave 3 scope pulled in. |
| Scope Gaps | PASS — all four SCOPE.md items fully addressed by FR-01 through FR-04. |
| Architecture Consistency | PASS — structural invariants respected; integration surface table matches spec. |
| Risk Completeness | PASS — all SR-series risks traced to R-series risks with test scenarios. |
| WARN-1 | RESOLVED in this brief — behavioral-only log-level coverage is the authoritative decision per ADR-001(c)/entry #4143. See Design Decisions section. |

The only variance requiring resolution was WARN-1 (SR-03 decision ownership split between
architecture and specification). This brief resolves it by adopting the architect's behavioral-only
position (ADR-001(c)) as the single authoritative source. The specification's "Option A preferred"
language is superseded by this brief. No functional fixes to source documents are required.
