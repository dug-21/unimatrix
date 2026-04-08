# Test Plan: coherence.rs
# crt-051 — contradiction_density_score() Unit Tests

## Component Responsibility

`crates/unimatrix-server/src/infra/coherence.rs`

Pure, stateless scoring functions for all three Lambda dimensions. `contradiction_density_score()`
is the sole function being changed. It must accept `(contradiction_pair_count: usize, total_active: u64)`
and return `f64` in `[0.0, 1.0]`. No I/O, no async, no side effects.

---

## Test Site: contradiction_density_score()

### Current State (before crt-051)

Three tests exist under `// -- contradiction_density_score tests --` at lines 193–208:

| Line | Current Name | Problem |
|------|-------------|---------|
| ~196 | `contradiction_density_zero_active` | First arg annotated as `u64` conceptually; name is acceptable |
| ~201 | `contradiction_density_quarantined_exceeds_active` | Name contains "quarantined" — must be replaced |
| ~206 | `contradiction_density_no_quarantined` | Name contains "quarantined" — must be replaced |

### Required State (after crt-051)

Five tests total under the same section header. The three old tests are fully rewritten (new
names, updated semantic framing); two new tests are added.

---

## Test 1: `contradiction_density_zero_active`

**Action**: Rename acceptable; update doc comment to remove quarantine framing.

**Purpose**: Empty-database guard. When `total_active == 0`, any pair count is meaningless
and the function must return `1.0` immediately without division.

**Arrangement**: No setup needed (pure function).

**Act**: `contradiction_density_score(0, 0)`

**Assert**: `== 1.0`

**AC Coverage**: AC-03

**Notes**: Args `(0, 0)` are numerically identical to the old test. The change is semantic:
the first `0` now means "zero detected contradiction pairs" (type `usize`), not "zero
quarantined entries" (type `u64`). The assertion value is unchanged. Update the doc comment
in the test to reference pair count, not quarantine count.

---

## Test 2: `contradiction_density_pairs_exceed_active`

**Action**: Full rewrite. Old name: `contradiction_density_quarantined_exceeds_active`.

**Purpose**: Degenerate-input clamping. When `contradiction_pair_count > total_active`,
the raw formula produces a value below 0.0. The `.clamp(0.0, 1.0)` call must produce 0.0.
This test also guards against operand-order inversion (R-07): a bug that computes
`pair_count - total_active` instead of `1.0 - pair_count/total_active` would produce a
large positive value that clamps to 1.0, not 0.0.

**Arrangement**: No setup.

**Act**: `contradiction_density_score(200, 100)`

**Assert**: `== 0.0`

**AC Coverage**: AC-04, R-07 (degenerate formula path)

**Notes**: The numeric inputs (200, 100) are identical to the old test. The first arg now
means "200 detected pairs" (usize), not "200 quarantined entries" (u64). The formula
produces `1.0 - 200.0/100.0 = -1.0`, clamped to `0.0`. Use `assert_eq!` — this is an
exact result with no floating-point rounding.

---

## Test 3: `contradiction_density_no_pairs`

**Action**: Full rewrite. Old name: `contradiction_density_no_quarantined`.

**Purpose**: Zero-pair nominal case. When zero contradiction pairs are detected in a
database with active entries, the score is 1.0 (no detected contradiction health risk).
This is the "scan ran and found nothing" path.

**Arrangement**: No setup.

**Act**: `contradiction_density_score(0, 100)`

**Assert**: result `== 1.0`

**AC Coverage**: AC-02

**Notes**: Distinct from the two cold-start tests (Tests 4 and 5) in that it documents
the "scan found zero pairs" interpretation explicitly. All three (this test and AC-17's two
tests) call `contradiction_density_score(0, N>0)` — they produce the same output via the
same code path, but exist as separate tests to document intent.

---

## Test 4: `contradiction_density_cold_start_cache_absent`

**Action**: New test. Covers AC-17, Case 1 of the required cold-start pair.

**Purpose**: Cold-start behavior when the contradiction scan has never run (cache is `None`).
In `compute_report()` Phase 2, when `ContradictionScanCacheHandle` is `None`, `report.contradiction_count`
stays at its initialized value of 0. By the time Phase 5 calls `contradiction_density_score()`,
the input is `(0, total_active)`. This test documents that path.

**Arrangement**: No setup (pure function test).

**Act**: `contradiction_density_score(0, 50)`

**Assert**: `(result - 1.0).abs() < 1e-10`

**AC Coverage**: AC-17 (Case 1: cache absent)

**Notes**: Uses `total_active = 50` (non-zero) to distinguish from the empty-database guard
(AC-03, which uses `total_active = 0`). The comment inside the test must state:
"Simulates cold-start: scan cache is None, contradiction_count defaults to 0."
This test and Test 5 are explicitly distinct per the human guidance in the spawn prompt —
their value is documentation of intent for future readers and resilience against changes
that might distinguish the two cases.

