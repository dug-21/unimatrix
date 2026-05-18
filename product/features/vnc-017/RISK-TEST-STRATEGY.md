# Risk-Based Test Strategy: vnc-017

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | SPEC FR-07 states `Ok(true)/Ok(false)` but architecture and ADR-003 define `Result<(), EdgeRedirectError>` — conflicting return contract for `redirect_graph_edge` | High | High | Critical |
| R-02 | `query_incoming_edges` SQL excludes Supersedes at query level (ADR-002) but SPEC FR-04 requires loop-level exclusion — contradicted by OQ-01 in SPEC; implementer may choose either, producing divergent behavior | High | Med | Critical |
| R-03 | `query_incoming_edges` skips the index if the query planner uses a full-scan fallback (e.g., missing bind parameter or incorrect pool) — AC-04 unit test may not catch production pool behavior | Med | Low | High |
| R-04 | source-status validation (`store.get(source_id)`) adds one read-pool call per incoming edge — under fan-in=50, this is 50 extra reads inline on the MCP hot path; not bounded separately from the redirect ceiling | Med | Low | High |
| R-05 | Ceiling truncation (ADR-004 N=50) silently processes the first 50 rows by insertion order — no ordering guarantee on `query_incoming_edges` means which 50 edges are redirected is non-deterministic | Med | Med | High |
| R-06 | `Contradicts` bidirectional redirect: if A→B (Contradicts) exists and B is quarantined, the source-validation check is on B (the source of the incoming edge) — but the 4-row transaction also writes B→new_target; B is quarantined so that write violates graph integrity | High | Med | Critical |
| R-07 | Zero-edge path: `query_incoming_edges` returns empty but the Supersedes exclusion hides actual incoming edges — if Supersedes rows are the only incoming rows, AC-11 (no response append) is correct but AC-01 (no stale edges) is violated by definition (Supersedes are intentionally left) | Med | Low | High |
| R-08 | Partial-redirect state persists until next `DependencyOnDeprecated` tick — no test currently covers the window between a partial redirect and detection rule re-evaluation | Med | Med | High |
| R-09 | `Ok(false)` counter contract ambiguity in AC-09: spec says "treated as success" but does not specify whether `redirected++` or `0` — response text could misreport actual count | Med | Med | High |
| R-10 | Phase B declared-edge writes (step 8b) and the auto-redirect loop (step 8c) both write to `graph_edges` for the same `context_correct` call — if Phase B writes an edge `C → new_entry.id` and the redirect loop also attempts `C → A → new_entry.id`, the INSERT OR IGNORE may silently absorb the duplicate without error; duplicate edge with different `created_at` is lost | Med | Low | High |
| R-11 | `context_correct` response text format tested only at unit level (stubs) — integration test AC-06/AC-12 must also assert the appended string appears in the actual MCP response body | Med | Med | High |
| R-12 | `tracing::info!` summary log emitted even when all edges are Supersedes (found > 0 pre-filter, found == 0 post-filter) — FR-09 log and FR-13 omission are ambiguous for the mixed-type zero-non-Supersedes case | Low | Med | Med |
| R-13 | `context_edge(mode="redirect")` regression: if the implementer touches `edge_write.rs` despite NFR-09, existing redirect tests may pass while subtly breaking the `Contradicts` 4-row path | Med | Low | High |
| R-14 | TOCTOU race on source-status check: `store.get(source_id)` reads Active status; between that read and `redirect_graph_edge` call, the source entry is quarantined by another concurrent operation — graph integrity violation inserted | Low | Low | Med |

---

## Risk-to-Scenario Mapping

### R-01: SPEC/Architecture Return Contract Contradiction for redirect_graph_edge

**Severity**: High
**Likelihood**: High
**Impact**: Implementer writes `Ok(true)/Ok(false)` match arms that do not compile against the actual `Result<(), EdgeRedirectError>` signature, or silently omits the conflict case. If the spec's table is followed, a compile error surfaces immediately — but if the implementer reconciles by choosing one interpretation, AC-09 (Ok(false) treated as success) may be silently mis-handled as `Ok(false)` can never occur with the `Ok(())` signature.

**Historical evidence**: Lesson #4042 — "Pseudocode for non-obvious return semantics must lead with a contract table — callee file written first is the divergence risk." Lesson #4041 — `write_graph_edge` returns `bool`; `redirect_graph_edge` returns `Result<(), _>`. These are different functions and the spec must not conflate them.

