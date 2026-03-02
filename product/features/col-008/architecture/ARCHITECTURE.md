# Architecture: col-008 Compaction Resilience

## System Overview

col-008 adds compaction defense to the cortical implant architecture. When Claude Code compresses conversation history (PreCompact event), previously-injected knowledge would be lost. col-008 intercepts this event, queries the server for the most important entries from the current session's injection history, and re-injects them into the compacted window via stdout.

The feature touches three subsystems: the hook process (client-side PreCompact handler), the UDS listener (server-side CompactPayload dispatch), and a new SessionRegistry (server-side session state management). It also modifies col-007's ContextSearch handler to record injection history.

## Component Breakdown

### Component 1: PreCompact Hook Handler (hook process)

**Location**: `crates/unimatrix-server/src/hook.rs`
**Responsibility**: Extract session_id from Claude Code's stdin JSON, send CompactPayload request via UDS, print response content to stdout.

Changes to existing code:
- Add `"PreCompact"` arm to `build_request()` that constructs `HookRequest::CompactPayload`
- Update `is_fire_and_forget` check to exclude CompactPayload (it is synchronous)
- Update `write_stdout()` to handle `HookResponse::BriefingContent`

### Component 2: SessionRegistry (server-side state)

**Location**: `crates/unimatrix-server/src/session.rs` (new module)
**Responsibility**: Manage per-session state including injection history and co-access dedup. Unified container for all session-scoped server-side state.

New code:
- `SessionState` struct: session metadata + injection history + compaction count
- `SessionRegistry` struct: thread-safe wrapper around `HashMap<String, SessionState>`
- Absorbs col-007's `CoAccessDedup` — the dedup set becomes a field on `SessionState`
- Methods: `register_session()`, `record_injection()`, `get_state()`, `increment_compaction()`, `clear_session()`

### Component 3: CompactPayload Dispatcher (server-side)

**Location**: `crates/unimatrix-server/src/uds_listener.rs`
**Responsibility**: Handle `HookRequest::CompactPayload`, construct prioritized knowledge payload from session state, return `HookResponse::BriefingContent`.

Changes to existing code:
- Add `HookRequest::CompactPayload` arm to `dispatch_request()`
- Primary path: fetch entries by ID from injection history, sort by priority, format within token budget
- Fallback path: query entries by category (decisions, conventions) when no injection history exists
- Increment compaction_count in session state

### Component 4: ContextSearch Injection Tracking (server-side modification)

**Location**: `crates/unimatrix-server/src/uds_listener.rs`
**Responsibility**: After ContextSearch returns entries, record the injected entry IDs and confidence scores in the session's injection history.

Changes to existing code (col-007 modification):
- After building the `HookResponse::Entries`, call `session_registry.record_injection(session_id, entries)`
- Requires `session_id` from the ContextSearch request (wire protocol change)
- SessionRegister handler creates session state in the registry

### Component 5: Wire Protocol Changes

**Location**: `crates/unimatrix-engine/src/wire.rs`
**Responsibility**: Activate `CompactPayload` and `BriefingContent` stubs. Add `session_id` to `ContextSearch`.

Changes to existing code:
- Remove `#[allow(dead_code)]` from `CompactPayload` and `BriefingContent`
- Add `session_id: Option<String>` with `#[serde(default)]` to `ContextSearch`

## Component Interactions

```
Claude Code                Hook Process              UDS Listener            SessionRegistry
    |                          |                          |                         |
    |--PreCompact------------->|                          |                         |
    |  stdin: {session_id}     |                          |                         |
    |                          |--CompactPayload--------->|                         |
    |                          |  via LocalTransport       |                         |
    |                          |                          |--get_state(session_id)->|
    |                          |                          |<--SessionState----------|
    |                          |                          |                         |
    |                          |                          |  [if injection history available]
    |                          |                          |--fetch entries by ID--->|
    |                          |                          |  (entry_store.get)      | (entry_store)
    |                          |                          |<--full entries-----------|
    |                          |                          |--sort by priority------>|
    |                          |                          |--format within budget-->|
    |                          |                          |                         |
    |                          |                          |  [if no injection history]
    |                          |                          |--query by category----->|
    |                          |                          |  (decisions, conventions)| (entry_store)
    |                          |                          |<--entries by category---|
    |                          |                          |--format within budget-->|
    |                          |                          |                         |
    |                          |                          |--increment_compaction-->|
    |                          |                          |                         |
    |                          |<--BriefingContent--------|                         |
    |                          |                          |                         |
    |<--stdout: compaction-----|                          |                         |
    |   defense payload        |                          |                         |
```

