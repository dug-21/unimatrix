# Risk-Based Test Strategy: vnc-006

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | SearchService produces different result ordering than existing inline paths | High | Med | High |
| R-02 | StoreService atomic transaction fails to commit, leaving inconsistent state | High | Low | Med |
| R-03 | SecurityGateway S1 false positives on legitimate search queries | Med | Med | Med |
| R-04 | AuditSource::Internal abused by non-system code to bypass content scanning | High | Low | Med |
| R-05 | ConfidenceService batching changes timing semantics, causing test flakiness | Low | Med | Low |
| R-06 | Existing tests break due to call graph changes (services interposed) | Med | High | High |
| R-07 | UDS audit writes (S5) block the fire-and-forget response path | High | Low | Med |
| R-08 | Store::insert_in_txn diverges from Store::insert behavior | Med | Med | Med |
| R-09 | SecurityGateway::new_permissive leaks into production code | Med | Low | Low |
| R-10 | SearchService embedding reuse (query_embedding in results) exposes internal state | Low | Low | Low |
| R-11 | ServiceError conversion loses error context when mapping to transport errors | Med | Med | Med |
| R-12 | Quarantine exclusion (S4) inconsistency between SearchService and direct Store lookups | Med | Low | Low |

## Risk-to-Scenario Mapping

### R-01: SearchService Result Ordering Divergence
**Severity**: High
**Likelihood**: Med
**Impact**: Agents receive different search results after refactoring, potentially changing knowledge delivery quality. Floating-point re-ranking with provenance/co-access/feature boosts could reorder results if computation sequence changes.

**Test Scenarios**:
1. Seed a test store with 10+ entries spanning multiple topics. Run the same query through the old inline path and SearchService. Compare result IDs, ordering, and scores to 6 decimal places.
2. Test with co-access anchors, feature tags, and provenance (self-authored entries) all active simultaneously — the combination is most sensitive to ordering.
3. Test with similarity_floor and confidence_floor set — verify same entries are filtered.

**Coverage Requirement**: Comparison test with fixed seed data covering all boost types. Must pass with exact score match (not approximate).

### R-02: StoreService Atomic Transaction Failure
**Severity**: High
**Likelihood**: Low
**Impact**: Entry inserted but audit record missing (partial transaction), or audit written but entry missing. Inconsistent state between ENTRIES table and AUDIT_LOG.

**Test Scenarios**:
1. Insert an entry via StoreService; verify both entry and audit record exist in the same read transaction.
2. Simulate `insert_in_txn` failure (e.g., invalid entry data) — verify no entry and no audit written.
3. Verify existing `Store::insert()` still works independently (regression).

**Coverage Requirement**: Integration test with real Store instance. Verify atomicity by reading both tables after insert/failure.

### R-03: SecurityGateway S1 False Positives on Search Queries
**Severity**: Med
**Likelihood**: Med
**Impact**: Legitimate search queries about injection patterns (e.g., "how do we handle prompt injection?") trigger ScanWarning. While warnings don't block searches, excessive warnings pollute audit logs and could confuse monitoring.

**Test Scenarios**:
1. Search for "prompt injection detection patterns" — should produce ScanWarning but return results.
2. Search for common developer queries (e.g., "error handling", "authentication") — should not produce ScanWarning.
3. Search with actual injection attempt ("ignore previous instructions") — should produce ScanWarning and still return results.

**Coverage Requirement**: Unit test for ScanWarning behavior. Verify warnings are informational, not blocking.

### R-04: AuditSource::Internal Bypass Abuse
**Severity**: High
**Likelihood**: Low
**Impact**: If code outside the intended internal write paths constructs AuditSource::Internal, it bypasses S1 content scanning. Malicious or buggy content could enter the knowledge base without scanning.

**Test Scenarios**:
1. Verify `AuditSource::Internal` is `pub(crate)` — cannot be constructed from outside the crate.
2. Grep codebase for all `AuditSource::Internal` construction sites — verify each is in a legitimate internal write path.
3. Verify `validate_write` with Internal source skips scan but applies S3 validation.

**Coverage Requirement**: Code inspection + unit test verifying S3 applies even with Internal source.

