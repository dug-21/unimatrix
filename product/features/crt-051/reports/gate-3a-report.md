# Gate 3a Report: crt-051

> Gate: 3a (Design Review)
> Date: 2026-04-08
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 3 components match architecture decomposition and contracts |
| Specification coverage | PASS | FR-01 through FR-12 all mapped; pseudocode follows AC-15 (architect's approach) over the internally inconsistent FR-12 |
| Risk coverage | PASS | All 8 risks (R-01 through R-08) have test plan coverage; R-01 grep gate and AC-17 two-test split both present |
| Interface consistency | PASS | Shared type `usize` for `contradiction_pair_count` consistent across all three components; OVERVIEW contracts match per-component usage |
| Knowledge stewardship — architect agent | FAIL | No `## Knowledge Stewardship` section in architect report |
| Knowledge stewardship — spec agent | WARN | Stewardship section present, `Queried:` entry present, but no `Stored:` or "nothing novel to store" disposition |
| Knowledge stewardship — risk strategist | PASS | Queried and Stored entries both present with reasons |
| Knowledge stewardship — pseudocode agent | PASS | Queried entries present; "Deviations: none" note implies nothing novel |
| Knowledge stewardship — test plan agent | PASS | Queried and "nothing novel to store" both present with reason |

---

## Detailed Findings

### Check 1: Architecture Alignment

**Status**: PASS

**Evidence**:

- `pseudocode/OVERVIEW.md` identifies exactly the three components called out in the architecture: `infra/coherence.rs`, `services/status.rs`, `mcp/response/mod.rs`. No additional components introduced.
- `pseudocode/coherence.md` new signature (`contradiction_pair_count: usize, total_active: u64) -> f64`) matches `ARCHITECTURE.md` "After" block exactly. Function body (guard, formula, clamp) is identical.
- `pseudocode/status.md` Phase 5 replacement matches the architecture's "After" code block exactly, including the required two-line ordering comment referencing Phase 2 and ADR-001.
- `pseudocode/response.md` specifies `contradiction_count: 15` / `contradiction_density_score: 0.7000` / `total_active: 50` — matches the architecture's "correct resolution" block exactly.
- `pseudocode/OVERVIEW.md` Unchanged Paths section preserves all constraints: `generate_recommendations()` signature unchanged, `DEFAULT_WEIGHTS.contradiction_density` unchanged, seven other fixtures unchanged.
- No ADR decisions violated. The only ADR (ADR-001) confirms pair-count semantics, cold-start = 1.0, and the `contradiction_count: 15` fixture approach — all reflected faithfully in pseudocode.

### Check 2: Specification Coverage

**Status**: PASS

**Evidence by FR**:

- FR-01 (new signature): `pseudocode/coherence.md` "New Signature" section.
- FR-02 (formula): `pseudocode/coherence.md` Function Body — `1.0 - (contradiction_pair_count AS f64 / total_active AS f64)` with `.clamp(0.0, 1.0)`.
- FR-03 (empty-database guard): `pseudocode/coherence.md` — `IF total_active == 0: RETURN 1.0`.
- FR-04 (cold-start 1.0): `pseudocode/coherence.md` Implementation Notes — cold-start covered by formula producing `1.0 - 0.0 = 1.0`.
- FR-05 (degenerate clamping): `pseudocode/coherence.md` — `score.clamp(0.0, 1.0)`.
- FR-06 (updated call site): `pseudocode/status.md` "New Code" block.
- FR-07 (generate_recommendations unchanged): `pseudocode/coherence.md` "Unchanged Function" section; `pseudocode/status.md` "Unchanged Code" section.
- FR-08 (no total_quarantined at scoring call site): `pseudocode/status.md` explicitly passes `report.contradiction_count`.
- FR-09 (updated doc comment): `pseudocode/coherence.md` "Doc Comment" section — old quarantine description absent, new pair-count description present.
- FR-10 (stale cache limitation): Doc comment includes "The cache is rebuilt approximately every 60 minutes. A stale cache is a known limitation (SR-07)."
- FR-11 (rewrite tests in coherence.rs): `pseudocode/coherence.md` Unit Tests section — 3 old tests renamed/rewritten, 2 new tests specified. Note: test plan specifies 6 tests (splitting cold-start into two cases per spawn-prompt requirement); pseudocode lists 5. This is a minor discrepancy in favor of the test plan being the authoritative specification for test count. Delivery should follow the 6-test plan.
- FR-12 (update response/mod.rs fixture): Pseudocode correctly follows the architect's approach (`contradiction_count: 15`, `contradiction_density_score: 0.7000`). The spec's FR-12 body text says "update to `contradiction_density_score: 1.0`" but AC-15 and the risk strategy both mandate `contradiction_count: 15` with `0.7000` preserved. The pseudocode follows AC-15 and the architecture — this is the correct resolution. The FR-12 body text is an internal spec inconsistency that pre-dates pseudocode; it does not affect delivery because AC-15 is the binding criterion.

**No scope additions**: Pseudocode contains no features not present in the specification.

**Non-functional requirements addressed**: NFR-01 (purity) confirmed by pure function body; NFR-04 (f64 precision) documented in test pseudocode with `1e-10` epsilon; NFR-02/NFR-03 confirmed by OVERVIEW "No types introduced, no new imports, no schema changes."

### Check 3: Risk Coverage

**Status**: PASS

**Evidence**:

- **R-01 (Critical — missed call site)**: `test-plan/status.md` Scenario S-02 specifies two grep patterns (`contradiction_density_score.*total_quarantined` and `total_quarantined.*contradiction_density_score`) with expected zero matches. `test-plan/OVERVIEW.md` Risk-to-Test Mapping row R-01 confirms this. Spawn-prompt-required grep verification is present.
- **R-02 (Critical — SR-02 fixture)**: `test-plan/response.md` Scenario R-01 explicitly asserts `contradiction_count: 15` and `contradiction_density_score: 0.7000`. Architect's approach confirmed.
- **R-03 (High — test rewrite incomplete)**: `test-plan/coherence.md` Tests 1–3 specify renamed tests with updated parameter semantics. No test name in test plan contains "quarantined".
- **R-04 (High — phase ordering)**: `test-plan/status.md` Scenarios S-03 (ordering comment) and S-05 (line number confirmation Phase 2 before Phase 5) both present.
- **R-05 (High — cold-start AC-17 missing)**: `test-plan/coherence.md` has two explicit cold-start tests (Tests 4 and 5: `contradiction_density_cold_start_cache_absent` and `contradiction_density_cold_start_no_pairs_found`), both using `total_active = 50` (non-zero). Spawn-prompt AC-17 two-test requirement is satisfied.
- **R-06 (High — generate_recommendations accidentally modified)**: `test-plan/status.md` Scenario S-04 reads both the signature in `coherence.rs` and the call site in `status.rs` (~line 784–790).
- **R-07 (Medium — degenerate formula path)**: `test-plan/coherence.md` Test 2 (`pairs_exceed_active`, 200/100 → 0.0) and Test 6 (`partial`, 5/100 → 0.95) together guard against operand-order inversion.
- **R-08 (Low — grep false-positive)**: `test-plan/status.md` Scenario S-02 includes explicit note on manual triage of matches.

All risks from the Risk-Based Test Strategy have at least one corresponding test scenario. Risk priorities are reflected in test plan emphasis (Critical risks have multi-scenario coverage; Low risk has a note-level treatment).

### Check 4: Interface Consistency

**Status**: PASS

**Evidence**:

- `pseudocode/OVERVIEW.md` establishes the shared type: `report.contradiction_count: usize`. All three component pseudocode files use this consistently.
- `coherence.md` new parameter is `contradiction_pair_count: usize` — matches `StatusReport.contradiction_count` type directly, no cast required. Matches architecture Integration Surface table.
- `status.md` passes `report.contradiction_count` (no cast) to the new `usize` parameter. Matches architecture.
- `response.md` sets `contradiction_count: 15` (untyped literal consistent with `usize` field type).
- `pseudocode/OVERVIEW.md` data-flow description matches per-component descriptions without contradiction.
- `generate_recommendations()` signature (`total_quarantined: u64` as fifth param) is consistently documented as unchanged in `coherence.md`, `status.md`, and `OVERVIEW.md`.

### Check 5: Knowledge Stewardship — Architect Agent

**Status**: FAIL

**Evidence**: `agents/crt-051-agent-1-architect-report.md` has no `## Knowledge Stewardship` section. The report contains "Outputs", "Key Decisions", "Integration Surprises Found", and "Open Questions" sections, but stewardship is entirely absent. The architect is an active-storage agent (stores ADRs); stewardship documentation of what was stored (ADR #4259) is required. The ADR was stored — the missing piece is the required section documenting it.

**Issue**: Missing `## Knowledge Stewardship` section in architect report. Must be added with `Stored: entry #4259 "ADR-001 contradiction_density_score input source" via /uni-store-adr`.

### Check 6: Knowledge Stewardship — Spec Agent

**Status**: WARN

**Evidence**: `agents/crt-051-agent-2-spec-report.md` has `## Knowledge Stewardship` section with `Queried: mcp__unimatrix__context_briefing` entry. However, there is no `Stored:` entry and no "nothing novel to store -- {reason}" line. The spec agent is an active-storage agent category — after resolving VARIANCE-01 and AC-15 (a discrepancy between the spec's FR-12 text and the architect's approach), a lesson about "spec FR vs AC precedence when internally inconsistent" could be relevant, or the agent should have explicitly stated "nothing novel to store" with a reason.

**Issue (WARN, not FAIL)**: Stewardship section is present and `Queried:` is documented. Only the `Stored:` disposition is missing. This is a minor gap, not a missing section.

### Check 7: Knowledge Stewardship — Risk Strategist

**Status**: PASS

**Evidence**: `RISK-TEST-STRATEGY.md` contains `## Knowledge Stewardship` with `Queried:` (entries #2758, #3253, #3946, #4258, #4257 listed) and `Stored: nothing novel to store -- crt-051 risks are feature-specific. The SR-02 architect-vs-spec discrepancy pattern is too narrow to generalize.` Full compliance.

### Check 8: Knowledge Stewardship — Pseudocode Agent

**Status**: PASS

**Evidence**: `agents/crt-051-agent-1-pseudocode-report.md` has `## Knowledge Stewardship` with three `Queried:` entries (briefing, search). Ends with "Deviations from established patterns: none." Pseudocode agents are read-only; `Queried:` entries are required and present. The implicit "nothing to store" is adequately communicated. PASS.

### Check 9: Knowledge Stewardship — Test Plan Agent

**Status**: PASS

**Evidence**: `agents/crt-051-agent-2-testplan-report.md` has `## Knowledge Stewardship` with `Queried:` entries (briefing, entries #4258, #4257, #4259, #4202 listed) and `Stored: nothing novel to store -- the fixture audit pattern (#4258) and scoring function semantic change pattern were already stored by the architect agent. The AC-17 two-test split pattern is feature-specific.` Full compliance.

---

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| Missing `## Knowledge Stewardship` section in architect report | crt-051-agent-1-architect | Add section to `agents/crt-051-agent-1-architect-report.md` with `Stored: entry #4259 "ADR-001 contradiction_density_score input source" via /uni-store-adr` |

---

## Non-Blocking Notes for Delivery

1. **Pseudocode vs test-plan test count discrepancy**: `pseudocode/coherence.md` specifies 5 tests (3 rewrites + 2 new); `test-plan/coherence.md` specifies 6 tests (3 rewrites + 3 new, splitting cold-start into two cases). The 6-test plan is authoritative per the spawn prompt requirement. Delivery must implement 6 tests.

2. **FR-12 internal spec inconsistency**: `SPECIFICATION.md` FR-12 body says update `contradiction_density_score` to `1.0`, but AC-15 says keep `0.7000` and set `contradiction_count: 15`. The pseudocode, architecture, and risk strategy are all aligned on AC-15. Delivery follows AC-15 — no code action needed, but note the discrepancy exists.

3. **Spec agent stewardship (WARN)**: `agents/crt-051-agent-2-spec-report.md` is missing the `Stored:` disposition in its stewardship section. This is a documentation gap only; it does not block delivery.
