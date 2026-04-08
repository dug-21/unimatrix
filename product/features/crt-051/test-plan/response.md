# Test Plan: response/mod.rs
# crt-051 — Test Fixture Update in make_coherence_status_report()

## Component Responsibility

`crates/unimatrix-server/src/mcp/response/mod.rs`

Serialization and formatting of `StatusReport` into MCP response text. Contains test
fixtures that construct `StatusReport` with hardcoded field values. The `make_coherence_status_report()`
helper function (line ~1397) is the only fixture requiring change.

---

## Change Summary

One field update in `make_coherence_status_report()`:

| Field | Before | After | Reason |
|-------|--------|-------|--------|
| `contradiction_count` | `0` | `15` | New formula: `1.0 - 15/50 = 0.7000` exactly |
| `contradiction_density_score` | `0.7000` | `0.7000` (unchanged) | Preserved — mathematically consistent now |
| `coherence` | `0.7450` | `0.7450` (unchanged) | Independently hardcoded; must not change |
| `total_quarantined` | `3` | `3` (unchanged) | Not an input to scoring; unchanged |

---

## No New Tests Required

No new test functions are needed in `response/mod.rs`. The existing test suite
(`test_coherence_markdown_section` and related tests at ~line 1458) exercises the
fixture through `format_status_report()` and will continue to pass after the field update.

The fixture update is a **semantic correction** — the fixture now represents a coherent
database state under new scoring semantics rather than an internally inconsistent one.

---

## Verification Scenarios (All Static — Read)

### Scenario R-01: AC-15 — make_coherence_status_report() fields consistent with new formula

**Method**: Read `make_coherence_status_report()` at ~line 1397.

**Assertions**:

1. `contradiction_count: 15`
   - Confirms the architect's approach is used (not the spec writer's approach of setting score to 1.0)
   - Makes the formula verifiable by inspection: `1.0 - 15.0/50.0 = 0.7000`

2. `contradiction_density_score: 0.7000`
   - Unchanged from pre-fix value
   - Now semantically justified: 15 pairs / 50 active entries = 0.70 density ratio

3. `total_active: 50`
   - Must be present (already in fixture) to make the formula relationship checkable

4. `coherence: 0.7450`
   - Unchanged — this is an independently hardcoded Lambda value, not computed from
     dimension scores in this fixture. Must remain 0.7450.

5. `total_quarantined: 3`
   - Unchanged — this field is no longer an input to `contradiction_density_score()`
     but remains a valid `StatusReport` field for `generate_recommendations()`.

**AC Coverage**: AC-15, R-02

---

### Scenario R-02: AC-15 — Seven other fixtures retain contradiction_density_score: 1.0

**Method**: Read / Grep

**What to check**: The seven other fixtures in `response/mod.rs` that construct
`StatusReport` with `contradiction_density_score` all have:
- `contradiction_density_score: 1.0`
- `contradiction_count: 0` (or `contradiction_count: 1` where the test is about
  contradiction pairs in the `contradictions` Vec — see note below)

These are consistent with the new semantics (0 detected pairs → score 1.0) and require
no change.

**Note**: Several fixtures (around lines 958, 1037, 1118 in the current file) have
`contradiction_count: 1` paired with `contradiction_density_score: 1.0`. This appears
inconsistent at first glance, but is acceptable: those fixtures may be testing
contradiction formatting (the `contradictions: vec![pair]` field) rather than scoring
semantics. Verify the fixtures are internally consistent and that no test asserts
`contradiction_density_score: 1.0` while relying on the old quarantine-based formula.

**Grep assertion**:
```
Pattern: contradiction_density_score
File: crates/unimatrix-server/src/mcp/response/mod.rs
Expected: 8 matches total (1 at 0.7000, 7 at 1.0)
```

**AC Coverage**: AC-15 (no spurious changes), R-02

---

### Scenario R-03: Existing tests continue to pass

**Method**: `cargo test -p unimatrix-server mcp::response`

**What to check**: All response formatting tests pass after the fixture update.

Key test to verify: `test_coherence_markdown_section` (~line 1459) calls
`make_coherence_status_report()` and calls `format_status_report()`. It asserts on
text content (`contains("### Coherence")`, `contains("**Lambda**")`, etc.) but does NOT
assert the value `0.7000` directly. The field update therefore does not break this test.

If any test in `response/mod.rs` does assert the string `"0.7000"` or `"0.70"` — that
assertion would now be wrong if `contradiction_density_score` were changed to 1.0. By using
the architect's approach (keeping score at `0.7000`), all such string assertions are
preserved.

**AC Coverage**: AC-10, AC-11

---

## Fixture Audit Context (per Pattern #4258)

Pattern #4258 (Unimatrix entry) established: "when a scoring function's semantics change,
search for ALL hardcoded output values across every test fixture file — not just the
function's own unit tests."

Full audit for `contradiction_density_score` values in `response/mod.rs`:

| Approx Line | Fixture Context | Value | Change Needed? |
|-------------|----------------|-------|----------------|
| ~603–616 | Unknown fixture | `0`, `1.0` | No — 0 pairs → 1.0 is correct |
| ~697–710 | Unknown fixture | `0`, `1.0` | No — 0 pairs → 1.0 is correct |
| ~958–971 | Contradiction pair fixture | `1`, `1.0` | No — see note in R-02 |
| ~1037–1050 | Contradiction pair fixture | `1`, `1.0` | No — see note in R-02 |
| ~1118–1131 | Contradiction pair fixture | `1`, `1.0` | No — see note in R-02 |
| ~1411–1422 | `make_coherence_status_report()` | `0` → `15`, `0.7000` unchanged | **YES — SR-02** |

All `contradiction_density_score: 1.0` fixtures require no change under new semantics.
Only the `0.7000` fixture requires `contradiction_count: 0` → `15`.

---

## Risks Covered by This Component

| Risk | Covered By |
|------|-----------|
| R-02 (SR-02 fixture semantic divergence) | Scenario R-01 (AC-15 read) |
| R-02 variant (spec writer's approach inadvertently used) | Scenario R-01 assertion #1 (`contradiction_count: 15` confirmed) |
| R-02 variant (spurious changes to other fixtures) | Scenario R-02 (7 fixtures retain 1.0) |