### Injection Tracking Flow (col-007 ContextSearch extension)

```
Claude Code                Hook Process              UDS Listener            SessionRegistry
    |                          |                          |                         |
    |--UserPromptSubmit------->|                          |                         |
    |  stdin: {prompt, sess_id}|                          |                         |
    |                          |--ContextSearch----------->|                         |
    |                          |  {query, session_id}      |                         |
    |                          |                          |  [search pipeline]       |
    |                          |                          |  embed -> HNSW -> rank   |
    |                          |                          |                         |
    |                          |                          |--record_injection------>|
    |                          |                          |  (session_id, entries)   |
    |                          |                          |                         |
    |                          |<--Entries response--------|                         |
```

## Technology Decisions

### ADR-001: Unified SessionRegistry Replacing CoAccessDedup

See `architecture/ADR-001-session-registry.md`.

Col-007 introduced `CoAccessDedup` — a standalone struct managing co-access dedup sets per session. Col-008 introduces `SessionRegistry` — a unified session state container that absorbs CoAccessDedup's functionality and adds injection history tracking. The SessionRegistry is designed for extensibility by col-009 (confidence signals) and col-010 (session persistence).

### ADR-002: ID-Based Fetch for Compaction Payload (No Embedding)

See `architecture/ADR-002-id-based-compaction.md`.

The CompactPayload handler uses ID-based entry fetching (`entry_store.get(id)`) rather than semantic search. This avoids ONNX runtime dependency at PreCompact time, keeps latency under 15ms server-side, and leverages the injection history (which already represents semantically-relevant entries from prior ContextSearch calls).

### ADR-003: Priority-Based Token Budget Allocation

See `architecture/ADR-003-token-budget-allocation.md`.

The 2000-token budget (8000 bytes) is allocated by entry category priority: decisions first, then high-confidence injections, then conventions. Named constants define each category's byte share. Entries within each category are sorted by confidence descending.

## Integration Points

### Existing Components Used (Read-Only Integration)

| Component | Crate | What col-008 Uses |
|-----------|-------|-------------------|
| `AsyncEntryStore` | unimatrix-core | `get()` for ID-based entry fetching, `query()` for category-based fallback lookups |
| `Store` | unimatrix-store | Inherited from UDS listener parameters (no new direct usage) |

### Existing Components Modified

| Component | Change | Risk |
|-----------|--------|------|
| `hook.rs` | Add PreCompact arm, update write_stdout for BriefingContent | Low — additive |
| `uds_listener.rs` | Add CompactPayload handler, integrate SessionRegistry, injection tracking in ContextSearch | Medium — extends col-007's handler |
| `wire.rs` | Activate CompactPayload/BriefingContent stubs, add session_id to ContextSearch | Low — additive with `#[serde(default)]` |
| `main.rs` | Pass SessionRegistry to `start_uds_listener()` | Low — mechanical |

### New Components

