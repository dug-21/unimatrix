# Specification: col-008 Compaction Resilience

## Objective

Implement compaction defense for Unimatrix's cortical implant. When Claude Code compresses conversation history (PreCompact event), the hook handler queries the server for previously-injected knowledge entries, constructs a prioritized payload within a 2000-token budget, and re-injects it into the compacted window via stdout. This preserves critical context (architectural decisions, conventions, feature-specific knowledge) that would otherwise be lost during compaction.

## Functional Requirements

### FR-01: PreCompact Hook Handler

- FR-01.1: When Claude Code fires the `PreCompact` event, the hook subcommand extracts `session_id` from the stdin JSON.
- FR-01.2: The hook constructs a `HookRequest::CompactPayload` with the session_id and empty `injected_entry_ids` (server has the state). Optional fields (`role`, `feature`, `token_limit`) are set to `None` (server uses defaults or session state).
- FR-01.3: The hook sends the CompactPayload request as a synchronous UDS request (not fire-and-forget) and waits for the response within the 40ms transport timeout.
- FR-01.4: On receiving `HookResponse::BriefingContent`, the hook prints the `content` field to stdout if non-empty. On receiving `HookResponse::Error` or on transport failure, the hook exits 0 with no stdout output.
- FR-01.5: The `PreCompact` arm in `build_request()` must not be classified as fire-and-forget. The `is_fire_and_forget` check must exclude `HookRequest::CompactPayload`.

### FR-02: SessionRegistry

- FR-02.1: A `SessionRegistry` manages per-session state in a `Mutex<HashMap<String, SessionState>>`.
- FR-02.2: `SessionState` contains: session_id, role (Option<String>), feature (Option<String>), injection_history (Vec<InjectionRecord>), coaccess_seen (HashSet<Vec<u64>>), compaction_count (u32).
- FR-02.3: `InjectionRecord` contains: entry_id (u64), confidence (f64), timestamp (u64).
- FR-02.4: `register_session(session_id, role, feature)` creates a new SessionState. If a session with the same ID already exists, it is overwritten (handles reconnection).
- FR-02.5: `record_injection(session_id, entries)` appends InjectionRecords to the session's injection_history. Each entry is recorded with its entry_id, confidence, and current timestamp. Duplicate entry_ids across injections are allowed (same entry can be injected on multiple prompts).
- FR-02.6: `check_and_insert_coaccess(session_id, entry_ids)` provides the same dedup behavior as col-007's CoAccessDedup. Returns true if the entry set is new for this session.
- FR-02.7: `get_state(session_id)` returns a clone of the SessionState if it exists, None otherwise.
- FR-02.8: `increment_compaction(session_id)` increments the compaction_count for the session.
- FR-02.9: `clear_session(session_id)` removes all state for the session (called on SessionClose).
- FR-02.10: If record_injection or check_and_insert_coaccess is called for an unregistered session, the operation is silently ignored (no panic, no error).

### FR-03: Server-Side CompactPayload Dispatch

