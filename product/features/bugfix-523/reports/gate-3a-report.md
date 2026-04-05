# Gate 3a Report: bugfix-523

> Gate: 3a (Design Review)
> Date: 2026-04-05
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All four items match approved architecture; component boundaries, file assignments, structural landmarks correct |
| Specification coverage | PASS | FR-01 through FR-04 fully implemented in pseudocode; all 29 ACs have test coverage; constraints honored |
| Risk coverage | PASS | All 12 risks (R-01 through R-12) map to named test scenarios; Critical risks fully covered |
| Interface consistency | PASS | Shared types in OVERVIEW.md consistent with per-component usage; no contradictions |
| Item 1 gate placement | PASS | Gate placed AFTER candidate_pairs.is_empty() and BEFORE get_provider().await — correct per architecture |
| Item 2 log sites | PASS | Exactly two warn! sites changed; non-finite cosine site explicitly documented as unchanged |
| Item 3 field count | PASS | All 19 fields enumerated with guard forms; Groups A/B/C correct |
| Item 4 insertion order | PASS | Order is (1) capability check, (2) sanitize guard, (3) payload extraction, (4) registry calls |
| AC-04/AC-05 test strategy | PASS | Behavioral-only per ADR-001(c)/entry #4143; documented with required verbatim statement |
| AC-01 non-empty candidates note | PASS | Explicitly noted in pseudocode, test plan, and IMPLEMENTATION-BRIEF |
| nan-guards test count | PASS | Exactly 21 named test functions: 19 NaN + 2 Inf |
| Knowledge stewardship | PASS | All pseudocode files have Knowledge Stewardship sections with Queried: entries |

---

## Detailed Findings

### Check 1: Architecture Alignment

**Status**: PASS

**Evidence**:

`pseudocode/OVERVIEW.md` maps all four items to the correct source files defined in
ARCHITECTURE.md's component breakdown:
- Item 1: `nli_detection_tick.rs` — matches architecture
- Item 2: `nli_detection_tick.rs` — matches architecture
- Item 3: `infra/config.rs` — matches architecture
- Item 4: `uds/listener.rs` — matches architecture

The data-flow diagram in `pseudocode/OVERVIEW.md` is identical to the architecture's
component interaction map. Items 1 and 2 are noted as requiring the same implementation
agent (C-08 / SR-06 constraint), consistent with architecture requirement.

The integration surface table in ARCHITECTURE.md matches the function and constant
references in each pseudocode file. No new abstractions, types, or dependencies are
introduced.

Technology decisions match: no new crates, no schema changes, no new error variants.

ADR-001 decisions (gate placement, 19-field scope, behavioral-only test strategy) are
reflected in all four pseudocode files and both test plan overview files.

---

### Check 2: Specification Coverage

**Status**: PASS

**FR-01 (NLI gate)**:
`pseudocode/nli-tick-gate.md` implements FR-01 exactly: inserts
`if !config.nli_enabled { tracing::debug!("..."); return; }` at the PATH B entry gate.
Debug message text matches the prescribed string. Comment update at `get_provider()` call
site is specified. C-01 (background.rs unchanged) is explicitly stated as an invariant.

**FR-02 (log downgrade)**:
`pseudocode/log-downgrade.md` implements FR-02 exactly: changes `warn!` to `debug!` at
exactly two sites. The table of three log sites clearly marks the non-finite cosine site as
unchanged. Before/after blocks are provided for both changed sites.

**FR-03 (NaN guards)**:
`pseudocode/nan-guards.md` implements FR-03 exactly: covers all 19 fields with correct guard
forms and correct dereference for loop-body fields (Groups B and C). The `!v.is_finite() ||`
prefix form matches the specification for both exclusive (`<=`/`>=`) and inclusive (`<`/`>`)
guard forms. crt-046 exclusion is explicitly noted.

**FR-04 (session sanitization)**:
`pseudocode/session-sanitization.md` implements FR-04 exactly: inserts the guard in the
correct arm with the correct pattern (mirroring lines 731–738 of the general RecordEvent arm),
correct error code (`ERR_INVALID_PAYLOAD`), and correct message qualifier `(rework_candidate)`.

