# Scope Risk Assessment: crt-013

## Risk Register

### SR-01: W_COAC Removal Triggers Unintended Confidence Recalculation
- **Likelihood:** Medium
- **Impact:** High
- **Description:** The scope recommends Option A (delete W_COAC, keep stored weights at 0.92). However, if any code path references W_COAC for normalization or if test assertions are coupled to the 6+1 factor structure, removing it could cascade. More critically, `co_access_affinity()` at confidence.rs:239 may be called from paths not discovered during scoping (e.g., batch recomputation in coherence gate maintenance, or future col-015 validation). If it's truly dead code, removal is safe — but the scope's evidence is "never called in production," not "never called anywhere."
- **Mitigation:** Architect must grep exhaustively for `co_access_affinity`, `W_COAC`, and the 0.08 constant across all crates before confirming Option A. Include a pseudocode step that verifies zero callers exist at compile time (the Rust compiler will catch this, but the architect should confirm no dynamic dispatch hides a call).

### SR-02: crt-011 Dependency Not Yet Landed
- **Likelihood:** Medium
- **Impact:** High
- **Description:** Scope explicitly states "Depends on crt-011 for correct confidence data. Session count dedup must be landed first." crt-013 is Wave 3; crt-011 is Wave 1. If crt-011 is incomplete or introduces regressions, crt-013's status penalty validation tests (Component 2) will produce misleading results — tests would pass or fail based on corrupted confidence data, not on the penalty logic itself. The scope doesn't define a gate check to verify crt-011 is merged and stable before starting.
- **Mitigation:** Architect should define explicit preconditions: crt-011 merged to main, CI green, no open regressions. Component 2 integration tests should use deterministic confidence values injected via test fixtures rather than relying on live confidence computation, isolating the penalty validation from upstream data quality.

### SR-03: Status Scan Optimization Assumes SQL Equivalence Without Proof
- **Likelihood:** Medium
- **Impact:** Medium
- **Description:** Component 4 replaces Rust-side iteration over all entries with SQL aggregation queries. The scope assumes these are equivalent but doesn't account for edge cases: NULL handling differences between Rust deserialization defaults and SQL COALESCE, entries with malformed `trust_source` or empty `created_by` fields, or entries that fail deserialization (currently silently skipped by Rust iteration but would be counted by SQL). AC-10 requires "identical results" verified by comparison test, but the definition of "identical" when the old code silently drops malformed entries is ambiguous.
- **Mitigation:** Architect should specify that the comparison test runs both old and new paths on the same dataset and diffs the output struct field-by-field. Document known divergences (e.g., malformed entries) as accepted differences in the ADR.

### SR-04: Episodic Augmentation Removal May Break Downstream Import Paths
- **Likelihood:** Low
- **Impact:** Medium
- **Description:** Removing `episodic.rs` from `unimatrix-adapt` and its `lib.rs` declaration is straightforward, but the scope doesn't inventory all `use` statements or re-exports that reference episodic types. If any agent definition, test fixture, or integration test imports `EpisodicAugmenter` or related types, compilation will fail. The scope says "all references cleaned up" (AC-01) but doesn't enumerate them.
- **Mitigation:** Architect should include a discovery step: `grep -r "episodic" --include="*.rs"` across the workspace. Low risk because Rust's compiler will catch missing imports, but the pseudocode should list all affected files to avoid surprise during implementation.

### SR-05: Briefing k Configuration Scope Creep
- **Likelihood:** Low
- **Impact:** Low
- **Description:** Making `k` configurable via "server config struct with env var override" (recommended approach) touches the server's configuration surface area. If the server config is not already externalized (vnc-005 is listed as future work), this introduces a one-off config pattern that may conflict with the eventual config externalization design. The scope says "no new MCP tools or parameters" but a new env var is a new parameter at the operational level.
- **Mitigation:** Keep the implementation minimal — a const with an env var override at construction time. Do not introduce a full config struct if one doesn't already exist. The architect should check what config patterns exist in the server crate and follow them.

### SR-06: Integration Test Determinism for Penalty Validation
- **Likelihood:** Medium
- **Impact:** Medium
- **Description:** Component 2 tests require entries with specific similarity scores (0.90, 0.88, 0.95, 0.85). Vector similarity depends on embedding content, and getting exact similarity values from the ONNX embedding pipeline is non-trivial. The scope provides calculated examples but doesn't address how tests will control embedding similarity to hit these exact thresholds. If tests use approximate similarity, assertions may be fragile.
- **Mitigation:** Architect should design tests that either (a) inject pre-computed embeddings with known cosine similarity, or (b) assert on relative ranking (deprecated < active) rather than absolute score values. Option (b) is more robust and aligns with what the penalties actually guarantee.