- FR-03.1: The UDS dispatcher handles `HookRequest::CompactPayload` by looking up the session's injection history.
- FR-03.2: **Primary path** (injection history available): collect unique entry IDs from injection_history. For duplicate entry_ids, keep the highest confidence value. Fetch full entries via `entry_store.get(id)`. Skip quarantined entries (status == Quarantined). Include deprecated entries with an indicator.
- FR-03.3: Partition fetched entries into categories: "decision" entries, "convention" entries, and all others ("injection" category for budget purposes).
- FR-03.4: Allocate token budget using dynamic priority allocation per ADR-003: session context first, then decisions (up to DECISION_BUDGET_BYTES), then high-confidence injections (up to INJECTION_BUDGET_BYTES), then conventions (up to CONVENTION_BUDGET_BYTES). Unused budget rolls over to the next category.
- FR-03.5: Within each category, entries are sorted by confidence descending. Entries are added until the category's budget is exhausted. Partial entry truncation follows the same rules as col-007's format_injection (truncate at char boundary if remaining budget >= 100 bytes, skip otherwise).
- FR-03.6: **Fallback path** (no injection history — server restart or first compaction before any injection): query entries by category. Fetch active decisions via `entry_store.query(category: "decision", status: Active)`. Fetch active conventions via `entry_store.query(category: "convention", status: Active)`. If `feature` is available from the request or SessionState, filter decisions by feature tag. Sort each group by confidence descending. Format within budget.
- FR-03.7: The formatted payload includes a header identifying it as compaction context ("Unimatrix Compaction Context"), the session context section (role, feature, compaction count), and entry sections grouped by category.
- FR-03.8: The dispatcher returns `HookResponse::BriefingContent` with the formatted content and an estimated token_count (bytes / 4).
- FR-03.9: If both primary and fallback paths produce no entries, return `HookResponse::BriefingContent` with empty content and token_count 0.
- FR-03.10: After constructing the response, increment the session's compaction_count via `session_registry.increment_compaction(session_id)`.

### FR-04: ContextSearch Injection Tracking

- FR-04.1: The `HookRequest::ContextSearch` gains a `session_id: Option<String>` field with `#[serde(default)]`.
- FR-04.2: After the ContextSearch handler produces the response entries, if `session_id` is present and non-empty, call `session_registry.record_injection(session_id, entries)` with the entry IDs and confidence scores from the response.
- FR-04.3: Injection tracking is fire-and-forget — errors in recording do not affect the ContextSearch response.
- FR-04.4: The SessionRegister handler in the UDS dispatcher calls `session_registry.register_session()` to create the session state.
- FR-04.5: The SessionClose handler in the UDS dispatcher calls `session_registry.clear_session()` to remove all session state.

### FR-05: Wire Protocol Changes

- FR-05.1: Remove `#[allow(dead_code)]` from `HookRequest::CompactPayload` and `HookResponse::BriefingContent`.
- FR-05.2: Add `session_id: Option<String>` with `#[serde(default)]` to `HookRequest::ContextSearch`.
- FR-05.3: Existing `CompactPayload` fields remain unchanged: session_id, injected_entry_ids, role, feature, token_limit. The server uses its own injection history preferentially but may fall back to `injected_entry_ids` if its state is missing (for future extensibility).

### FR-06: Compaction Payload Formatting

- FR-06.1: The compaction payload is formatted as structured plain text with clear section headers.
- FR-06.2: The session context section includes: role (if available), feature (if available), and compaction count.
- FR-06.3: Each entry section includes: entry title, category, confidence (as percentage), and content.
- FR-06.4: Entries are grouped by their budget category (decisions, injections, conventions) with section separators.
- FR-06.5: Entry IDs are included as metadata for downstream tracing.
- FR-06.6: The formatted output must not contain control characters, ANSI escape codes, or non-UTF-8 sequences (same constraint as col-007).
- FR-06.7: Multi-byte UTF-8 content must be truncated at character boundaries, not byte boundaries.

## Non-Functional Requirements

### NFR-01: Latency

- End-to-end PreCompact hook execution (process start to stdout output) must complete within 50ms on the hot path.
- Target p95 server-side processing: 15ms (ID fetch + sort + format for ~20 entries).
- No embedding operations at PreCompact time.
- Fallback path (category query) must also complete within 50ms total.

### NFR-02: Resource Usage

- SessionRegistry memory: bounded by active session count x injection history size. Target: <100KB per session (500 injections x ~200 bytes each). Total: <1MB for 10 concurrent sessions.
- No new redb tables or schema changes.
- No new external crate dependencies.

### NFR-03: Reliability

- Hook process always exits 0 regardless of server state, session state, or errors.
- Server unavailability results in silent skip (no stdout, no error visible to user).
- Missing session state triggers fallback path, not an error.
- Entry fetch failures for individual entries are skipped (best-effort — include what can be fetched).