| Component | Change | Risk |
|-----------|--------|------|
| `session.rs` | New module: SessionState, SessionRegistry, InjectionRecord | Medium — new stateful component |

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `SessionState` | `struct { session_id: String, role: Option<String>, feature: Option<String>, injection_history: Vec<InjectionRecord>, coaccess_seen: HashSet<Vec<u64>>, compaction_count: u32 }` | session.rs (new) |
| `InjectionRecord` | `struct { entry_id: u64, confidence: f64, timestamp: u64 }` | session.rs (new) |
| `SessionRegistry` | `struct { sessions: Mutex<HashMap<String, SessionState>> }` | session.rs (new) |
| `SessionRegistry::register_session()` | `fn(&self, session_id: &str, role: Option<String>, feature: Option<String>)` | session.rs (new) |
| `SessionRegistry::record_injection()` | `fn(&self, session_id: &str, entries: &[(u64, f64)])` | session.rs (new) |
| `SessionRegistry::get_state()` | `fn(&self, session_id: &str) -> Option<SessionState>` | session.rs (new) — returns clone |
| `SessionRegistry::check_and_insert_coaccess()` | `fn(&self, session_id: &str, entry_ids: &[u64]) -> bool` | session.rs (new) — absorbs CoAccessDedup |
| `SessionRegistry::increment_compaction()` | `fn(&self, session_id: &str)` | session.rs (new) |
| `SessionRegistry::clear_session()` | `fn(&self, session_id: &str)` | session.rs (new) |
| `start_uds_listener()` | Modified: adds `session_registry: Arc<SessionRegistry>` parameter | uds_listener.rs |
| `dispatch_request()` | Modified: adds `session_registry: &Arc<SessionRegistry>` parameter | uds_listener.rs |
| `build_request()` | Modified: adds `"PreCompact"` arm returning `HookRequest::CompactPayload` | hook.rs |
| `write_stdout()` | Modified: handles `HookResponse::BriefingContent` | hook.rs |
| `format_compaction_payload()` | `fn(entries: &[EntryRecord], session: &SessionState, max_bytes: usize) -> Option<String>` | hook.rs or uds_listener.rs (new) |
| `HookRequest::ContextSearch.session_id` | `session_id: Option<String>` with `#[serde(default)]` | wire.rs (modified) |
| `MAX_COMPACTION_BYTES` | `const: usize = 8000` | uds_listener.rs (new) |
| `DECISION_BUDGET_BYTES` | `const: usize = 1600` | uds_listener.rs (new) — ~400 tokens |
| `INJECTION_BUDGET_BYTES` | `const: usize = 2400` | uds_listener.rs (new) — ~600 tokens |
| `CONVENTION_BUDGET_BYTES` | `const: usize = 1600` | uds_listener.rs (new) — ~400 tokens |
| `CONTEXT_BUDGET_BYTES` | `const: usize = 800` | uds_listener.rs (new) — ~200 tokens |

## Files to Create/Modify

### New Files

| File | Summary |
|------|---------|
| `crates/unimatrix-server/src/session.rs` | SessionState, SessionRegistry, InjectionRecord |
| `product/features/col-008/architecture/ADR-001-session-registry.md` | Unified session state design |
| `product/features/col-008/architecture/ADR-002-id-based-compaction.md` | ID-based fetch decision |
| `product/features/col-008/architecture/ADR-003-token-budget-allocation.md` | Priority budget allocation |

### Modified Files

| File | Summary |
|------|---------|
| `crates/unimatrix-server/src/uds_listener.rs` | CompactPayload handler, SessionRegistry integration, injection tracking in ContextSearch, SessionRegister creates session state |
| `crates/unimatrix-server/src/hook.rs` | PreCompact arm in build_request(), BriefingContent in write_stdout() |
| `crates/unimatrix-engine/src/wire.rs` | Activate CompactPayload/BriefingContent stubs, add session_id to ContextSearch |
| `crates/unimatrix-server/src/main.rs` | Create SessionRegistry, pass to start_uds_listener() |
| `crates/unimatrix-server/src/lib.rs` | Add `pub mod session;` |

## Architectural Constraints

1. **No shared search function** (inherited from col-007 ADR-001). The CompactPayload handler uses ID-based fetch, not the search pipeline. No need for the shared extraction that col-007 deferred.

2. **Dispatcher is fully async** (inherited from col-007 ADR-002). The CompactPayload handler is async for entry_store.get() calls.

3. **SessionRegistry replaces CoAccessDedup** (ADR-001). All session-scoped state lives in SessionRegistry. CoAccessDedup's HashSet<Vec<u64>> becomes a field on SessionState.

4. **No embedding at PreCompact time** (ADR-002). The compaction payload is constructed from ID-based fetches and category lookups. No ONNX dependency.

5. **Hook process remains synchronous** (inherited from col-006 ADR-002). No tokio runtime in the hook path. All async operations happen server-side.

6. **In-memory state only** (no persistence). SessionState is lost on server restart. The briefing fallback handles this case.