**Test Scenarios**:
1. Unit test: compile-time verification that the redirect loop match arm handles `Ok(())` and `Err(EdgeRedirectError)` with no unreachable arms.
2. Unit test: seed an already-redirected edge (pre-insert `C → B`), call the redirect loop for `C → A → B`, assert `redirected == 1`, `failed == 0`, and the `Ok(false)` case is unreachable (no branch exists for it).
3. Code review gate: SPECIFICATION FR-07's `Ok(true)/Ok(false)` table must be corrected to match the ADR-003 contract table before implementation ships.

**Coverage Requirement**: The ADR-003 return contract table (`Ok(()) = success`, `Err = warn+failed`) must be the sole implementation reference. The spec's FR-07 table must not be implemented as written.

---

### R-02: Supersedes Exclusion Level Contradiction (Spec vs Architecture)

**Severity**: High
**Likelihood**: Med
**Impact**: If the implementer follows SPEC FR-04 (loop-level exclusion), Supersedes rows are fetched then discarded. If the implementer follows the ARCHITECTURE (SQL-level, ADR-002), they are excluded before fetch. AC-10 passes either way. But the contradiction means no single source of truth, and a code reviewer may request changes post-delivery.

**Test Scenarios**:
1. AC-10 unit test: seed a Supersedes row pointing at A; call `context_correct(A → B)`; assert the Supersedes row is unchanged (passes either implementation).
2. AC-10 extended: assert that `query_incoming_edges` does NOT return Supersedes rows (structural test — validates SQL-level exclusion per ADR-002 is implemented, not just that the loop skips them).
3. Integration test: seed only Supersedes incoming rows for the original entry; assert response text contains no "Redirected" append (AC-11 zero-edge behavior).

**Coverage Requirement**: Structural test on `query_incoming_edges` return value for Supersedes rows is required to distinguish SQL-level from loop-level exclusion.

---

### R-03: query_incoming_edges Index Usage and Pool Correctness

**Severity**: Med
**Likelihood**: Low
**Impact**: If `read_pool()` is inadvertently called as `write_pool_server()`, or if the bind parameter is mistyped, the query may execute without using `idx_graph_edges_target_id`, causing a full scan. Undetected until load testing or large datasets.

**Test Scenarios**:
1. AC-04 unit test: seed 1,000 edge rows for a different target_id and 3 rows for the test target_id; call `query_incoming_edges`; assert exactly 3 rows returned. (High-cardinality seeding verifies the WHERE clause is filtering correctly, not returning all rows.)
2. AC-04 extended: assert the returned `relation_type` values match the seeded values exactly (not swapped).
3. Code review: verify `read_pool()` is used (not `write_pool_server()`), and a comment documents the shared-pool implementation detail per C-07.

**Coverage Requirement**: High-cardinality unit test is required. Index usage cannot be asserted in tests but correctness is validated by the filter accuracy test.

---

### R-04: Per-Edge Source Validation Read Amplification

**Severity**: Med
**Likelihood**: Low
**Impact**: 50 `store.get(source_id)` calls plus 50 `redirect_graph_edge` transactions = 100 DB operations inline on the MCP call path. Acceptable given WAL mode and observed cardinalities, but unvalidated under load.

**Test Scenarios**:
1. Unit test: seed 10 incoming edges, all with Active sources; call redirect loop; assert `redirect_graph_edge` was called exactly 10 times, `store.get` was called exactly 10 times. (Mock-based or assertion on call count via a test double.)
2. Performance benchmark (optional, not gate-required): time 50-edge redirect under in-memory SQLite; assert completion within 200ms.

**Coverage Requirement**: Call-count assertion on the validation path confirms no extra reads are introduced per-edge. No performance gate required.

---

### R-05: Non-Deterministic Truncation Under Ceiling

**Severity**: Med
**Likelihood**: Med
**Impact**: When `query_incoming_edges` returns > 50 rows, the first 50 by SQLite insertion order are redirected. Different entries may be redirected on re-run if rows were inserted in different orders (e.g., after a migration or repair). The test must establish an ordering expectation to validate truncation behavior.

**Test Scenarios**:
1. Unit test: seed 55 incoming edges in a deterministic order; call redirect loop; assert exactly 50 redirects occur and the `tracing::warn!` truncation log is emitted with `total_found=55`.
2. Unit test: assert the response text includes `"(truncated from 55, see logs)"` per ADR-004.
3. Integration test: confirm that after truncation, the 5 unredirected edges still appear as `target_id = original_id` in `graph_edges`.