### NFR-04: Compatibility

- All existing MCP tool integration tests pass without modification.
- Existing hook handlers continue to work identically.
- Wire protocol changes are additive only (new fields with `#[serde(default)]`, removing `#[allow(dead_code)]`).
- col-007's CoAccessDedup behavior is preserved identically within SessionRegistry.

## Acceptance Criteria with Verification Methods

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | PreCompact hook constructs CompactPayload with session_id and sends via UDS | Unit test: `build_request("PreCompact", ...)` returns `HookRequest::CompactPayload` with correct session_id. |
| AC-02 | UDS listener dispatches CompactPayload and returns BriefingContent | Integration test: send CompactPayload via UDS after registering a session with injections, verify BriefingContent response contains expected entries. |
| AC-03 | Server maintains per-session injection history updated after ContextSearch | Unit test: register session, record injections, verify get_state returns correct injection_history. Integration test: ContextSearch with session_id populates injection history. |
| AC-04 | Compaction payload includes injected entries sorted by confidence with decisions prioritized | Unit test: construct payload from known injection history with mixed categories, verify decisions appear first, sorted by confidence within each group. |
| AC-05 | Token budget enforced (8000 bytes) with priority-based allocation | Unit test: construct payload from entries exceeding budget, verify total bytes <= MAX_COMPACTION_BYTES. Verify category caps are respected. |
| AC-06 | Fallback path produces payload using category-based lookups when no injection history | Integration test: send CompactPayload for a session with no prior injections, verify response contains active decisions and conventions from knowledge base. |
| AC-07 | Graceful degradation when server unavailable | Unit test: hook process with no UDS socket exits 0 with empty stdout (same pattern as col-007). |
| AC-08 | SessionState created on SessionRegister, cleaned on SessionClose | Unit test: register session, verify state exists. Close session, verify state is removed. |
| AC-09 | ContextSearch wire message includes session_id | Unit test: serialize/deserialize ContextSearch with and without session_id. Verify backward compatibility (missing field defaults to None). |
| AC-10 | Compaction payload formatted as structured plain text | Unit test: verify payload format includes header, session context, entry sections with titles/categories/confidence/content. |
| AC-11 | All existing MCP integration tests pass | Run full test suite. Zero failures. |
| AC-12 | CompactPayload server-side processing under 15ms | Benchmark test: 10 iterations of CompactPayload dispatch with ~20 entries in injection history, p95 < 15ms. |

## Domain Models

### Key Entities

| Entity | Definition | Lifecycle |
|--------|-----------|-----------|
| SessionState | In-memory per-session state: injection history, co-access dedup set, session metadata, compaction count | Created on SessionRegister, updated on ContextSearch and CompactPayload, destroyed on SessionClose |
| InjectionRecord | A single injection event: entry_id + confidence + timestamp | Created on each ContextSearch, consumed on CompactPayload |
| Compaction Payload | Token-budgeted knowledge re-injection: prioritized entries formatted as structured text | Constructed per CompactPayload request, ephemeral |
| SessionRegistry | Thread-safe container for all SessionState instances, keyed by session_id | Created on server startup, lives for server lifetime |

### Constants

| Constant | Value | Location | Purpose |
|----------|-------|----------|---------|
| `MAX_COMPACTION_BYTES` | 8000 | uds_listener.rs | Total byte budget for compaction payload (~2000 tokens) |
| `DECISION_BUDGET_BYTES` | 1600 | uds_listener.rs | Soft cap for decision entries (~400 tokens) |
| `INJECTION_BUDGET_BYTES` | 2400 | uds_listener.rs | Soft cap for re-injected entries (~600 tokens) |
| `CONVENTION_BUDGET_BYTES` | 1600 | uds_listener.rs | Soft cap for convention entries (~400 tokens) |
| `CONTEXT_BUDGET_BYTES` | 800 | uds_listener.rs | Soft cap for session context section (~200 tokens) |

