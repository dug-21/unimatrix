# Implementation Brief: col-007 Automatic Context Injection

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/col-007/SCOPE.md |
| Scope Risk Assessment | product/features/col-007/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/col-007/architecture/ARCHITECTURE.md |
| Specification | product/features/col-007/specification/SPECIFICATION.md |
| Risk Strategy | product/features/col-007/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-007/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| hook-handler | pseudocode/hook-handler.md | test-plan/hook-handler.md |
| uds-dispatch | pseudocode/uds-dispatch.md | test-plan/uds-dispatch.md |
| injection-format | pseudocode/injection-format.md | test-plan/injection-format.md |
| session-warming | pseudocode/session-warming.md | test-plan/session-warming.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Implement automatic knowledge injection into every Claude Code prompt via the UserPromptSubmit hook. The hook process extracts the user's prompt, sends a semantic search request to the Unimatrix server over UDS, and prints matched knowledge entries to stdout for injection into Claude's context. Pre-warm the ONNX embedding model on SessionStart to ensure sub-50ms latency on the hot path.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| UDS shared state approach | Parameter expansion: pass individual Arc services to `start_uds_listener()`. Duplicate ~40 lines of search pipeline orchestration rather than extract a shared function into unimatrix-engine. Preserves clean crate boundaries. | Architecture + Human (SR-02) | architecture/ADR-001-uds-shared-state.md |
| Dispatch async model | Fully async `dispatch_request()`. All handlers are async, not just ContextSearch. Mechanical change, future-proofs for col-008+ handlers. | Architecture | architecture/ADR-002-async-uds-dispatch.md |
| Co-access dedup strategy | Session-scoped in-memory `HashMap<String, HashSet<Vec<u64>>>`. Cleared on SessionClose. No persistence across server restarts. Bounded memory (~200KB worst case). | Architecture | architecture/ADR-003-session-coaccess-dedup.md |
| Injection recording | Deferred to col-010. col-007 does NOT record injection events. col-010 introduces typed INJECTION_LOG table. | Human (SR-05) | N/A (scope decision) |
| Token budget | Byte-based constant `MAX_INJECTION_BYTES = 1400` (~350 tokens at 4 bytes/token). Not a real tokenizer. | Human + Specification | N/A |
| Pre-warming strategy | Blocking on SessionStart: await `get_adapter()` then run warmup `embed_entry("", "warmup")`. Hook process is fire-and-forget so blocking is safe server-side. | Human (SR-04) + Architecture | N/A |

## Build Order

### Wave 1: Wire Protocol Activation (no runtime dependencies)

1. **wire-protocol changes** -- Remove `#[allow(dead_code)]` from ContextSearch, Entries, EntryPayload. Add `prompt: Option<String>` field to HookInput with `#[serde(default)]`.

### Wave 2: Hook Handler + Injection Formatting (depends on Wave 1)

2. **hook-handler** -- Add `"UserPromptSubmit"` arm to `build_request()` in hook.rs that constructs `HookRequest::ContextSearch` from the prompt field. Update `is_fire_and_forget` check to exclude ContextSearch. Add `format_injection()` function and update `write_stdout()` to handle `HookResponse::Entries`.

3. **injection-format** -- Implement `format_injection()` in hook.rs: iterate entries in rank order, format each with title/category/confidence/content, accumulate bytes until `MAX_INJECTION_BYTES`, handle truncation and multi-byte UTF-8 safety.

### Wave 3: Server-Side Dispatch (depends on Wave 1)

4. **uds-dispatch** -- Make `dispatch_request()` async. Expand `start_uds_listener()` signature with additional Arc parameters. Implement `HookRequest::ContextSearch` handler: embed query, adapt, L2 normalize, HNSW search, fetch entries, re-rank, co-access boost, filter by similarity/confidence floors, return `HookResponse::Entries`. Add `CoAccessDedup` struct and integrate with SessionClose cleanup. Update `main.rs` call site.

5. **session-warming** -- Extend the `SessionRegister` handler to await `embed_service.get_adapter()` and run warmup embedding via `spawn_blocking`. Return Ack only after warming completes.

### Wave 4: Integration

