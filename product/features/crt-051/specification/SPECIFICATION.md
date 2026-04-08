# SPECIFICATION: crt-051 — Fix contradiction_density_score() to Use Scan-Detected Pair Count

## Objective

`contradiction_density_score()` currently computes the Lambda coherence dimension using
`total_quarantined / total_active` — a quarantine-to-active ratio that has no causal
relationship to contradiction health. This feature replaces that broken proxy with
`contradiction_pair_count / total_active`, drawn from the contradiction scan cache that
already runs every ~60 minutes and is already exposed on the `StatusReport`. The Lambda
weight for the contradiction dimension (0.31, second-highest) is preserved unchanged;
only the input data source is corrected.

---

## Functional Requirements

### FR-01: New Signature for contradiction_density_score()

`contradiction_density_score()` in `crates/unimatrix-server/src/infra/coherence.rs`
must accept `(contradiction_pair_count: usize, total_active: u64) -> f64`.
The `total_quarantined: u64` parameter must be removed. No other parameters may be added.

### FR-02: Scoring Formula

The score must be computed as:
```
1.0 - (contradiction_pair_count as f64 / total_active as f64)
```
clamped to `[0.0, 1.0]`. This is the pair-count interpretation confirmed by the human
(open question 1 resolution): raw detected pair count, not unique-entries-in-pairs count.

### FR-03: Empty-Database Guard

When `total_active == 0`, the function must return `1.0` immediately (before any
division). This guard is unchanged from the current implementation.

### FR-04: Cold-Start Behavior

When `contradiction_pair_count == 0` (either because no scan has run yet, or because the
scan ran and found no contradictions), the score must be `1.0`. This is the optimistic
default consistent with codebase patterns (`phase_affinity_score`, Lambda missing
dimensions). No distinction is required between "scan not yet run" and "scan ran, zero
pairs found" — both produce `contradiction_count == 0` in `StatusReport`, and both
correctly score 1.0.

### FR-05: Degenerate Input Clamping

When `contradiction_pair_count > total_active` (a degenerate case where many overlapping
pairs are detected from a small active set), the raw formula produces a value < 0.0.
The existing `.clamp(0.0, 1.0)` call handles this; no additional guard is required.

### FR-06: Updated Call Site in compute_report()

The call site in `services/status.rs::compute_report()` at approximately line 747 must
pass `report.contradiction_count` (type `usize`) instead of `report.total_quarantined`
(type `u64`). `report.contradiction_count` is populated in Phase 2 (lines ~583–591) from
the `ContradictionScanCacheHandle` before Phase 5 Lambda computation. The Phase 2 → Phase 5
ordering is already correct in the current code; no sequencing changes are required.

### FR-07: generate_recommendations() Unchanged

`generate_recommendations()` in `coherence.rs` receives `total_quarantined: u64` as a
separate parameter and uses it to surface a "review quarantined entries" recommendation.
This path must not be altered. `total_quarantined` continues to be passed to
`generate_recommendations()` from `compute_report()` at line ~789.

### FR-08: No total_quarantined at Scoring Call Site

`total_quarantined` must not appear as an argument to `contradiction_density_score()`
anywhere in the codebase after this fix.

### FR-09: Updated Doc Comment

The doc comment on `contradiction_density_score()` must describe the new semantics.
Minimum required content: the function returns the fraction of contradiction-free health
based on the detected pair count from the contradiction scan cache, and returns 1.0 when
the database is empty or when no contradictions have been detected (cold-start or clean
state). The old "quarantined-to-active ratio" description must be removed.

### FR-10: Stale Cache Known Limitation

The doc comment must note that the scan cache is rebuilt every ~60 minutes; the score
reflects cache-age contradiction state, not real-time state. This is a known limitation,
acceptable for the current design, and out of scope for future fixes.

### FR-11: Rewrite contradiction_density_score Unit Tests in coherence.rs

The three existing unit tests for `contradiction_density_score()` that encode
quarantine-semantics must be fully rewritten (new names, new inputs, new assertions).
Test names must not contain "quarantined". See section on Test Sites below.

### FR-12: Update contradiction_density_score Fixture in response/mod.rs

