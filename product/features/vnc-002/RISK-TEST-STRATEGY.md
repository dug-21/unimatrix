# Risk-Based Test Strategy: vnc-002

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Content scanning regex patterns produce false positives on legitimate knowledge entries | High | High | Critical |
| R-02 | Near-duplicate detection at 0.92 threshold misses true duplicates or blocks distinct entries | High | Medium | Critical |
| R-03 | Combined audit+data transaction fails mid-write, leaving entry without audit event or audit without entry | High | Medium | Critical |
| R-04 | Input validation allows control characters or oversized strings to reach storage | High | Medium | Critical |
| R-05 | Capability check bypass — agents access tools they lack permission for | High | Low | Critical |
| R-06 | EmbedNotReady state causes context_store to fail without clear guidance | Medium | High | High |
| R-07 | Output framing markers in stored content break the framing boundary | Medium | Low | Medium |
| R-08 | Category allowlist rejects valid categories or accepts invalid ones | Medium | Medium | High |
| R-09 | Format-selectable response produces invalid output or wrong default format | Medium | Medium | High |
| R-10 | context_search with metadata filters returns incorrect results (filter-search mismatch) | High | Medium | Critical |
| R-11 | i64 to u64 conversion for entry IDs allows negative IDs to reach the store | Medium | Medium | High |
| R-12 | Audit event monotonic IDs break when mixing combined and standalone transaction paths | High | Medium | Critical |
| R-13 | context_lookup default status filter omits expected entries or returns deprecated ones | Medium | Medium | High |
| R-14 | AuditLog::write_in_txn opens a nested transaction instead of reusing the caller's transaction | High | Low | High |
| R-15 | Content scanning OnceLock initialization fails under concurrent first-use | Low | Low | Low |
| R-16 | Existing vnc-001 tests regress due to new server state fields or error variants | High | Medium | Critical |

## Risk-to-Scenario Mapping

### R-01: Content Scanning False Positives

**Severity**: High
**Likelihood**: High
**Impact**: Agents cannot store legitimate knowledge entries that incidentally contain words matching injection patterns. Common developer documentation discusses prompt injection, role-based access, and system prompts — all of which could match patterns.

**Test Scenarios**:
1. Store an entry describing an error handling convention (no injection patterns). Verify scanning passes.
2. Store an entry containing "ignore previous instructions" in content. Verify scanning rejects with `InstructionOverride` category.
3. Store an entry containing "act as" in a non-injection context (e.g., "the service should act as a proxy"). Verify pattern specificity — the pattern should require more context than just "act as" alone.
4. Store an entry containing a code example with a system prompt reference in a code block. Verify the scanner handles code-embedded text.
5. Store an entry with an email address in content. Verify PII detection triggers.
6. Store an entry with an AWS-style access key (AKIA...). Verify API key detection triggers.
7. Store an entry with a phone number in XXX-XXX-XXXX format. Verify detection.
8. Verify scanning the same content twice returns the same result (deterministic).
9. Verify that title scanning only checks injection patterns, not PII patterns.
10. Store an entry with a GitHub token format (ghp_xxxxx). Verify detection.

**Coverage Requirement**: At least one positive and one negative test per pattern category. Pattern specificity tested to minimize false positives on developer documentation.

### R-02: Near-Duplicate Detection Accuracy

**Severity**: High
**Likelihood**: Medium
**Impact**: False negatives allow duplicate knowledge to accumulate, degrading search quality. False positives block agents from storing legitimately distinct entries.

**Test Scenarios**:
1. Store entry A, then attempt to store entry B with identical title+content. Verify duplicate detected with similarity >= 0.92.
2. Store entry A, then store entry C with completely different content. Verify no duplicate detected.
3. Store entry A, then store entry D with same topic but significantly different content. Verify no false positive.
4. Store entry A, then store entry E with slight rewording (same meaning). Verify duplicate detected if similarity >= 0.92.
5. Store entry into an empty vector index (no existing entries). Verify no duplicate (search returns empty).
6. Verify duplicate response contains: existing entry ID, content preview, similarity score, `"duplicate": true` flag.
7. Verify duplicate detection uses the same embedding that would be stored (title+content concatenation).
8. Test with the EmbedServiceHandle in Loading state. Verify EmbedNotReady error before duplicate check.

**Coverage Requirement**: Boundary conditions at 0.92 threshold. Empty index. Identical content. Distinctly different content. Near-similar content.

### R-03: Combined Transaction Failure

**Severity**: High
**Likelihood**: Medium
**Impact**: Entry exists without audit trail (compliance violation) or audit event references a nonexistent entry (dangling reference). Data integrity is compromised.