### R-05: ConfidenceService Batching Timing Change
**Severity**: Low
**Likelihood**: Med
**Impact**: Previously 8 independent tasks could run concurrently; now a batch is sequential. If tests depend on confidence being computed before a subsequent operation (despite fire-and-forget contract), they could become flaky.

**Test Scenarios**:
1. Call `recompute(&[id1, id2, id3])` — verify all three entries get updated confidence.
2. Call `recompute(&[])` — verify no spawn_blocking call (no-op).
3. Call `recompute(&[nonexistent_id])` — verify warn log, no panic.

**Coverage Requirement**: Unit test for batch semantics. Integration test verifying confidence update after store+recompute.

### R-06: Existing Test Breakage from Call Graph Changes
**Severity**: Med
**Likelihood**: High
**Impact**: ~680 existing tests may reference inline search/write logic that has moved to services. Tests that mock at the transport level continue to work; tests that mock internal functions may break.

**Test Scenarios**:
1. Run full test suite after refactoring — all existing tests pass (zero regressions).
2. Any test that directly calls the old inline search function is migrated to call SearchService.
3. New tests for services are added alongside existing transport-level tests.

**Coverage Requirement**: CI gate: all existing tests pass. No test deletions allowed — tests move with their code.

### R-07: UDS Audit Writes Blocking Fire-and-Forget Path
**Severity**: High
**Likelihood**: Low
**Impact**: If S5 audit emission blocks (e.g., AuditLog write contention), UDS response latency increases, potentially causing hook timeouts. Hook callers expect sub-100ms responses.

**Test Scenarios**:
1. Verify `gateway.emit_audit()` uses fire-and-forget pattern (returns immediately).
2. Measure UDS search latency before and after: must remain within 10% of baseline.
3. Verify audit log write failure does not propagate to service result.

**Coverage Requirement**: Unit test verifying `emit_audit` is non-blocking. Performance benchmark (optional).

### R-08: Store::insert_in_txn Divergence from Store::insert
**Severity**: Med
**Likelihood**: Med
**Impact**: `insert_in_txn` may miss an index write or counter increment that `insert` performs, leading to data inconsistency (e.g., entry exists but missing from TOPIC_INDEX).

**Test Scenarios**:
1. Insert via `insert_in_txn` then verify all 7 index tables (ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX, FEATURE_ENTRIES) are populated.
2. Compare entry inserted via `insert()` vs `insert_in_txn()` — all fields and indexes identical.
3. Verify counter increments (next_entry_id, status counters) match between both paths.

**Coverage Requirement**: Integration test in unimatrix-store comparing both insert paths.

### R-11: ServiceError Context Loss
**Severity**: Med
**Likelihood**: Med
**Impact**: When `ServiceError::Core(CoreError)` is converted to rmcp `ErrorData` or `HookResponse::Error`, error details may be lost. Debugging becomes harder if transport error messages are less informative.

**Test Scenarios**:
1. Trigger each ServiceError variant and verify the transport-layer error message preserves the key detail.
2. Verify `ServiceError::ContentRejected` includes the pattern category and description in both MCP and UDS error formats.

**Coverage Requirement**: Unit test for each ServiceError-to-transport-error conversion.

## Integration Risks

| Risk ID | Risk | Test Scenario |
|---------|------|---------------|
| IR-01 | SearchService and BriefingService (vnc-007) both need embedding — SearchResults.query_embedding must be compatible with future BriefingService | Verify query_embedding is standard f32 vector, same format as embed_entry output |
| IR-02 | StoreService writes audit via AuditLog but tools.rs also writes audit independently — double audit entries possible | Verify tools.rs removes its own audit write for operations delegated to StoreService |
| IR-03 | ConfidenceService called from both tools.rs and uds_listener.rs — both must pass correct entry IDs | Verify each call site passes the right IDs (new entry, deprecated original, etc.) |
| IR-04 | UnimatrixServer constructor grows with ServiceLayer::new() — all required Arc references must be available at construction | Integration test: server construction succeeds with all dependencies |

## Edge Cases