The `make_coherence_status_report()` helper in
`crates/unimatrix-server/src/mcp/response/mod.rs` has `contradiction_density_score: 0.7000`
and `total_quarantined: 3` / `contradiction_count: 0`. After the fix, a
`contradiction_count == 0` with any `total_active` produces a score of `1.0`, so the
fixture field must be updated to `contradiction_density_score: 1.0`. The `coherence: 0.7450`
field in the same fixture is independently hardcoded and is NOT recomputed from dimension
scores — it must remain `0.7450` unless explicitly recalculated.

---

## Non-Functional Requirements

### NFR-01: Function Purity

`contradiction_density_score()` must remain pure: no I/O, no async, no side effects,
deterministic given its inputs. This is a hard constraint, not a preference.

### NFR-02: No Schema Changes

No database schema migration. `COUNTERS.total_quarantined` and `StatusReport.total_quarantined`
remain in place. `StatusReport.contradiction_count` (type `usize`) already exists and
requires no schema change.

### NFR-03: No New Dependencies

No new crates, no new imports beyond what is already available in `coherence.rs`.

### NFR-04: f64 Precision Discipline

Floating-point comparisons in unit tests must use `abs() < epsilon` tolerance, not `==`,
consistent with existing tests in `coherence.rs`. The tolerance for assertions on computed
scores is `< 1e-10` for exact calculations, `< 0.001` for approximate.

### NFR-05: Compilation

`cargo build --workspace` must succeed with zero errors after the change.

### NFR-06: Clippy Clean

`cargo clippy --workspace -- -D warnings` must pass with zero warnings.

### NFR-07: Test Suite Green

`cargo test --workspace` must pass with zero failures.

---

## Acceptance Criteria

### From SCOPE.md (AC-01 through AC-13) — All Preserved

**AC-01** — `contradiction_density_score()` signature is
`fn contradiction_density_score(contradiction_pair_count: usize, total_active: u64) -> f64`.
The `total_quarantined` parameter is removed.
_Verification: Read updated function signature in `coherence.rs`._

**AC-02** — `contradiction_density_score(0, N)` returns `1.0` for any `N > 0`
(cold-start or no contradictions detected).
_Verification: Unit test `contradiction_density_no_pairs` (or equivalent) with N=100 asserts 1.0._

**AC-03** — `contradiction_density_score(0, 0)` returns `1.0` (empty database guard).
_Verification: Unit test `contradiction_density_zero_active` asserts 1.0._

**AC-04** — `contradiction_density_score(N, N)` returns `0.0` (all-pairs-equal-active, clamped at zero).
_Verification: Unit test `contradiction_density_pairs_equal_active` with N=100 asserts 0.0._

**AC-05** — For `pair_count > 0` and `total_active > pair_count`, the score is strictly
between 0.0 and 1.0.
_Verification: Unit test with `contradiction_density_score(5, 100)` asserts result is in (0.0, 1.0) and approximately `0.95`._

**AC-06** — The call site in `status.rs::compute_report()` passes `report.contradiction_count`
(not `report.total_quarantined`) to `contradiction_density_score()`.
_Verification: Read updated call site in `status.rs` lines ~747–748._

**AC-07** — `report.contradiction_count` is populated in Phase 2 of `compute_report()` (lines
~583–591) before the Lambda computation in Phase 5 (lines ~747–756). The ordering invariant
is preserved.
_Verification: Inspect `status.rs` — Phase 2 contradiction cache read precedes Phase 5 call to
`contradiction_density_score`. An inline comment `// Phase 2: contradiction cache (must precede Phase 5 Lambda)` or equivalent must be present at the Phase 2 read site, or the existing comment structure must make the ordering unambiguous._

**AC-08** — `generate_recommendations()` still receives `total_quarantined` as its parameter;
the quarantine recommendation path is not altered.
_Verification: Read `generate_recommendations()` signature and call site in `status.rs` line ~784–790. Both unchanged._

**AC-09** — `total_quarantined` is NOT passed to `contradiction_density_score()` anywhere
in the codebase.
_Verification: `Grep` for `contradiction_density_score.*total_quarantined` and
`total_quarantined.*contradiction_density_score` returns zero matches._

**AC-10** — All unit tests in `coherence.rs` pass. Tests for `contradiction_density_score()`
assert the new pair-count semantics (see FR-11 and SR-01 below).
_Verification: `cargo test -p unimatrix-server infra::coherence` exits 0._

**AC-11** — `cargo test --workspace` passes with zero failures.
_Verification: CI output or local run._

**AC-12** — `cargo clippy --workspace -- -D warnings` passes with zero warnings.
_Verification: CI output or local run._