**Test Scenarios**:
1. Execute `insert_with_audit` with valid entry and audit event. Verify both entry and audit event exist in the database after commit.
2. Verify the audit event's `target_ids` contains the newly inserted entry's ID.
3. Verify the audit event's `event_id` is correctly incremented from the COUNTERS table.
4. Execute `insert_with_audit`, then a standalone `log_event` for a read operation. Verify audit IDs are sequential (no gaps).
5. Verify the write transaction commits atomically — if the database is read immediately after, both entry and audit are visible.
6. Verify vector mapping (entry_id -> hnsw_data_id) is written in the same transaction.
7. Test with a NewEntry that would cause the store to auto-compute content_hash. Verify the hash is correct after combined write.

**Coverage Requirement**: Atomicity verified. Sequential audit IDs verified across combined and standalone paths. All three writes (entry, mapping, audit) verified in same transaction.

### R-04: Input Validation Bypass

**Severity**: High
**Likelihood**: Medium
**Impact**: Oversized strings could cause memory issues. Control characters could corrupt display or storage. Invalid data reaches the database.

**Test Scenarios**:
1. Submit title with exactly 200 characters. Verify accepted.
2. Submit title with 201 characters. Verify rejected with `InvalidInput { field: "title", ... }`.
3. Submit content with exactly 50,000 characters. Verify accepted.
4. Submit content with 50,001 characters. Verify rejected.
5. Submit query with exactly 1,000 characters. Verify accepted.
6. Submit query with 1,001 characters. Verify rejected.
7. Submit topic containing NULL byte (U+0000). Verify rejected.
8. Submit content containing newline (U+000A). Verify accepted (content allows newlines).
9. Submit topic containing newline. Verify rejected (non-content fields reject newlines).
10. Submit tags with 20 items. Verify accepted. Submit with 21 items. Verify rejected.
11. Submit individual tag with 50 characters. Verify accepted. 51 characters. Verify rejected.
12. Submit id = -1. Verify rejected. Submit id = 0. Verify behavior (0 is sentinel — may be rejected).
13. Submit k = 0. Verify rejected. Submit k = 101. Verify rejected or clamped.
14. Submit limit = 0. Verify rejected.

**Coverage Requirement**: Every length limit tested at boundary (max, max+1). Every control character rule tested. ID validation tested for negative and zero.

### R-05: Capability Check Bypass

**Severity**: High
**Likelihood**: Low
**Impact**: Unrestricted agents write to the knowledge base, potentially injecting malicious content. Audit trail shows operations by unauthorized agents.

**Test Scenarios**:
1. Auto-enrolled Restricted agent calls context_store. Verify -32003 error with agent ID and "Write" capability.
2. Auto-enrolled Restricted agent calls context_search. Verify success (Search is allowed).
3. Auto-enrolled Restricted agent calls context_lookup. Verify success (Read is allowed).
4. Auto-enrolled Restricted agent calls context_get. Verify success (Read is allowed).
5. Privileged agent ("human") calls context_store. Verify success.
6. Verify capability check occurs BEFORE validation — a denied agent with invalid params gets -32003, not -32602.
7. Verify audit log records Outcome::Denied for capability denials.

**Coverage Requirement**: Each tool tested with both authorized and unauthorized agents. Execution order verified (capability before validation).

### R-06: EmbedNotReady Impact on context_store

**Severity**: Medium
**Likelihood**: High
**Impact**: Agents cannot store entries until embedding model loads. If the model takes 30+ seconds to download, early store attempts fail without clear guidance.

**Test Scenarios**:
1. Call context_store with embed handle in Loading state. Verify error -32004 with guidance.
2. Call context_store with embed handle in Failed state. Verify error includes failure reason.
3. Call context_store after embed handle transitions to Ready. Verify success.
4. Call context_search with embed handle in Loading state. Verify same -32004 error.
5. Call context_lookup (no embedding needed). Verify success regardless of embed state.
6. Call context_get (no embedding needed). Verify success regardless of embed state.

**Coverage Requirement**: All three embed states tested for embedding-dependent tools. Non-embedding tools verified to work in all states.

### R-07: Output Framing Boundary Break

**Severity**: Medium
**Likelihood**: Low
**Impact**: If stored content contains `[/KNOWLEDGE DATA]`, a consuming agent could interpret subsequent content as instructions rather than data.

