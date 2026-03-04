# Risk-Based Test Strategy: vnc-008

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Import path breakage: mass module moves break `use crate::*` imports, compilation fails | High | High | Critical |
| R-02 | Behavioral divergence: ToolContext construction subtly differs from inline ceremony (identity, format, audit) | High | Med | High |
| R-03 | StatusService produces different report than inline context_status for same data | High | Med | High |
| R-04 | format_status_change generic does not produce byte-identical output to original functions | Med | Med | Med |
| R-05 | SessionWrite capability serde incompatibility with existing AGENT_REGISTRY entries | High | Low | Med |
| R-06 | Test migration: tests depend on module-private items that become unreachable after move | Med | High | High |
| R-07 | Circular import between module groups (e.g., mcp/ imports uds/, infra/ imports services/) | Med | Med | Med |
| R-08 | UDS capability enforcement rejects a legitimate UDS operation | High | Low | Med |
| R-09 | response.rs split breaks pub(crate) visibility of helpers used by other modules | Med | Med | Med |
| R-10 | Re-export stubs cause name collisions or ambiguous imports during migration | Low | Med | Low |
| R-11 | StatusService direct-table access breaks if Store table definitions change | Med | Low | Low |

## Risk-to-Scenario Mapping

### R-01: Import Path Breakage
**Severity**: High
**Likelihood**: High
**Impact**: Compilation failure. Blocks all development until resolved. Difficult to debug in a large diff.

**Test Scenarios**:
1. Each migration step (infra, mcp, uds, capability+StatusService, cleanup) compiles with `cargo check`
2. Full test suite passes after each migration step
3. Re-exports from old paths resolve correctly during intermediate steps

**Coverage Requirement**: `cargo check` and `cargo test` after each of the 5 migration steps. No step may introduce compilation errors.

### R-02: ToolContext Behavioral Divergence
**Severity**: High
**Likelihood**: Med
**Impact**: MCP tools return different responses. Agents receive incorrect data or errors.

**Test Scenarios**:
1. `build_context()` with valid agent_id and format returns identical identity, format, and AuditContext to the inline ceremony
2. `build_context()` with unknown agent_id auto-enrolls as Restricted (same as current behavior)
3. `build_context()` with invalid format returns same error as `parse_format()` directly
4. `require_cap()` with insufficient capability returns same error code as `registry.require_capability()`
5. Each of 12 tool handlers produces identical output before and after ToolContext integration (existing test suite)

**Coverage Requirement**: Unit tests for `build_context()` and `require_cap()`. Existing handler integration tests must pass unchanged.

### R-03: StatusService Report Divergence
**Severity**: High
**Likelihood**: Med
**Impact**: context_status returns incorrect metrics. Health monitoring and maintenance operations affected.

**Test Scenarios**:
1. `StatusService::compute_report()` with empty database returns zero counts
2. `StatusService::compute_report()` with known test data returns identical StatusReport fields to the inline implementation
3. `StatusService::compute_report()` with topic_filter and category_filter applies filters correctly
4. `StatusService::run_maintenance()` performs confidence refresh, graph compaction, co-access cleanup (same as maintain=true path)
5. Snapshot test: capture StatusReport from inline implementation, compare to StatusService output for same data

**Coverage Requirement**: Snapshot comparison test with known data. Integration test verifying maintenance operations.

### R-04: format_status_change Output Mismatch
**Severity**: Med
**Likelihood**: Med
**Impact**: MCP responses differ from pre-refactoring output. Agents may parse responses incorrectly.

**Test Scenarios**:
1. `format_status_change("Deprecated", "deprecated", "deprecated", ...)` produces byte-identical output to `format_deprecate_success()`
2. Same for quarantine and restore variants
3. All three output formats (summary, markdown, JSON) match
4. With reason=Some and reason=None for each variant

**Coverage Requirement**: Unit tests comparing generic output to original function output across all format modes and reason variants. 3 variants x 3 formats x 2 reason states = 18 test cases.

### R-05: SessionWrite Serde Incompatibility
**Severity**: High
**Likelihood**: Low
**Impact**: Existing AGENT_REGISTRY entries fail to deserialize. Server crashes on startup or corrupts registry.

**Test Scenarios**:
1. Deserialize a bincode-encoded `Capability` vec that was serialized without `SessionWrite` — should succeed (existing variants decode correctly)
2. Serialize a `Capability` vec with `SessionWrite`, deserialize it — round-trip succeeds
3. Deserialize an `AgentRecord` from pre-vnc-008 format — all fields decode, `capabilities` vec does not contain `SessionWrite`

