# Risk-Based Test Strategy: vnc-003

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Correction chain inconsistency: supersedes/superseded_by mismatch if transaction partially commits | High | Medium | Critical |
| R-02 | VECTOR_MAP orphan: entry exists without vector mapping after GH #14 fix regression | High | Medium | Critical |
| R-03 | Status counter desync: counters drift from actual entry counts during correction (two status changes in one txn) | High | Medium | Critical |
| R-04 | Deprecated entry correction: allowing corrections on deprecated entries creates invalid chains | Medium | Medium | High |
| R-05 | Content scanning bypass on context_correct: new content escapes scanning | High | Low | High |
| R-06 | Capability escalation: restricted agent accesses Admin-only context_status | High | Low | High |
| R-07 | Briefing token budget overflow: assembled content exceeds budget, consuming excessive agent context | Medium | Medium | High |
| R-08 | Embed not ready race in context_correct: embedding fails mid-operation after transaction started | Medium | Medium | High |
| R-09 | Category inheritance bypass: correction inherits a category that was removed from allowlist at runtime | Low | Low | Medium |
| R-10 | Status report torn read: concurrent writes during status computation produce inconsistent metrics | Medium | Low | Medium |
| R-11 | Deprecation idempotency audit gap: no record of redundant deprecation attempts | Low | Medium | Medium |
| R-12 | Briefing feature boost wrong ordering: feature-tagged entries not actually prioritized | Low | Medium | Medium |
| R-13 | allocate_data_id leak: data_id consumed but transaction rolls back, causing sparse HNSW ID space | Low | Low | Low |
| R-14 | insert_hnsw_only failure after commit: VECTOR_MAP present but HNSW entry missing until restart | Medium | Low | Medium |
| R-15 | context_status full scan performance: degradation at large entry counts | Low | Low | Low |

## Risk-to-Scenario Mapping

### R-01: Correction Chain Inconsistency (Critical)

**Severity**: High
**Likelihood**: Medium
**Impact**: If the original entry has superseded_by set but the correction entry does not exist (or vice versa), agents following the correction chain will encounter broken links. This corrupts the knowledge graph permanently.

**Test Scenarios**:
1. Verify that after `context_correct`, the original has `superseded_by = new_id` AND the correction has `supersedes = original_id`
2. Verify that the original's `correction_count` is incremented by exactly 1
3. Verify that the original's status is `Deprecated` and the correction's status is `Active`
4. Verify that both entries are readable via `context_get` after correction
5. Verify a chain of corrections: A -> B -> C (correct A to get B, then correct B to get C). Verify all links are consistent.
6. Verify that if the original_id does not exist, the entire operation fails and no partial state is written

**Coverage Requirement**: All 6 scenarios must pass. The chain-of-corrections test (scenario 5) validates multi-step consistency.

### R-02: VECTOR_MAP Orphan After GH #14 Fix (Critical)

**Severity**: High
**Likelihood**: Medium
**Impact**: If the VECTOR_MAP write is not in the combined transaction, an entry exists without a vector mapping. The entry is stored but not semantically discoverable via search.

**Test Scenarios**:
1. After `insert_with_audit`, verify VECTOR_MAP contains the entry_id -> data_id mapping
2. After `correct_with_audit`, verify VECTOR_MAP contains the new correction's mapping
3. Verify that `allocate_data_id()` returns strictly increasing values
4. Verify that `insert_hnsw_only` inserts into HNSW without writing VECTOR_MAP (the server already wrote it)
5. Verify that the existing `VectorIndex::insert()` method still works unchanged (backward compat)
6. Verify VECTOR_MAP mapping is present even if HNSW insert is skipped (simulated crash scenario)

**Coverage Requirement**: All 6 scenarios must pass. Scenario 6 validates the crash-safety improvement.

### R-03: Status Counter Desync During Correction (Critical)

**Severity**: High
**Likelihood**: Medium
**Impact**: `correct_with_audit` changes two entries' statuses in one transaction (original: Active->Deprecated, correction: new Active). If counters are not updated correctly, `total_active` and `total_deprecated` will drift from reality. `context_status` would report incorrect counts.