**Test Scenarios**:
1. Store and retrieve an entry with content containing `[KNOWLEDGE DATA]`. Verify framing still works (markers on separate lines).
2. Store and retrieve an entry with content containing `[/KNOWLEDGE DATA]`. Verify the response structure.
3. Verify that metadata (title, topic, category) is OUTSIDE the framing markers.
4. Verify that summary and json format responses do NOT contain framing markers.
5. Test output framing for search results in markdown format (multiple entries) — each entry framed independently.

**Coverage Requirement**: Content containing markers tested. Metadata placement verified. Multi-entry framing verified.

### R-08: Category Allowlist Correctness

**Severity**: Medium
**Likelihood**: Medium
**Impact**: Valid categories rejected (agents cannot store entries). Invalid categories accepted (namespace pollution).

**Test Scenarios**:
1. Validate each of the 6 initial categories: outcome, lesson-learned, decision, convention, pattern, procedure. All must pass.
2. Validate "unknown" category. Must fail with -32007 listing all valid categories.
3. Validate "Convention" (uppercase C). Must fail (case-sensitive).
4. Add a new category at runtime via `add_category`. Validate the new category. Must pass.
5. Validate empty string category. Must fail.
6. Verify error message contains all valid categories in alphabetical order.

**Coverage Requirement**: All initial categories tested. Case sensitivity tested. Runtime extension tested. Empty input tested.

### R-09: Format-Selectable Response Validity

**Severity**: Medium
**Likelihood**: Medium
**Impact**: Invalid format output causes parsing failures. Wrong default format wastes context window.

**Test Scenarios**:
1. Call each tool with no `format` parameter, verify response uses summary format (compact, no full content).
2. Call each tool with `format: "summary"`, verify one compact line per entry.
3. Call each tool with `format: "markdown"`, verify full content with metadata header and output framing.
4. Call each tool with `format: "json"`, parse Content block as JSON, verify it deserializes to expected structure.
5. Call each tool with `format: "invalid"`, verify validation error listing valid options.
6. For search results in json format: verify array length matches result count, each entry has `similarity` field.
7. For store duplicate in each format: verify duplicate indicator present.
8. For empty results: summary/markdown have helpful message, json returns `[]`.
9. Verify `context_get` returns full content in all three formats (no summary truncation).

**Coverage Requirement**: All four tools tested. JSON validity verified. Content consistency between markdown and JSON verified. Special cases (empty, duplicate) tested.

### R-10: Search with Metadata Filter Mismatch

**Severity**: High
**Likelihood**: Medium
**Impact**: context_search returns entries that don't match metadata filters, or misses entries that should match. Agents receive incorrect knowledge.

**Test Scenarios**:
1. Store 5 entries in topic "rust" and 5 in topic "python". Search with query + topic="rust". Verify only "rust" entries returned.
2. Store entries in categories "convention" and "decision". Search with category="convention". Verify only "convention" entries.
3. Search with tags=["auth"]. Verify only entries tagged "auth" returned.
4. Search with topic + category filter. Verify intersection (both must match).
5. Search with metadata filters that match zero entries. Verify empty result (not an error).
6. Search WITHOUT metadata filters. Verify results come from all entries (unfiltered).

**Coverage Requirement**: Each filter type tested independently and in combination. Empty filter results tested. Unfiltered search tested.

### R-11: i64 to u64 Conversion

**Severity**: Medium
**Likelihood**: Medium
**Impact**: Negative IDs could be cast to large u64 values, potentially matching existing entries by accident.

**Test Scenarios**:
1. Call context_get with id = 1 (valid). Verify success or not-found (not crash).
2. Call context_get with id = -1. Verify InvalidInput error before any store access.
3. Call context_get with id = 0. Verify rejection or correct handling (0 is sentinel, first entry ID = 1).
4. Call context_lookup with id = -5. Verify rejection.
5. Call context_get with id = i64::MAX. Verify no overflow or panic.

**Coverage Requirement**: Negative, zero, and max boundary tested. Conversion occurs before store access.

### R-12: Audit ID Monotonicity Across Transaction Paths

**Severity**: High
**Likelihood**: Medium
**Impact**: Duplicate or out-of-order audit IDs break cross-session continuity and audit trail integrity.

**Test Scenarios**:
1. Perform: context_store (combined path) -> context_get (standalone path) -> context_store (combined path). Verify audit event IDs are 1, 2, 3 (strictly increasing, no gaps).
2. Perform 10 mixed operations (store + search + lookup + get). Verify all audit IDs are sequential.
3. Stop and restart the server. Perform operations. Verify audit IDs continue from where they left off.
4. Verify COUNTERS["next_audit_id"] is correctly updated in both combined and standalone paths.