## User Workflows

### Primary Flow: Compaction Defense

1. Agent has been working for a while; Claude Code has injected knowledge on multiple prompts (col-007)
2. Conversation history grows large; Claude Code triggers compaction
3. Claude Code fires PreCompact hook before compressing
4. Hook process reads stdin JSON, extracts session_id
5. Hook sends CompactPayload request to server via UDS
6. Server looks up session's injection history — finds 25 previously-injected entries
7. Server fetches entries by ID, partitions by category (3 decisions, 18 injections, 4 conventions)
8. Server fills budget: session context (200 bytes), decisions sorted by confidence (1400 bytes), top injections (2200 bytes), conventions (1500 bytes) = ~5300 bytes within 8000 limit
9. Server returns formatted BriefingContent
10. Hook prints to stdout
11. Claude Code preserves this content in the compacted window
12. Agent continues working with critical decisions and conventions intact

### Fallback Flow: No Injection History

1. Server restarted mid-session (session state lost)
2. Claude Code fires PreCompact
3. Hook sends CompactPayload to server
4. Server looks up session — no injection history
5. Server queries knowledge base: active decisions (8 found), active conventions (12 found)
6. Server formats top entries by confidence within budget
7. Agent gets generic but relevant context after compaction

### Degraded Flow: Server Unavailable

1. Claude Code fires PreCompact
2. Hook attempts UDS connect, fails (server not running)
3. Hook exits 0 with no stdout output
4. Claude compresses without knowledge preservation
5. Agent continues but may have lost injected context (same as pre-col-008 behavior)

## Constraints

### From SCOPE.md

- redb exclusive file lock: all data access through IPC
- 50ms latency budget for hook execution
- Zero regression on existing MCP tools and hook handlers
- Single binary (unimatrix-server)
- No new redb tables or schema changes
- Edition 2024, MSRV 1.89
- Linux + macOS only (UDS transport)
- In-memory session state only (no persistence)

### Additional Specification Constraints

- Quarantined entries excluded from compaction payload
- Deprecated entries included with indicator (they may still be the most recent version the agent has seen)
- Entry status must be checked at CompactPayload time (entries may change status between injection and compaction)
- Token budget defined in bytes (MAX_COMPACTION_BYTES = 8000), not tokens
- Category budgets are soft caps — unused budget rolls over to next priority category
- Session_id on ContextSearch is optional — col-007 hooks populated before col-008 will not have it
- CompactPayload's injected_entry_ids field is a fallback hint — server prefers its own tracked history

## Dependencies

| Dependency | Type | Used For |
|-----------|------|----------|
| `unimatrix-core` (AsyncEntryStore) | Internal crate | `get()` for ID-based entry fetch, `query()` for category-based fallback |
| `unimatrix-store` (Store, EntryRecord, Status) | Internal crate | Entry record types, status enum |
| `unimatrix-engine` (wire types) | Internal crate | HookRequest, HookResponse, CompactPayload, BriefingContent |
| col-007 (ContextSearch handler) | Feature dependency | Injection tracking point — col-008 adds recording after ContextSearch returns |
| col-006 (UDS transport) | Feature dependency | Transport layer, LocalTransport, graceful degradation pattern |

No new external crate dependencies.

## NOT in Scope

- Session lifecycle persistence / SESSIONS table (col-010)
- Injection recording to redb / INJECTION_LOG table (col-010)
- Confidence feedback from compaction events (col-009)
- Disk-based compaction cache / sidecar file
- Adaptive injection volume reduction on repeated compaction
- Correction chain tracking in session state
- Schema v4 migration
- Embedding / semantic search at PreCompact time
- Runtime-configurable token budgets (compile-time constants only)
- Hook configuration automation (alc-003)