### SR-07: "No Schema Migrations" Constraint May Block Status Optimization
- **Likelihood:** Low
- **Impact:** Medium
- **Description:** Component 4 assumes SQL aggregation queries work on existing schema. If the current schema lacks indexes on `trust_source`, `created_by`, `supersedes`, or `status`, the aggregation queries may be slower than the full scan for small-to-medium datasets (index overhead > sequential scan). The scope says "no schema migrations" but adding indexes is a schema change.
- **Mitigation:** Architect should verify existing SQL indexes cover the GROUP BY and WHERE clauses needed. If indexes are missing, classify adding them as a "non-breaking schema addition" (not a migration) or flag as out of scope. For the current dataset size, this is likely a non-issue — the optimization matters at scale.

## Dependency Analysis

### Hard Dependencies
- **crt-011 (Wave 1):** Must be merged and stable. Confidence data integrity directly affects Component 2 test validity. This is the highest-risk dependency.
- **crt-010 (landed):** Status penalties implemented in search.rs. Component 2 validates these. If crt-010 had bugs, crt-013 tests would expose them — this is a feature, not a risk.

### Soft Dependencies
- **nxs-009 (Wave 2):** Observation metrics normalization. No direct dependency, but if nxs-009 changes the observation store schema, status scan queries in Component 4 may need adjustment. Low risk — nxs-009 targets OBSERVATION_METRICS, not the entries table.
- **col-015 (Wave 4):** End-to-end validation depends on crt-013 completing correctly. crt-013's ADR (co-access architecture) will inform col-015's test design for co-access signal validation.
- **vnc-005 (future):** Config externalization. Briefing k configuration (Component 3) should not prematurely create config patterns that conflict. Low risk with minimal implementation.

### Cross-Crate Impact
Four crates affected: `unimatrix-engine`, `unimatrix-adapt`, `unimatrix-server`, `unimatrix-store`. Changes are additive (new Store methods, new tests) or subtractive (removing dead code). No new cross-crate interfaces introduced. Risk is low — changes are well-contained within existing abstraction boundaries.

## Assumption Validation

| # | Assumption | Holds? | Notes |
|---|-----------|--------|-------|
| A1 | `co_access_affinity()` is dead code | **Likely yes, verify** | Scope says "never called in production." Compiler will confirm, but architect should verify no dynamic dispatch or conditional compilation paths exist. |
| A2 | Episodic augmentation stub has no real callers | **Likely yes, verify** | No-op by design (crt-006 SR-04). Rust compiler will catch removal issues. |
| A3 | crt-010 penalties work as designed | **Assumed, testing validates** | The math in scope (0.598 << 0.845) looks correct. Component 2 exists precisely to validate this assumption. |
| A4 | SQL aggregation queries are equivalent to Rust iteration | **Partially** | Edge cases around NULL handling and malformed entries may produce different counts. See SR-03. |
| A5 | No schema migrations needed | **Yes** | All tables and columns exist. New Store methods use existing schema. Adding SQL indexes is borderline — clarify with architect. |
| A6 | k=3 default preserves existing behavior | **Yes** | Trivially true if the new code path uses the same default. |
| A7 | crt-011 will be landed before crt-013 starts | **Assumed** | Wave ordering in product vision confirms this. No gate mechanism defined in scope. |
| A8 | MicroLoRA and co-access boost double-counting is acceptable | **Accepted by design** | Scope explicitly keeps both, deferring empirical evaluation to col-015. This is a conscious trade-off, not an oversight. |

## Recommendations for Architecture Phase

1. **Isolate Component 2 tests from live confidence computation (SR-02, SR-06).** Design integration tests with injected embeddings or pre-set similarity scores rather than relying on the full embedding pipeline. This avoids coupling test validity to crt-011's confidence correctness and eliminates fragile floating-point assertions. Assert on relative ranking, not absolute scores.

2. **Verify W_COAC and episodic.rs are truly dead before designing removal (SR-01, SR-04).** Run exhaustive grep across all crates, including test code, build scripts, and feature gates. The Rust compiler is the ultimate arbiter, but the pseudocode should enumerate all affected files upfront to prevent implementation surprises. Decide W_COAC disposition (Option A/B/C) in the ADR with explicit justification.

3. **Define the SQL-vs-Rust equivalence contract for status scan (SR-03).** The comparison test (AC-10) needs a precise definition of "identical." Specify which fields are compared, how NULLs are handled, and whether malformed entries are expected to produce different counts. Consider implementing both paths temporarily and running them side-by-side in a single test, then removing the old path only after the comparison passes.
