# Test Plan Overview: crt-051
# Fix contradiction_density_score() — Replace Quarantine Proxy with Real Contradiction Count

## Test Strategy Summary

crt-051 is a surgical three-file fix with a pure function at its center. The testing surface
is deliberately small: one scoring function rewrite, one call site change, and one fixture
field update. The risk profile is dominated by silent-pass failure modes — wrong values that
compile and run without errors. The strategy therefore combines:

1. **Unit tests** (coherence.rs) — verify the pure function's arithmetic and boundary
   behavior under the new semantics, including the two required cold-start cases.
2. **Static verification** (grep assertions) — AC-06, AC-09: confirm the call site passes
   the correct field and that `total_quarantined` no longer appears at the scoring call site.
3. **Read verification** — AC-07/AC-16: confirm the phase-ordering comment is present in
   status.rs; AC-08: confirm `generate_recommendations()` is unchanged.
4. **Fixture integrity** (response/mod.rs) — confirm `contradiction_count: 15` and
   `contradiction_density_score: 0.7000` in `make_coherence_status_report()`.
5. **Integration smoke gate** — mandatory minimum for all changes.

No integration test additions are required. See Integration Harness Plan below.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Risk Description | Test Method | AC(s) |
|---------|----------|-----------------|-------------|-------|
| R-01 | Critical | `total_quarantined` still at scoring call site | Grep: `contradiction_density_score.*total_quarantined` → 0 matches | AC-09 |
| R-02 | Critical | SR-02 fixture retains stale 0.7000 or uses wrong approach | Read `make_coherence_status_report()`: confirm `contradiction_count: 15`, `contradiction_density_score: 0.7000` | AC-15 |
| R-03 | High | Test rewrite incomplete — first arg type still u64 | Read coherence.rs test block; confirm names and parameter semantics | AC-14 |
| R-04 | High | Phase ordering violated — Phase 5 uses stale contradiction_count: 0 | Read status.rs Phase 2 and Phase 5; confirm comment documents dependency | AC-07, AC-16 |
| R-05 | High | Cold-start AC-17 missing — no dedicated test for (0, N>0) | Read coherence.rs: confirm two cold-start tests exist, both with non-zero total_active | AC-17 |
| R-06 | High | `generate_recommendations()` accidentally modified | Read signature + call site in status.rs ~784–790 | AC-08 |
| R-07 | Medium | Degenerate formula path untested | Unit tests: pairs_exceed_active (→ 0.0), partial (→ 0.95) | AC-04, AC-05 |
| R-08 | Low | Grep gate false-positive on AC-09 | Scope grep to exclude comments and test helpers; verify zero production matches | AC-09 |

---

## Cross-Component Test Dependencies

The three components form a linear dependency chain:

```
coherence.rs (pure function)
    ↓ used by
status.rs (call site) — uses report.contradiction_count populated by Phase 2
    ↓ serialized into
response/mod.rs (fixture) — hardcodes field values consistent with new semantics
```

The unit tests in `coherence.rs` are independent of status.rs and response/mod.rs.
The fixture in `response/mod.rs` must be internally consistent with the new formula
(`contradiction_count: 15`, `total_active: 50`, `contradiction_density_score: 0.7000`).
The call site in `status.rs` is verified by grep and read, not by a new test.

---

## Test Counts by Component

| Component | Tests Rewritten | New Tests | Fixture Updates | Grep Assertions |
|-----------|----------------|-----------|-----------------|-----------------|
| coherence.rs | 3 | 2 | — | — |
| status.rs | 0 | 0 | — | 2 (AC-06, AC-09) |
| response/mod.rs | 0 | 0 | 1 field update | — |
| **Total delta** | **3** | **2** | **1** | **2** |

---

## Integration Harness Plan

### Applicable Suites

| Suite | Reason | Action |
|-------|--------|--------|
| `smoke` | Mandatory gate for all changes | Run: `python -m pytest suites/ -v -m smoke --timeout=60` |
| `tools` | `context_status` is one of the 9 exercised tools | Run: `python -m pytest suites/test_tools.py -v --timeout=60` |
| `confidence` | Lambda scoring is part of the confidence/coherence system | Run: `python -m pytest suites/test_confidence.py -v --timeout=60` |

Suites NOT required: `lifecycle`, `volume`, `security`, `contradiction`, `edge_cases`,
`protocol` — no schema changes, no lifecycle flows, no security boundaries, no API contract
changes. The contradiction suite tests scan infrastructure which is not modified.

### New Integration Tests Required

None. The behavior change is entirely within the Lambda computation. The `context_status`
tool's MCP interface is unchanged — same parameters, same JSON response field names and
types. The only observable MCP-level change is that `contradiction_density_score` values
will differ on databases with detected contradiction pairs, but the test harness uses a
freshly built binary against fresh in-memory state, which will have no contradiction pairs
(cold-start). Cold-start produces `contradiction_density_score: 1.0` both before and after
the fix — the existing integration assertions are therefore unaffected.

Rationale for no new integration tests: the fix is a pure-function input substitution. The
behavior difference is only observable when: (a) the contradiction scan has run, and (b)
contradiction pairs exist in the database. The integration harness runs against a fresh
binary with an empty database — neither condition holds. Unit tests in `coherence.rs` give
complete formula coverage. The fixture in `response/mod.rs` provides the only non-trivial
score path (`contradiction_count: 15`, score `0.7000`), and that fixture is exercised by
`test_coherence_markdown_section` and related tests in the existing test suite.

### Fixture Audit (per entry #4258 pattern)

Seven fixtures in `response/mod.rs` have `contradiction_density_score: 1.0` and
`contradiction_count: 0` — consistent with new semantics (0 pairs → 1.0, no change needed).
One fixture (`make_coherence_status_report`) has `contradiction_density_score: 0.7000` and
`contradiction_count: 0` — requires update to `contradiction_count: 15` (SR-02).
No other files contain hardcoded `contradiction_density_score` values.

---

## Acceptance Criteria Index

| AC-ID | Component | Verification Method |
|-------|-----------|---------------------|
| AC-01 | coherence.rs | Read function signature |
| AC-02 | coherence.rs | Unit test `contradiction_density_no_pairs` |
| AC-03 | coherence.rs | Unit test `contradiction_density_zero_active` |
| AC-04 | coherence.rs | Unit test `contradiction_density_pairs_equal_active` (or `pairs_exceed_active`) |
| AC-05 | coherence.rs | Unit test `contradiction_density_partial` |
| AC-06 | status.rs | Grep for call site argument |
| AC-07 | status.rs | Read Phase 2 + Phase 5 ordering comment |
| AC-08 | status.rs | Read `generate_recommendations()` signature and call site |
| AC-09 | status.rs | Grep: `total_quarantined.*contradiction_density_score` → 0 matches |
| AC-10 | coherence.rs | `cargo test -p unimatrix-server infra::coherence` exits 0 |
| AC-11 | all | `cargo test --workspace` exits 0 |
| AC-12 | all | `cargo clippy --workspace -- -D warnings` exits 0 |
| AC-13 | coherence.rs | Read doc comment |
| AC-14 | coherence.rs | Read test block (names, types, cases) |
| AC-15 | response/mod.rs | Read `make_coherence_status_report()` fields |
| AC-16 | status.rs | Read Phase 5 comment |
| AC-17 | coherence.rs | Read test block (two cold-start tests with non-zero total_active) |