**Coverage Requirement**: Round-trip serde test for Capability enum with and without SessionWrite. Backward compatibility test with pre-vnc-008 serialized data.

### R-06: Test Migration Dependency Breakage
**Severity**: Med
**Likelihood**: High
**Impact**: Tests fail because they depend on `pub(crate)` items that are now in a different module group.

**Test Scenarios**:
1. Audit all `#[cfg(test)]` blocks in moved modules — verify they compile after the move
2. Integration tests in `tests/` directory — verify `use unimatrix_server::*` imports resolve
3. Tests that construct `ServerError`, `AuditEvent`, etc. — verify types are accessible

**Coverage Requirement**: Full `cargo test` after each migration step. Track test count before and after — no net reduction.

### R-07: Circular Import Between Groups
**Severity**: Med
**Likelihood**: Med
**Impact**: Compilation failure with "circular dependency" error or complex import resolution.

**Test Scenarios**:
1. `infra/` compiles without any `use crate::services::*` or `use crate::mcp::*` imports
2. `mcp/` does not import from `uds/`
3. `uds/` does not import from `mcp/`
4. No module group imports from a module group that imports from it

**Coverage Requirement**: Grep-based verification of import direction. `cargo check` compilation.

### R-08: UDS Capability False Rejection
**Severity**: High
**Likelihood**: Low
**Impact**: Hook injection stops working. Context injection, session tracking, and signal recording fail silently.

**Test Scenarios**:
1. UDS SessionRegister succeeds with SessionWrite capability
2. UDS RecordEvent succeeds with SessionWrite capability
3. UDS ContextSearch succeeds with Search capability
4. UDS CompactPayload succeeds with Search + Read capabilities
5. UDS Briefing succeeds with Search + Read capabilities
6. All existing UDS integration tests pass unchanged

**Coverage Requirement**: Unit test for each UDS operation against UDS_CAPABILITIES. Existing UDS integration tests must pass.

### R-09: response.rs Split Visibility Breakage
**Severity**: Med
**Likelihood**: Med
**Impact**: Formatting functions become inaccessible to callers in tools.rs or other modules.

**Test Scenarios**:
1. All `format_*` functions are accessible from `mcp/tools.rs` via `use crate::mcp::response::*`
2. `ResponseFormat`, `parse_format`, `StatusReport`, `Briefing` types are accessible from callers
3. `entry_to_json` helper is accessible within `mcp/response/` sub-modules

**Coverage Requirement**: Compilation verification. All existing tests that call formatting functions pass.

### R-10: Re-Export Name Collisions
**Severity**: Low
**Likelihood**: Med
**Impact**: Ambiguous import warnings or errors during intermediate migration steps.

**Test Scenarios**:
1. During step 1 (infra/ move): `use crate::audit` resolves to `crate::infra::audit` via re-export
2. No `ambiguous import` compiler warnings during any migration step
3. Re-export removal in step 5 does not break any remaining consumers

**Coverage Requirement**: `cargo check` with no warnings (except explicitly allowed) at each step.

### R-11: StatusService Table Definition Drift
**Severity**: Med
**Likelihood**: Low
**Impact**: StatusService computation silently produces wrong results if table structure changes.

**Test Scenarios**:
1. StatusService uses the same table constants (ENTRIES, COUNTERS, etc.) as the original code
2. Table constant imports come from `unimatrix_store`, not hardcoded

**Coverage Requirement**: Code review verification. Integration test with known data.

## Integration Risks

### Import Direction Violations
- **Risk**: A developer adds a `use unimatrix_store::Store` in `mcp/tools.rs` for a quick fix, bypassing the service layer
- **Mitigation**: Code review convention documented. Consider a CI grep check: `grep -r "use unimatrix_store" crates/unimatrix-server/src/mcp/ | grep -v EntryRecord | grep -v Status`
- **Test**: Grep-based CI check on import patterns

### Service Layer Bypass
- **Risk**: UDS handler calls `store.insert()` directly instead of going through StoreService
- **Mitigation**: Module visibility rules. Document that only `services/` and `infra/` may import foundation crates for storage access.
- **Test**: Grep-based verification

### Cross-Transport Coupling
- **Risk**: `mcp/tools.rs` imports a helper from `uds/listener.rs` (e.g., `run_confidence_consumer`)
- **Mitigation**: Currently, tools.rs imports `run_confidence_consumer` and `run_retrospective_consumer` from uds_listener.rs. These must be moved to `services/` or `infra/` before the module split.
- **Test**: Verify no `use crate::mcp::*` in uds/ and no `use crate::uds::*` in mcp/