**AC-13** — The doc comment on `contradiction_density_score()` describes new semantics
(see FR-09). Old "quarantined-to-active ratio" description is absent.
_Verification: Read updated doc comment in `coherence.rs`._

### Risk-Derived Additions

**AC-14** (SR-01) — Full test rewrite for coherence.rs contradiction_density_score tests.
The three tests `contradiction_density_zero_active`, `contradiction_density_quarantined_exceeds_active`,
and `contradiction_density_no_quarantined` must be replaced with tests that:
(a) use `contradiction_pair_count: usize` as the first argument (not a `u64` quarantine count),
(b) have names that do not contain the substring "quarantined",
(c) cover: zero-active guard (→ 1.0), zero-pairs with active entries (→ 1.0), pair-count exceeds active clamped (→ 0.0), and a mid-range value (e.g. 5 pairs / 100 active → 0.95).
_Verification: Read the test block in `coherence.rs`; confirm names and parameter types._

**AC-15** (SR-02) — The `make_coherence_status_report()` fixture in
`crates/unimatrix-server/src/mcp/response/mod.rs` must be updated to
`contradiction_count: 15` (changed from `0`), keeping `contradiction_density_score: 0.7000`
(unchanged). `1.0 - 15/50 = 0.7000` exactly — this exercises the non-trivial scoring path
and gives the fixture coherent semantics for the first time. The `contradiction_density_score`
field value remains `0.7000`. No other field requires change (`coherence: 0.7450` is
independently hardcoded).
_Verification: Read `make_coherence_status_report()` in `response/mod.rs`; assert `contradiction_count: 15` and `contradiction_density_score: 0.7000`._

**AC-16** (SR-03) — Phase ordering invariant documented in code. Either an inline comment
at the Phase 2 contradiction cache read in `compute_report()` (lines ~583–591 of `status.rs`)
explicitly states that Phase 2 must precede Phase 5 Lambda, or the block comments already
present (Phase 2 / Phase 5 labels) are sufficient. No structural enforcement is required.
_Verification: Inspect the Phase 2 and Phase 5 blocks in `status.rs`; confirm the ordering relationship is documented by comment or block label._

**AC-17** (SR-05) — Cold-start behavior produces score=1.0 with a dedicated unit test.
A test named `contradiction_density_cold_start` (or with "cold" in the name) must call
`contradiction_density_score(0, 50)` (or similar non-zero active count) and assert the
result equals 1.0. This test is distinct from AC-03 (zero-active guard) — it specifically
models the "scan has not run yet, active entries exist, no pairs known" scenario.
_Verification: Read the test block in `coherence.rs`; confirm a cold-start test exists with non-zero total_active._

---

## Domain Models

### contradiction_density_score (function)

The Lambda coherence dimension measuring knowledge-base contradiction health. After this
fix:
- **Input A** (`contradiction_pair_count: usize`): Number of contradiction pairs detected
  by the heuristic scan (`scan_contradictions`). Each pair represents two entries flagged
  as potentially contradictory based on HNSW nearest-neighbor + negation/directive/sentiment
  signals. This is a raw pair count, not a count of unique entries appearing in pairs.
- **Input B** (`total_active: u64`): Number of active entries in the knowledge base.
- **Output**: A score in `[0.0, 1.0]` where `1.0` = no detected contradictions and `0.0`
  = contradiction density at or above one pair per active entry.
- **Formula**: `1.0 - (pair_count / total_active)`, clamped to `[0.0, 1.0]`.

### ContradictionScanCacheHandle

`Arc<RwLock<Option<ContradictionScanResult>>>`. Holds the most recent output of
`scan_contradictions()`. Populated every `CONTRADICTION_SCAN_INTERVAL_TICKS` (4 ticks,
~60 min). When `None`, no scan has completed (cold-start). When `Some(result)`,
`result.pairs` contains `Vec<ContradictionPair>` — the set of detected contradiction pairs.
The cache is read in Phase 2 of `compute_report()` to populate
`StatusReport.contradiction_count`.

### StatusReport.contradiction_count

Type: `usize`. Populated from `contradiction_cache` in Phase 2 of `compute_report()`.
Represents the number of detected contradiction pairs in the most recent scan. Zero on
cold-start (scan not yet run) or when the scan found no contradictions. This field is the
bridge between the scan infrastructure and `contradiction_density_score()`.

### StatusReport.total_quarantined

