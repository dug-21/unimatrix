# Specification: col-007 Automatic Context Injection

## Objective

Implement automatic knowledge injection into every Claude Code prompt via the UserPromptSubmit hook. The hook process extracts the user's prompt, sends a semantic search request to the Unimatrix server over UDS, and prints matched knowledge entries to stdout for injection into Claude's context. Pre-warm the ONNX embedding model on SessionStart to ensure sub-50ms latency on the hot path.

## Functional Requirements

### FR-01: UserPromptSubmit Hook Handler

- FR-01.1: When Claude Code fires the `UserPromptSubmit` event, the hook subcommand extracts the `prompt` field from the stdin JSON.
- FR-01.2: The hook constructs a `HookRequest::ContextSearch` with the prompt as the query string. Optional fields (`role`, `task`, `feature`) are populated from session context if available (via `HookInput` fields), or set to `None`.
- FR-01.3: The hook sends the ContextSearch request as a synchronous UDS request (not fire-and-forget) and waits for the response within the 40ms transport timeout.
- FR-01.4: On receiving `HookResponse::Entries`, the hook formats the entries and prints to stdout. On receiving `HookResponse::Error` or on transport failure, the hook exits 0 with no stdout output.
- FR-01.5: The `UserPromptSubmit` arm in `build_request()` must not be classified as fire-and-forget. The `is_fire_and_forget` check must exclude `HookRequest::ContextSearch`.

### FR-02: Server-Side ContextSearch Dispatch

- FR-02.1: The UDS dispatcher handles `HookRequest::ContextSearch` by running the search pipeline: embed the query, adapt via MicroLoRA, L2 normalize, HNSW search, fetch entries, re-rank, co-access boost, truncate.
- FR-02.2: The search pipeline uses the same parameters as the MCP `context_search` tool: `ef_search=32`, confidence re-rank formula `0.85*similarity + 0.15*confidence`, co-access boost with anchor top 3.
- FR-02.3: Results are filtered by similarity floor (`SIMILARITY_FLOOR = 0.5`) and confidence floor (`CONFIDENCE_FLOOR = 0.3`). Entries below either threshold are excluded.
- FR-02.4: Quarantined entries are excluded from results (matching MCP behavior).
- FR-02.5: The dispatcher returns `HookResponse::Entries` with `items` containing the filtered, ranked entries and `total_tokens` estimated via byte-length heuristic.
- FR-02.6: If the embed service is not ready (`EmbedNotReady`), the dispatcher returns `HookResponse::Entries` with empty items (silent skip, not an error).
- FR-02.7: The `k` parameter defaults to `INJECTION_K = 5` if not specified in the request.

### FR-03: Injection Formatting

- FR-03.1: Entries are formatted as structured plain text with a header line identifying the source ("Unimatrix Context") and individual entry blocks.
- FR-03.2: Each entry block includes: title, category, confidence score (formatted as percentage), and content.
- FR-03.3: Entry IDs are included in the output (as a comment or metadata line) for downstream tracing by col-008/col-009.
- FR-03.4: Entries are added to the output in rank order (highest-scoring first) until the cumulative byte count reaches `MAX_INJECTION_BYTES`.
- FR-03.5: If adding the next entry would exceed the byte budget, the entry is truncated to fit. If the remaining budget is less than 100 bytes (too small for meaningful content), the entry is omitted entirely.
- FR-03.6: If no entries survive filtering (empty results or all below thresholds), the function returns `None` and no stdout output is produced (silent skip).
- FR-03.7: The formatted output must not contain control characters, ANSI escape codes, or non-UTF-8 sequences.

### FR-04: SessionStart Pre-Warming

- FR-04.1: When the UDS dispatcher receives a `SessionRegister` request, it awaits `embed_service.get_adapter()` to block until the embedding model is loaded (or failed).
- FR-04.2: If the adapter is successfully obtained, the dispatcher runs `adapter.embed_entry("", "warmup")` via `spawn_blocking` to force ONNX runtime initialization.
- FR-04.3: The `Ack` response is sent only after warming completes (or the adapter fails to load). The hook process is fire-and-forget and has already disconnected, so this blocking does not affect hook latency.
- FR-04.4: If the embed service fails to load (`EmbedFailed`), the warming step is skipped and the `Ack` is sent normally. Subsequent ContextSearch requests will return empty results per FR-02.6.
- FR-04.5: Warming is idempotent. Multiple SessionStart events do not cause multiple warmings -- `get_adapter()` returns immediately if the model is already loaded.