All NFRs are addressed: NFR-01 (Path A/C unconditional) covered by gate placement after both
paths; NFR-02 (validate at startup) inherent in function placement; NFR-03 (O(1) sync call)
addressed by using existing synchronous function; NFR-04 (no new deps) explicitly stated;
NFR-05 (no regression) addressed by AC-27 and AC-29.

No scope additions detected. No unrequested features implemented.

---

### Check 3: Risk Coverage

**Status**: PASS

All 12 risks from RISK-TEST-STRATEGY.md map to test scenarios in the test plans.

**R-01 (Path A/C accidentally gated)**: Covered by T-02 (`test_nli_gate_path_a_informs_edges_still_written_nli_disabled`) and T-03 (`test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled`). Both are present in `test-plan/nli-tick-gate.md`. Both are marked non-negotiable.

**R-02 (gate at wrong structural position)**: Covered by T-03 (structural proof: cosine Supports edges with `nli_enabled=false` can only come from Path C, proving gate is after `run_cosine_supports_path`). Code inspection of `// === PATH B entry gate ===` landmark is specified as required.

**R-03 (19-field NaN coverage)**: Covered by 19 individually named NaN tests in `test-plan/nan-guards.md`, plus 2 representative Inf tests. Count requirement (exactly 19) documented. Individual naming is present.

**R-04 (guard after session_id first use)**: Covered by T-08 (AC-28 runtime test) plus mandatory code inspection requirement. Both are required; `test-plan/session-sanitization.md` explicitly states the test alone is insufficient.

**R-05 (wrong warn! site downgraded)**: Covered by T-05, T-06, T-07 (behavioral) plus mandatory code review checklist requiring confirmation of exactly two changes and unchanged non-finite cosine site. `test-plan/log-downgrade.md` includes this checklist.

**R-06 (test module absent)**: Addressed by Gate 3a presence-count requirement in `test-plan/nan-guards.md` (count = 21 required before pass). Named test function lists are in pseudocode and test plan files.

**R-07 (wrong field name string)**: Addressed in `test-plan/nan-guards.md` with explicit spot-check procedure for AC-17 through AC-24 against the `fusion_weight_checks` array.

**R-08 (AC-29 regression)**: Covered by T-09 (`test_dispatch_rework_candidate_valid_path_not_regressed`).

**R-09 (AC-03 regression)**: Covered by T-04 (`test_nli_gate_nli_enabled_path_not_regressed`).

**R-10 (AC-27 regression)**: Covered by `cargo test -p unimatrix-server -- infra::config` clean run and explicit boundary-value check for `w_sim`.

**R-11 (AC-04/AC-05 behavioral-only unacknowledged)**: Addressed in all log-downgrade pseudocode and test plan files with the verbatim required gate report statement. `test-plan/OVERVIEW.md` repeats this requirement. Also addressed in IMPLEMENTATION-BRIEF.md as the authoritative WARN-1 resolution.

**R-12 (cross-field invariant NaN pass-through)**: Documented as covered upstream by AC-07 + AC-08. No additional test required. Pseudocode correctly notes the per-field guard fires before cross-field checks.

Integration risks are addressed: SR-06 same-agent constraint documented; `background.rs` unchanged verification specified in test-plan OVERVIEW.

---

### Check 4: Interface Consistency

**Status**: PASS

`pseudocode/OVERVIEW.md` defines shared types:

| Type | Component usage | Consistent? |
|------|----------------|-------------|
| `InferenceConfig` (`nli_enabled: bool`) | `nli-tick-gate.md` — reads `config.nli_enabled`; `nan-guards.md` — validates float fields | Yes |
| `ConfigError::NliFieldOutOfRange` | `nan-guards.md` only | Yes — architecture specifies this is the established variant |
| `HookResponse::Error { code: i64, message: String }` | `session-sanitization.md` only | Yes |
| `HookEvent` (field `session_id: String`) | `session-sanitization.md` — references `&event.session_id` | Yes |
| `ERR_INVALID_PAYLOAD` | `session-sanitization.md` only | Yes |