**Coverage Requirement**: Ceiling truncation must be tested with seeded data exceeding the ceiling. Both the warn log and the response text format must be verified.

---

### R-06: Contradicts Bidirectional Redirect with Quarantined Source

**Severity**: High
**Likelihood**: Med
**Impact**: Entry B holds a Contradicts edge `B → A` (with its reverse `A → B`). B is quarantined. The redirect loop queries `target_id = A`, which returns `B → A`. Source validation checks B's status (Quarantined) and skips — correct. However, if the reverse `A → B` also appears as an incoming edge to A... it does not, because `A → B` has `target_id = B`, not A. So R-06 risk is specifically about the *source* of the incoming edge being quarantined — the source validation correctly prevents the 4-row write that would create an edge from a quarantined source. The test must verify this path explicitly.

**Historical evidence**: Entry #4459 (graph-edges pattern: source-validation posture for Contradicts redirect loops) — applied as FR-06 and AC-08.

**Test Scenarios**:
1. AC-08 unit test: seed edge `B → A` (Contradicts) where B is Quarantined; call redirect loop; assert: edge `B → A` unchanged, `skipped == 1`, `failed == 0`, `tracing::warn!` emitted with source_id and "quarantined".
2. AC-08 variant: repeat with B in Deprecated status; same assertions.
3. AC-07 integration test: seed a valid Contradicts edge `C → A` (C is Active); call `context_correct(A → B)`; assert both `C → B` and `B → C` (reverse) rows exist in `graph_edges`.
4. Mixed test: seed one valid Contradicts (Active source) and one invalid Contradicts (Quarantined source); assert the valid one redirects (both directions) and the invalid one is skipped without failure count increment.

**Coverage Requirement**: Both quarantined and deprecated source statuses must be tested. The Contradicts bidirectional success path must also be covered.

---

### R-07: Supersedes-Only Incoming Edge Case

**Severity**: Med
**Likelihood**: Low
**Impact**: If the only incoming edges are Supersedes rows, `query_incoming_edges` (SQL-level exclusion per ADR-002) returns empty. AC-11 (no response append) is satisfied. AC-01 ("no stale non-Supersedes edges") is trivially satisfied. AC-10 (Supersedes row unchanged) must be verified specifically for this case.

**Test Scenarios**:
1. AC-10 unit test: seed only a Supersedes row pointing at A; call `context_correct(A → B)`; assert: Supersedes row `target_id = A` still exists, no new Supersedes row `target_id = B`, response text has no "Redirected" append.
2. Verify AC-08 zero-overhead behavior (FR-13): assert no `tracing::info!` summary log is emitted.

**Coverage Requirement**: This edge case is low-risk but must be explicit since it confirms the Supersedes exclusion and the zero-log behavior interact correctly.

---

### R-08: Partial Redirect Leaves DependencyOnDeprecated Detectable

**Severity**: Med
**Likelihood**: Med
**Impact**: After a partial redirect (some edges skipped due to quarantined source, some failed due to SQL error), the next graph tick runs `DependencyOnDeprecated` detection on the unredirected edges. These correctly surface as detections. SR-07 integration test (AC-16) is specified but the scenario of partial redirect followed by detection is not explicitly covered.

**Test Scenarios**:
1. AC-16 integration test (full redirect): after AC-06 flow (all edges redirected), trigger a graph state tick; assert no DependencyOnDeprecated event raised for the redirected edge source.
2. Partial redirect integration test (optional but recommended): after a redirect where one edge was skipped (quarantined source), trigger a graph tick; assert DependencyOnDeprecated is raised for the unredirected edge.

**Coverage Requirement**: AC-16 (full redirect clears detection) is required. Partial-redirect detection persistence is recommended.

---

### R-09: Ok(false) Counter Ambiguity in Response Text

**Severity**: Med
**Likelihood**: Med
**Impact**: The spec says `Ok(false)` is "treated as success" but does not prescribe whether `redirected++` increments. If the implementer increments `redirected` for `Ok(false)`, the response text says "Redirected 1" even though no row moved. If not incremented, the response says "Redirected 0" for a no-op. Neither is a correctness defect, but the spec ambiguity can produce behavior that surprises at gate review.

**Historical evidence**: Lesson #4042 — non-obvious return semantics require a leading contract table. ADR-003 clarifies `Ok(())` (conflict silently included) = `redirected++`. The AC-09 ambiguity is a spec gap.

