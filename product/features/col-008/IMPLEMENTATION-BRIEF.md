# Implementation Brief: col-008 Compaction Resilience

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/col-008/SCOPE.md |
| Scope Risk Assessment | product/features/col-008/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/col-008/architecture/ARCHITECTURE.md |
| Specification | product/features/col-008/specification/SPECIFICATION.md |
| Risk Strategy | product/features/col-008/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-008/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| session-registry | pseudocode/session-registry.md | test-plan/session-registry.md |
| compact-dispatch | pseudocode/compact-dispatch.md | test-plan/compact-dispatch.md |
| hook-handler | pseudocode/hook-handler.md | test-plan/hook-handler.md |
| wire-protocol | pseudocode/wire-protocol.md | test-plan/wire-protocol.md |
| injection-tracking | pseudocode/injection-tracking.md | test-plan/injection-tracking.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Implement compaction defense for the cortical implant. When Claude Code compresses conversation history, the PreCompact hook queries the server for previously-injected knowledge entries, constructs a prioritized payload within a 2000-token budget, and re-injects critical context into the compacted window via stdout. The server maintains per-session injection history (SessionRegistry), tracks which entries were injected during ContextSearch calls, and serves the compaction payload from in-memory state.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Session state management | Unified SessionRegistry replaces col-007's CoAccessDedup. Single module owns all per-session state: injection history, co-access dedup, session metadata, compaction count. | Architecture | architecture/ADR-001-session-registry.md |
| Compaction strategy | ID-based entry fetch from injection history (no embedding). Fallback to category-based query when no history available. | Architecture + ASS-014 D8 | architecture/ADR-002-id-based-compaction.md |
| Token budget allocation | Dynamic priority-based allocation with soft caps per category. Fill order: context, decisions, injections, conventions. Unused budget rolls over. | Architecture | architecture/ADR-003-token-budget-allocation.md |
| ContextSearch session_id | Add `session_id: Option<String>` with `#[serde(default)]` to ContextSearch wire message. | Human (Q1 resolution) | N/A (wire change) |
| CoAccessDedup integration | SessionRegistry wraps CoAccessDedup functionality. coaccess_seen becomes a field on SessionState. | Human (Q2 resolution) | architecture/ADR-001-session-registry.md |
| Fallback complexity | Minimal — entry_store.query() with category filters (decision, convention). No embedding, no semantic search. | Human (Q3 resolution) | architecture/ADR-002-id-based-compaction.md |
| Budget constants | Named constants (MAX_COMPACTION_BYTES, DECISION_BUDGET_BYTES, etc.). Same pattern as col-007's MAX_INJECTION_BYTES. | Human (Q4 resolution) | architecture/ADR-003-token-budget-allocation.md |
| injected_entry_ids field | Kept on CompactPayload as hint/fallback. Server prefers its own tracked history. | Human (Q5 resolution) | N/A (scope decision) |

## Build Order

### Wave 1: Wire Protocol Changes (no runtime dependencies)

1. **wire-protocol** — Remove `#[allow(dead_code)]` from CompactPayload and BriefingContent. Add `session_id: Option<String>` with `#[serde(default)]` to ContextSearch.

### Wave 2: SessionRegistry (no UDS dependencies)

2. **session-registry** — New module `session.rs`. Implement SessionState, InjectionRecord, SessionRegistry with all methods: register_session, record_injection, get_state, check_and_insert_coaccess, increment_compaction, clear_session. Unit tests for all methods.

### Wave 3: Hook Handler + Integration (depends on Wave 1)

3. **hook-handler** — Add `"PreCompact"` arm to `build_request()` in hook.rs. Update `is_fire_and_forget` to exclude CompactPayload. Update `write_stdout()` to handle BriefingContent.

### Wave 4: Server-Side Dispatch + Injection Tracking (depends on Wave 1 + Wave 2)

4. **injection-tracking** — Modify col-007's ContextSearch handler to call `session_registry.record_injection()` after building the response. Modify SessionRegister handler to call `session_registry.register_session()`. Modify SessionClose handler to call `session_registry.clear_session()`. Replace CoAccessDedup usage with SessionRegistry's check_and_insert_coaccess.

5. **compact-dispatch** — Implement CompactPayload handler in dispatch_request(). Primary path: get_state, fetch entries by ID, partition by category, allocate budget, format payload. Fallback path: query by category, format. Implement format_compaction_payload(). Wire SessionRegistry through start_uds_listener() and main.rs.

## Risk Hotspots (Top 5)