No contradictions between component files. Data flow diagram in OVERVIEW.md is consistent
with per-component pseudocode. The structural landmark table in OVERVIEW.md matches the
line references in each pseudocode file.

Interface surface table in ARCHITECTURE.md:

| Interface | Pseudocode usage |
|-----------|-----------------|
| `config.nli_enabled` | `nli-tick-gate.md` — gate condition |
| `nli_handle.get_provider()` | `nli-tick-gate.md` — explicitly noted as what gate avoids |
| `sanitize_session_id(s: &str) -> Result<(), String>` | `session-sanitization.md` — called with `&event.session_id` |
| `ERR_INVALID_PAYLOAD` | `session-sanitization.md` — returned in `HookResponse::Error.code` |
| `ConfigError::NliFieldOutOfRange { path, field, value, reason }` | `nan-guards.md` — all 19 guards use this variant |

No discrepancies found.

---

### Check 5: Item 1 Gate Placement (Spawn Prompt Key Validation)

**Status**: PASS

The spawn prompt specifies: "gate placement AFTER `candidate_pairs.is_empty()` fast-exit, BEFORE `get_provider().await`. If pseudocode places it earlier (gating Path C too), that is a REWORKABLE FAIL."

`pseudocode/nli-tick-gate.md` shows the following structural sequence:

```
// [unchanged] if candidate_pairs.is_empty() { return; }
// [NEW]       if !config.nli_enabled { tracing::debug!("..."); return; }
// [unchanged] let provider = match nli_handle.get_provider().await { ... }
```

The gate is explicitly after `candidate_pairs.is_empty()` and before `get_provider().await`. Path C (`run_cosine_supports_path`) completes before this region — the pseudocode shows its `.await` call ending at line 544 and the PATH B entry gate comment block starting at line 546. The gate does NOT precede `run_cosine_supports_path`.

ADR-001(a) structural invariant (Path A and Path C unconditional) is preserved. The
IMPLEMENTATION-BRIEF.md resolved decision confirms the structural landmark explicitly.

---

### Check 6: Item 2 Log Sites (Spawn Prompt Key Validation)

**Status**: PASS

The spawn prompt specifies: "pseudocode must change exactly two warn! sites. If the non-finite cosine site is included, that is a REWORKABLE FAIL."

`pseudocode/log-downgrade.md` provides a table of three log sites:

| Line | Site | Current Level | After Change |
|------|------|---------------|--------------|
| 765–770 | Non-finite cosine guard | `warn!` | `warn!` — UNCHANGED |
| 796–800 | `category_map.get(src_id)` None arm | `warn!` | `debug!` — CHANGED |
| 806–810 | `category_map.get(tgt_id)` None arm | `warn!` | `debug!` — CHANGED |

The non-finite cosine site is explicitly marked UNCHANGED with the comment "THIS SITE IS UNCHANGED — do NOT downgrade". Only two sites change. The rationale (operational anomaly vs. expected degraded-mode) is correctly cited from entry #3467.

---

### Check 7: Item 3 Field Count (Spawn Prompt Key Validation)

**Status**: PASS

The spawn prompt specifies: "pseudocode must enumerate all 19 fields with guard forms. Group B/C must show loop-body prefix with correct dereference. Count of fields: exactly 19."

`pseudocode/nan-guards.md` enumerates:
- Group A: 11 fields (table with line ranges)
- Group B: 6 fields (w_sim, w_nli, w_conf, w_coac, w_util, w_prov)
- Group C: 2 fields (w_phase_histogram, w_phase_explicit)
- Total: 11 + 6 + 2 = 19

Group B and C loop-body prefix uses `!value.is_finite() || ` (without `*` dereference for the
is_finite call, using `*value` for comparisons). This is correct: `value` is `&f64`, so
`value.is_finite()` auto-derefs correctly, and `*value < 0.0` uses explicit deref for
comparison — consistent with the existing guard form. The implementation note in the
pseudocode explains this dereference behavior explicitly.

All 19 fields match the SPECIFICATION.md field checklist. No fields are missing or added.

---