**Test Scenarios**:
1. AC-09 unit test: pre-insert `C → B`; call redirect loop for `C → A → B`; assert response text contains "Redirected 1 incoming edges" (i.e., `redirected++` for `Ok(())`/conflict) OR "Redirected 0 incoming edges" (no increment) — the test must match the architectural decision, which prescribes `redirected++` per ADR-003's table.
2. AC-09 variant: assert `failed == 0` regardless.
3. Assert no `tracing::warn!` log for the conflict case.

**Coverage Requirement**: The counter increment behavior for `Ok(())` including implicit conflicts must be explicitly asserted in the test, not just "no failure." ADR-003 prescribes `redirected++`.

---

### R-10: Phase B + Redirect Loop Double-Write to graph_edges

**Severity**: Med
**Likelihood**: Low
**Impact**: Phase B writes edges declared in `params.edges` for the new entry. The redirect loop writes edges from deprecated entry's incomers. If an agent declares `C → new_entry.id` in Phase B and `C → A` also exists in `graph_edges`, the loop will attempt `C → A → new_entry.id`. The `INSERT OR IGNORE` absorbs the duplicate silently; the row survives with the Phase B `created_at`. No data corruption, but `redirected` counter may be inflated by 1.

**Test Scenarios**:
1. Unit test: seed `C → A` in `graph_edges`; in the same call, pass `C → B` (new entry) in Phase B `params.edges`; call `context_correct(A → B)`; assert `graph_edges` contains exactly one `C → B` row (no duplicate), and the `created_at` is the Phase B write's timestamp.
2. Assert `failed == 0` (the INSERT OR IGNORE does not error).

**Coverage Requirement**: This scenario is low-risk but covers the Phase B / redirect loop interaction, which has no dedicated AC.

---

### R-11: Response Text Append Verified in Integration Test

**Severity**: Med
**Likelihood**: Med
**Impact**: AC-12 and AC-13 are specified as unit tests with stubs. If the response text is appended in a code path not exercised by the stub (e.g., a different branch for the MCP response format), integration tests could pass while unit tests miss the real format.

**Test Scenarios**:
1. AC-06 integration test: assert the returned `CallToolResult` text from `context_correct` contains the string `"Redirected 2 incoming edges"` (exact substring match) when 2 Prerequisite edges are redirected.
2. AC-12 integration test: verify the format is `"Redirected N incoming edges (0 failed, see logs)"` (not `"Redirected N edges"` or other variants).

**Coverage Requirement**: The full MCP response text must be asserted in the integration test, not just in unit stubs.

---

### R-12: Summary Log Emission for Mixed-Type Zero-Non-Supersedes Case

**Severity**: Low
**Likelihood**: Med
**Impact**: If `query_incoming_edges` returns only Supersedes rows pre-loop (under loop-level exclusion) or 0 rows (under SQL-level exclusion), FR-09 (emit info summary) and FR-13 (omit summary when empty) are ambiguous. An implementer following FR-04 (loop-level) may emit a summary for "found=1, skipped-supersedes=1" which then triggers a response append incorrectly.

**Test Scenarios**:
1. Unit test: seed one Supersedes incoming edge; under SQL-level exclusion, assert `query_incoming_edges` returns 0 and no log is emitted. Under loop-level exclusion, assert a log IS emitted but response text is NOT appended.
2. The behavior chosen must match the ADR-002 decision (SQL-level exclusion preferred).

**Coverage Requirement**: This is a spec consistency gap. Low severity, but the test disambiguates the exclusion level choice.

---

### R-13: context_edge(mode="redirect") Regression

**Severity**: Med
**Likelihood**: Low
**Impact**: If any change to `edge_write.rs` is made (despite NFR-09), the Contradicts 4-row logic or RAII transaction behavior of the existing tool could regress. AC-15 requires existing tests pass without modification.

**Test Scenarios**:
1. Run existing `context_edge(mode="redirect")` test suite unchanged; assert all pass.
2. If `edge_write.rs` was not touched: structural review confirms NFR-09 is satisfied.

**Coverage Requirement**: No new tests needed if NFR-09 is honored. Existing test suite is the gate.

---

### R-14: TOCTOU Race on Source Status Check

**Severity**: Low
**Likelihood**: Low
**Impact**: `store.get(source_id)` returns Active; before `redirect_graph_edge` is called, a concurrent `context_quarantine` sets the source to Quarantined. The redirect proceeds and inserts an edge from a quarantined source. SQLite WAL serializes writes, but read-then-write patterns across two operations are not covered by a single transaction.