| Priority | Risk | What to Watch | Mitigation |
|----------|------|---------------|------------|
| 1 | R-05: Injection tracking fails silently | If session_id is missing or session not registered, injection history stays empty. CompactPayload always falls back. | Integration test: ContextSearch -> verify SessionState -> CompactPayload returns tracked entries. |
| 2 | R-03: Token budget overflow with multi-byte UTF-8 | Truncation at byte boundary must not produce invalid UTF-8. Total output must not exceed MAX_COMPACTION_BYTES. | Unit test: format_compaction_payload with multi-byte content, verify output.len() <= MAX_COMPACTION_BYTES and valid UTF-8. |
| 3 | R-07: CoAccessDedup behavior regression | SessionRegistry absorbs CoAccessDedup. Existing co-access behavior must be preserved exactly. | Replicate col-007's CoAccessDedup tests against SessionRegistry methods. |
| 4 | R-12: SessionRegister not called before ContextSearch | Claude Code should fire SessionStart before UserPromptSubmit, but there is no enforcement. If ContextSearch arrives first, injection tracking is silently skipped. | Integration test: ContextSearch without prior SessionRegister — verify graceful handling. |
| 5 | R-10: PreCompact classified as fire-and-forget | If is_fire_and_forget returns true for CompactPayload, hook exits before receiving response. | Unit test: verify CompactPayload excluded from fire-and-forget classification. |

## Files to Create/Modify

### New Files

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/session.rs` | Create | SessionState, InjectionRecord, SessionRegistry with all methods |

### Modified Files

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-engine/src/wire.rs` | Modify | Remove `#[allow(dead_code)]` from CompactPayload, BriefingContent. Add `session_id: Option<String>` to ContextSearch. |
| `crates/unimatrix-server/src/uds_listener.rs` | Modify | Add CompactPayload handler with primary + fallback paths. Integrate SessionRegistry into SessionRegister, SessionClose, ContextSearch handlers. Add format_compaction_payload function. Add budget constants. Replace CoAccessDedup with SessionRegistry. |
| `crates/unimatrix-server/src/hook.rs` | Modify | Add PreCompact arm to build_request(). Update is_fire_and_forget. Update write_stdout() for BriefingContent. |
| `crates/unimatrix-server/src/main.rs` | Modify | Create SessionRegistry, pass to start_uds_listener(). |
| `crates/unimatrix-server/src/lib.rs` | Modify | Add `pub mod session;` |

## Data Structures

### SessionState (new)

```rust
pub(crate) struct SessionState {
    pub session_id: String,
    pub role: Option<String>,
    pub feature: Option<String>,
    pub injection_history: Vec<InjectionRecord>,
    pub coaccess_seen: HashSet<Vec<u64>>,
    pub compaction_count: u32,
}
```

### InjectionRecord (new)

```rust
pub(crate) struct InjectionRecord {
    pub entry_id: u64,
    pub confidence: f64,
    pub timestamp: u64,
}
```

### SessionRegistry (new)

```rust
pub(crate) struct SessionRegistry {
    sessions: Mutex<HashMap<String, SessionState>>,
}

impl SessionRegistry {
    pub fn new() -> Self;
    pub fn register_session(&self, session_id: &str, role: Option<String>, feature: Option<String>);
    pub fn record_injection(&self, session_id: &str, entries: &[(u64, f64)]);
    pub fn get_state(&self, session_id: &str) -> Option<SessionState>;
    pub fn check_and_insert_coaccess(&self, session_id: &str, entry_ids: &[u64]) -> bool;
    pub fn increment_compaction(&self, session_id: &str);
    pub fn clear_session(&self, session_id: &str);
}
```

### ContextSearch (modified — add session_id)

```rust
ContextSearch {
    query: String,
    session_id: Option<String>,  // NEW
    role: Option<String>,
    task: Option<String>,
    feature: Option<String>,
    k: Option<u32>,
    max_tokens: Option<u32>,
}
```

## Function Signatures

### Hook Process (hook.rs)

```rust
// Modified: add PreCompact arm
fn build_request(event: &str, input: &HookInput) -> HookRequest;

// Modified: handle HookResponse::BriefingContent
fn write_stdout(response: &HookResponse) -> Result<(), Box<dyn std::error::Error>>;
```

### UDS Listener (uds_listener.rs)

```rust
// Modified: add session_registry parameter
pub async fn start_uds_listener(
    socket_path: &Path,
    store: Arc<Store>,
    // ... existing params from col-007 ...
    session_registry: Arc<SessionRegistry>,
    server_uid: u32,
    server_version: String,
) -> io::Result<(JoinHandle<()>, SocketGuard)>;

// Modified: add session_registry parameter
async fn dispatch_request(
    request: HookRequest,
    // ... existing params from col-007 ...
    session_registry: &Arc<SessionRegistry>,
) -> HookResponse;

// New: format compaction payload from entries
fn format_compaction_payload(
    entries_by_category: &CompactionCategories,
    session: &SessionState,
    max_bytes: usize,
) -> Option<String>;
```

## Constants

| Constant | Value | Location | Purpose |
|----------|-------|----------|---------|
| `MAX_COMPACTION_BYTES` | 8000 | uds_listener.rs | Total byte budget (~2000 tokens) |
| `DECISION_BUDGET_BYTES` | 1600 | uds_listener.rs | Soft cap for decision entries (~400 tokens) |
| `INJECTION_BUDGET_BYTES` | 2400 | uds_listener.rs | Soft cap for re-injected entries (~600 tokens) |
| `CONVENTION_BUDGET_BYTES` | 1600 | uds_listener.rs | Soft cap for convention entries (~400 tokens) |
| `CONTEXT_BUDGET_BYTES` | 800 | uds_listener.rs | Soft cap for session context section (~200 tokens) |

