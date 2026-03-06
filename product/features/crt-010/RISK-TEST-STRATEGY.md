# Risk-Based Test Strategy: crt-010

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `get_embedding` via hnsw_rs internal API may not be supported — `get_point_indexation().get_point_data()` is undocumented and may not exist or may change across versions | High | High | Critical |
| R-02 | Penalty multipliers (0.7/0.5) cause incorrect ranking when deprecated entry has significantly higher similarity (e.g., >30% delta), making penalty insufficient or excessive | High | Med | High |
| R-03 | Strict mode returns empty results on queries where only deprecated entries have semantic matches, degrading UDS silent injection to zero context | High | Med | High |
| R-04 | Cosine similarity computation on hot search path adds latency — per-successor vector fetch + dot product for each superseded result in top-k | Med | High | High |
| R-05 | Dangling `superseded_by` references cause silent information loss — deprecated entry penalized/dropped but successor never injected | Med | Med | Med |
| R-06 | Co-access deprecated exclusion changes `compute_search_boost` signature, breaking all existing callers across server and briefing paths | Med | High | High |
| R-07 | MCP explicit `status: Deprecated` filter bypasses penalties but supersession injection still runs, potentially injecting Active successors into a deprecated-only query | Med | Med | Med |
| R-08 | Compaction prunes deprecated embeddings from HNSW but successor injection needs predecessor in HNSW top-k to trigger — post-compaction, supersession injection becomes unreachable | High | High | Critical |
| R-09 | Race between compaction (removes deprecated from VECTOR_MAP) and concurrent search (reads VECTOR_MAP for successor embedding) | Med | Low | Med |
| R-10 | BriefingService injection history filtering changes affect briefing quality — overly aggressive filtering removes relevant context from compact payloads | Med | Med | Med |
| R-11 | `cosine_similarity` helper assumes L2-normalized inputs; denormalized vectors produce scores outside [0,1], corrupting ranking | High | Low | Med |
| R-12 | `RetrievalMode` default of `Flexible` silently changes behavior for any existing callers that previously got unfiltered results | Med | Med | Med |

## Risk-to-Scenario Mapping

### R-01: hnsw_rs get_embedding API Availability
**Severity**: High
**Likelihood**: High
**Impact**: Entire supersession injection feature blocked if embeddings cannot be retrieved from HNSW. Fallback to re-embedding adds ONNX inference to hot path.

**Test Scenarios**:
1. Unit test: `VectorIndex::get_embedding(known_id)` returns the exact embedding that was inserted
2. Unit test: `VectorIndex::get_embedding(unknown_id)` returns `None`
3. Unit test: After `compact()`, `get_embedding` still works for surviving entries

**Coverage Requirement**: Verify hnsw_rs data layer API works before building injection pipeline on top of it.

### R-02: Penalty Multiplier Ranking Correctness
**Severity**: High
**Likelihood**: Med
**Impact**: Wrong penalty values silently serve stale knowledge (too lenient) or suppress all deprecated entries (too harsh, equivalent to strict mode).

**Test Scenarios**:
1. Active entry sim=0.88, Deprecated entry sim=0.90 → Active ranks higher (AC-02)
2. Active entry sim=0.88, Deprecated entry sim=0.95 → verify ranking at extreme similarity gaps
3. Superseded entry (0.5x) ranks below plain deprecated (0.7x) at equal similarity (AC-03)
4. All results deprecated, flexible mode → results still returned (not empty)
5. Entry with `superseded_by` gets SUPERSEDED_PENALTY, not DEPRECATED_PENALTY

**Coverage Requirement**: Ranking invariant tests with parametric similarity values across the penalty boundary.

### R-03: Strict Mode Empty Results
**Severity**: High
**Likelihood**: Med
**Impact**: UDS agents receive zero context for queries in knowledge domains dominated by deprecated entries (currently 70% of entries). Agents operate blind.

**Test Scenarios**:
1. All matching entries deprecated → strict mode returns empty vec, not panic (AC-10)
2. Mix of deprecated and Active entries → strict mode returns only Active subset
3. Only superseded entries match → successor injection still adds Active successors even when all direct matches are dropped
4. Empty result returns `total_tokens: 0` signaling no injection occurred