6. **main.rs integration** -- Pass additional Arc parameters from main.rs to `start_uds_listener()`. Wire up the async_entry_store, async_vector_store, embed_handle, and adapt_service.

## Risk Hotspots (Top 5)

| Priority | Risk | What to Watch | Mitigation |
|----------|------|---------------|------------|
| 1 | R-01: Pipeline drift between MCP and UDS search | Entry IDs and ordering must match for identical queries across MCP and UDS. Duplicated orchestration (~40 lines) must stay in sync. | Integration test comparing MCP context_search and UDS ContextSearch for 3+ queries. |
| 2 | R-05: SessionStart/UserPromptSubmit race condition | If UserPromptSubmit fires before SessionStart warming completes, first prompt hits cold ONNX path. EmbedNotReady must return empty results, not error. | Integration test: ContextSearch without prior SessionRegister returns empty Entries. |
| 3 | R-08: UDS timeout under server load | ContextSearch must complete within 40ms transport timeout on hot path. Embedding + HNSW + rerank + boost ~12ms total. | Latency benchmark test: 10 iterations p95 < 50ms. |
| 4 | R-03: Byte budget overflow with multi-byte UTF-8 | Truncation at byte boundary must not produce invalid UTF-8. CJK (3 bytes) and emoji (4 bytes) entries must respect budget. | Unit test: format_injection with multi-byte content, verify output.len() <= MAX_INJECTION_BYTES and valid UTF-8. |
| 5 | R-02: Async dispatch breaks existing handlers | Making dispatch_request() async changes all existing handler paths. Tests must add .await. | Existing dispatch unit tests updated with .await, all 6 handler variants verified. |

## Files to Create/Modify

### Modified Files

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-engine/src/wire.rs` | Modify | Remove `#[allow(dead_code)]` from ContextSearch, Entries, EntryPayload. Add `prompt: Option<String>` field to HookInput with `#[serde(default)]`. |
| `crates/unimatrix-server/src/hook.rs` | Modify | Add `"UserPromptSubmit"` arm to `build_request()`. Add `format_injection()` function. Add `MAX_INJECTION_BYTES` constant. Update `write_stdout()` to handle Entries. Update `is_fire_and_forget` check. |
| `crates/unimatrix-server/src/uds_listener.rs` | Modify | Make `dispatch_request()` async. Expand `start_uds_listener()` with additional Arc parameters. Add ContextSearch handler with full search pipeline. Add `CoAccessDedup` struct. Add SessionStart warming to SessionRegister handler. Add SessionClose dedup cleanup. Add constants: SIMILARITY_FLOOR, CONFIDENCE_FLOOR, INJECTION_K, EF_SEARCH. |
| `crates/unimatrix-server/src/main.rs` | Modify | Pass `embed_handle`, `async_vector_store`, `async_entry_store`, `adapt_service` to `start_uds_listener()`. |

### No New Files (source code)

All changes are modifications to existing files. No new crates, no new source files.

## Data Structures

### HookInput (modified -- add prompt field)

```rust
#[derive(Deserialize, Debug, Clone)]
pub struct HookInput {
    #[serde(default)]
    pub hook_event_name: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub transcript_path: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,  // NEW: UserPromptSubmit prompt text
    #[serde(flatten)]
    pub extra: serde_json::Value,
}
```

### EntryPayload (existing stub -- activate)

```rust
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EntryPayload {
    pub id: u64,
    pub title: String,
    pub content: String,
    pub confidence: f64,
    pub similarity: f64,
    pub category: String,
}
```

### CoAccessDedup (new struct in uds_listener.rs)

```rust
pub(crate) struct CoAccessDedup {
    sessions: Mutex<HashMap<String, HashSet<Vec<u64>>>>,
}

impl CoAccessDedup {
    pub fn new() -> Self { ... }
    pub fn check_and_insert(&self, session_id: &str, entry_ids: &[u64]) -> bool { ... }
    pub fn clear_session(&self, session_id: &str) { ... }
}
```

### HookRequest::ContextSearch (existing stub -- activate)

```rust
ContextSearch {
    query: String,
    role: Option<String>,
    task: Option<String>,
    feature: Option<String>,
    k: Option<u32>,
    max_tokens: Option<u32>,
}
```