**Test Scenarios**:
1. This risk is not testable via deterministic unit tests. Document as an accepted degraded state consistent with ADR-003 partial-write posture.
2. Verify in code review that no lock is held between `store.get` and `redirect_graph_edge` (consistent with NFR-05).

**Coverage Requirement**: Accepted as low-probability infrastructure race. Document in code comments; no test gate required.

---

## Integration Risks

**SQL-level vs loop-level Supersedes exclusion**: The specification (FR-04) and architecture (ADR-002) contradict each other. This is the highest-priority integration risk because it affects what `query_incoming_edges` returns and whether the function is re-usable by future callers. ADR-002 must be the implementation reference; SPEC FR-04 and OQ-01 must be treated as superseded by ADR-002.

**Phase B and redirect loop ordering**: Both steps write to `graph_edges` in the same `context_correct` call. No shared transaction spans them (C-01). A Phase B edge for `new_entry.id` could collide with a redirect for the same source. The `INSERT OR IGNORE` prevents corruption but `created_at` for the surviving row may differ from what either writer intended.

**Contradicts bidirectionality with mixed source status**: When a fan-in batch contains both valid and invalid Contradicts edges, the skip-with-warn correctly prevents bad writes but produces an asymmetric result: some forward edges redirected, some not, while their corresponding reverse edges may be in an intermediate state. This is accepted per ADR-003 but must be explicitly tested (R-06, scenario 4).

---

## Edge Cases

- **Exactly 50 incoming edges**: ceiling is not triggered; all 50 are processed. Test must verify no warn log for exactly-at-ceiling.
- **Exactly 51 incoming edges**: ceiling triggers; 50 processed, 1 not. Warn log emitted. Response text includes truncation notation.
- **All incoming edges have Quarantined sources**: `redirected == 0`, `skipped == N`, `failed == 0`. Response text: "Redirected 0 incoming edges (0 failed, see logs)" — or omitted if `found == 0` is the gating condition. Spec FR-10 gates on `total_found > 0` (non-Supersedes edges found), so this still triggers append. Verify the response text is not misleading.
- **Single incoming edge that is already redirected (idempotent call)**: `Ok(())` returned; `redirected == 1`, `failed == 0`. No warn log.
- **`original_id` does not exist in `graph_edges`**: `query_incoming_edges` returns empty; zero-overhead path (AC-11). Correct_entry has already committed so the entry is valid.
- **Correction chain (A → B → C exists, then `context_correct(A → B)` called)**: edges pointing at A redirect to B (not C). AC-03 structural verification. B already has `superseded_by` set... wait — B cannot have `superseded_by` set if A is being corrected to B now; the chain A→B→C means B was corrected to C. But if B is already Deprecated (superseded by C), `context_correct` cannot be called on A to produce B — the existing handler would reject an attempt to correct A to an already-deprecated result. This scenario is a non-issue by construction (AC-03 unit test must use a genuinely two-hop chain where corrections happen in sequence, not simultaneously).

---

## Security Risks

**Untrusted input surface**: `context_correct` accepts `original_id` from the MCP caller. The redirect loop operates on `original_id` only after the entry has been validated as Active and the correction transaction has committed — the `original_id` is now Deprecated. The redirect loop's additional input is `query_incoming_edges(original_id)` — no caller-controlled data enters the SQL beyond the already-validated `original_id` integer.

**SQL injection**: `query_incoming_edges` uses `sqlx` parameterized queries (`WHERE target_id = ?1`). The `relation_type` string returned from the database is used as an argument to `redirect_graph_edge` but originates from the database itself (not from caller input). No injection vector.

**Blast radius**: A malicious caller with write access cannot control which edges are redirected via `context_correct` — the incoming edges are read from the database, not from the request. The ceiling (N=50) prevents a crafted hub entry with thousands of incoming edges from causing unbounded latency inline. The skip-with-warn for quarantined sources prevents a caller from using `context_correct` to force writes from quarantined nodes.

**Pool accessor correctness**: `write_pool_server()` is used for redirect transactions. If the read pool and write pool were ever split, using the wrong accessor for writes would fail silently on reads (no-op writes). The mandatory comment per C-07 documents this risk at the call site.

---

## Failure Modes