## Constraints

### Hard Constraints

- redb exclusive file lock: all data access through IPC
- 50ms latency budget for hook execution
- Zero regression on existing MCP tools and hook handlers
- Single binary (unimatrix-server)
- No new redb tables or schema changes
- No shared search function (inherited from col-007 ADR-001)
- No embedding at PreCompact time (ADR-002)

### Soft Constraints

- Linux + macOS only (UDS transport)
- Token budget is byte-based heuristic (4 bytes/token)
- In-memory session state only (no persistence)
- Category budgets are soft caps (rollover allowed)
- Edition 2024, MSRV 1.89
- No new external crate dependencies

## Dependencies

### Internal Crates Used

| Crate | Purpose |
|-------|---------|
| `unimatrix-engine` | Wire protocol types (HookRequest, HookResponse, CompactPayload, BriefingContent) |
| `unimatrix-core` | AsyncEntryStore (get, query), QueryFilter, Status |
| `unimatrix-store` | Store, EntryRecord |

### No New External Dependencies

All required crates are already in the workspace.

### Feature Dependencies

| Feature | Relationship |
|---------|-------------|
| col-006 | UDS transport, hook subcommand, wire protocol types, LocalTransport |
| col-007 | ContextSearch handler (injection tracking point), CoAccessDedup (absorbed into SessionRegistry) |

### Downstream Dependents

| Feature | What It Needs from col-008 |
|---------|---------------------------|
| col-009 | SessionState.injection_history for confidence signal generation |
| col-010 | SessionRegistry as in-memory backing for persistent session records |

## NOT in Scope

- Injection recording / INJECTION_LOG table (col-010)
- Confidence feedback from compaction (col-009)
- Session lifecycle persistence / SESSIONS table (col-010)
- Disk-based compaction cache / sidecar file
- Adaptive injection volume reduction
- Correction chain tracking in session
- Schema v4 migration
- Embedding / semantic search at PreCompact time
- Runtime-configurable token budgets
- New redb tables
- New external crate dependencies

## Test Strategy Summary

From RISK-TEST-STRATEGY.md: 12 risks mapped to 47 test scenarios across 3 priority levels.

| Priority | Risk Count | Scenarios | Key Risks |
|----------|-----------|-----------|-----------|
| High | 4 | 17 | R-03 (budget overflow), R-05 (injection tracking), R-10 (fire-and-forget), R-12 (missing SessionRegister) |
| Medium | 6 | 22 | R-01 (lock contention), R-02 (stale entries), R-04 (fallback empty), R-06 (session_id mismatch), R-07 (CoAccessDedup regression), R-11 (fetch failures) |
| Low | 2 | 8 | R-08 (latency), R-09 (wire compatibility) |

### Test Infrastructure

Extends existing test infrastructure from col-006 and col-007. Key test helpers needed:

| Helper | Purpose |
|--------|---------|
| SessionRegistry unit test fixtures | Controlled injection history for payload construction tests |
| format_compaction_payload unit tests | Known entry sets with various categories and sizes |
| End-to-end lifecycle test | SessionRegister -> ContextSearch (injection) -> CompactPayload -> verify payload |

## Acceptance Criteria Checklist

- [ ] AC-01: PreCompact hook sends CompactPayload with session_id via UDS
- [ ] AC-02: UDS dispatches CompactPayload and returns BriefingContent
- [ ] AC-03: Per-session injection history maintained after ContextSearch
- [ ] AC-04: Payload includes injected entries sorted by confidence, decisions prioritized
- [ ] AC-05: Token budget enforced (8000 bytes) with priority allocation
- [ ] AC-06: Fallback produces payload from category lookups when no history
- [ ] AC-07: Graceful degradation when server unavailable
- [ ] AC-08: SessionState created on register, cleaned on close
- [ ] AC-09: ContextSearch wire message includes session_id
- [ ] AC-10: Payload formatted as structured plain text
- [ ] AC-11: Existing MCP integration tests pass
- [ ] AC-12: Server-side processing under 15ms

## Alignment Status

From ALIGNMENT-REPORT.md: **6 PASS. Zero VARIANCE. Zero FAIL.**

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Implements "compaction resilience" — the second leg of invisible delivery |
| Milestone Fit | PASS | Correctly scoped to M5 Collective phase |
| Scope Gaps | PASS | All 12 acceptance criteria addressed |
| Scope Additions | PASS | No scope additions; disk cache and adaptive volume correctly deferred |
| Architecture Consistency | PASS | Consistent with col-006/007 UDS patterns, session state, async dispatch |
| Risk Completeness | PASS | 12 risks, 47 scenarios, all scope risks traced |

No variances require human approval.