Type: `u64`. Counts entries in `Quarantined` status from the `COUNTERS` table. Still
present in `StatusReport` and still used by `generate_recommendations()`. NOT an input
to `contradiction_density_score()` after this fix.

### Lambda (coherence: f64)

Composite knowledge-base health metric `[0.0, 1.0]`. Computed from three weighted
dimensions: `graph_quality` (0.46), `embedding_consistency` (0.23),
`contradiction_density` (0.31). Weights are unchanged by this feature.

### Cold-Start

The period between server startup and the completion of the first contradiction scan.
During this window `ContradictionScanCacheHandle` is `None`, `contradiction_count` is 0,
and `contradiction_density_score` returns 1.0 (optimistic). The `contradiction_scan_performed`
boolean in `StatusReport` distinguishes cold-start (false) from post-scan (true) for
operators who inspect the JSON.

---

## User Workflows

### Operator reads context_status

1. `context_status` is invoked (with or without `maintain: true`).
2. `compute_report()` executes. Phase 1 reads counters; Phase 2 reads the contradiction
   scan cache and sets `report.contradiction_count` (0 if cold-start or no pairs detected).
3. Phase 5 computes Lambda. `contradiction_density_score(report.contradiction_count,
   report.total_active)` produces a score reflecting actual detected contradiction pairs.
4. Lambda is returned in the response. `contradiction_density_score` in the JSON response
   reflects real contradiction health, not quarantine activity.

### Agent/developer verifies Lambda semantics

Before this fix: a database with 3 quarantined entries and 10 active entries scores
contradiction_density = 0.70, regardless of whether those entries contradict anything.

After this fix: a database with 0 detected pairs scores contradiction_density = 1.0,
regardless of quarantine count. A database with 5 detected pairs and 100 active entries
scores contradiction_density = 0.95.

---

## Constraints

- `contradiction_density_score()` must remain a pure function: no I/O, no async, no side effects.
- The only permitted inputs are `contradiction_pair_count: usize` and `total_active: u64`.
- No schema migration. `COUNTERS.total_quarantined` and `StatusReport.total_quarantined` are unchanged.
- The contradiction scan itself (`scan_contradictions` in `infra/contradiction.rs`,
  `ContradictionScanCacheHandle`) must not be modified.
- `generate_recommendations()` signature and behavior must not change.
- `StatusReport` JSON response schema must not change (field names and types are unchanged;
  only field values will differ on deployments that have detected contradictions).
- The type of `contradiction_pair_count` is `usize` — the same type as
  `StatusReport.contradiction_count`. No lossy cast.
- The `as f64` cast from `usize` is safe and follows the existing pattern in
  `graph_quality_score` (`stale_count as f64`).
- Lambda weights in `DEFAULT_WEIGHTS` must not be changed (`contradiction_density: 0.31`).

---

## Dependencies

### Existing Components Used (No New Dependencies)

| Component | Location | Role |
|-----------|----------|------|
| `contradiction_density_score()` | `infra/coherence.rs:68` | Function being changed |
| `compute_report()` | `services/status.rs` | Call site being updated |
| `StatusReport.contradiction_count` | `services/status.rs:533` | Source of pair count (already populated in Phase 2) |
| `ContradictionScanCacheHandle` | `services/status.rs` (Phase 2 ~line 583) | Already read; no new read needed |
| `generate_recommendations()` | `infra/coherence.rs:114` | Unchanged; still uses `total_quarantined` |
| `make_coherence_status_report()` | `mcp/response/mod.rs:1397` | Test fixture requiring update |

### No New Crates

This feature introduces no new dependencies.

---

## NOT in Scope

- Changing Lambda weights. `contradiction_density: 0.31` is preserved (ADR-001 crt-048).
- Writing Contradicts edges to `GRAPH_EDGES`. That is a separate future decision requiring
  NLI infrastructure.
- Re-enabling `run_post_store_nli` or any NLI write path deleted in crt-038.
- Modifying `scan_contradictions()` or `ContradictionScanCacheHandle`.
- Removing `total_quarantined` from `StatusReport`, `COUNTERS`, or `generate_recommendations()`.
- Changing the `context_status` output schema (field names/types are unchanged).
- Implementing a new contradiction detection strategy.
- Real-time (non-cached) contradiction scoring.
- Pair density normalization relative to N² theoretical pairs (not selected; raw pair count used).
- Distinguishing "scan not yet run" from "scan ran, found zero pairs" — both return 1.0 and
  both are correct behavior.

