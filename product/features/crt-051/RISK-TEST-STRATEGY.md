# Risk-Based Test Strategy: crt-051

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Missed call site: `total_quarantined` still passed to `contradiction_density_score()` somewhere after the fix | High | Med | Critical |
| R-02 | SR-02 fixture divergence: `response/mod.rs` fixture retains stale 0.7000 value or uses wrong approach, silently wrong or silent pass | High | High | Critical |
| R-03 | Test rewrite incomplete: coherence.rs tests renamed but first argument type still `u64` (not `usize`), causing silent semantic drift | Med | Med | High |
| R-04 | Phase ordering violated: Phase 5 Lambda call silently uses `contradiction_count: 0` because Phase 2 was refactored above or hoisted | High | Low | High |
| R-05 | Cold-start AC-17 missing: no dedicated test exercises `contradiction_density_score(0, N>0)`, leaving cold-start semantics untested | Med | Med | High |
| R-06 | `generate_recommendations()` accidentally modified: `total_quarantined` removed from its call site, silently disabling quarantine recs | High | Low | High |
| R-07 | Lambda regression: non-zero `contradiction_pair_count` path never exercised in tests, formula bug undetected in edge cases | Med | Low | Med |
| R-08 | Grep gate false-positive: AC-09 grep for `total_quarantined.*contradiction_density_score` matches test helper or comment, causing false CI failure | Low | Med | Low |

---

## Risk-to-Scenario Mapping

### R-01: Missed call site — total_quarantined still at scoring site
**Severity**: High
**Likelihood**: Med
**Impact**: Bug survives the fix. Lambda contradiction dimension continues to track quarantine count. The entire purpose of crt-051 is negated. May compile silently because `total_quarantined: u64` is implicitly coerced in some contexts.

**Test Scenarios**:
1. After implementation, run `Grep` for `contradiction_density_score.*total_quarantined` across the entire workspace — must return zero matches (AC-09).
2. Run `Grep` for `total_quarantined.*contradiction_density_score` — must return zero matches (AC-09, reverse argument order).
3. Run `Grep` for `contradiction_density_score(` to enumerate all call sites — confirm exactly one call site exists and it passes `report.contradiction_count`.

**Coverage Requirement**: AC-09 must be verified by static grep, not just by compilation. Type mismatch (`u64` vs `usize`) would be caught by compiler, but a legacy call using a `u64` cast to `usize` at the wrong point would not.

---

### R-02: SR-02 fixture — 0.7000 vs 1.0 disagreement (architect vs spec)
**Severity**: High
**Likelihood**: High
**Impact**: If the spec writer's approach (set `contradiction_density_score: 1.0`) is used but `contradiction_count: 0` is left at 0, the fixture no longer exercises a non-trivial score path. Formatting tests for sub-1.0 scores become vacuous. If the architect's approach (set `contradiction_count: 15`) is used, it tests the real formula and preserves the 0.7000 value, which is mathematically exact: `1.0 - 15/50 = 0.7000`.

**Recommendation: Use the architect's approach — set `contradiction_count: 15`, keep `contradiction_density_score: 0.7000`.**

Rationale:
- The spec writer's approach produces `contradiction_count: 0` with `contradiction_density_score: 1.0`. This is trivially consistent (0 pairs → 1.0) but tests nothing about the formula. Every zero-pairs database produces 1.0; the fixture becomes indistinguishable from the 7 other fixtures.
- The architect's approach sets `contradiction_count: 15`, `total_active: 50`, giving `1.0 - 15/50 = 0.7000` exactly. This exercises the non-trivial scoring path, validates the formula in a fixture context, and preserves the existing hardcoded `0.7000` value (so no downstream assertion on `0.7000` breaks unexpectedly).
- The old `0.7000` was never computed from the old formula either (`1.0 - 3/50 = 0.940 ≠ 0.70`), so it was always a manually-set scenario value. The architect's approach gives it coherent meaning for the first time.
- `coherence: 0.7450` at line 1419 is independently hardcoded and must not change regardless of which approach is chosen.