| Case | Expected Behavior | Test |
|------|-------------------|------|
| Empty search query ("") | S3 validation passes (empty is valid), search returns empty results | Unit test |
| Query exactly at 10,000 char limit | S3 passes | Boundary test |
| Query at 10,001 chars | S3 rejects with ValidationFailed | Boundary test |
| k=0 | S3 rejects (range 1-100) | Boundary test |
| k=100 | S3 passes | Boundary test |
| k=101 | S3 rejects | Boundary test |
| Store with zero entries | SearchService returns empty results, no error | Integration test |
| Embedding service unavailable | ServiceError::EmbeddingFailed | Unit test |
| All search results are quarantined | Empty results returned | Integration test |
| insert_in_txn with entry that has no tags | TAG_INDEX write skipped, all other indexes written | Integration test |
| Confidence recompute on deleted entry | Warn log, skip, continue batch | Unit test |
| AuditContext with None session_id | Audit event written with empty session field | Unit test |
| Concurrent SearchService calls | No data races (all state is Arc-wrapped, read-only except fire-and-forget) | Concurrent integration test |

## Security Risks

### Search Queries (FR-01, SearchService)

- **Untrusted input**: Query string from user prompts (via UDS hooks) or agent queries (via MCP)
- **Damage potential**: Injection patterns in queries are embedded, not executed. Primary risk is detection evasion and adversarial embedding manipulation. Low blast radius — worst case is irrelevant search results.
- **Mitigation**: S1 warn-mode scanning (detection signal), S3 length/char validation (bounds enforcement). No hard-reject — false positive risk outweighs injection risk for read operations.

### Write Operations (FR-02, StoreService)

- **Untrusted input**: Title, content, category, tags from agents (MCP) or system (Internal)
- **Damage potential**: Knowledge poisoning — malicious content enters the KB, served to future agents. High blast radius — all agents consuming the KB are affected.
- **Mitigation**: S1 hard-reject scanning (25+ injection patterns, PII detection), S3 structural validation, capability check (Write required), AuditSource::Internal bypasses S1 only for system-generated content.

### AuditSource Forgery

- **Untrusted input**: Transport constructs AuditContext — MCP agent_id is self-asserted, UDS uid is kernel-verified
- **Damage potential**: False audit trail. Low blast radius — audit is for forensics, not access control.
- **Mitigation**: AuditContext is append-only with monotonic IDs. Inconsistencies (e.g., Mcp source from UDS socket) are detectable in forensic analysis. `pub(crate)` on Internal prevents external forgery.

## Failure Modes

| Failure | Expected Behavior | Recovery |
|---------|-------------------|----------|
| Embedding service unavailable during search | Return ServiceError::EmbeddingFailed, transport returns error to caller | Retry on next request (embed service lazy-loads) |
| Store read failure during search | Return ServiceError::Core, transport returns error | Retry on next request |
| Store write failure during insert | Transaction rolls back atomically — no partial state | Retry on next request |
| Audit write failure | Fire-and-forget — log warning, service continues | No recovery needed — audit is best-effort |
| Confidence recompute failure | Log warning per entry, skip — batch continues | Confidence updated on next access |
| ContentScanner initialization failure | OnceLock panics (fatal) | Server restart required — should not happen in practice |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 | R-04, R-09 | pub(crate) on Internal and new_permissive; code inspection gate |
| SR-02 | R-08 | insert_in_txn is pub(crate), WriteTransaction not in public API |
| SR-03 | R-05 | ConfidenceService::new_permissive for test setup |
| SR-04 | R-01 | Comparison test harness for search results |
| SR-05 | R-01 | Snapshot tests with fixed seed data, exact score comparison |
| SR-06 | — | Deferred to vnc-009 — interface only, no enforcement |
| SR-07 | R-07 | emit_audit is fire-and-forget, never blocks |
| SR-08 | — | OnceLock::get_or_init is thread-safe by design; concurrency test added |
| SR-09 | R-06 | CI gate: all existing tests pass, no deletions |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 3 (R-01, R-06, R-07) | 8 scenarios |
| Medium | 5 (R-02, R-03, R-04, R-08, R-11) | 12 scenarios |
| Low | 4 (R-05, R-09, R-10, R-12) | 6 scenarios |
| **Total** | **12** | **26 scenarios** |
