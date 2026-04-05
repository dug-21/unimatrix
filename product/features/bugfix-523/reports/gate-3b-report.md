# Gate 3b Report: bugfix-523

> Gate: 3b (Code Review)
> Date: 2026-04-05
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All four items match validated pseudocode exactly |
| Architecture compliance | PASS | Component boundaries, ADR decisions, structural landmarks all honored |
| Interface implementation | PASS | Function signatures, error types, constants all correct; nli_informs_ppr_weight type corrected to f32 (not f64 as in spec) |
| Test case alignment | WARN | AC-29 test function named `test_dispatch_rework_candidate_valid_session_id_succeeds` instead of `test_dispatch_rework_candidate_valid_path_not_regressed` (test plan's prescribed name); semantically complete |
| Code quality | PASS | Compiles cleanly; no stubs/placeholders; no .unwrap() in non-test code; pre-existing files exceed 500 lines but no new files created |
| Security | PASS | sanitize_session_id guard correctly placed; no hardcoded secrets; no path traversal; input validation at all UDS boundaries |
| Knowledge stewardship | PASS | All three implementation agent reports contain ## Knowledge Stewardship blocks with Queried: and Stored: entries |

---

## Detailed Findings

### Check 1: Pseudocode Fidelity

**Status**: PASS

**Item 1 — NLI Tick Gate**

The gate `if !config.nli_enabled { tracing::debug!("graph inference tick: NLI disabled by config; Path B skipped"); return; }` is present at lines 561–564 of `nli_detection_tick.rs`. This exactly matches the prescribed pseudocode in `pseudocode/nli-tick-gate.md`.

The comment at the `get_provider()` call site has been updated to remove "Expected when nli_enabled=false (production default)" language. The updated comment reads: "The nli_enabled=false case is handled by the explicit gate above; Err here is a transient provider-not-ready condition only." This matches the pseudocode requirement.

Structural landmark comment `// === PATH B entry gate ===` is present at line 546.

**Item 2 — Log Downgrade**

Exactly two `warn!` → `debug!` changes in `run_cosine_supports_path`:

1. `category_map.get(src_id)` None arm (line 807): now `tracing::debug!` with message "Path C: source entry not found in category_map (deprecated mid-tick?) — skipping"
2. `category_map.get(tgt_id)` None arm (line 817): now `tracing::debug!` with message "Path C: target entry not found in category_map (deprecated mid-tick?) — skipping"

The non-finite cosine guard at line 777 remains `tracing::warn!` — confirmed unchanged. No other changes to `run_cosine_supports_path`.

"AC-04 and AC-05 log-level assertions are behavioral-only per ADR-001(c) (Unimatrix entry #4143). Log level verified by code review. No `tracing-test` harness used."

**Item 3 — NaN Guards**

All 19 field guards in `InferenceConfig::validate()` have been prefixed with `!v.is_finite() || ` (Group A) or `!value.is_finite() || ` (Groups B/C):

- Group A (11 inline fields): `nli_entailment_threshold`, `nli_contradiction_threshold`, `nli_auto_quarantine_threshold`, `supports_candidate_threshold`, `supports_edge_threshold`, `ppr_alpha`, `ppr_inclusion_threshold`, `ppr_blend_weight`, `nli_informs_cosine_floor`, `nli_informs_ppr_weight`, `supports_cosine_threshold` — all using the `let v = self.<field>; if !v.is_finite() || ...` pattern.
- Group B (6 fusion weight loop fields): guard changed from `if *value < 0.0 || *value > 1.0` to `if !value.is_finite() || *value < 0.0 || *value > 1.0` in `fusion_weight_checks` loop.
- Group C (2 phase weight loop fields): identical transformation in `phase_weight_checks` loop.

crt-046 fields (`goal_cluster_similarity_threshold`, `w_goal_cluster_conf`, `w_goal_boost`) were NOT double-modified — confirmed.

**Item 4 — Session Sanitization**

The guard block at lines 666–678 of `listener.rs` matches the pseudocode exactly:

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

The message qualifier `(rework_candidate)` is present and distinct from the general arm message. `ERR_INVALID_PAYLOAD` is used (not `-32003`).

---

### Check 2: Architecture Compliance

**Status**: PASS

**Item 1 gate placement** (spawn prompt Key Check 1):

- Gate is AFTER `candidate_pairs.is_empty()` block (line 552–555)
- Gate is BEFORE `nli_handle.get_provider().await` (line 571)
- Gate is AFTER `run_cosine_supports_path(...)` call (line 536–544)
- Structural landmark comment `// === PATH B entry gate ===` at line 546 is present and unchanged
- `background.rs` was NOT modified (C-01 compliance)

**Item 2 log sites** (spawn prompt Key Check 2):

Exactly two `warn!` → `debug!` changes confirmed in `run_cosine_supports_path`. Non-finite cosine `warn!` site at line 777 remains `warn!`. ADR-001 (entry #4017) structural invariant (Path A and Path C unconditional) preserved.

**Item 3 field count** (spawn prompt Key Check 3):

All 19 fields enumerated with correct guard forms. Loop-body dereference in Groups B/C correct: `!value.is_finite()` (auto-deref on `&f64`) combined with `*value < 0.0` (explicit deref for comparison). No new error variants introduced.

**Item 4 insertion order** (spawn prompt Key Check 4):

Code inspection of `post_tool_use_rework_candidate` arm confirms:
1. Capability check (`if !uds_has_capability(Capability::SessionWrite)`) — line 660–665
2. `sanitize_session_id(&event.session_id)` guard — lines 666–678 (NEW)
3. `event.payload.get("tool_name")` extraction — line 679
4. `session_registry.record_rework_event(&event.session_id, ...)` — line 703

No use of `event.session_id` appears between the capability check closing brace (line 665) and the `sanitize_session_id` call (line 668). SR-05 / C-07 compliance confirmed.

---

### Check 3: Interface Implementation

**Status**: PASS

All interfaces are implemented as designed:

| Interface | Implementation | Correct? |
|-----------|---------------|---------|
| `config.nli_enabled` field access | `if !config.nli_enabled { ... }` | Yes |
| `tracing::debug!` message text | Exact match: "graph inference tick: NLI disabled by config; Path B skipped" | Yes |
| `sanitize_session_id(&str) -> Result<(), String>` | Called with `&event.session_id` | Yes |
| `ERR_INVALID_PAYLOAD` constant | Used in `HookResponse::Error { code: ERR_INVALID_PAYLOAD, ... }` | Yes |
| `ConfigError::NliFieldOutOfRange { path, field, value, reason }` | Existing variant used; no new variants | Yes |
| `nli_informs_ppr_weight` type | `f32` in source (not `f64` as listed in spec/architecture) — implementation and test both use `f32::NAN` correctly | Yes (corrected) |

**Type annotation discrepancy noted**: SPECIFICATION.md and ARCHITECTURE.md both list `nli_informs_ppr_weight` as `f64`. The actual field is `f32`. The implementation agent (`bugfix-523-agent-4-nan-guards`) detected this at compile time, corrected the test to use `f32::NAN`, and stored lesson #4144 in Unimatrix about verifying types in source before implementing. The production guard code is correct. This is a documentation error in the spec/architecture, not an implementation error.

---

### Check 4: Test Case Alignment

**Status**: WARN

All required tests are present and functionally complete. One function name deviates from the test plan:

| AC | Required name (test plan) | Actual name | Functionally complete? |
|----|--------------------------|-------------|----------------------|
| AC-01 | `test_nli_gate_path_b_skipped_nli_disabled` | `test_nli_gate_path_b_skipped_nli_disabled` | Yes |
| AC-02 (Path A) | `test_nli_gate_path_a_informs_edges_still_written_nli_disabled` | `test_nli_gate_path_a_informs_edges_still_written_nli_disabled` | Yes |
| AC-02 (Path C) | `test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled` | `test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled` | Yes |
| AC-03 | `test_nli_gate_nli_enabled_path_not_regressed` | `test_nli_gate_nli_enabled_path_not_regressed` | Yes |
| AC-04 (src) | `test_cosine_supports_path_skips_missing_category_map_src` | `test_cosine_supports_path_skips_missing_category_map_src` | Yes |
| AC-04 (tgt) | `test_cosine_supports_path_skips_missing_category_map_tgt` | `test_cosine_supports_path_skips_missing_category_map_tgt` | Yes |
| AC-05 | `test_cosine_supports_path_nonfinite_cosine_handled` | `test_cosine_supports_path_nonfinite_cosine_handled` | Yes |
| AC-06..AC-24 | `test_nan_guard_<field>` (19 tests) | All 19 present with correct names | Yes |
| AC-25 | `test_inf_guard_nli_entailment_threshold_f32` | `test_inf_guard_nli_entailment_threshold_f32` | Yes |
| AC-26 | `test_inf_guard_ppr_alpha_f64` | `test_inf_guard_ppr_alpha_f64` | Yes |
| AC-28 | `test_dispatch_rework_candidate_invalid_session_id_rejected` | `test_dispatch_rework_candidate_invalid_session_id_rejected` | Yes |
| AC-29 | `test_dispatch_rework_candidate_valid_path_not_regressed` | **`test_dispatch_rework_candidate_valid_session_id_succeeds`** | Yes (name differs) |

The AC-29 test asserts: `HookResponse::Ack` returned, `record_rework_event` called once (via `state.rework_events.len() == 1`). Functionally complete per the test plan requirements, despite the name deviation.

**NaN test count**: 19 NaN tests + 2 Inf tests = 21 total (matches requirement exactly).

**Field name spot-check** (R-07 mitigation for AC-17..AC-24):
- `fusion_weight_checks` array strings: `"w_sim"`, `"w_nli"`, `"w_conf"`, `"w_coac"`, `"w_util"`, `"w_prov"` — verified to match test strings exactly.
- `phase_weight_checks` array strings: `"w_phase_histogram"`, `"w_phase_explicit"` — verified to match test strings exactly.

**AC-01 non-empty candidates requirement**: `test_nli_gate_path_b_skipped_nli_disabled` uses `supports_candidate_threshold: 0.60` with identical embeddings (cosine = 1.0), producing non-empty `candidate_pairs`. The empty-pairs fast-exit is not exercised, so the nli_enabled gate is correctly reached. Comment in test confirms this requirement.

---

### Check 5: Code Quality

**Status**: PASS

**Build**: `cargo build --workspace` completes with zero errors. 17 pre-existing warnings in `unimatrix-server` (not introduced by this batch).

**Tests**: `cargo test --workspace` — all test suites pass with zero failures, zero regressions.

**Stubs/placeholders**: None introduced by this batch. Pre-existing TODO comment at line 72 of `nli_detection_tick.rs` (`MAX_COSINE_SUPPORTS_PER_TICK` config-promote) is pre-existing from crt-040.

**`.unwrap()` in non-test code**: None introduced by this batch. All `.unwrap()` calls in affected files are within `#[cfg(test)]` modules.

**File line counts** (pre-existing, not created by this batch):
- `nli_detection_tick.rs`: 3702 lines (pre-existing large file; surgical additions only)
- `config.rs`: 8301 lines (pre-existing large file; surgical additions only)
- `listener.rs`: 7682 lines (pre-existing large file; surgical additions only)

All three files exceed the 500-line guidance. However, these are pre-existing files; no new files were created and no new modules could reasonably be split from a 3-line guard insertion. This is consistent with the project's approach of surgical bugfix changes to large service files.

**Clippy** (`cargo clippy -p unimatrix-server`): No errors from `unimatrix-server` crate. Pre-existing collapsible-if and other warnings in `unimatrix-engine` and `unimatrix-observe` are not in scope for this bugfix. The server crate itself emits 13 pre-existing warnings (no new ones from this batch).

**cargo audit**: `cargo-audit` is not installed in this environment. No CVE check was possible. This is an environment limitation, not a code issue.

---

### Check 6: Security

**Status**: PASS

**Item 4 — Session injection closure confirmed**:

The `sanitize_session_id` guard is placed before any use of `event.session_id` in the `post_tool_use_rework_candidate` arm. The canonical path-traversal input `"../../etc/passwd"` is rejected by `sanitize_session_id` (allowlist: `[a-zA-Z0-9\-_]+`, max 128 chars). `record_rework_event` and `record_topic_signal` are not reached on rejection.

**No hardcoded secrets**: Confirmed. All new code uses existing constants and function references.

**No new path operations**: No file operations introduced in this batch.

**No command injection**: No shell invocations.

**Serialization safety**: No new deserialization paths. Existing `HookEvent` deserialization is unchanged.

**Input validation at UDS boundary**: `sanitize_session_id` is synchronous, O(length), bounded at 128 chars per NFR-03.

---

### Check 7: Knowledge Stewardship Compliance

**Status**: PASS

All three implementation agent reports contain `## Knowledge Stewardship` sections with `Queried:` entries:

**`bugfix-523-agent-3-nli-tick`** (Items 1 + 2, `nli_detection_tick.rs`):
- Queried: `mcp__unimatrix__context_briefing` — returned ADR #4143, pattern #3675, ADR #4017
- Stored: entry #4145 "HNSW pair direction in run_graph_inference_tick tests is non-deterministic" via `/uni-store-pattern`

**`bugfix-523-agent-4-nan-guards`** (Item 3, `config.rs`):
- Queried: `mcp__unimatrix__context_briefing` + `context_search` — returned entries #4132, #4133, #4131, #4044, #4143
- Stored: entry #4144 "Verify InferenceConfig field types in source before writing NaN tests — brief type column can be stale" via `/uni-store-pattern`

**`bugfix-523-agent-5-session-sanitization`** (Item 4, `listener.rs`):
- Queried: `mcp__unimatrix__context_briefing` + `context_search` — returned entries #3902, #4141, #322, #300, #4143
- Stored: "nothing novel to store — the pattern applied here (#4141) is already in Unimatrix and fully describes what was implemented." Reason provided: pattern already captured, only the rework_candidate arm being the last gap, which is implicit from the fix.

All three agents satisfy stewardship requirements. No missing blocks.

---

### Code Inspection: Item 4 Insertion Order (R-04 — Non-Negotiable)

**Required insertion order** (per test-plan/session-sanitization.md):
1. Capability check — line 660–665
2. `sanitize_session_id` guard — lines 666–678
3. `event.payload.get("tool_name")` — line 679
4. `session_registry.record_rework_event` — line 703

**Confirmed**: No use of `event.session_id` appears between the capability check closing brace (line 665) and the guard block opening (line 668). The comment `// GH #523 (SEC-02): Validate session_id before any registry or DB writes.` is present immediately before the guard.

**Code review confirms**: guard uses `ERR_INVALID_PAYLOAD` constant, warn message contains `(rework_candidate)`, `HookResponse::Error` variant is used.

---

### Code Review: Item 2 — Non-Finite Cosine warn! Site Unchanged

**Confirmed**: Line 777 in `nli_detection_tick.rs`:
```
if !cosine.is_finite() {
    tracing::warn!(
        src_id,
        tgt_id,
        "Path C: non-finite cosine for candidate pair — skipping"
    );
```
This site remains `tracing::warn!`. Exactly two `warn!` → `debug!` changes exist in `run_cosine_supports_path` and no others.

---

## Rework Required

None. All FAIL-level issues are absent. The WARN on AC-29 test function naming is minor and does not block progress.

---

## Warnings

| Warning | Artifact | Detail |
|---------|----------|--------|
| AC-29 test name deviation | `uds/listener.rs` test | Test plan specifies `test_dispatch_rework_candidate_valid_path_not_regressed`; implementation uses `test_dispatch_rework_candidate_valid_session_id_succeeds`. Functionally complete; name only differs. |
| `nli_informs_ppr_weight` type annotation in spec/architecture | SPECIFICATION.md, ARCHITECTURE.md | Both list this field as `f64`; actual struct field is `f32`. Implementation and test are correct (use `f32`). The spec/architecture contain a stale type annotation. Agent stored lesson #4144 about this trap. |
| cargo-audit not available | Environment | `cargo-audit` is not installed; CVE check could not be run. Pre-existing environment limitation. |
| Pre-existing 500-line files | All three files | `nli_detection_tick.rs` (3702), `config.rs` (8301), `listener.rs` (7682) all exceed 500 lines. These are pre-existing; this batch added only surgical changes. Not a regression. |

---

## Gate Report Acknowledgment (Required by ADR-001(c) / entry #4143)

"AC-04 and AC-05 log-level assertions are behavioral-only per ADR-001(c) (Unimatrix entry #4143). Log level verified by code review. No `tracing-test` harness used."

Non-finite cosine `warn!` site verified by code review to be unchanged. Exactly two `warn!`→`debug!` changes in `run_cosine_supports_path`.

Field name strings for AC-17..AC-24 verified against `fusion_weight_checks` / `phase_weight_checks` array entries: exact match confirmed.

Item 4 insertion order verified by code inspection: guard appears immediately after capability check, before `event.payload.get('tool_name')`. No use of `event.session_id` between capability check and guard. `ERR_INVALID_PAYLOAD` code used.

---

## Knowledge Stewardship

- Stored: nothing novel to store -- gate-3b findings are feature-specific. The AC-29 test naming deviation is a minor one-off. The `nli_informs_ppr_weight` type discrepancy pattern is already captured in Unimatrix entry #4144 by the implementation agent. No new cross-feature validation patterns are visible from this gate.