**Test Scenarios**:
1. Read `make_coherence_status_report()` in `response/mod.rs`: confirm `contradiction_count: 15` and `contradiction_density_score: 0.7000` (architect's approach).
2. Confirm `total_active: 50` in the same fixture (to make the formula verifiable by inspection).
3. Confirm the seven other fixtures retain `contradiction_density_score: 1.0` and `contradiction_count: 0` — no spurious changes.
4. Run `cargo test -p unimatrix-server mcp::response` — all response formatting tests pass.

**Coverage Requirement**: The fixture must represent a semantically valid state under the new formula. `contradiction_count: 15` with `total_active: 50` satisfies this.

---

### R-03: Test rewrite incomplete — first argument type still u64
**Severity**: Med
**Likelihood**: Med
**Impact**: Tests compile (Rust allows numeric literals to satisfy `usize` in most contexts) but semantically document the wrong type. A future human reading the test sees `(3_u64, 50)` and infers quarantine semantics. The test suite misleads rather than documents.

**Test Scenarios**:
1. Read the test block in `coherence.rs` — confirm the three rewritten tests use `usize`-typed first arguments (or untyped numeric literals consistent with `usize`).
2. Confirm no test name contains the substring "quarantined" (AC-14b).
3. Confirm the four required cases are covered: zero-active guard, zero-pairs with active > 0, pair-count exceeds active (clamped to 0.0), mid-range value (AC-14c).

**Coverage Requirement**: All three old tests fully replaced; no test name encodes old semantics.

---

### R-04: Phase ordering violated — contradiction_count is 0 at Phase 5
**Severity**: High
**Likelihood**: Low
**Impact**: If Phase 2 contradiction cache read is ever moved after Phase 5 Lambda computation (e.g., by a future refactor of `compute_report()` that groups "fast" reads together), `report.contradiction_count` will always be 0 at scoring time, producing a permanently optimistic `contradiction_density_score: 1.0` even on warm caches with real pairs. The bug would be silent — no compile error, no test failure, just wrong Lambda.

**Test Scenarios**:
1. Read `status.rs` lines ~583–593 (Phase 2) and ~747–748 (Phase 5): confirm Phase 2 precedes Phase 5 and a comment marks the dependency (AC-16/AC-07).
2. Confirm the comment at the Phase 5 call site explicitly names Phase 2 as a prerequisite (per ADR-001 architecture doc).

**Coverage Requirement**: AC-16 comment verification. No runtime test is feasible for ordering; the guard is documentation and code review discipline.

---

### R-05: Cold-start AC-17 missing
**Severity**: Med
**Likelihood**: Med
**Impact**: The cold-start scenario (`contradiction_count: 0`, `total_active > 0`) is the most common real-world state for a freshly deployed server. Without a named test, this behavior is covered only incidentally by AC-02. AC-17 requires a test with "cold" in the name that makes the intent explicit.

**Test Scenarios**:
1. Read `coherence.rs` test block: confirm a test named `contradiction_density_cold_start` (or containing "cold") exists and calls `contradiction_density_score(0, N)` with `N > 0`, asserting result == 1.0 (AC-17).
2. Confirm this test is distinct from `contradiction_density_zero_active` (AC-03), which uses `total_active == 0`.

**Coverage Requirement**: The cold-start test must use a non-zero `total_active` value (e.g., 50) to distinguish from the empty-database guard.

---

### R-06: generate_recommendations() accidentally modified
**Severity**: High
**Likelihood**: Low
**Impact**: If `total_quarantined` is removed from `generate_recommendations()` call site during the change (because AC-09 is read as "remove `total_quarantined` everywhere"), the quarantine recommendation path silently stops working. No test in the current suite directly asserts quarantine recommendation output.

**Test Scenarios**:
1. Read `generate_recommendations()` signature in `coherence.rs` — confirm `total_quarantined: u64` parameter is present (AC-08).
2. Read the call site at `status.rs` line ~784–790 — confirm `report.total_quarantined` is still the argument.
3. AC-09 grep is scoped: `total_quarantined` must not appear at the `contradiction_density_score()` call site, but is still expected at the `generate_recommendations()` call site.

**Coverage Requirement**: Both positive (recommendations call site unchanged) and negative (scoring call site clean) confirmations required. AC-08 and AC-09 together cover this if both are verified.

---

### R-07: Lambda regression — degenerate formula path untested
**Severity**: Med
**Likelihood**: Low
**Impact**: `contradiction_pair_count > total_active` produces a raw score below 0.0 before clamping. The clamp handles it, but a formula error (e.g., wrong operand order: `pair_count - active` instead of `1.0 - pair/active`) would produce a positive value > 1.0 and clamp to 1.0, silently reporting a perfect score when contradictions dominate.

**Test Scenarios**:
1. Unit test `contradiction_density_pairs_exceed_active`: `contradiction_density_score(200, 100)` returns exactly `0.0` (AC-04 analog with pair_count > active, AC-14c).
2. Unit test `contradiction_density_partial`: `contradiction_density_score(5, 100)` returns `0.95` within `1e-10` (AC-05).

**Coverage Requirement**: Both above-active and mid-range values must be tested to catch operand-order inversion.

---

## Integration Risks

**Call site isolation**: `contradiction_density_score()` has exactly one production call site (Phase 5 of `compute_report()`). The function is not exported to other crates. The integration surface is narrow but the call site change is load-bearing — a one-character mistake (`total_quarantined` vs `contradiction_count`) silently keeps the bug alive.

**Type boundary**: `StatusReport.contradiction_count` is `usize`; `total_active` is `u64`. The function signature accepts `(usize, u64)` in that order. Argument transposition (`total_active` passed first, `contradiction_count` second) would compile — both are numeric — but would produce wildly wrong scores. Test R-07's mid-range case (`score(5, 100) ≈ 0.95`) would fail if transposed.

**`generate_recommendations()` independence**: This function takes `total_quarantined` as its fifth parameter. It shares the `coherence.rs` module with `contradiction_density_score()`. The risk is spatial proximity during editing — a developer working in `coherence.rs` could accidentally alter both functions. Read-verify both after implementation.

---

## Edge Cases

**EC-01 — Single active entry, one pair**: `contradiction_density_score(1, 1)` → `1.0 - 1/1 = 0.0`. Clamped at 0.0. Valid.

**EC-02 — Pairs from a single prolific contradicting entry**: 10 pairs involving one entry against 10 others, `total_active: 20` → `1.0 - 10/20 = 0.50`. Pair-count normalization (not unique-entry normalization) is confirmed by ADR-001. This is correct behavior per the decision.

**EC-03 — Very large databases**: `usize` pair count with `u64` active. On 64-bit targets `usize` is 64-bit; `as f64` cast is safe (per spec constraint). No overflow risk at expected scales.

**EC-04 — contradiction_count initialized to 0 at StatusReport construction**: `status.rs:533` initializes `contradiction_count: 0`. If Phase 2 cache read is skipped due to a lock acquisition failure (e.g., poisoned RwLock), `contradiction_count` stays 0 and score stays 1.0 (optimistic). This is the existing behavior for lock failures and is acceptable per scope.

---

## Security Risks

`contradiction_density_score()` accepts only two numeric parameters (`usize`, `u64`). Both are derived from internal state (`StatusReport` fields populated from the in-memory cache and SQLite counters), not from external input. No untrusted data enters the function.

The contradiction scan cache (`ContradictionScanCacheHandle`) is populated from internal HNSW nearest-neighbor results, not from user-supplied input. There is no injection surface in this change.

The `as f64` casts are safe — no integer-to-float precision loss risk at values relevant to this feature (pair counts and active entry counts are bounded by database size, far below f64 precision limits).

**Blast radius if crt-051 is defective**: Limited to Lambda computation quality. No data is written. No external API contract breaks. The worst case is a wrong `contradiction_density_score` value in `context_status` output — the same category of error as the pre-existing bug.

---

## Failure Modes

**FM-01 — compilation failure**: Type mismatch if `total_quarantined: u64` is passed to the new `contradiction_pair_count: usize` parameter without cast. Compiler catches this immediately. Resolution: fix argument at call site.

**FM-02 — test failure, wrong fixture value**: If the spec writer's approach is taken (score: 1.0) but a downstream test asserts the old 0.7000, the test fails. Resolution: use architect's approach (contradiction_count: 15) to preserve 0.7000.

**FM-03 — clippy warning on unused `total_quarantined` in coherence.rs**: If `total_quarantined` parameter is removed from `contradiction_density_score()` but a test helper still constructs `StatusReport` with `total_quarantined` and passes it, clippy is not triggered (field is still in the struct). Risk is low.

**FM-04 — silent wrong Lambda post-fix**: If the call site change is made but the phase ordering comment is omitted and a future refactor reorders phases, Lambda silently returns optimistic scores. No test catches this at the time — it is a future regression risk. Mitigation: the required comment at Phase 5 (AC-16) is the documentation guard.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (3 coherence.rs tests require full rewrite) | R-03 | Architecture specifies full rewrite with updated names and types; AC-14 enumerates required test cases |
| SR-02 (0.7000 fixture semantic divergence) | R-02 | Resolved: use architect's approach — set `contradiction_count: 15` to produce `0.7000` via the new formula |
| SR-03 (phase ordering not type-enforced) | R-04 | Architecture adds comment at Phase 5 call site; AC-16 requires comment presence |
| SR-04 (pair count vs unique entry count unresolved) | — | Resolved before delivery: pair count confirmed by human (ADR-001). Not an open risk. |
| SR-05 (cold-start optimistic score) | R-05 | AC-17 adds a named cold-start unit test; cold-start behavior accepted as correct |
| SR-06 (generate_recommendations accidental modification) | R-06 | AC-08 (positive) + AC-09 (negative) together guard this path |
| SR-07 (stale cache known limitation) | — | Accepted; documented in doc comment per FR-10. Not testable by unit test. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | 7 scenarios (3 grep verifications + 4 fixture verifications) |
| High | 4 (R-03, R-04, R-05, R-06) | 8 scenarios (test rewrite verification + phase comment + cold-start test + recommendations read) |
| Medium | 1 (R-07) | 2 scenarios (clamped edge case + mid-range formula) |
| Low | 1 (R-08) | 1 scenario (grep gate scope check) |

---

## Knowledge Stewardship

- Queried: `context_search` for lesson-learned failures/gate/scoring — found #2758 (grep non-negotiable tests), #3253 (non-negotiable test name verification pattern), #3946 (grep gate false-positives from test helpers), #4258 (scoring function semantic change: enumerate all hardcoded fixture values), #4257 (audit Lambda dimension inputs before new infrastructure). Entries #4258 and #3946 directly inform R-02 and R-08 respectively.
- Stored: nothing novel to store — crt-051 risks are feature-specific. The SR-02 architect-vs-spec discrepancy pattern (non-trivial fixture value vs trivial-but-semantically-valid fixture) is too narrow to generalize across features at this time.