### Check 8: Item 4 Insertion Order (Spawn Prompt Key Validation)

**Status**: PASS

The spawn prompt specifies: "insertion order must be (1) capability check, (2) sanitize guard, (3) payload extraction, (4) registry calls. Wrong order = REWORKABLE FAIL."

`pseudocode/session-sanitization.md` shows the following structural insertion order:

```
[1] Capability check — EXISTING, unchanged
[2] sanitize_session_id guard — NEW, inserted here
[3] Payload field extraction — EXISTING, unchanged
[4] record_rework_event + record_topic_signal + observation spawn — EXISTING, unchanged
```

The before/after blocks confirm event.session_id is not used between steps 1 and 2. The
structural constraint is documented as a maintenance invariant. This matches architecture
specification exactly (ARCHITECTURE.md SR-05 compliance section, SPECIFICATION.md C-07).

---

### Check 9: AC-04/AC-05 Test Strategy (Spawn Prompt Key Validation)

**Status**: PASS

The spawn prompt specifies: "AC-04/AC-05 must be documented as behavioral-only with citation to ADR-001(c)/entry #4143. If test plan prescribes tracing-test harness for these ACs, that is a REWORKABLE FAIL."

Multiple artifacts document the behavioral-only decision:

1. `pseudocode/log-downgrade.md`: "Log level is NOT asserted in tests per ADR-001(c) (entry #4143). Coverage is behavioral-only." Includes the verbatim required gate report statement.

2. `test-plan/log-downgrade.md`: Opens with the mandatory gate acknowledgment block citing ADR-001(c) entry #4143. States: "the `tracing-test` harness is not to be added."

3. `test-plan/OVERVIEW.md`: Repeats the behavioral-only decision with the verbatim statement and cites IMPLEMENTATION-BRIEF.md as the authoritative source.

4. `IMPLEMENTATION-BRIEF.md` Design Decisions section: "This is the authoritative decision. It supersedes SPECIFICATION.md's 'Option A preferred' statement."

No pseudocode or test plan file prescribes the `tracing-test` harness for AC-04 or AC-05. The SPECIFICATION.md "Option A preferred" language has been explicitly superseded by the IMPLEMENTATION-BRIEF.md WARN-1 resolution.

---

### Check 10: AC-01 Non-Empty Candidates Requirement (Spawn Prompt Key Validation)

**Status**: PASS

The spawn prompt specifies: "AC-01 test must note the non-empty candidates requirement."

`pseudocode/nli-tick-gate.md` AC-01 scenario:
> "NOTE: candidate_pairs must be non-empty to reach the nli_enabled check. The empty-candidates fast-exit fires before the nli_enabled gate, so an empty pair list would not exercise the new gate."

`test-plan/nli-tick-gate.md` T-01:
> "Construct `candidate_pairs: Vec<(u64, u64, f32)>` with **at least one pair** — e.g., `vec![(1, 2, 0.85_f32)]`. This is mandatory: if the pair list is empty, the `candidate_pairs.is_empty()` fast-exit fires before the `nli_enabled` check and the new gate is never exercised."

`IMPLEMENTATION-BRIEF.md` Item 1 test requirements:
> "**This test must use non-empty `candidate_pairs`** — if the pair list is empty the `candidate_pairs.is_empty()` fast-exit fires before the `nli_enabled` check, so the new gate is never reached."

The requirement is noted in three separate artifacts.

---

### Check 11: nan-guards Test Count (Spawn Prompt Key Validation)

**Status**: PASS

The spawn prompt specifies: "exactly 21 named test functions for nan-guards.md (19 NaN + 2 Inf). Count mismatch = REWORKABLE FAIL."

`pseudocode/nan-guards.md` "Required Test Function Names" section lists:
- Group A NaN tests: 11 functions (`test_nan_guard_nli_entailment_threshold` through `test_nan_guard_supports_cosine_threshold`)
- Group B NaN tests: 6 functions (`test_nan_guard_w_sim` through `test_nan_guard_w_prov`)
- Group C NaN tests: 2 functions (`test_nan_guard_w_phase_histogram`, `test_nan_guard_w_phase_explicit`)
- Inf tests: 2 functions (`test_inf_guard_nli_entailment_threshold_f32`, `test_inf_guard_ppr_alpha_f64`)
- Total: 11 + 6 + 2 + 2 = **21**