---

## Test 5: `contradiction_density_cold_start_no_pairs_found`

**Action**: New test. Covers AC-17, Case 2 of the required cold-start pair.

**Purpose**: Cold-start behavior when the scan has run but found no contradiction pairs
(cache is `Some([])`, i.e., `pairs.len() == 0`). Phase 2 sets `report.contradiction_count = 0`
in this case too. By Phase 5, the input is again `(0, total_active)`. This test documents
that distinct code path.

**Arrangement**: No setup (pure function test).

**Act**: `contradiction_density_score(0, 50)`

**Assert**: `(result - 1.0).abs() < 1e-10`

**AC Coverage**: AC-17 (Case 2: cache present, no pairs found)

**Notes**: Uses the same args as Test 4 (`0, 50`). Both tests call the same pure function
with the same inputs and assert the same output. The distinction is in the comment/name
only — documenting two upstream conditions that both yield `contradiction_count: 0`. The
comment inside the test must state: "Simulates warm cache with zero pairs found
(contradiction_scan_performed: true, contradiction_count: 0)."

---

## Test 6: `contradiction_density_partial`

**Action**: New test. Covers AC-05.

**Purpose**: Mid-range formula verification. Confirms the scoring formula is correct for a
non-trivial, non-degenerate input. Also guards against operand-order inversion: if the
formula were inverted (`pair_count/total_active` instead of `1.0 - pair_count/total_active`),
this test would produce ~0.05 instead of ~0.95.

**Arrangement**: No setup.

**Act**: `contradiction_density_score(5, 100)`

**Assert**:
- `result > 0.0` (strictly positive)
- `result < 1.0` (strictly below 1.0)
- `(result - 0.95).abs() < 1e-10`

**AC Coverage**: AC-05, R-07 (mid-range formula verification)

**Notes**: Use epsilon tolerance `< 1e-10` for the exact value assertion (not `< 0.001`) —
`1.0 - 5.0/100.0 = 0.95` is exactly representable in f64. The three-part assertion (> 0.0,
< 1.0, ≈ 0.95) is the full form; the last assertion is sufficient on its own but the range
assertions document intent.

---

## Summary of Required Test Block State

After crt-051, the `// -- contradiction_density_score tests --` block in `coherence.rs`
must contain exactly these five tests, in this order, with no test names containing
the substring "quarantined":

1. `contradiction_density_zero_active` — rewritten (AC-03)
2. `contradiction_density_pairs_exceed_active` — rewritten (AC-04, R-07)
3. `contradiction_density_no_pairs` — rewritten (AC-02)
4. `contradiction_density_cold_start_cache_absent` — new (AC-17 Case 1)
5. `contradiction_density_cold_start_no_pairs_found` — new (AC-17 Case 2)
6. `contradiction_density_partial` — new (AC-05, R-07)

The implementation brief lists 5 tests (3 rewrites + 2 new). The spawn prompt mandates 2
cold-start tests (AC-17 split). This plan therefore specifies 6 tests total: 3 rewrites + 3
new. The "cold_start" test from the implementation brief's table is split into the two
explicit cases (Tests 4 and 5 above), with `contradiction_density_partial` counting as
the second new test.

---

## Doc Comment Verification (AC-01, AC-13)

The doc comment on `contradiction_density_score()` must:
- State the new parameter name: `contradiction_pair_count`
- State the formula: `1.0 - contradiction_pair_count / total_active`, clamped to [0.0, 1.0]
- State the cold-start/zero-pairs behavior: returns 1.0
- State the empty-database guard: returns 1.0 when `total_active == 0`
- State the cache-staleness known limitation (~60 min rebuild)
- NOT contain any reference to "quarantined" or "quarantine-to-active ratio"

---

## NFR Assertions (per component)

- **NFR-04** (f64 precision): All float assertions use `abs() < epsilon` form. Exact
  calculations use `< 1e-10`; the `assert_eq!` form is acceptable only for values
  guaranteed to be exact (e.g., `== 0.0`, `== 1.0` from the early-return paths).
- **NFR-06** (clippy): No unused variables, no `as u64` casts on the first argument (the
  type is `usize` throughout).
- **NFR-07** (test suite green): `cargo test -p unimatrix-server infra::coherence` exits 0.

---

## Risks Covered by This Component

| Risk | Covered By |
|------|-----------|
| R-01 (missed call site) | Not in this component; covered by grep in status.md |
| R-02 (SR-02 fixture) | Not in this component; covered in response.md |
| R-03 (test rewrite incomplete, u64 args) | Tests 1–3 above |
| R-04 (phase ordering) | Not in this component; covered in status.md |
| R-05 (cold-start AC-17 missing) | Tests 4 and 5 above |
| R-06 (generate_recommendations accidentally modified) | Not in this component; covered in status.md |
| R-07 (degenerate formula path untested) | Tests 2 and 6 above |
| R-08 (grep false-positive) | Not in this component |