**Test Scenarios**:
1. Insert 5 entries. Correct one. Verify `total_active=5` (4 original + 1 correction), `total_deprecated=1`
2. Deprecate an entry, then verify counters. Then correct another entry and verify counters again (mixed operations).
3. Verify counter state after a chain: correct A->B, then correct B->C. `total_active` should be original_count - 2 + 1 (only C is active).
4. Verify that `context_status` reports match actual entry counts by scanning ENTRIES

**Coverage Requirement**: All 4 scenarios, plus scenario 4 serves as a cross-check between counters and actual data.

### R-04: Correcting a Deprecated Entry (High)

**Severity**: Medium
**Likelihood**: Medium
**Impact**: A deprecated entry was already marked as irrelevant. Correcting it creates a new active entry that supersedes something the system already decided to discard. This is semantically confusing and could resurface bad knowledge.

**Test Scenarios**:
1. Deprecate entry A, then attempt `context_correct(original_id=A)`. Verify error returned.
2. Verify the error does not leave any partial state (no new entry created, no fields modified on A)
3. Correct A->B, then attempt to correct A again (A is now deprecated via correction). Verify error.

**Coverage Requirement**: All 3 scenarios.

### R-05: Content Scanning Bypass on context_correct (High)

**Severity**: High
**Likelihood**: Low
**Impact**: If content scanning is not applied to correction content, an agent can inject malicious content by correcting an existing entry. The correction bypasses the scanning gate that context_store enforces.

**Test Scenarios**:
1. Attempt `context_correct` with content containing an injection pattern. Verify rejection.
2. Attempt `context_correct` with a title containing an injection pattern. Verify rejection.
3. Attempt `context_correct` with PII in content. Verify rejection.
4. Verify that valid correction content passes scanning (no false positive regression)

**Coverage Requirement**: All 4 scenarios. Scenarios 1-3 verify the scanning gate is active on the correction path.

### R-06: Capability Escalation on context_status (High)

**Severity**: High
**Likelihood**: Low
**Impact**: `context_status` exposes knowledge base internals (entry counts, trust_source distribution, attribution gaps). If a restricted agent can access it, they gain visibility into the system's security posture.

**Test Scenarios**:
1. Call `context_status` with a Restricted agent. Verify MCP error -32003.
2. Call `context_status` with a Privileged agent (has Admin). Verify success.
3. Verify all 4 v0.2 tools enforce their documented capability requirements:
   - context_correct: Write required
   - context_deprecate: Write required
   - context_status: Admin required
   - context_briefing: Read required

**Coverage Requirement**: All 3 scenarios.

### R-07: Briefing Token Budget Overflow (High)

**Severity**: Medium
**Likelihood**: Medium
**Impact**: If the budget enforcement fails, a briefing could return an excessively large response, consuming the agent's context window and reducing its effectiveness.

**Test Scenarios**:
1. Store 20 conventions and 20 duties for a role. Request a briefing with max_tokens=500. Verify output is within budget.
2. Verify that truncation removes from the relevant context section first, then duties, then conventions.
3. Verify the default budget (3000 tokens = ~12000 chars) is applied when max_tokens is omitted.
4. Verify min/max bounds on max_tokens (min 500, max 10000).

**Coverage Requirement**: All 4 scenarios.

### R-08: Embed Not Ready Race in context_correct (High)

**Severity**: Medium
**Likelihood**: Medium
**Impact**: `context_correct` requires embedding the new content. If the embed service is not ready, the operation must fail cleanly without leaving partial state (e.g., the original entry should NOT be deprecated if embedding fails).

**Test Scenarios**:
1. Call `context_correct` when embedding model is not ready. Verify EmbedNotReady error.
2. Verify the original entry is unchanged after the failed correction attempt.
3. After the embed model becomes ready, verify correction succeeds.

**Coverage Requirement**: All 3 scenarios. Scenario 2 validates no partial state.