**Coverage Requirement**: Test empty-result path end-to-end through UDS listener response formatting.

### R-04: Successor Similarity Computation Latency
**Severity**: Med
**Likelihood**: High
**Impact**: p95 search latency regresses beyond 15% threshold (NFR-1.1). Each superseded result triggers a store fetch + vector fetch + cosine computation.

**Test Scenarios**:
1. Benchmark: 200-entry KB, 50% deprecated, 25% with supersession chains — measure p95 latency vs baseline (AC-16)
2. Worst case: all top-k results are superseded → N successor fetches in single search
3. Batch fetch optimization: successors fetched in single store read, not N individual reads (FR-2.2)

**Coverage Requirement**: Benchmark test measuring latency with and without supersession injection.

### R-05: Dangling Supersession References
**Severity**: Med
**Likelihood**: Med
**Impact**: Deprecated entry penalized or dropped from results, but its successor never appears because the referenced ID doesn't exist. Knowledge gap.

**Test Scenarios**:
1. `superseded_by: 99999` (non-existent) → search completes, no panic, no injection (AC-07)
2. Successor exists but is itself Deprecated → skip injection, no recursion (AC-06 partial)
3. Successor exists but is Quarantined → skip injection
4. Successor exists but itself has `superseded_by` set → skip injection (FR-2.3c)

**Coverage Requirement**: Test all invalid successor states — missing, deprecated, quarantined, itself superseded.

### R-06: Co-Access Signature Change Breaks Callers
**Severity**: Med
**Likelihood**: High
**Impact**: Compilation failure or runtime error at every call site of `compute_search_boost` and `compute_briefing_boost`. Architecture open question #1 confirms BriefingService is an affected caller.

**Test Scenarios**:
1. `compute_search_boost` with deprecated_ids containing anchor → zero boost for that anchor (AC-08)
2. `compute_search_boost` with deprecated_ids containing partner → zero boost for that pair (AC-08)
3. `compute_search_boost` with empty deprecated_ids → identical behavior to current (backward compat)
4. `compute_briefing_boost` also receives deprecated_ids and applies same filtering
5. Co-access pairs involving deprecated entries still stored (AC-09)

**Coverage Requirement**: All existing co-access tests updated to pass `deprecated_ids` parameter. New tests for filtering behavior.

### R-07: Explicit Status Filter + Supersession Injection Interaction
**Severity**: Med
**Likelihood**: Med
**Impact**: Agent requests `status: Deprecated` to see deprecated entries, but injection adds Active successors that weren't requested, polluting intentionally scoped results.

**Resolution**: Supersession injection is **disabled** when an explicit `status: "deprecated"` filter is set (FR-6.2). The agent has a reason for requesting deprecated content and can follow `superseded_by` references themselves.

**Test Scenarios**:
1. MCP search with explicit `status: Deprecated` → deprecated entries returned at full score, no penalty (AC-14)
2. MCP search with explicit `status: Deprecated` → supersession injection is disabled; no Active successors injected (AC-14b)
3. MCP search with no explicit status → flexible mode with penalties and injection active (AC-13)

**Coverage Requirement**: Explicit status filter interaction with injection and penalty pathways. Verify injection disable rule.

### R-08: Post-Compaction Supersession Injection Unreachable — RESOLVED
**Severity**: High → **Accepted** (not a new behavior)
**Likelihood**: High
**Original Impact**: After compaction prunes deprecated entries from HNSW, those entries never appear in HNSW top-k results. Supersession injection triggers only when a deprecated entry IS in results. Result: successor injection becomes dead code after compaction.

**Resolution**: This is not a new behavior introduced by crt-010. Since col-013, the background tick (`background.rs:234-257`) already runs compaction with entries filtered to `Status::Active` (`status.rs:175-181`). Deprecated entries are already excluded from HNSW rebuilds. The design tension (injection unreachable post-compaction) is the existing status quo. Pre-compaction, injection provides a recall boost. Post-compaction, stale entries are gone entirely — net positive.

