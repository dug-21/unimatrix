# Risk-Based Test Strategy: vnc-007

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | CompactPayload behavioral regression: token-budget proportional allocation produces different entry selection than byte-budget fixed allocation | High | Med | High |
| R-02 | MCP briefing semantic search regression: BriefingService delegates to SearchService instead of bespoke embed path, changing co-access anchor selection | Med | Med | Med |
| R-03 | Feature flag does not compose with rmcp `#[tool]` macro, causing compilation failure or silent tool registration | Med | Med | Med |
| R-04 | Injection history path accidentally couples to SearchService, introducing latency on CompactPayload hot path | High | Low | Med |
| R-05 | Quarantine exclusion gap in injection history path (entries fetched by ID may have been quarantined since injection) | Med | Med | Med |
| R-06 | Budget overflow in mixed-source assembly (conventions + injection history + semantic all active) | Med | Low | Low |
| R-07 | format_briefing duties removal breaks existing test assertions that check for "Duties" in output | Low | High | Med |
| R-08 | BriefingService EmbedNotReady fallback returns incomplete briefing without clear indication | Med | Low | Low |
| R-09 | CompactPayload format text diverges from current format (section headers, entry formatting) | Med | Med | Med |
| R-10 | dispatch_unknown_returns_error test breakage when Briefing is wired | Low | High | Low |

## Risk-to-Scenario Mapping

### R-01: CompactPayload Behavioral Regression
**Severity**: High
**Likelihood**: Med
**Impact**: Changed compaction output degrades agent context quality. Agents may lose important context during compaction or receive less relevant entries.

**Test Scenarios**:
1. Snapshot test: populate knowledge base with known entries, set up session with injection history, call old `handle_compact_payload` and new BriefingService path, compare selected entries and ordering
2. Budget boundary test: create entries that exactly fill the current byte-budget sections (decisions=3200 bytes, injections=2400 bytes). Verify the same entries are selected after token conversion
3. Edge case: single very large entry that exceeds one section's proportional allocation but fits in another. Verify no cross-section cascade

**Coverage Requirement**: Snapshot comparison test with at least 3 entries per category (decisions, injections, conventions) at different confidence levels. Both primary path (injection history) and fallback path (category query) must be covered.

### R-02: MCP Briefing Semantic Search Regression
**Severity**: Med
**Likelihood**: Med
**Impact**: Different co-access anchors change which entries receive boost, potentially reordering results. Conventions + relevant_context content may differ from pre-refactoring output.

**Test Scenarios**:
1. Integration test: populate knowledge base with conventions and task-relevant entries, call context_briefing before and after refactoring, compare returned entry IDs
2. Co-access anchor test: verify BriefingService passes already-collected convention entry IDs as co-access anchors to SearchService (improving relevance vs current behavior of using only search result IDs)
3. Feature boost test: provide feature tag, verify feature-tagged entries are boosted in semantic search results

**Coverage Requirement**: At least one integration test with populated knowledge base comparing pre/post entry selection.

### R-03: Feature Flag + rmcp Macro Compatibility
**Severity**: Med
**Likelihood**: Med
**Impact**: Build failure or tool registered despite feature being off, undermining the feature flag purpose.

**Test Scenarios**:
1. Compile test: `cargo build --no-default-features` succeeds without errors
2. Tool count test: with feature off, verify the MCP tool router has one fewer tool (no context_briefing)
3. Compile test: `cargo build` (default features) succeeds, context_briefing is registered
4. If rmcp macro is incompatible with `#[cfg]`: verify fallback approach works (tool returns error when feature off)

**Coverage Requirement**: CI must run both feature configurations.

### R-04: Injection History Path Latency
**Severity**: High
**Likelihood**: Low
**Impact**: CompactPayload hook response times regress, visible as slower context window compaction.

**Test Scenarios**:
1. Code review: verify `include_semantic=false` path has zero imports/calls to SearchService, EmbedServiceHandle, VectorIndex
2. Unit test: create BriefingService with a SearchService that panics on call. Call assemble with `include_semantic=false`. Verify no panic (SearchService never invoked)

**Coverage Requirement**: One unit test proving SearchService isolation when include_semantic=false.

### R-05: Quarantine Exclusion in Injection History
**Severity**: Med
**Likelihood**: Med
**Impact**: Quarantined entries appear in compaction output, potentially delivering harmful or incorrect knowledge.

**Test Scenarios**:
1. Unit test: inject entry ID into injection_history, quarantine that entry in the store, call assemble, verify entry is excluded from result
2. Unit test: mix of active, deprecated, and quarantined entries in injection history, verify only quarantined are excluded (deprecated entries should appear with indicator, matching current behavior)

**Coverage Requirement**: Explicit quarantine exclusion test for injection history path.

### R-06: Budget Overflow with Mixed Sources
**Severity**: Med
**Likelihood**: Low
**Impact**: Output exceeds max_tokens, consuming more context window than expected.

**Test Scenarios**:
1. Unit test: set max_tokens=10 (very small), provide all three sources (conventions, semantic, injection). Verify total output does not exceed budget
2. Unit test: max_tokens=0 or max_tokens=1, verify graceful handling (empty or minimal result, no panic)

**Coverage Requirement**: One boundary test with constrained budget.

### R-07: Duties Removal Test Breakage
**Severity**: Low
**Likelihood**: High
**Impact**: Existing tests fail because they assert on "Duties" presence in briefing output.