**Coverage Requirement**: Interleaved combined/standalone paths tested. Sequential IDs verified. Cross-session continuity verified.

### R-13: Default Status Filter Behavior

**Severity**: Medium
**Likelihood**: Medium
**Impact**: context_lookup without explicit status returns deprecated entries (noise) or omits active entries (data loss).

**Test Scenarios**:
1. Store 3 active entries and 2 deprecated entries. Call context_lookup with no status parameter. Verify only active entries returned.
2. Call context_lookup with status="deprecated". Verify only deprecated entries returned.
3. Call context_lookup with status="active" explicitly. Verify same result as default.
4. Call context_lookup with status="proposed". Verify only proposed entries returned.
5. Call context_lookup with status="invalid". Verify InvalidInput error.
6. Call context_lookup with id=X (pointing to a deprecated entry) and no status. Verify the entry is returned (id lookup ignores status filter).

**Coverage Requirement**: Default filter behavior tested. All status values tested. ID-based lookup bypasses status filter.

### R-14: write_in_txn Transaction Isolation

**Severity**: High
**Likelihood**: Low
**Impact**: If write_in_txn opens its own transaction instead of using the caller's, the atomicity guarantee is broken.

**Test Scenarios**:
1. Open a write transaction, call write_in_txn, commit. Verify audit event is visible after commit.
2. Open a write transaction, call write_in_txn, DO NOT commit (drop transaction). Verify audit event is NOT visible (transaction was rolled back).
3. Verify write_in_txn uses the same COUNTERS table within the caller's transaction scope.

**Coverage Requirement**: Commit and rollback scenarios tested. Transaction isolation verified.

### R-15: OnceLock Concurrent Initialization

**Severity**: Low
**Likelihood**: Low
**Impact**: Two threads call ContentScanner::global() simultaneously, potentially causing a data race or double initialization.

**Test Scenarios**:
1. Call ContentScanner::global() from two threads concurrently. Verify both receive the same instance (same pointer address).
2. Verify the returned scanner has the expected number of patterns after concurrent init.

**Coverage Requirement**: Concurrent access tested. OnceLock guarantees verified.

### R-16: vnc-001 Test Regression

**Severity**: High
**Likelihood**: Medium
**Impact**: Adding new fields to UnimatrixServer or new variants to ServerError breaks existing test compilation or runtime behavior.

**Test Scenarios**:
1. Run full `cargo test -p unimatrix-server`. Verify all 72 existing tests pass.
2. Verify `make_server()` test helper in server.rs compiles with the new `categories` and `store` fields.
3. Verify error mapping tests still cover all existing error variants.
4. Verify audit log tests still pass with the new `write_in_txn` method added to AuditLog.

**Coverage Requirement**: All existing 72 tests pass unchanged or with minimal updates to test helpers (adding new required fields).

## Test Priority Order

**Critical (implement first — test-first):**
1. R-03: Combined transaction atomicity (foundational for context_store correctness)
2. R-05: Capability check enforcement (security gate)
3. R-04: Input validation boundary tests (security gate)
4. R-01: Content scanning false positive tuning (security gate)
5. R-12: Audit ID monotonicity across paths (data integrity)
6. R-16: vnc-001 regression (no regressions)

**High (implement second):**
7. R-02: Near-duplicate threshold accuracy
8. R-10: Search with metadata filter correctness
9. R-06: EmbedNotReady handling
10. R-11: i64/u64 conversion
11. R-13: Default status filter
12. R-14: write_in_txn transaction isolation
13. R-08: Category allowlist correctness
14. R-09: Format-selectable response validity

**Medium/Low (implement last):**
15. R-07: Output framing boundary
16. R-15: OnceLock concurrency

## Coverage Requirements Summary

| Area | Minimum Coverage |
|------|-----------------|
| Input validation | Every length limit at boundary (max, max+1). Every control character rule. Negative/zero ID. |
| Content scanning | At least 1 positive + 1 negative per pattern category. Title vs content scanning distinction. |
| Capability checks | All 4 tools with authorized + unauthorized agents. Order verification (before validation). |
| Near-duplicate | Identical content, distinct content, near-similar content, empty index. |
| Combined transaction | Atomicity, sequential audit IDs, cross-path interleaving. |
| Response format | JSON validity for all 4 tools. Content consistency. Special cases (empty, duplicate). |
| Category allowlist | All 6 initial categories + invalid + case sensitivity + runtime extension. |
| Metadata-filtered search | Each filter independently + combined + empty results + unfiltered. |
| Integration | All 72 vnc-001 tests pass. Full workspace `cargo test` green. |