| Failure | Expected Behavior | Testable |
|---------|-------------------|----------|
| `query_incoming_edges` returns SQL error | Redirect loop does not execute; `context_correct` returns success (correction committed); warn log emitted | Yes — unit test with DB error injection |
| `redirect_graph_edge` returns `Err` for one edge | Loop continues; `failed++`; warn log; correction succeeds | Yes — AC-04 stub test |
| All `redirect_graph_edge` calls fail | `redirected == 0`, `failed == N`; correction succeeds; response: "Redirected 0 incoming edges (N failed, see logs)" | Yes — stub test |
| Source entry deleted between `store.get` and `redirect_graph_edge` | `redirect_graph_edge` returns `Err(TargetNotFound)` (wrong direction) — actually returns `Ok(())` if DELETE succeeds on a non-existent row. The DELETE is a no-op; the INSERT inserts a dangling edge. Mitigated by FK constraints if they exist (verify). | Low-probability race; accepted |
| `context_correct` atomic step fails | Redirect loop never executes (guard: loop only runs after `correct_entry` commits) | Yes — existing context_correct failure tests |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (Med): Per-edge transaction fan-in latency | R-04, R-05 | ADR-004 ceiling at N=50 bounds the worst-case inline latency. R-04 covers source validation read amplification (unceilinged). R-05 covers non-deterministic truncation ordering. |
| SR-02 (Med): `redirect_graph_edge` return contract ambiguity | R-01, R-09 | ADR-003 clarifies `Result<(), EdgeRedirectError>` contract. R-01 flags the SPEC FR-07 table still uses `Ok(true)/Ok(false)` — this is an unresolved documentation defect. R-09 covers the `Ok(false)` counter increment ambiguity remaining in AC-09. |
| SR-03 (Low): Pool accessor correctness | R-03 | ADR-003 posture + C-07 comment requirement. Addressed by code review. R-03 covers query-level correctness (pool + index). |
| SR-04 (Low): Supersedes fetched-then-discarded wastefulness | R-02, R-07 | ADR-002 resolves this at SQL level. R-02 flags the SPEC vs. Architecture contradiction. R-07 covers the Supersedes-only incoming edge edge case. |
| SR-05 (Low): Zero-edge response text ambiguity | R-12 | ADR-004 resolves: omit line when `found == 0`. R-12 flags the spec ambiguity for mixed-type zero-non-Supersedes case under loop-level exclusion. |
| SR-06 (High): Contradicts source validation — quarantined/deprecated source | R-06 | ADR-003 resolves: skip-with-warn. R-06 covers both the success and skip paths for Contradicts bidirectionality, including mixed-status fan-in batches. |
| SR-07 (Med): Partial redirect leaves DependencyOnDeprecated detectable | R-08 | ADR-003 partial-write posture accepted. R-08 requires AC-16 integration test to confirm full redirect clears detection. Partial-redirect detection persistence is recommended coverage. |
| SR-08 (Low): read.rs file size / merge conflict risk | — | Not architecture-level. Accepted; no test coverage warranted. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-06) | 9 scenarios — return contract structural test, Supersedes exclusion structural test, Contradicts mixed-status tests |
| High | 7 (R-03 through R-05, R-07 through R-11) | 14 scenarios — index/pool unit tests, ceiling tests, zero-edge log, Phase B collision, response text integration assertion |
| Medium | 3 (R-12, R-13, R-14) | 4 scenarios — mixed-type log ambiguity, context_edge regression suite, TOCTOU documentation |
| Low | 1 (R-14) | Accepted; no test gate |

**Minimum test gate**: All Critical scenarios (9) + AC-06, AC-07, AC-08, AC-09, AC-10, AC-11, AC-12, AC-14, AC-15, AC-16 from specification must pass before merge.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned graph edges redirect — found #4077 (graph direction semantics), #4042 (return contract table discipline), #4076 (test omission gate failure)
- Queried: `/uni-knowledge-search` for risk patterns graph edge write transaction — found #4041 (write_graph_edge bool return vs redirect_graph_edge Result), #4417 (agent edge write placement), #4435 (Phase A/B split)
- Queried: `/uni-knowledge-search` for source validation quarantined deprecated — found #4459 (source-validation posture, pre-staged for this feature)
- Queried: `/uni-knowledge-search` for SQLite INSERT OR IGNORE unique constraint — found #4396 (TOCTOU WAL race, informs R-14)
- Stored: nothing novel to store — R-01 (spec/architecture return contract contradiction) is feature-specific, not a recurring pattern yet. If a second feature ships with the same divergence, store as a pattern then.
