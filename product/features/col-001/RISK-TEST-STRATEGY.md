# Risk-Based Test Strategy: col-001

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | OUTCOME_INDEX insert fails within write transaction, rolling back the entire entry creation | High | Low | Med |
| R-02 | Structured tag validation rejects valid agent input due to overly strict parsing | High | Med | High |
| R-03 | Non-outcome entries inadvertently trigger outcome tag validation | High | Low | Med |
| R-04 | StoreParams backward incompatibility — existing JSON without feature_cycle fails to deserialize | High | Low | Med |
| R-05 | OUTCOME_INDEX not populated for outcome entries with non-empty feature_cycle (silent data loss) | High | Med | High |
| R-06 | Outcome statistics in context_status are incorrect or inconsistent with actual data | Med | Med | Med |
| R-07 | Store::open fails or existing databases fail to open after adding 13th table | High | Low | Med |
| R-08 | Tags with colon in non-outcome entries are rejected or altered | High | Med | High |
| R-09 | Missing `type` tag error message is unclear, leading to agent confusion | Low | Med | Low |
| R-10 | Outcome entries without feature_cycle silently lack index linkage without user awareness | Med | Med | Med |
| R-11 | OUTCOME_INDEX scan in context_status is expensive at scale | Low | Low | Low |
| R-12 | Concurrent outcome stores create inconsistent OUTCOME_INDEX state | Med | Low | Low |

## Risk-to-Scenario Mapping

### R-01: Transaction Rollback on OUTCOME_INDEX Failure
**Severity**: High
**Likelihood**: Low
**Impact**: Outcome entry is not created despite valid input. Agent receives error instead of success.

**Test Scenarios**:
1. Store an outcome entry with valid tags and feature_cycle — verify both ENTRIES and OUTCOME_INDEX contain the entry after commit.
2. Verify that when OUTCOME_INDEX insert is part of the transaction, a read immediately after commit shows consistent state.

**Coverage Requirement**: Integration test confirming atomicity of entry + OUTCOME_INDEX within insert_with_audit.

### R-02: Overly Strict Tag Validation
**Severity**: High
**Likelihood**: Med
**Impact**: Agents cannot store valid outcomes because validation is too restrictive.

**Test Scenarios**:
1. Store outcome with all recognized keys (type, gate, phase, result, agent, wave) — all accepted.
2. Store outcome with `gate:3a`, `gate:custom-gate`, `gate:1b` — all accepted (open string).
3. Store outcome with `agent:col-001-agent-1-architect` — accepted (any non-empty string).
4. Store outcome with `wave:0`, `wave:2`, `wave:99` — accepted (non-negative integer string).
5. Store outcome with mixed plain and structured tags — plain tags pass through.
6. Store outcome with `type:feature` plus additional unrecognized plain tag like `important` — accepted.

**Coverage Requirement**: Unit tests for every recognized key with valid values. Unit tests for edge case values (empty value, special characters in value portion).

### R-03: Non-Outcome Validation Leakage
**Severity**: High
**Likelihood**: Low
**Impact**: Entries with other categories that happen to use colon-format tags are incorrectly rejected.

**Test Scenarios**:
1. Store `category: "convention"` with tag `scope:global` — accepted without validation.
2. Store `category: "decision"` with tag `severity:high` — accepted without validation.
3. Store `category: "pattern"` with tag `foo:bar` — accepted without validation.
4. Verify `validate_outcome_tags` is ONLY called when `category == "outcome"`.

**Coverage Requirement**: Integration test for each non-outcome category with colon tags.

### R-04: StoreParams Backward Incompatibility
**Severity**: High
**Likelihood**: Low
**Impact**: Existing agents that call context_store without the new feature_cycle field get errors.

**Test Scenarios**:
1. Deserialize StoreParams JSON without `feature_cycle` field — succeeds, field is None.
2. Deserialize StoreParams JSON with `feature_cycle: null` — succeeds, field is None.
3. Deserialize StoreParams JSON with `feature_cycle: "col-001"` — succeeds, field is Some.
4. Full context_store call without feature_cycle — entry stored with empty feature_cycle.

**Coverage Requirement**: Unit test for StoreParams deserialization. Integration test for backward-compatible store call.

### R-05: OUTCOME_INDEX Population Gap
**Severity**: High
**Likelihood**: Med
**Impact**: Outcome entries exist but are not findable via OUTCOME_INDEX, breaking col-002 aggregation.