### R-09: Category Inheritance Bypass (Medium)

**Severity**: Low
**Likelihood**: Low
**Impact**: If entry A has category "procedure" and the category is removed from the allowlist at runtime, correcting A without providing a new category would inherit "procedure" -- but "procedure" is still a valid initial category, so this is a non-issue for the initial set. However, if a runtime-added category is later conceptually "removed" (not supported), inheritance could carry forward an invalid category.

**Test Scenarios**:
1. Correct an entry, inheriting its category. Verify the inherited category is not re-validated.
2. Correct an entry with an explicit new category. Verify the new category IS validated.
3. Correct an entry with an explicit invalid category. Verify rejection.

**Coverage Requirement**: All 3 scenarios. This is defense-in-depth for a low-risk scenario.

### R-10: Status Report Torn Read (Medium)

**Severity**: Medium
**Likelihood**: Low
**Impact**: If status report reads happen across multiple transactions, a concurrent write between reads could produce inconsistent metrics (e.g., total_active + total_deprecated + total_proposed != total entries).

**Test Scenarios**:
1. Insert entries, then read status report. Verify that status counts sum to total entries.
2. Verify that category distribution counts sum to the expected total.
3. Verify the report uses a single read transaction (architectural test: check the code path).

**Coverage Requirement**: Scenarios 1-2 as data tests, scenario 3 as code review.

### R-11: Deprecation Idempotency Audit Gap (Medium)

**Severity**: Low
**Likelihood**: Medium
**Impact**: When deprecation is idempotent (no-op on already-deprecated), no audit event is logged. This means repeated deprecation attempts are invisible in the audit trail.

**Test Scenarios**:
1. Deprecate an entry. Verify audit event logged.
2. Deprecate the same entry again. Verify NO new audit event (idempotent no-op).
3. Verify the response indicates the entry was already deprecated.

**Coverage Requirement**: All 3 scenarios.

### R-12: Briefing Feature Boost Wrong Ordering (Medium)

**Severity**: Low
**Likelihood**: Medium
**Impact**: If feature boost is not applied correctly, agents may receive less relevant context when working on a specific feature.

**Test Scenarios**:
1. Store 3 entries: one tagged with "vnc-003", two without. Request briefing with feature="vnc-003". Verify the tagged entry appears first in relevant context.
2. Request briefing without feature parameter. Verify results are ordered by similarity only.
3. Request briefing with feature that no entries are tagged with. Verify results are unchanged (no filtering).

**Coverage Requirement**: All 3 scenarios.

### R-14: insert_hnsw_only Failure After Commit (Medium)

**Severity**: Medium
**Likelihood**: Low
**Impact**: If `insert_hnsw_only` fails (e.g., invalid embedding dimension) after the combined transaction has already committed, the VECTOR_MAP entry exists but the HNSW index does not contain the point. The entry would not appear in search results until server restart (when HNSW is rebuilt from VECTOR_MAP).

**Test Scenarios**:
1. Verify that `insert_hnsw_only` validates embedding dimension before HNSW insert.
2. Verify that if HNSW insert fails, the VECTOR_MAP mapping still exists (committed earlier).
3. Verify that after a successful `insert_hnsw_only`, the entry appears in search results immediately.

**Coverage Requirement**: All 3 scenarios.

## Integration Risks

1. **UnimatrixServer gains vector_index field**: The server constructor signature changes, affecting all test `make_server()` calls and the main binary initialization. Must update all call sites.

2. **Category allowlist count change**: Tests that assert `valid_categories.len() == 6` must be updated to `8`. This affects `test_validate_unknown_rejected` and `test_list_categories_sorted` in categories.rs.

3. **insert_with_audit refactor**: The GH #14 fix changes the internal logic of `insert_with_audit`, which is used by `context_store`. All existing `context_store` tests must still pass unchanged.

4. **VectorIndex API addition**: `allocate_data_id()` and `insert_hnsw_only()` are new public methods on `VectorIndex`. The existing `VectorIndex::insert()` must remain unchanged and still work for non-server callers.