### HookResponse::Entries (existing stub -- activate)

```rust
Entries {
    items: Vec<EntryPayload>,
    total_tokens: u32,
}
```

## Function Signatures

### Hook Process (hook.rs)

```rust
// Modified: add UserPromptSubmit arm
fn build_request(event: &str, input: &HookInput) -> HookRequest;

// New: format matched entries as structured text
fn format_injection(entries: &[EntryPayload], max_bytes: usize) -> Option<String>;

// Modified: handle HookResponse::Entries
fn write_stdout(response: &HookResponse) -> Result<(), Box<dyn std::error::Error>>;
```

### UDS Listener (uds_listener.rs)

```rust
// Modified: expanded signature with additional Arc parameters
pub async fn start_uds_listener(
    socket_path: &Path,
    store: Arc<Store>,
    embed_service: Arc<EmbedServiceHandle>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    adapt_service: Arc<AdaptationService>,
    server_uid: u32,
    server_version: String,
) -> io::Result<(JoinHandle<()>, SocketGuard)>;

// Modified: sync -> async, expanded parameters
async fn dispatch_request(
    request: HookRequest,
    store: &Arc<Store>,
    embed_service: &Arc<EmbedServiceHandle>,
    vector_store: &Arc<AsyncVectorStore<VectorAdapter>>,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
    adapt_service: &Arc<AdaptationService>,
    server_version: &str,
    coaccess_dedup: &CoAccessDedup,
) -> HookResponse;
```

### CoAccessDedup (uds_listener.rs)

```rust
impl CoAccessDedup {
    pub fn new() -> Self;
    /// Returns true if the entry set is new (not seen before for this session).
    pub fn check_and_insert(&self, session_id: &str, entry_ids: &[u64]) -> bool;
    /// Remove all dedup state for a session.
    pub fn clear_session(&self, session_id: &str);
}
```

## Constants

| Constant | Value | Location | Purpose |
|----------|-------|----------|---------|
| `MAX_INJECTION_BYTES` | `1400` | hook.rs | Byte budget for stdout injection output (~350 tokens at 4 bytes/token) |
| `SIMILARITY_FLOOR` | `0.5` | uds_listener.rs | Minimum cosine similarity for injection candidates |
| `CONFIDENCE_FLOOR` | `0.3` | uds_listener.rs | Minimum confidence score for injection candidates |
| `INJECTION_K` | `5` | uds_listener.rs | Maximum number of entries to search for |
| `EF_SEARCH` | `32` | uds_listener.rs | HNSW expansion factor (mirrors tools.rs constant) |

## Constraints

### Hard Constraints

- **redb exclusive file lock**: Hook processes cannot open the database; all data access through IPC to the running server
- **50ms latency budget**: End-to-end hook execution under 50ms on hot path. Estimated ~12ms.
- **Zero regression**: All existing MCP tool integration tests (174+) pass without modification
- **Single binary**: Hook subcommand is part of `unimatrix-server`
- **No shared search function**: Pipeline orchestration is duplicated per ADR-001
- **No injection recording**: Deferred to col-010

### Soft Constraints

- Linux + macOS only (UDS transport)
- Token budget is byte-based heuristic (4 bytes/token)
- Similarity and confidence floors are compile-time constants
- Co-access dedup is in-memory only (no persistence)
- Edition 2024, MSRV 1.89
- No new external crate dependencies

## Dependencies

### Internal Crates Used

| Crate | Purpose |
|-------|---------|
| `unimatrix-engine` | Wire protocol types (HookRequest, HookResponse, EntryPayload, HookInput), co-access pair generation (`generate_pairs`), confidence re-ranking (`rerank_score`), transport |
| `unimatrix-core` | AsyncEntryStore, AsyncVectorStore, EmbedService trait |
| `unimatrix-store` | Store (co-access recording via `record_co_access`) |
| `unimatrix-adapt` | AdaptationService (MicroLoRA embedding adaptation) |
| `unimatrix-embed` | `l2_normalized` (embedding normalization) |
| `unimatrix-server` (same crate) | EmbedServiceHandle (lazy-loading embed service access) |

### No New External Dependencies

All required crates are already in the workspace.