## Edge Cases

### Empty Database
- StatusService::compute_report() with empty tables should return zero counts and empty distributions, not errors.

### New Capability Variant Serde
- An AGENT_REGISTRY entry serialized before vnc-008 does not contain `SessionWrite` in its capabilities vec. Deserialization must not fail. The agent simply does not have SessionWrite — they have whatever capabilities they had before.

### ToolContext with System Agent
- System agent has all capabilities. build_context() must resolve "system" identity correctly and not over-restrict.

### UDS Ping (No Capability Required)
- Ping has no capability requirement. The capability check must not apply to Ping.

### Re-Export During Migration
- During intermediate steps, both `use crate::audit::AuditEvent` and `use crate::infra::audit::AuditEvent` should resolve to the same type. They must not be two different types.

## Security Risks

### SessionWrite Capability Boundary
- **Untrusted input**: UDS connections receive arbitrary data from hook processes. SessionWrite limits what operations UDS can perform, but input validation (S3 via SecurityGateway) must still apply to all inputs.
- **Blast radius**: If SessionWrite is overly permissive, a compromised hook process could write arbitrary session data. Mitigated by: SessionWrite only permits operational writes (session records, injection logs, signals), not knowledge writes.
- **Escalation path**: SessionWrite cannot be escalated to Write or Admin. The capability enum has no hierarchy between SessionWrite and Write — they are disjoint.

### Direct-Table Access in StatusService
- **Risk**: StatusService reads directly from redb tables. If a malicious entry is crafted to exploit deserialization, StatusService is exposed.
- **Mitigation**: StatusService only reads via `deserialize_entry()` which is the same path used by Store::get(). No new deserialization surface.

### Module Visibility as Security Boundary
- **Risk**: `pub(crate)` visibility is the primary enforcement mechanism. Any code within the crate can bypass module boundaries.
- **Mitigation**: This is convention enforcement, not security enforcement. The service layer's security gates (SecurityGateway S1-S5) are the actual security boundary. Module visibility is defense-in-depth.

## Failure Modes

### Compilation Failure During Migration
- **Behavior**: `cargo check` fails at an intermediate step
- **Recovery**: Each step is a separate commit. `git revert` the failing step. Fix and re-apply.

### Test Count Reduction
- **Behavior**: Some tests fail to compile after module move
- **Recovery**: Tests must move with their modules. If a test depends on a `pub(crate)` item in a different module group, the test must be relocated or the item's visibility adjusted.

### StatusService Panic
- **Behavior**: StatusService panics during compute_report() (e.g., table not found)
- **Recovery**: StatusService runs inside spawn_blocking. Panic does not crash the server. Error propagated to caller via JoinError.

### UDS Capability Rejection
- **Behavior**: A UDS operation fails with capability error
- **Recovery**: Check UDS_CAPABILITIES constant. The operation either needs a different capability mapping or the operation is legitimately restricted.

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 | R-07 | Import direction rules defined. Compilation verification at each step. |
| SR-02 | R-02 | ADR-002: ToolContext via UnimatrixServer method. rmcp constraint addressed. |
| SR-03 | R-01, R-10 | ADR-004: Sequential migration with re-exports. 5 steps instead of big bang. |
| SR-04 | R-08 | Exhaustive UDS operation capability matrix defined. Unit tests per operation. |
| SR-05 | R-03, R-11 | ADR-001: StatusService inherits direct-table access. Snapshot tests verify equivalence. |
| SR-06 | R-08 | AC-22/AC-23: Explicit tests that SessionWrite rejects knowledge writes and admin ops. |
| SR-07 | R-09 | ToolContext placed in mcp/context.rs (MCP-specific). No shared/ module needed. |
| SR-08 | — | Mitigated by designing against vnc-007 SCOPE.md/ARCHITECTURE.md as expected baseline. |
| SR-09 | R-06 | Test migration tracked. Tests move with code. No test deletions. |
| SR-10 | R-09 | response.rs dependency graph mapped. Shared types in mod.rs. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 5 scenarios (one per migration step) |
| High | 4 (R-02, R-03, R-06, R-08) | 17 scenarios |
| Medium | 5 (R-04, R-05, R-07, R-09, R-11) | 28 scenarios |
| Low | 1 (R-10) | 3 scenarios |
| **Total** | **11** | **53 scenarios** |