5. **Audit ID continuity**: v0.2 tools sharing the COUNTERS["next_audit_id"] counter with v0.1 tools. Mixed operation sequences must produce strictly monotonic IDs.

## Edge Cases

1. **Correct entry with same content**: Correcting an entry with identical content is allowed (the reason may be metadata changes). Near-duplicate detection does not apply to corrections.
2. **Deprecate then undeprecate**: There is no "undeprecate" operation. A deprecated entry stays deprecated. To restore knowledge, create a new entry.
3. **context_status on empty database**: All counts should be 0, distributions empty. Verify graceful handling.
4. **context_briefing with no matching entries**: When no conventions/duties exist for the role, those sections are empty. When no search results, relevant context is empty. Briefing still returns with empty sections.
5. **context_briefing with max_tokens=500**: Minimal budget. Conventions alone may exceed this. Verify truncation works correctly.
6. **context_correct with all optional overrides**: Topic, category, tags, title all provided. None inherited. Verify all overrides applied.
7. **context_correct inheriting all fields**: No optional params provided. All metadata inherited from original. Verify exact inheritance.
8. **Concurrent corrections on same entry**: Two agents correct the same entry simultaneously. The second correction should fail because the first deprecates the original. The redb write serialization ensures one transaction completes first.

## Security Risks

### Untrusted Input Assessment

| Component | Untrusted Input | Damage Potential | Blast Radius |
|-----------|----------------|-----------------|-------------|
| context_correct | `content`, `title`, `reason`, `topic`, `category`, `tags` | Injection via correction content, PII leak | Entry propagates to all future retrievers |
| context_deprecate | `id`, `reason` | Malicious deprecation of valid knowledge | Knowledge silenced, agents lose access |
| context_status | `topic`, `category` | Information disclosure (knowledge base internals) | Admin-gated, limited blast radius |
| context_briefing | `role`, `task`, `feature` | Injection via crafted role/task strings | Brief consumed by requesting agent only |

### Mitigation

- **context_correct**: Content scanning (same as context_store) + category validation + input length limits. The correction content is the highest-risk input -- it becomes a new active entry.
- **context_deprecate**: Capability check (Write required). The `reason` field is stored in the audit log only, not in the entry itself. Input validation on reason length.
- **context_status**: Capability check (Admin required). No write operations. Filter params are validated for length/control chars.
- **context_briefing**: Capability check (Read required). The `role` and `task` params are used as query inputs, not stored. Input validation on length. The `task` is embedded, which passes through the embedding model -- no execution risk.

### Path Traversal / Deserialization

No file system paths are involved. All data flows through redb tables with type-safe keys. Bincode deserialization is used for audit events (established pattern). No new deserialization attack surface.

## Failure Modes

1. **Embed model not ready during context_correct**: Return `ServerError::EmbedNotReady` (MCP -32004). No state changes. Agent can retry.
2. **Embed model not ready during context_briefing**: Graceful degradation -- return lookup-only briefing with indicator. Agent gets partial orientation.
3. **Entry not found during correction/deprecation**: Return `ServerError::Core(EntryNotFound)` (MCP -32001). No state changes.
4. **Content scan hit during correction**: Return `ServerError::ContentScanRejected` (MCP -32006). No state changes.
5. **Write transaction failure (redb I/O)**: Return `ServerError::Core(StoreError)` (MCP -32603). Transaction rolled back, no state changes.
6. **HNSW insert failure after commit**: VECTOR_MAP mapping exists but entry is not searchable until restart. Log warning. Entry is still accessible via context_get/context_lookup.
7. **Admin capability denied on context_status**: Return MCP error -32003. No data exposed.

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-03) | 16 scenarios |
| High | 5 (R-04, R-05, R-06, R-07, R-08) | 17 scenarios |
| Medium | 4 (R-09, R-10, R-11, R-12, R-14) | 14 scenarios |
| Low | 2 (R-13, R-15) | noted, not mandatory |
| **Total** | **14** | **47+ scenarios** |