**Test Scenarios**:
1. Update all format_briefing tests to remove duties assertions
2. Add negative test: verify "Duties" and "duties" do NOT appear in briefing output for any format
3. Verify Briefing struct construction sites compile without duties field

**Coverage Requirement**: All existing briefing format tests pass after duties removal.

### R-08: EmbedNotReady Fallback
**Severity**: Med
**Likelihood**: Low
**Impact**: Briefing returns conventions-only (no semantic context), which may be insufficient for agents.

**Test Scenarios**:
1. Unit test: BriefingService with EmbedNotReady SearchService, verify `search_available=false` in result
2. Verify conventions are still returned when search is unavailable

**Coverage Requirement**: One unit test for graceful degradation.

### R-09: CompactPayload Format Text Divergence
**Severity**: Med
**Likelihood**: Med
**Impact**: Agents that parse compaction output may break if format changes.

**Test Scenarios**:
1. Verify section headers ("## Decisions", "## Key Context", "## Conventions") are preserved
2. Verify entry format (title, confidence, content truncation) matches current format
3. Verify header format ("--- Unimatrix Compaction Context ---", role/feature/compaction lines)

**Coverage Requirement**: Format comparison tests for CompactPayload output.

### R-10: dispatch_unknown_returns_error Test
**Severity**: Low
**Likelihood**: High
**Impact**: Test compilation or assertion failure.

**Test Scenarios**:
1. Update test to use a different unimplemented request variant (or define a test-only variant)

**Coverage Requirement**: Test passes after update.

## Integration Risks

| Risk | Scenario | Mitigation |
|------|----------|-----------|
| BriefingService + SearchService interface mismatch | SearchService's ServiceSearchParams may not expose co_access_anchors in the right format | Verify at implementation time; SearchService already accepts `Option<Vec<u64>>` |
| BriefingService + SessionRegistry coupling | BriefingService must NOT access SessionRegistry directly | Code review: BriefingService receives InjectionEntry vec, not session state |
| ServiceLayer construction order | BriefingService needs SearchService clone during construction | SearchService is Clone (vnc-006); verify construction order in ServiceLayer::new() |

## Edge Cases

| Edge Case | Expected Behavior |
|-----------|------------------|
| Empty knowledge base (no conventions, no entries) | Return empty BriefingResult with search_available=true/false based on embed status |
| All entries in injection history are quarantined | Return empty InjectionSections |
| max_tokens=0 | Return empty or near-empty BriefingResult (no panic) |
| role is None but include_conventions=true | Skip convention lookup (no topic to query) |
| task is None but include_semantic=true | Skip semantic search (no query to embed) |
| Injection history with duplicate entry_ids at different confidence | Keep highest confidence per entry_id |
| Very large injection history (1000+ entries) | Budget allocation truncates; no unbounded memory growth |
| Feature tag matches no entries | Feature boost has no effect; results sorted by confidence/similarity only |

## Security Risks

| Concern | Assessment |
|---------|-----------|
| Untrusted input via BriefingParams | role, task, feature are validated by S3 (length, control chars). max_tokens is validated by range check. No path traversal or injection risk — these are string parameters used for queries, not file paths or shell commands. |
| Injection history entry_ids | Entry IDs are u64 — no injection risk. Entries are fetched from store and quarantine-checked. |
| Blast radius of BriefingService compromise | Read-only service. Cannot write, modify, or delete entries. Worst case: returns unexpected entries in briefing. Bounded by token budget. |
| Feature flag bypass | Feature flag is compile-time. No runtime bypass possible. |

## Failure Modes

| Failure | Expected Behavior |
|---------|------------------|
| SearchService embed fails (EmbedNotReady) | search_available=false, continue with non-semantic sources |
| AsyncEntryStore query fails | Return ServiceError::Core, transport maps to error response |
| Entry fetch by ID fails (entry deleted) | Skip entry, continue with remaining entries (matches current behavior) |
| SecurityGateway S3 validation fails | Return ServiceError::ValidationFailed, transport maps to error response |
| Token budget exceeded mid-assembly | Stop adding entries, return what fits |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (feature flag + rmcp macro) | R-03 | ADR-001: gate method with `#[cfg]`; fallback to wrapper if incompatible. Test both configurations. |
| SR-02 (SearchService interface for briefing) | R-02 | ADR-002: delegate to SearchService with k=3. ServiceSearchParams already supports all needed knobs. |
| SR-03 (budget behavioral equivalence) | R-01 | ADR-003: proportional token allocation. Snapshot tests compare old vs new output. |
| SR-04 (S2 rate limiting scope) | -- | ADR-004: deferred to vnc-009. Not applicable to vnc-007 architecture. |
| SR-05 (duties category confusion) | R-07 | Code comment on allowlist entry. Tests verify no duties in output. |
| SR-06 (CompactPayload latency) | R-04 | Architecture ensures include_semantic=false path has zero SearchService involvement. Unit test with panicking SearchService proves isolation. |
| SR-07 (dispatch test breakage) | R-10 | Update test to use different unimplemented variant. Trivial. |
| SR-08 (vnc-006 interface stability) | -- | Architecture pins to vnc-006 interfaces documented in ARCHITECTURE.md. Revalidate post-merge if needed. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 2 (R-01, R-04) | 6 scenarios |
| Medium | 5 (R-02, R-03, R-05, R-07, R-09) | 12 scenarios |
| Low | 3 (R-06, R-08, R-10) | 4 scenarios |
| **Total** | **10** | **22 scenarios** |