### FR-05: Co-Access Pair Generation with Session Dedup

- FR-05.1: After a successful ContextSearch response with 2+ entries, the server generates co-access pairs from the injected entry IDs using the existing `generate_pairs()` function.
- FR-05.2: Before recording pairs, the server checks the session-scoped dedup set. If the canonical sorted entry ID vector has already been recorded for this session, co-access recording is skipped.
- FR-05.3: Co-access pairs are recorded via `store.record_co_access()` with the current timestamp.
- FR-05.4: The dedup set for a session is cleared when a `SessionClose` request is received for that session_id.
- FR-05.5: Co-access recording is fire-and-forget (errors are logged but do not affect the ContextSearch response).

### FR-06: HookInput Extension

- FR-06.1: The `HookInput` struct gains a `prompt: Option<String>` field with `#[serde(default)]`.
- FR-06.2: The field is `None` for non-UserPromptSubmit events (backward compatible).
- FR-06.3: The `extra` flatten field continues to capture any fields not explicitly named.

## Non-Functional Requirements

### NFR-01: Latency

- End-to-end UserPromptSubmit hook execution (process start to stdout output) must complete within 50ms on the hot path (ONNX model already loaded).
- Target p95 latency: 12-15ms on the hot path.
- Cold path (first call before SessionStart warming): up to 250ms is acceptable (one-time cost).

### NFR-02: Resource Usage

- Co-access dedup memory: bounded by session count x unique entry sets. Target < 1MB.
- No new redb tables or schema changes.
- No new external crate dependencies.

### NFR-03: Reliability

- Hook process always exits 0 regardless of server state, search results, or errors.
- Server unavailability results in silent skip (no stdout, no error visible to user).
- Server errors (embed failure, HNSW failure, redb read failure) result in empty results, not error responses to the hook.

### NFR-04: Compatibility

- All existing MCP tool integration tests (174+) pass without modification.
- Existing hook handlers (Ping, SessionRegister, SessionClose, RecordEvent) continue to work identically (Ack responses).
- Wire protocol changes are additive only (new named field with `#[serde(default)]`, removing `#[allow(dead_code)]`).

## Acceptance Criteria with Verification Methods

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | UserPromptSubmit hook extracts `prompt` field and sends ContextSearch via UDS | Unit test: `build_request("UserPromptSubmit", ...)` returns `HookRequest::ContextSearch` with correct query. Integration test: hook process with mock stdin produces ContextSearch request. |
| AC-02 | UDS listener dispatches ContextSearch and returns Entries response | Integration test: send ContextSearch via UDS, verify Entries response with expected entry payloads. |
| AC-03 | Search pipeline produces equivalent results via MCP and UDS | Integration test: same query via `context_search` MCP tool and ContextSearch UDS request returns same entry IDs in same order. |
| AC-04 | Formatted stdout includes title, category, confidence, content | Unit test: `format_injection()` with known entries produces expected text format. |
| AC-05 | Token budget enforced (1400 bytes) | Unit test: `format_injection()` with entries exceeding budget truncates output. Verify byte count <= MAX_INJECTION_BYTES. |
| AC-06 | Co-access pairs generated with session dedup | Integration test: send same ContextSearch twice for same session, verify co-access pairs recorded once. Third search with different entries records new pairs. |
| AC-07 | SessionStart pre-warms ONNX model | Integration test: send SessionRegister, then ContextSearch. Verify ContextSearch returns results (not EmbedNotReady). |
| AC-08 | Graceful degradation when server unavailable | Unit test: hook process with no UDS socket exits 0 with empty stdout. |
| AC-09 | Silent skip on empty/low-quality results | Unit test: `format_injection()` with empty entries returns None. Integration test: ContextSearch with unrelated query returns empty Entries. |
| AC-10 | HookInput.prompt field works correctly | Unit test: deserialize JSON with and without prompt field. Verify backward compatibility. |
| AC-11 | Existing MCP integration tests pass | Run full test suite (1025 unit + 174 integration). Zero failures. |
| AC-12 | Hot-path latency under 50ms | Benchmark test: 10 iterations of ContextSearch via UDS, p95 < 50ms. |

