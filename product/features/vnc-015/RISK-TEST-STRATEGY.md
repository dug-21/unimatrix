# Risk-Based Test Strategy: vnc-015

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `from_str()` arm missing for one or more of the 10 new RelationType variants — silent row-drop at Pass 2b R-10 guard, no compile error | High | High | Critical |
| R-02 | `redirect_graph_edge` transaction implemented via raw `BEGIN`/`COMMIT` SQL strings against sqlx pool — multi-connection pool silently loses data (lesson #2269) | High | High | Critical |
| R-03 | `write_graph_edge` bool semantics misread — loop uses `Ok(_) => true` contract instead of `rows_affected() > 0` three-case contract; causes incorrect error handling or double-logging | High | Med | Critical |
| R-04 | Bidirectional Contradicts partial write — first direction (A→B) succeeds, second direction (B→A) fails infrastructure error; graph is permanently asymmetric with no signal to caller | High | Med | Critical |
| R-05 | `context_edge` redirect partial failure — old edge deleted, new edge insert fails (non-transaction path) leaves entry with no edge; data loss for supersession workflow | High | Med | Critical |
| R-06 | `context_edge` source status check fails open — `SourceFrozen` validation bypassed (wrong `Status` enum comparison or integer mismatch) allows Write-capable agents to mutate edges on quarantined or deprecated entries, corrupting frozen knowledge | Med | Low | High |
| R-07 | `query_contradicts_edges_for_entry` bidirectional fix changes existing caller behavior — pre-existing tests assert old asymmetric result, false-pass or false-fail after fix (SR-06) | Med | High | High |
| R-08 | Target validation N DB lookups for N edges — latency proportional to edge slice size; no bound enforced; edge slice > 10 adds measurable pre-insert latency on hot path | Med | Med | High |
| R-09 | Self-referential check sequencing — ADR-001 reveals `source_id` for `context_store` is only known post-insert (auto-increment); if check runs pre-insert on a placeholder value, it is vacuous | Med | Med | High |
| R-10 | `default_rules()` signature change breaks all callers outside `context_cycle_review` — callers that pass only `history` gain a compile error; tests silently pass `vec![]` and fire no findings | Med | High | High |
| R-11 | `RelatedTo` PPR weight misconfigured — unequal weight vs existing 4 positive types causes silent scoring drift; OR `Advances`/`Motivates` accidentally added to PPR (should be write-only, Phase 2 deferral) | Med | Low | Medium |
| R-12 | Duplicate entry suppression gap — edge writes triggered before duplicate guard check; phantom edges written for content that was never uniquely stored (pattern #4417) | Med | Low | Medium |
| R-13 | `new_target_id` present on add/remove mode not rejected — stale or confused MCP client sends extra field; handler ignores it silently rather than erroring | Low | Med | Medium |
| R-14 | `stale_dependency_edges` SQL JOIN uses wrong status integer constant — `status = 0` (Active) instead of `status = 1` (Deprecated); count always zero | Med | Low | Medium |
| R-15 | `EDGE_SOURCE_AGENT` constant not used — inline magic string `"agent"` used in one or more write paths; attribution diverges across surfaces | Low | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: from_str() Arm Missing — Silent Row-Drop
**Severity**: High
**Likelihood**: High
**Impact**: Any of the 10 new variants written to GRAPH_EDGES is silently discarded by `build_typed_relation_graph` Pass 2b R-10 guard. Edges exist in the DB but are invisible to PPR, graph_expand, and all traversal. Phase 2 (`context_graph`) launches on a sparse graph. No error, no warning, no test failure at the write site.

**Test Scenarios**:
1. Unit test per variant: `assert_eq!(RelationType::from_str(v.as_str()), Ok(v))` round-trip for all 10 new variants.
2. Integration test per variant: insert a GRAPH_EDGES row with the variant string directly; call `build_typed_relation_graph`; assert edge count equals 1 (not 0).
3. Gate-3a grep: verify each variant string appears in `graph.rs` enum body, `as_str()` match, and `from_str()` match independently.
4. Regression: all 6 existing variants still parse and survive Pass 2b unchanged.

**Coverage Requirement**: Every one of the 10 new variants must have an explicit `from_str()` round-trip test AND a Pass 2b survival test. No variant may be covered only by a single omnibus test — each must be individually assertable.

---

### R-02: redirect_graph_edge Transaction — Manual BEGIN/COMMIT sqlx Pool Risk
**Severity**: High
**Likelihood**: High
**Impact**: If `redirect_graph_edge` implements atomicity via raw `sqlx::query("BEGIN").execute(pool)` / `COMMIT` / `ROLLBACK`, each statement acquires a different pool connection. The DELETE and INSERT run on separate connections — the DELETE auto-commits, the INSERT has no open transaction. Under `write_max_connections >= 2`, data is silently lost: old edge deleted, new edge never written. No error, no panic, no log (lesson #2269, gate 3b failure in nxs-011). The Contradicts 4-row case (2 deletes + 2 inserts) multiplies the exposure.

**Test Scenarios**:
1. Code review gate: verify `redirect_graph_edge` uses `pool.begin().await?` returning a `Transaction<'_, Sqlite>` RAII guard; all 4 SQL statements execute against `&mut *txn`.
2. Integration test — redirect non-Contradicts edge: add A→B; redirect to B'; assert A→B deleted AND A→B' present in one assertion block.
3. Integration test — redirect Contradicts edge: add A↔B; redirect to B'; assert all 4 rows updated atomically (A→B gone, B→A gone, A→B' present, B'→A present).
4. Integration test — redirect to non-existent new_target: transaction must roll back; original A→B remains unchanged after failed redirect.
5. Integration test — verify `write_pool_server()` `write_max_connections` value; confirm the test DB config uses >= 2 to expose pool-multiplexing issues.

**Coverage Requirement**: The Contradicts 4-row atomic case must be tested as a single assertion set. Rollback-on-failure must be verified by confirming original rows survive a failed redirect.

---

### R-03: write_graph_edge Bool Semantics Misread
**Severity**: High
**Likelihood**: Med
**Impact**: Edge write loop written with `Ok(_) => true` semantics (nli_detection pattern mismatch) causes one of two failure modes: (a) loop treats UNIQUE conflict (idempotent `false`) as an error and aborts edge writes or surfaces spurious errors to caller; (b) loop fails to log true SQL errors. Pattern #4041 documents this exact crt-040 Gate 3a rework root cause.

**Test Scenarios**:
1. Integration test — re-assert the same edge twice via `context_store` with same `edges` entry; assert no error returned, assert GRAPH_EDGES has exactly 1 row for that triplet.
2. Integration test — re-assert same Contradicts edge; assert no error, assert exactly 2 rows (A→B and B→A), not 4.
3. Code review gate: `validate_and_write_edges` loop must be headed by the three-case contract table (pattern #4041); `bool` return must be checked, not `Result`.

**Coverage Requirement**: Idempotent re-assertion (UNIQUE conflict path) must be explicitly tested for both a non-Contradicts edge and a Contradicts edge. The SQL error path (`false` from Err) must be covered by a log-verification test or comment noting the internal logging behavior.

---

### R-04: Bidirectional Contradicts Partial Write
**Severity**: High
**Likelihood**: Med
**Impact**: `validate_and_write_edges` calls `write_graph_edge` twice for Contradicts (A→B then B→A). These are sequential fire-and-forget writes — not transactional. If the second write fails infrastructure error, the graph has `(A, B, Contradicts)` but not `(B, A, Contradicts)`. PPR and contradiction suppression see only one direction. Contradiction detection may miss the relationship depending on which direction `query_contradicts_edges_for_entry` queries. No rollback of first direction. Caller receives success response.

**Test Scenarios**:
1. Integration test — write Contradicts edge; query GRAPH_EDGES for both `(A, B, Contradicts)` and `(B, A, Contradicts)`; assert both present.
2. Integration test — verify `query_contradicts_edges_for_entry(A)` and `query_contradicts_edges_for_entry(B)` both return the edge post-bidirectional write (covers R-07 as well).
3. Code review: verify both `write_graph_edge` calls are in the same handler execution path (not deferred to tick), and that the first write's `bool` return is checked before proceeding to the second.

**Coverage Requirement**: Both directions must be asserted after every Contradicts write test. A test that only checks one direction provides no coverage for this risk.

---

### R-05: context_edge Redirect Partial Failure — Data Loss
**Severity**: High
**Likelihood**: Med
**Impact**: If `redirect_graph_edge` is not fully transactional (see R-02), the delete-then-insert compound can leave the source entry with no edge at all. Unlike the partial-write posture of ADR-003 (entry exists without edges — degraded but not data loss), a failed redirect produces a worse outcome: the old relationship is retracted and the new one is not established. For the supersession use case (`Advances → A` retargeted to `Advances → A'`), the author's entry now has no `Advances` edge to either version. No rollback, no error to caller.

**Test Scenarios**:
1. Integration test — inject a simulated write failure after DELETE but before INSERT (via bad `new_target_id`); assert original `(source_id, target_id, relation_type)` row is still present (transaction rolled back).
2. Integration test — redirect to a `new_target_id` that is quarantined; assert validation fires before transaction begins; assert original edge unchanged.
3. Integration test — successful redirect; confirm atomicity by asserting the old row absent AND new row present in a single query after the call.

**Coverage Requirement**: The rollback-on-failure scenario is the critical coverage gap. At minimum one test must confirm old edge survives when the redirect fails. The Contradicts 4-row atomic case requires its own test (R-02 Scenario 3 covers this jointly).

---

### R-06: context_edge Source Status Check Fails Open
**Severity**: Med
**Likelihood**: Low
**Impact**: There is no ownership check on `context_edge` — any `Capability::Write` agent may operate on any active entry. The only guard preventing mutation of frozen entries is the `SourceFrozen` check (`Status::Quarantined` or `Status::Deprecated`). If this check fails open — wrong status integer comparison, `Status` enum variant mismatch, or fetch-error swallowed — a Write-capable agent can retarget or remove edges on quarantined or deprecated entries. Blast radius: permanently altered graph relationships on entries that were intentionally frozen.

**Test Scenarios**:
1. Integration test — enroll agent A; create entry E; quarantine E via `context_quarantine`; agent A calls `context_edge(mode: "add")` on E; assert `SourceFrozen` returned, GRAPH_EDGES unchanged.
2. Integration test — create entry E; deprecate E via `context_correct`; agent A calls `context_edge(mode: "redirect")` on the deprecated E; assert `SourceFrozen`, no mutation.
3. Integration test — create active entry E; agent A calls `context_edge` on E; assert success (positive baseline — active entries are operatable).
4. Code review: confirm `SourceFrozen` check uses `Status::Quarantined` and `Status::Deprecated` from `unimatrix_store::schema` — not integer literals `1` or `3`.

**Coverage Requirement**: Quarantined and deprecated source cases must be explicitly tested for at least one `context_edge` mode each. The active-source positive baseline must be tested for all three modes (add, remove, redirect).

---

### R-07: query_contradicts_edges_for_entry Behavior Change Breaks Existing Callers
**Severity**: Med
**Likelihood**: High
**Impact**: The fix changes `WHERE target_id = ?1` to `WHERE (source_id = ?1 OR target_id = ?1)`. Pre-vnc-015, NLI writes Contradicts edges with direction determined by detection order — unidirectional. Existing tests that assert only one direction result will now return 2 rows. Any caller that processes the result as a scalar (`.first()`, single-row expectation) will produce incorrect behavior. The risk is not the fix itself but undiscovered callers that silently change behavior.

**Test Scenarios**:
1. Audit test: identify all call sites of `query_contradicts_edges_for_entry` in the codebase before writing new tests; document expected behavior change for each.
2. Integration test — write NLI-style unidirectional Contradicts (A→B only, simulating pre-vnc-015 data); call function with A and B; assert A returns 1 row (FROM direction), B returns 1 row (TO direction) — old data handled by OR clause.
3. Integration test — write bidirectional Contradicts (both A→B and B→A); call with A; assert 2 rows returned (not 1); call with B; assert 2 rows.
4. Regression test: existing `suppress_contradicts` behavior verified unchanged after the query fix.

**Coverage Requirement**: The OR-clause fix must be tested against both pre-vnc-015 unidirectional data (transition period compatibility) and post-vnc-015 bidirectional data. Each existing caller must be either tested for correct new behavior or annotated as safe.

---

### R-08: Target Validation N DB Lookups — Performance
**Severity**: Med
**Likelihood**: Med
**Impact**: `validate_and_write_edges` performs one `store.get_entry_by_id()` call per target_id before any write. For typical slices (1–5 edges) this is acceptable (ADR-010). No upper bound is enforced on the `edges` vec length. A malicious or buggy caller can declare 50 edges, triggering 50 sequential async DB reads on the hot `context_store` path. Each hits `read_pool()` so no write blocking, but latency accumulates and exceeds user-facing timeout thresholds at high N.

**Test Scenarios**:
1. Validation test: confirm no `max_edges` limit is enforced; document the accepted design choice (no limit per SCOPE.md).
2. Performance test (advisory): time `context_store` with `edges` slice of 1, 5, 10, 20; chart latency growth; flag if any size exceeds a documented threshold.
3. Integration test — edge slice with mixed valid/invalid entries; confirm first-error-abort fires immediately (does not validate all N before returning); confirm no partial writes for the validated-so-far edges.

**Coverage Requirement**: First-error-abort behavior (not validate-all-collect-all) must be explicitly tested — a slice of 5 edges where edge[2] is invalid should return error referencing edge[2] with 0 DB writes, and should not have validated edges[3] and [4].

---

### R-09: Self-Referential Check Sequencing — source_id Not Yet Known Pre-Insert
**Severity**: Med
**Likelihood**: Med
**Impact**: ADR-001 notes that for `context_store`, `source_id` is the auto-increment ID assigned by DB insert — it is not known before insert. The self-referential check `source_id == target_id` must run post-insert using the actual assigned ID. If the implementation runs it pre-insert against a placeholder (e.g., 0 or u64::MAX), the check is vacuous — any `target_id` would pass because the placeholder never equals a real entry ID. A caller who wants to create a self-referential entry can trivially succeed.

**Test Scenarios**:
1. Integration test — call `context_store` with `target_id` equal to the entry's actual auto-increment ID (determined by a pre-test insert to establish the next ID sequence); assert `SelfReferentialEdge` error returned and no entry written.
2. Integration test — call `context_edge` with `source_id == target_id`; assert `SelfReferentialEdge` error, no mutation.
3. Code review: confirm the self-referential check in `validate_and_write_edges` runs after entry insert returns the new `id`, not before.

**Coverage Requirement**: The `context_store` self-referential test must use the actual DB-assigned ID, not an arbitrary value. Using `target_id = 0` would pass trivially but not cover the real risk.

---

### R-10: default_rules() Signature Change Breaks Callers
**Severity**: Med
**Likelihood**: High
**Impact**: `default_rules()` gains a second parameter `stale_edges: Vec<(u64, u64)>`. All existing callers — including test helpers that call `default_rules(history)` — gain a compile error. Callers patched with `vec![]` as second argument will compile and run but `DependencyOnDeprecated` will never fire in those test contexts, masking a functional gap. The test `test_default_rules_has_22_rules` must be updated to assert 23 — if it is not, it continues to pass while the actual count is 23, falsely validating the count.

**Test Scenarios**:
1. Compile-time gate: after signature change, all callers of `default_rules()` must be updated; CI must pass cleanly with no `#[allow(unused_variables)]` suppressions.
2. Unit test — `default_rules(None, vec![]).len() == 23` (updated from 22).
3. Integration test — call `context_cycle_review` with a feature cycle that contains a stale Prerequisite edge; assert `DependencyOnDeprecated` finding appears with severity Warning and rule name `"dependency_on_deprecated"`.
4. Unit test — `DependencyOnDeprecatedRule::new(vec![(1, 2)]).detect(&[])` returns at least one finding with correct rule name and severity.

**Coverage Requirement**: The count test must be the first assertion after any change to `default_rules()`. The detection test must use a non-empty `stale_edges` vec to confirm the rule actually fires, not just that it was registered.

---

## Integration Risks

**Bidirectional Contradicts in context_edge vs validate_and_write_edges**: Both code paths must independently implement the two-write sequence for Contradicts. If only one path is correct, tests that cover one surface will not catch the defect in the other. Each surface (edges param, context_edge add, context_edge remove, context_edge redirect) must have its own Contradicts bidirectionality test.

**default_rules() signature breaking change**: The `context_cycle_review` handler, all `detection/mod.rs` test functions, and any integration test invoking `default_rules()` directly are all affected. The signature change must be applied atomically across the codebase — a partial update compiles with one caller and fails for another.

**query_contradicts_edges_for_entry call site inventory**: The ARCHITECTURE.md names `suppress_contradicts` as the existing caller. If other callers exist (e.g., in `context_cycle_review` hotspot logic or test helpers), the behavior change affects them too. Full grep of the codebase for calls to this function is required before implementation.

**edge_write.rs pub(crate) scope boundary**: `validate_and_write_edges`, `delete_graph_edge`, and `redirect_graph_edge` are `pub(crate)` in `unimatrix-server`. Integration tests that test these directly must be in the same crate. Tests in external test crates cannot import them; test coverage is only achievable via MCP handler integration tests.

---

## Edge Cases

**Empty edges vec (`Some([])`)**: Must be treated as no edges — identical behavior to `None`. Must not trigger target validation loops, must not write any GRAPH_EDGES rows, must not fail.

**Single-edge Contradicts via context_store**: The source entry (new) is the source of the Contradicts edge. The reverse direction writes `(target_id, source_id, Contradicts)` where `target_id` is an existing entry. Both directions must appear in GRAPH_EDGES even though the source does not yet exist when validation runs.

**redirect to same target (no-op redirect)**: `context_edge(mode: "redirect", target_id: X, new_target_id: X)`. The DELETE removes the existing edge; the INSERT re-creates it. Net result: no change. This is technically valid but potentially confusing — no error should be returned, but the behavior is idempotent.

**remove of non-existent edge**: `context_edge(mode: "remove")` where the edge does not exist in GRAPH_EDGES. DELETE affects 0 rows — this must return success, not `EdgeNotFound`. This must also hold for the reverse direction of a Contradicts remove.

**redirect mode with Contradicts: new_target already has a Contradicts edge from source**: The INSERT OR IGNORE means the second write is a no-op. The DELETE for the old target still fires. End state: old directions gone, new directions present (one pre-existing, one newly inserted). This is correct behavior but should be tested explicitly.

**redirect mode: new_target_id == old target_id**: Same as redirect to same target — valid no-op, no error.

**PPR with new variants in graph but not in positive_out_degree_weight**: The 9 write-only variants (`Advances`, `Motivates`, `Cites`, `Asserts`, `Mentions`, `Refutes`, `Tests`, `DerivedFrom`, `About`) appear in `TypedRelationGraph` but have no effect on PPR mass flow. A test that writes these variants and checks that PPR scoring is unchanged from baseline confirms the intentional absence. This includes `Advances` and `Motivates` — they are explicitly write-only in this feature; PPR expansion for directed semantic types is deferred to Phase 2.

**DependencyOnDeprecated with zero stale edges**: `DependencyOnDeprecatedRule::new(vec![]).detect(records)` must return empty findings — no false-positive Warning when stale_edges is empty.

---

## Security Risks

**context_edge write-open-by-design — no ownership check**: Any `Capability::Write` agent may operate on any active entry's edges regardless of who created it. This is the accepted design (agent_id is not a reliable ownership anchor in this RBAC model). The sole mutation guard is `SourceFrozen` (quarantined/deprecated source rejected). Implication: a Write-capable agent can retarget an ADR edge created by another agent. This is intentional — collaborative graph maintenance is a design goal. The risk is `SourceFrozen` failing open (covered by R-06), not the open-by-design write model itself.

**context_edge source_id injection — non-existent or quarantined source**: The source fetch (step 2) must return a hard error if the entry does not exist. A caller that passes a `source_id` referencing a quarantined entry must receive `SourceFrozen`, not proceed to mutation. If the source fetch is missing, any integer value could be used as a source_id and edges written with no source validation. Blast radius: phantom edges from non-existent sources would corrupt `TypedRelationGraph` traversal.

**Edge type string injection**: `RelationType::from_str()` is the sole guard against unknown edge type strings. If the `from_str()` match falls through to a wildcard arm (e.g., `_ => Some(RelatedTo)`) rather than returning `None` for unknown types, all unknown strings would be silently stored as `RelatedTo`. GRAPH_EDGES accepts arbitrary TEXT for `relation_type`; the guard is in Rust code only. Blast radius: phantom edge types stored in DB would not parse in `build_typed_relation_graph` (correctly dropped by R-10) but would still appear as rows and count toward graph density metrics.

**Unenrolled agent calling context_edge**: An agent without `Capability::Write` must be rejected at step 1 (capability gate) before reaching source fetch. If the capability gate fails open (returns Ok on a missing capability), any unenrolled caller can mutate edges. The existing gate is battle-tested for `context_store` — `context_edge` must use the same gate invocation, not a new implementation.

**stale_dependency_edges SQL injection via relation_type literal**: The `stale_dependency_edges` query uses `relation_type = 'Prerequisite'` as a hardcoded SQL literal in the query template. This is safe. However, if a developer refactors to use a format-string interpolation (e.g., `format!("...relation_type = '{}'", type_name)`), SQL injection becomes possible. Coverage: verify the query uses a bound parameter or hardcoded literal, not runtime string interpolation.

---

## Failure Modes

**Entry created, all edge writes fail (partial-write per ADR-003)**: Caller receives a success response. The entry exists in the DB with `created_by` and all metadata, but no declared edges exist in GRAPH_EDGES. PPR traversal will not surface this entry via Goal-tracing edges. The expected observable behavior: `context_store` returns the new entry with its ID; a subsequent query for the entry succeeds; a subsequent `context_edge(add)` on the same triplets from the same agent would re-assert the edges. No automated recovery.

**redirect transaction rolled back (new_target quarantined)**: Target validation fires before the transaction begins (step 6 in the 6-step pipeline). The transaction is never opened. Original edge unchanged. Caller receives `TargetQuarantined` error. This is the correct and tested path.

**redirect transaction rolled back (mid-transaction infrastructure error)**: Transaction opened via `pool.begin()`; DELETE succeeds; INSERT fails (pool error, disk full). The RAII `Transaction` guard drops, issuing ROLLBACK automatically. Original edge is restored. Caller receives an error. This is the correct behavior if R-02 is implemented correctly.

**DependencyOnDeprecated fires but stale_edges were queried for wrong cycle scope**: If the `context_cycle_review` handler queries stale edges for all features globally rather than for the current feature cycle's entries only, the rule fires for unrelated deprecations. Expected behavior: findings are scoped to entries in the current cycle. Failure mode: noisy findings unrelated to the active cycle, reducing developer trust in the rule.

**context_edge called on a deprecated source (SourceFrozen)**: Caller receives `SourceFrozen` error. No mutation occurs. Expected behavior confirmed by AC-23. The observable state: entry remains deprecated, GRAPH_EDGES unchanged.

**PPR run after RelatedTo edge written but before TypedRelationGraph cache refresh**: PPR is computed at query time from the `TypedRelationGraph`. If the graph cache is rebuilt periodically (background tick), there is a window where a newly written `RelatedTo` edge is in GRAPH_EDGES but not in the in-memory graph. PPR would not flow through it until the next rebuild. Expected behavior: eventual consistency. Not a defect — document as known eventual consistency window. Note: `Advances`/`Motivates` edges are write-only (no PPR) so this window does not apply to them.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (High) — 10 variants × 4 sites, silent from_str drop | R-01 | ADR-007: 10×4 checklist table in spec; Gate-3a grep verification; R-01 requires per-variant Pass 2b integration test |
| SR-02 (Med) — write_graph_edge bool semantics | R-03 | ADR-003 partial-write posture + pattern #4041 three-case contract; R-03 requires idempotent re-assertion test |
| SR-03 (Med) — no transaction spanning entry + edge writes | R-04, R-05 | ADR-003: accepted partial-write posture for context_store/context_correct; redirect_graph_edge is an explicit exception (transactional per ADR-009); R-04 covers Contradicts partial write; R-05 covers redirect data loss |
| SR-04 (Med) — tools.rs 500-line limit | — | ADR-005: edge_write.rs module extraction resolves this architecturally; no test risk |
| SR-05 (Low) — DependencyOnDeprecated injection interface | R-10 | ADR-004: typed `Vec<(u64, u64)>` constructor injection; R-10 covers signature change blast radius |
| SR-06 (Med) — query_contradicts_edges_for_entry behavior change | R-07 | ARCHITECTURE.md Component 7: OR-clause fix with caller audit; R-07 requires explicit transition-period compatibility tests |
| SR-07 (Med) — PPR weight for RelatedTo | R-11 | ADR-006: RelatedTo only at equal weight; R-11 requires PPR scoring regression test AND negative test confirming Advances/Motivates do NOT flow through PPR |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 5 (R-01–R-05) | 18 scenarios minimum — per-variant from_str round-trips (10), Pass 2b survival tests (10), transaction RAII pattern (1), redirect atomic rollback (2), Contradicts bidirectionality per surface (4), bool semantics idempotency (2) |
| High | 5 (R-06–R-10) | 12 scenarios — source status (SourceFrozen) cases (3), caller audit (2), first-error-abort (2), self-referential post-insert (2), default_rules count + detection (3) |
| Medium | 4 (R-11–R-14) | 6 scenarios — PPR regression (2), duplicate guard ordering (1), new_target_id mode rejection (1), stale count SQL correctness (2) |
| Low | 1 (R-15) | 1 scenario — EDGE_SOURCE_AGENT constant usage (1) |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` "lesson-learned failures gate rejection" — found #2758 (Gate 3c grep requirement), #1203 (cascading rework). Both general process lessons.
- Queried: `/uni-knowledge-search` "risk pattern graph edge write transaction partial failure" — found #4041 (write_graph_edge bool contract, directly elevates R-03), #3897 (bidirectional helper extraction, informs R-04), #4417 (MCP handler edge placement, informs R-12).
- Queried: `/uni-knowledge-search` "SQLite transaction write_pool_server begin commit atomicity" — found **#2269** (manual BEGIN/COMMIT loses data across pool connections — directly elevates R-02 from Medium to Critical), #4398 (atomic counter pattern).
- Queried: `/uni-knowledge-search` "RelationType from_str enum variant silent drop" — found #3950 (four-site extension requirement, confirms R-01), #3650 (bidirectional Contradicts traversal, informs R-07).
- Stored: nothing novel to store — risk patterns for this feature are either already in Unimatrix (#4041, #2269, #3950) or are feature-specific risks not warranting cross-feature storage.