`test-plan/nan-guards.md` "Gate 3a Presence Verification" section confirms: "Count: 11 + 6 + 2 + 2 = **21 tests**. Count must equal 21 before Gate 3a passes."

`IMPLEMENTATION-BRIEF.md` lists all 21 required test function names explicitly.

All 21 function names match between pseudocode and test plan files. There are no discrepancies.

---

### Check 12: Knowledge Stewardship Compliance

**Status**: PASS

All design-phase and pseudocode artifacts include Knowledge Stewardship sections:

**Pseudocode files** (read-only agents — Queried: entries required):

- `pseudocode/nli-tick-gate.md`: Knowledge Stewardship section present. Queried entries: Pattern #3675, ADR-001 (entry #4017), ADR-001 (entry #4143). "Deviations from established patterns: none."

- `pseudocode/log-downgrade.md`: Knowledge Stewardship section present. Queried entries: Entry #3467, ADR-001(c) (entry #4143). "Deviations from established patterns: none."

- `pseudocode/nan-guards.md`: Knowledge Stewardship section present. Queried entries: Lesson #4132, crt-046 guards as implementation reference. "Deviations from established patterns: none."

- `pseudocode/session-sanitization.md`: Knowledge Stewardship section present. Queried entries: Entry #3921, Entry #3902, ADR-001 (entry #4143). "Deviations from established patterns: none."

**Upstream source documents** (active-storage agents — Stored: or Declined: entries required):

- `RISK-TEST-STRATEGY.md` (risk-strategist): Knowledge Stewardship section present. Queried entries documented (four searches). Stored: "nothing novel to store — risks are feature-specific. The tracing-test behavioral-only pattern is already captured in #3935. The NaN guard pattern is already in #4133."

- `specification/SPECIFICATION.md` (specifier): Knowledge Stewardship section present. Queried entries documented. Stored: not explicitly shown as Stored/Declined in a block format — this is a WARN (minor, does not block).

**Test plan files**:

- `test-plan/OVERVIEW.md`: Knowledge Stewardship section present. Queried entries documented (four entries cited). Stored: "nothing novel to store at this stage — patterns already captured in #4133 and #4142."

No stewardship sections are missing. The specification WARN noted above does not block — the queried entries are present and the "nothing novel" decision is made (implied), though not using the exact "Declined:" or "nothing novel to store — {reason}" format. Given the overall completeness, this is a WARN not a FAIL.

---

## Rework Required

None.

---

## Warnings

| Warning | Artifact | Detail |
|---------|----------|--------|
| Stewardship format in specification | `specification/SPECIFICATION.md` | Queried entries are present but the Stored/Declined entry does not follow the exact `Stored:` / `nothing novel to store -- {reason}` format. The queried section ends with a statement that the content was confirmed current. Acceptable but not precisely on format. |

---

## Gate Report Acknowledgment (required by ADR-001(c))

Per R-11 and ADR-001(c) (Unimatrix entry #4143), this gate report documents:

"AC-04 and AC-05 log-level assertions are behavioral-only per ADR-001(c) (Unimatrix entry #4143). Log level verified by code review. No `tracing-test` harness used."

Cross-field NaN pass-through (R-12) is caught upstream by per-field guards (AC-07 + AC-08). No additional cross-field NaN test is required.

`f32::NEG_INFINITY` and `f64::NEG_INFINITY` are caught by `!v.is_finite()` — the two representative Inf tests (AC-25, AC-26) use positive infinity; negative infinity is not a separate test requirement. Documented here per edge case section of RISK-TEST-STRATEGY.md.

---

## Knowledge Stewardship

- Stored: nothing novel to store -- gate findings are feature-specific. All systemic patterns (NaN guard omission, behavioral-only log level test strategy, session guard omission) are already captured in Unimatrix entries #4133, #3935, and #3921 respectively. No new cross-feature validation pattern is visible from this batch.
