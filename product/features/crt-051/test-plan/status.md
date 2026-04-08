# Test Plan: status.rs
# crt-051 — Call Site Change in compute_report()

## Component Responsibility

`crates/unimatrix-server/src/services/status.rs`

`compute_report()` orchestrates multi-phase status data collection and assembles
`StatusReport`. Phase 2 (~line 583) reads the `ContradictionScanCacheHandle` and sets
`report.contradiction_count`. Phase 5 (~line 747) computes Lambda scores, including
calling `contradiction_density_score()`.

The change is one argument substitution at the Phase 5 call site: `report.total_quarantined`
→ `report.contradiction_count`. A phase-ordering comment is also added.

---

## No New Tests Required

No new unit tests are added in `status.rs` for this change. The specification confirms:
"No integration test in `status.rs` directly asserts a value for `contradiction_density_score`."

The call site change is verified by **static analysis** (grep + read), not runtime tests.
This is appropriate because:
1. The change is one argument substitution at a single call site.
2. The pure function's behavior is fully covered by unit tests in `coherence.rs`.
3. Integration tests in the harness run against a fresh binary with an empty database
   (cold-start), which produces `contradiction_density_score: 1.0` identically under both
   old and new semantics.

---

## Verification Scenarios (All Static)

### Scenario S-01: AC-06 — Correct argument at call site

**Method**: Grep + Read

**What to check**: `status.rs` lines ~747–748 pass `report.contradiction_count` (not
`report.total_quarantined`) as the first argument to `contradiction_density_score()`.

**Grep assertion**:
```
Grep pattern: "contradiction_density_score(report.contradiction_count"
Expected: exactly 1 match in status.rs
```

**Read assertion**: The call site reads:
```rust
report.contradiction_density_score =
    coherence::contradiction_density_score(report.contradiction_count, report.total_active);
```

**AC Coverage**: AC-06

---

### Scenario S-02: AC-09 — total_quarantined absent from scoring call site

**Method**: Grep (two patterns, both must return zero production matches)

**Grep assertion 1**:
```
Pattern: contradiction_density_score.*total_quarantined
Expected: 0 matches in entire workspace
```

**Grep assertion 2**:
```
Pattern: total_quarantined.*contradiction_density_score
Expected: 0 matches in entire workspace
```

**Note on R-08** (false-positive risk): These grep patterns may match comments or test
helper variables that happen to contain both substrings. Triage any matches manually —
comments and doc strings are not production call sites. The critical assertion is that no
production code path passes `total_quarantined` to the scoring function.

**AC Coverage**: AC-09

---

### Scenario S-03: AC-16 — Phase ordering comment present at Phase 5 call site

**Method**: Read

**What to check**: The Phase 5 call site (~line 747) has an inline comment that
explicitly names Phase 2 as a prerequisite. Minimum acceptable form:

```rust
// report.contradiction_count is populated in Phase 2 (contradiction cache read);
// Phase 5 must not be reordered above Phase 2. See crt-051 ADR-001.
```

Or equivalent: the existing block structure already labels Phase 2 and Phase 5 with
comments, and the Phase 5 block clearly reads `contradiction_count` which is set in
Phase 2. The dependency must be unambiguous to a future reader who might refactor.

**AC Coverage**: AC-07, AC-16

---

### Scenario S-04: AC-08 — generate_recommendations() unchanged

**Method**: Read (two locations)

**Location 1**: `generate_recommendations()` function signature in `coherence.rs` (~line 114).
Must still include `total_quarantined: u64` as a parameter. The signature must not have been
altered.

**Location 2**: The call site in `status.rs` at ~line 784–790. Must still pass
`report.total_quarantined` as the quarantine argument to `generate_recommendations()`.

**Exact expected call structure** (arguments may span multiple lines):
```rust
coherence::generate_recommendations(
    report.coherence,
    coherence::DEFAULT_LAMBDA_THRESHOLD,
    report.graph_stale_ratio,
    report.embedding_inconsistencies.len(),
    report.total_quarantined,   // <-- must be unchanged
)
```

**AC Coverage**: AC-08, R-06

**Why this matters** (R-06): The word "quarantined" must be removed from exactly one
location (the `contradiction_density_score()` call site). It must remain in
`generate_recommendations()`. A developer over-applying AC-09 could accidentally remove it
from the recommendations path. This read-verify step is the guard.

---

### Scenario S-05: AC-07 — Phase 2 precedes Phase 5 in compute_report()

**Method**: Read

**What to check**: In `compute_report()`, the contradiction cache read block (Phase 2,
~lines 583–591) appears before the Lambda scoring block (Phase 5, ~lines 747–756). This
is structural — not a dynamic ordering check, just confirmation that the sequential code
has not been reordered.

**Expected**: Phase 2 block is visible at a lower line number than Phase 5 block. A comment
at Phase 2 of the form `// Phase 2: contradiction cache (must precede Phase 5 Lambda)` or
the existing phase-label comments are sufficient.

**AC Coverage**: AC-07

---

## Scope Confirmation

The following aspects of `status.rs` are verified to be UNCHANGED:

- `StatusReport.total_quarantined: u64` field — present, unchanged
- `StatusReport.contradiction_count: usize` field initialization at ~line 533/544 — `0`, unchanged
- All Phase 1 reads (counters from SQLite) — unchanged
- Phase 2 contradiction cache read block (~lines 583–591) — unchanged (not modified by crt-051)
- Phase 5 `compute_lambda()` call — unchanged
- All other `compute_report()` phases — unchanged
- `generate_recommendations()` call site — unchanged

---

## Risks Covered by This Component

| Risk | Covered By |
|------|-----------|
| R-01 (missed call site — total_quarantined at scoring site) | Scenario S-02 (AC-09 grep) |
| R-04 (phase ordering violated) | Scenario S-03 (AC-16) + Scenario S-05 (AC-07) |
| R-06 (generate_recommendations accidentally modified) | Scenario S-04 (AC-08) |
| R-08 (grep false-positive on AC-09) | Scenario S-02 note on manual triage |