**Test Scenarios**:
1. Before compaction: deprecated entry in HNSW top-k → successor injected (confirms injection works when entries present)
2. After compaction: verify Active successors remain directly findable via their own embeddings (verification of existing behavior)
3. Integration test: full cycle — insert entries, deprecate with supersession, compact, search — verify Active successor still appears in results via its own embedding

**Coverage Requirement**: Verification test confirming Active successors remain findable post-compaction. No new compaction logic required.

### R-09: Compaction/Search Race Condition
**Severity**: Med
**Likelihood**: Low
**Impact**: Search reads VECTOR_MAP for successor embedding while background tick compaction is rewriting VECTOR_MAP — could get stale or missing mapping.

**Test Scenarios**:
1. Concurrent search during background tick compaction → search completes without error
2. `get_embedding` returns None for entry mid-compaction → injection skipped gracefully

**Coverage Requirement**: Error handling test; concurrent stress test is nice-to-have.

### R-10: BriefingService Injection History Over-Filtering
**Severity**: Med
**Likelihood**: Med
**Impact**: BriefingService strips deprecated entries from injection history, but those entries may have been the basis for prior agent decisions. Context continuity lost.

**Test Scenarios**:
1. Briefing payload excludes deprecated entries (AC-11)
2. Briefing payload includes Active entries that superseded previously-injected deprecated entries
3. Empty injection history after filtering → briefing handles gracefully

**Coverage Requirement**: BriefingService integration test with mixed Active/Deprecated injection history.

### R-11: Denormalized Vectors in Cosine Similarity
**Severity**: High
**Likelihood**: Low
**Impact**: If any embedding is not L2-normalized (bug in embed pipeline or manual entry), cosine similarity produces values >1.0 or <-1.0, corrupting the re-rank ordering.

**Test Scenarios**:
1. Unit test: cosine_similarity with normalized vectors → value in [0,1]
2. Unit test: cosine_similarity with zero vector → returns 0.0 (not NaN/panic)
3. Unit test: cosine_similarity with mismatched dimensions → defined behavior (error or truncate)

**Coverage Requirement**: Defensive bounds checking in cosine_similarity helper.

### R-12: Default Flexible Mode Behavioral Change
**Severity**: Med
**Likelihood**: Med
**Impact**: Any existing caller that relied on unfiltered HNSW results now gets deprecated entries penalized. Subtle ranking changes with no explicit opt-in.

**Test Scenarios**:
1. SearchService with default params → Flexible mode applied (FR-1.2)
2. Existing MCP search with topic/category → behavior unchanged (already filtered Active)
3. Existing MCP search without filters → now applies penalties (behavior change, AC-13)

**Coverage Requirement**: Regression tests verifying existing search behavior is preserved where expected.

## Integration Risks

- **SearchService ↔ VectorIndex**: New `get_embedding` method must work with the HNSW graph's internal data storage. The hnsw_rs crate uses `DataId` internally; the `IdMap` bidirectional mapping must correctly translate entry IDs to data IDs for embedding retrieval.
- **SearchService ↔ Engine crate**: `deprecated_ids: &HashSet<u64>` crosses crate boundary. SearchService must collect deprecated IDs from its result set before calling co-access boost. Timing: deprecated IDs must be computed AFTER status filtering but BEFORE co-access boost.
- **UDS listener ↔ SearchService**: `RetrievalMode::Strict` must be threaded through `handle_context_search`. The `ServiceSearchParams` struct gains a new required field — all constructors must be updated.
- **MCP tools ↔ SearchService**: Explicit `status` parameter must suppress penalties (FR-6.2). The interaction between `QueryFilter.status` and `RetrievalMode` needs clear precedence rules.
- **Compaction ↔ Supersession Injection**: C6 is already satisfied by col-013 background tick — deprecated entries are already excluded from HNSW rebuilds. After compaction, supersession injection cannot fire for pruned entries, but this is existing behavior (R-08, resolved). Active successors remain findable via their own embeddings.

## Edge Cases