---

## Open Questions

The following open questions from SCOPE.md are resolved:

**OQ-1 (Normalization strategy):** Pair-count formula confirmed by human. The score is
`1.0 - (pair_count / total_active)`, where `pair_count` is the raw count of detected
contradiction pairs (not unique entries in pairs). Resolved before delivery.

**OQ-2 (Cold-start handling):** Score of 1.0 when `contradiction_count == 0` is correct
in both cold-start and clean-database scenarios. `contradiction_scan_performed` in
`StatusReport` is the operator-visible signal distinguishing the two states. No code
change needed. Resolved.

**OQ-3 (StatusReport JSON response shape):** Field names and types are unchanged; only
values change for deployments with detected contradictions. This is a semantic change in
reporting (operators who assumed the dimension measured quarantine density will see
different behavior), but not a JSON schema change. A single changelog line is confirmed
(see Constraints). No API documentation change required beyond the doc comment update
(AC-13).

---

## Changelog Entry

Exactly one changelog line, as confirmed by human:

> contradiction_density in Lambda now reflects scan-detected contradiction pairs
> (previously used quarantined/active as a proxy, which had no relationship to actual
> contradictions).

---

## Test Sites Enumerated

### coherence.rs — 3 tests requiring full rewrite (SR-01)

File: `crates/unimatrix-server/src/infra/coherence.rs`

| Line | Current Name | Action |
|------|-------------|--------|
| 196 | `contradiction_density_zero_active` | Rewrite: rename acceptable (zero_active maps to empty-db guard), but must update parameter type from `u64` to `usize` for first arg |
| 201 | `contradiction_density_quarantined_exceeds_active` | Full rewrite: name must not contain "quarantined"; new name e.g. `contradiction_density_pairs_exceed_active`; new assertion: `contradiction_density_score(200, 100) == 0.0` (same numeric result, different semantics) |
| 206 | `contradiction_density_no_quarantined` | Full rewrite: name must not contain "quarantined"; new name e.g. `contradiction_density_no_pairs` or `contradiction_density_cold_start`; may be split into two tests to separately cover AC-02 (no pairs, active > 0) and AC-17 (cold-start named test) |

New tests to add (not replacing existing):
- `contradiction_density_cold_start` — covers AC-17: `contradiction_density_score(0, 50) == 1.0`
- `contradiction_density_partial` — covers AC-05: `contradiction_density_score(5, 100)` asserts result in (0.0, 1.0) and `(result - 0.95).abs() < 1e-10`

### response/mod.rs — 1 fixture field update (SR-02)

File: `crates/unimatrix-server/src/mcp/response/mod.rs`

| Line | Change |
|------|--------|
| 1422 | `contradiction_density_score: 0.7000` → `contradiction_density_score: 1.0` |

The `coherence: 0.7450` field at line 1419 is independently hardcoded and must NOT change
(it is asserted directly by `test_coherence_markdown_section` at line 1482).

No test in `response/mod.rs` directly asserts the value `0.7000` — the field exists only
in the fixture struct. Updating the field from `0.7000` to `1.0` will not cause any
existing assertion to fail, but must be done to keep the fixture semantically consistent
with the new behavior.

### services/status.rs — 1 call site update (no test assertions on contradiction_density_score)

File: `crates/unimatrix-server/src/services/status.rs`

| Lines | Change |
|-------|--------|
| 747–748 | Pass `report.contradiction_count` instead of `report.total_quarantined` |

No integration test in `status.rs` directly asserts a value for `contradiction_density_score`.
The initial value at line 544 (`contradiction_density_score: 1.0` in the report struct
initialization) is not a test assertion — it is a default value that is overwritten by
Phase 5 computation. No assertion update needed in `status.rs` beyond the call site fix.

---

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — returned 14 entries; top result (#4258, pattern)
  confirms: "when a scoring function's semantics change, search for ALL hardcoded output
  values across every test fixture file." Applied: all 8 fixtures in `response/mod.rs`
  enumerated, 7 at 1.0 (no change needed), 1 at 0.7000 (SR-02, requires update).
  Entry #4257 (pattern) also relevant: "when a Lambda/coherence dimension purports to
  measure X but its input is Y, audit whether an existing in-memory cache or report field
  already captures the correct data." Applied: `report.contradiction_count` already populated
  in Phase 2 — no new infrastructure needed.