## Domain Models

### Key Entities

| Entity | Definition | Lifecycle |
|--------|-----------|-----------|
| Injection | A single knowledge delivery event: prompt submitted, entries searched, results printed to stdout | Per-prompt, ephemeral |
| Entry Payload | A wire-protocol representation of a knowledge entry: id, title, content, confidence, similarity, category | Per-response, ephemeral |
| Co-Access Dedup Set | In-memory set of entry ID vectors already recorded for a session | Per-session, cleared on SessionClose |

### Constants

| Constant | Value | Location | Purpose |
|----------|-------|----------|---------|
| `MAX_INJECTION_BYTES` | 1400 | hook.rs | Byte budget for stdout injection output (~350 tokens at 4 bytes/token) |
| `SIMILARITY_FLOOR` | 0.5 | uds_listener.rs | Minimum cosine similarity for injection candidates |
| `CONFIDENCE_FLOOR` | 0.3 | uds_listener.rs | Minimum confidence score for injection candidates |
| `INJECTION_K` | 5 | uds_listener.rs | Maximum number of entries to search for |
| `EF_SEARCH` | 32 | uds_listener.rs | HNSW expansion factor (mirrors tools.rs) |

## User Workflows

### Primary Flow: Prompt Enrichment

1. User types a prompt in Claude Code (e.g., "implement the search pipeline extraction")
2. Claude Code fires UserPromptSubmit hook before processing the prompt
3. Hook process reads stdin JSON, extracts `prompt` field
4. Hook sends ContextSearch to server via UDS
5. Server embeds prompt, searches HNSW, re-ranks by confidence, applies co-access boost
6. Server filters by similarity floor and confidence floor
7. Server returns matched entries as HookResponse::Entries
8. Hook formats entries as plain text, prints to stdout
9. Claude Code adds the stdout text to Claude's context alongside the user's prompt
10. Claude sees relevant knowledge (ADRs, conventions, patterns) when processing the prompt

### Degraded Flow: Server Unavailable

1. User types a prompt
2. Claude Code fires UserPromptSubmit hook
3. Hook attempts UDS connect, fails (server not running)
4. Hook exits 0 with no stdout output
5. Claude processes the prompt without knowledge injection (same as pre-col-007 behavior)

### Warming Flow: Session Start

1. User starts a Claude Code session
2. Claude Code fires SessionStart hook
3. Hook sends SessionRegister via UDS (fire-and-forget, exits immediately)
4. Server receives SessionRegister, awaits embed model loading
5. Server runs warmup embedding to initialize ONNX runtime
6. Server is ready for ContextSearch requests

## Constraints

### From SCOPE.md

- redb exclusive file lock: all data access through IPC
- 50ms latency budget for hot path
- Zero regression on existing MCP tools
- Single binary (unimatrix-server)
- Edition 2024, MSRV 1.89
- Linux + macOS only (UDS transport)

### Additional Specification Constraints

- No injection recording (deferred to col-010)
- Token budget defined in bytes (MAX_INJECTION_BYTES = 1400), not tokens
- Similarity and confidence floors are compile-time constants (tunable via recompile, not runtime config)
- Co-access dedup is in-memory only (no persistence across server restarts)

## Dependencies

| Dependency | Type | Used For |
|-----------|------|----------|
| `unimatrix-engine` (wire, coaccess, confidence) | Internal crate | Wire protocol types, co-access pair generation, re-rank scoring |
| `unimatrix-core` (AsyncEntryStore, AsyncVectorStore, EmbedService) | Internal crate | Entry/vector operations, embedding |
| `unimatrix-store` (Store) | Internal crate | Co-access recording |
| `unimatrix-adapt` (AdaptationService) | Internal crate | MicroLoRA embedding adaptation |
| `unimatrix-embed` (l2_normalized) | Internal crate | Embedding normalization |
| `unimatrix-server` (EmbedServiceHandle) | Internal (same crate) | Lazy-loading embed service access |

No new external crate dependencies.

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