- **All entries deprecated**: Strict mode returns empty. Flexible mode returns all with penalties. Co-access has no boost sources.
- **Self-referential supersession**: Entry where `superseded_by == self.id`. Must not cause infinite loop.
- **Circular supersession**: A supersedes B, B supersedes A. Single-hop prevents infinite loop but both entries get penalized.
- **Successor with zero-vector embedding**: `get_embedding` returns Some but the vector is all zeros. Cosine similarity undefined (division by zero).
- **Concurrent deprecation during search**: Entry changes from Active to Deprecated between HNSW fetch and status filtering — stale status read.
- **Maximum supersession fan-in**: Multiple deprecated entries all superseded by the same Active entry. The successor should be injected once, not N times.
- **Empty query embedding**: If query embedding fails, entire search fails before status filtering is reached — existing error path.

## Security Risks

- **No new external input surfaces**: This feature changes internal pipeline behavior; no new MCP parameters or tools are added. Attack surface unchanged.
- **Penalty bypass via status parameter**: An agent can request `status: Deprecated` to bypass penalties. This is by design (FR-6.2) — the agent explicitly opts in. No escalation risk since deprecated entries contain outdated (not privileged) information.
- **Entry ID in `superseded_by`**: IDs are u64 set by internal operations (context_correct, context_deprecate). Not externally injectable. No injection or traversal risk.
- **Blast radius if SearchService is compromised**: SearchService is internal; if it serves wrong results, agents get incorrect knowledge silently (especially via UDS). The strict/flexible mode adds defense-in-depth by preferring no results over wrong results for UDS.

## Failure Modes

| Failure | Expected Behavior |
|---------|-------------------|
| `get_embedding` not implementable in hnsw_rs | Fall back to re-embedding via EmbedService. Higher latency but functionally correct. |
| Successor entry_store.get returns error | Skip injection for that entry. Log warning. Search continues (FR-2.7). |
| Co-access boost computation fails | Existing fallback: empty HashMap returned. Search continues without boost. |
| All HNSW results filtered in strict mode | Return empty result set. `total_tokens: 0`. No fallback (FR-1.5). |
| Compaction runs with all entries deprecated | Empty HNSW graph built. Valid state — search returns no results. |
| cosine_similarity receives zero-length or mismatched vectors | Return 0.0 similarity. Successor scores low but does not corrupt pipeline. |
| `deprecated_ids` HashSet construction fails (store read error) | Use empty HashSet — co-access boost applies normally. Log warning. |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (Successor similarity latency) | R-04 | ADR-002: cosine from stored embedding avoids re-embedding. Batch fetch (FR-2.2) limits store reads. Latency bounded by NFR-1.1 (15% p95 budget). |
| SR-02 (Arbitrary penalty values) | R-02 | ADR-005: named constants DEPRECATED_PENALTY=0.7, SUPERSEDED_PENALTY=0.5 in confidence.rs. Testable via ranking invariant ACs (AC-02, AC-03). |
| SR-03 (Vector pruning removes deprecated embeddings) | R-08 | C6 ALREADY SATISFIED (col-013 background tick). Post-compaction, successors findable via their own embeddings. R-08 resolved — not a new behavior. Verification test only. |
| SR-04 (UDS zero results) | R-03 | Architecture explicitly accepts empty results over wrong results. `total_tokens: 0` signals no injection. No fallback. |
| SR-05 (Single-hop transitive chain limit) | — | Accepted per non-goals. ADR-003 enforces single-hop consistently. |
| SR-06 (No schema changes verification) | R-12 | Confirmed: RetrievalMode is in-memory enum on ServiceSearchParams. No new tables or fields (NFR-3.1). |
| SR-07 (Cross-crate co-access dependency) | R-06 | ADR-004: `HashSet<u64>` interface keeps engine crate decoupled from server types. |
| SR-08 (Combinatorial test surface) | R-02, R-07, R-12 | Specification enumerates 17 acceptance criteria (AC-01 through AC-16 + AC-14b) covering mode × status × supersession × co-access matrix. |
| SR-09 (SearchService API backward compat) | R-12 | ADR-001: RetrievalMode defaults to Flexible, preserving current behavior for existing callers. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 3 scenarios |
| High | 4 (R-02, R-03, R-04, R-06) | 18 scenarios |
| Medium | 6 (R-05, R-07, R-09, R-10, R-11, R-12) | 17 scenarios |
| Accepted/Resolved | 1 (R-08) | 3 verification scenarios |
| **Total** | **12** | **41 scenarios** |