### Feature Dependencies

| Feature | Relationship |
|---------|-------------|
| col-006 | UDS transport, hook subcommand, wire protocol types, LocalTransport |
| crt-004 | Co-access pair generation and recording |
| crt-006 | MicroLoRA embedding adaptation pipeline |

### Downstream Dependents

| Feature | What It Needs from col-007 |
|---------|---------------------------|
| col-008 | Knowledge of injected entries (via col-010's INJECTION_LOG, not directly) |
| col-009 | Injection entry IDs for confidence signaling (via col-010's INJECTION_LOG) |
| col-010 | The injection pipeline to record against (col-010 adds recording layer on top) |

## NOT in Scope

- Injection recording / INJECTION_LOG table (col-010)
- Compaction resilience / PreCompact handler (col-008)
- Confidence feedback / signal generation (col-009)
- Session lifecycle persistence / SESSIONS table (col-010)
- Agent routing / semantic agent matching (col-011)
- Schema v4 migration (col-010)
- Prompt summarization or classification
- Runtime-configurable search parameters
- Real tokenizer for budget enforcement
- Hook configuration automation (alc-003)
- New redb tables or schema changes
- New external crate dependencies

## Test Strategy Summary

From RISK-TEST-STRATEGY.md: 12 risks mapped to 43 test scenarios across 4 priority levels.

| Priority | Risk Count | Scenarios | Key Risks |
|----------|-----------|-----------|-----------|
| High | 4 | 14 | R-01 (pipeline drift), R-05 (race condition), R-08 (timeout), R-11 (spawn_blocking) |
| Medium | 6 | 22 | R-03 (byte budget), R-04 (threshold suppression), R-09 (content parsing), R-10 (oversized prompt), R-12 (warming failure), R-02 (async dispatch) |
| Low | 2 | 7 | R-06 (memory leak), R-07 (HookInput flatten) |

### Test Infrastructure

Extends existing test infrastructure from col-006. Key test helpers needed:

| Helper | Purpose |
|--------|---------|
| Existing `TestUdsServer` or equivalent | UDS integration test with populated knowledge base |
| `format_injection` unit tests | Controlled EntryPayload inputs with various content types |
| Pipeline equivalence tests | Compare MCP and UDS search results for same queries |

## Acceptance Criteria Checklist

- [ ] AC-01: UserPromptSubmit hook extracts `prompt` field and sends ContextSearch via UDS
- [ ] AC-02: UDS listener dispatches ContextSearch and returns Entries response
- [ ] AC-03: Search pipeline produces equivalent results via MCP and UDS
- [ ] AC-04: Formatted stdout includes title, category, confidence, content
- [ ] AC-05: Token budget enforced (1400 bytes)
- [ ] AC-06: Co-access pairs generated with session dedup
- [ ] AC-07: SessionStart pre-warms ONNX model
- [ ] AC-08: Graceful degradation when server unavailable
- [ ] AC-09: Silent skip on empty/low-quality results
- [ ] AC-10: HookInput.prompt field works correctly
- [ ] AC-11: Existing MCP integration tests pass
- [ ] AC-12: Hot-path latency under 50ms

## Alignment Status

From ALIGNMENT-REPORT.md: **6 PASS. Zero VARIANCE. Zero FAIL.**

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly implements the "Hooks" leg and "invisible delivery" core value |
| Milestone Fit | PASS | Correctly scoped to M5 Collective phase |
| Scope Gaps | PASS | All 12 acceptance criteria addressed |
| Scope Additions | PASS | No scope additions; injection recording correctly deferred |
| Architecture Consistency | PASS | Consistent with PidGuard, EmbedServiceHandle, async wrapper patterns |
| Risk Completeness | PASS | 12 risks, 43 scenarios, all scope risks traced |

No variances require human approval.

## Hook Configuration Reference

For `.claude/settings.json` (manual setup, automated in alc-003):

```json
{
  "hooks": {
    "UserPromptSubmit": [{
      "hooks": [{
        "type": "command",
        "command": "unimatrix-server hook UserPromptSubmit"
      }]
    }]
  }
}
```

This supplements the existing SessionStart and Stop hooks configured by col-006.