**Test Scenarios**:
1. Store outcome with `feature_cycle: "col-001"` — verify OUTCOME_INDEX contains ("col-001", entry_id).
2. Store multiple outcomes for same feature_cycle — verify all are indexed.
3. Store outcome with feature_cycle then query OUTCOME_INDEX by prefix scan — returns all entries.
4. Verify OUTCOME_INDEX is populated in the same transaction as entry creation (not fire-and-forget).

**Coverage Requirement**: Integration test covering store + OUTCOME_INDEX read within same test.

### R-06: Incorrect Outcome Statistics
**Severity**: Med
**Likelihood**: Med
**Impact**: context_status reports wrong outcome counts, misleading col-002 analysis.

**Test Scenarios**:
1. Store 3 feature outcomes and 2 bugfix outcomes — outcomes_by_type shows correct counts.
2. Store outcomes with pass, fail, rework results — outcomes_by_result shows correct counts.
3. Store outcomes across 2 feature cycles — outcomes_by_feature_cycle shows correct counts.
4. Store non-outcome entries — total_outcomes excludes them.
5. Empty database — outcome fields are zero/empty, not error.

**Coverage Requirement**: Integration test with known outcome data, verifying all 4 StatusReport outcome fields.

### R-07: Store::open Failure with 13th Table
**Severity**: High
**Likelihood**: Low
**Impact**: Database cannot be opened. Complete system failure.

**Test Scenarios**:
1. Open a fresh database — 13 tables created successfully.
2. Open an existing database (created before col-001 with 12 tables) — 13th table created on open, no error.
3. Verify all 13 tables are accessible after open.

**Coverage Requirement**: Unit test in db.rs confirming 13-table open.

### R-08: Colon Tags on Non-Outcome Entries
**Severity**: High
**Likelihood**: Med
**Impact**: Existing agent patterns that use colon-format tags for non-outcome entries break.

**Test Scenarios**:
1. Store `category: "convention"` with tags `["scope:global", "priority:high"]` — stored successfully.
2. Store `category: "decision"` with tags `["foo:bar:baz"]` (multiple colons) — stored successfully.
3. Verify TAG_INDEX contains the exact colon-format tag strings.

**Coverage Requirement**: Integration test confirming non-outcome colon tags are stored verbatim.

### R-09: Unclear Error Messages
**Severity**: Low
**Likelihood**: Med
**Impact**: Agent retries with same bad input, wasting cycles.

**Test Scenarios**:
1. Missing type tag — error message explicitly says "type tag is required for outcome entries".
2. Unknown key — error message lists recognized keys.
3. Invalid type value — error message lists valid type values.

**Coverage Requirement**: Unit tests asserting error message content.

### R-10: Orphan Outcome Awareness
**Severity**: Med
**Likelihood**: Med
**Impact**: Agent doesn't realize their outcome is not linked to a workflow.

**Test Scenarios**:
1. Store outcome without feature_cycle — response includes a warning about missing workflow linkage.
2. Store outcome with empty string feature_cycle — same warning.
3. Store outcome WITH feature_cycle — no warning.

**Coverage Requirement**: Integration test checking response content for warning presence/absence.

### R-11: Status Scan Performance
**Severity**: Low
**Likelihood**: Low
**Impact**: Status report becomes slow with many outcomes.

**Test Scenarios**:
1. At expected scale (100 outcome entries), status report completes within 100ms.

**Coverage Requirement**: Not a unit test concern at current scale. Monitor if outcome count grows beyond 10K.

### R-12: Concurrent Outcome Stores
**Severity**: Med
**Likelihood**: Low
**Impact**: Race condition in OUTCOME_INDEX updates.

**Test Scenarios**:
1. redb provides serializable isolation per write transaction. Two concurrent outcome stores for the same feature_cycle should both succeed and both be indexed.

**Coverage Requirement**: Covered by redb's transactional guarantees. No custom test needed.

## Integration Risks

1. **insert_with_audit extension**: Adding OUTCOME_INDEX insert to the existing write transaction in `server.rs` must not break the existing ENTRIES + indexes + AUDIT_LOG + VECTOR_MAP commit sequence. Test: store a non-outcome entry and verify all existing indexes are populated correctly (regression test).

2. **StatusReport field addition**: Adding 4 fields to `StatusReport` changes its construction. All existing tests that construct `StatusReport` must be updated. Test: compile check + existing status tests pass.

3. **StoreParams schema evolution**: Adding `feature_cycle` to the MCP tool schema changes what agents see in tool descriptions. Agents using schema-aware MCP clients should handle the new optional field gracefully.

4. **CATEGORY_INDEX + TAG_INDEX intersection for outcome stats**: The status computation must correctly intersect category "outcome" with structured tag values. An off-by-one in prefix scanning would produce wrong counts.

## Edge Cases

1. **Empty tag value**: `type:` (key present, value empty) — should be rejected as invalid type value.
2. **Multiple colons**: `agent:col-001:agent:1` — split on first `:` only, value is `col-001:agent:1` — valid agent ID.
3. **Duplicate tags**: `["type:feature", "type:bugfix"]` — two type tags. Validation should reject (or accept last, TBD — recommend reject with clear error).
4. **Case sensitivity**: `Type:Feature` vs `type:feature` — keys and values should be case-sensitive (lowercase only).
5. **Whitespace**: `type: feature` (space after colon) — value is ` feature` with leading space. Should be rejected as invalid type value.
6. **Very long feature_cycle**: 1000+ character string — should pass input validation max length checks but work correctly.
7. **Unicode in tags**: `gate:\u{6d4b}\u{8bd5}` — valid gate value (any non-empty string).

## Security Risks

### Tag Injection
**Untrusted input**: Tag values come from MCP callers (agents).
**Damage potential**: A malicious tag value could contain control characters or SQL-like injection patterns. However, tags are stored as opaque strings in redb — no interpretation beyond string matching. Content scanning already validates entry content but not tags.
**Blast radius**: Limited. Tags are only used for exact-match lookup (TAG_INDEX) and display. No tag value is interpreted as code.
**Mitigation**: Input validation (existing `validate_store_params`) enforces no control characters in tag strings. Additional: validate tag string length.

### feature_cycle Injection
**Untrusted input**: feature_cycle value from StoreParams.
**Damage potential**: A crafted feature_cycle could pollute OUTCOME_INDEX with misleading keys. Example: `feature_cycle: "col-001\x00injected"` could create confusing index entries.
**Blast radius**: OUTCOME_INDEX only. Does not affect other tables or entry content.
**Mitigation**: Input validation: feature_cycle must match `[a-zA-Z0-9._-]+` pattern (alphanumeric, dots, dashes, underscores). Max length 128 characters.

## Failure Modes

| Failure | Expected Behavior |
|---------|-------------------|
| Unknown tag key on outcome | Error returned, entry NOT stored. Error lists recognized keys. |
| Missing type tag on outcome | Error returned, entry NOT stored. Error names the missing tag. |
| Invalid type/result value | Error returned, entry NOT stored. Error lists valid values. |
| OUTCOME_INDEX insert fails | Entire transaction rolls back. Entry NOT stored. Error returned. |
| Outcome stats scan fails | context_status returns error. No partial report. |
| Empty feature_cycle on outcome | Entry stored. OUTCOME_INDEX not populated. Warning in response. |
| Non-outcome with colon tags | Entry stored normally. No validation applied. |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 | R-07 | 13th table follows exact same Store::open pattern as existing 12. Tested explicitly. |
| SR-02 | R-03 | outcome_tags module isolated. Conditional call only for category "outcome" (ADR-001). |
| SR-03 | R-04 | StoreParams uses Option<String> with serde default. Deserialization tested. |
| SR-04 | R-10 | Warning in response when outcome lacks feature_cycle. Documented as non-goal. |
| SR-05 | — | ADR-003 designs extensible per-category validation. Accepted as-is for col-001. |
| SR-06 | R-03, R-08 | Validation ONLY fires for category "outcome". Integration tests confirm non-outcome isolation. |
| SR-07 | R-11 | At expected scale, negligible. Monitor if growth exceeds assumptions. |
| SR-08 | R-01 | OUTCOME_INDEX insert is inline in the write transaction. Atomic with entry creation. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 5 (R-02, R-03, R-05, R-08, R-04) | 18 scenarios |
| Medium | 4 (R-01, R-06, R-07, R-10) | 14 scenarios |
| Low | 3 (R-09, R-11, R-12) | 4 scenarios |
| **Total** | **12** | **36 scenarios** |
