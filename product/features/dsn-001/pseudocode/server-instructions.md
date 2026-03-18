# Pseudocode: server-instructions

**File**: `crates/unimatrix-server/src/server.rs` (modified)

## Purpose

Removes the hardcoded `const SERVER_INSTRUCTIONS: &str = "..."` and instead uses
`config.server.instructions` (an `Option<String>`) passed as a parameter to
`UnimatrixServer::new`. When `None`, falls back to a renamed private compiled
default. Three doc comments that reference `context_retrospective` are updated
to `context_cycle_review` (covered in the SR-05 checklist, part of tool-rename.md).

---

## Existing State (Pre-dsn-001)

`server.rs` currently has:
```
const SERVER_INSTRUCTIONS: &str = "Unimatrix is this project's knowledge engine. \
    Before starting implementation, architecture, or design tasks, search for relevant \
    patterns and conventions using the context tools. Apply what you find. After discovering \
    reusable patterns or making architectural decisions, store them for future reference. \
    Do not store workflow state or process steps.";
```

Used in `UnimatrixServer::new`:
```
instructions: Some(SERVER_INSTRUCTIONS.to_string()),
```

---

## Constant Rename

```
// REMOVE:
const SERVER_INSTRUCTIONS: &str = "...";

// ADD (private, backing value for Option::None path):
// Renamed to make clear this is only the default fallback.
// The public interface is config.server.instructions.
const SERVER_INSTRUCTIONS_DEFAULT: &str = "Unimatrix is this project's knowledge engine. \
    Before starting implementation, architecture, or design tasks, search for relevant \
    patterns and conventions using the context tools. Apply what you find. After discovering \
    reusable patterns or making architectural decisions, store them for future reference. \
    Do not store workflow state or process steps.";
```

---

## Constructor Change

```
// BEFORE:
pub fn new(
    entry_store: Arc<Store>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    embed_service: Arc<EmbedServiceHandle>,
    registry: Arc<AgentRegistry>,
    audit: Arc<AuditLog>,
    categories: Arc<CategoryAllowlist>,
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    adapt_service: Arc<AdaptationService>,
) -> Self

// AFTER: add instructions parameter
pub fn new(
    entry_store: Arc<Store>,
    vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    embed_service: Arc<EmbedServiceHandle>,
    registry: Arc<AgentRegistry>,
    audit: Arc<AuditLog>,
    categories: Arc<CategoryAllowlist>,
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    adapt_service: Arc<AdaptationService>,
    instructions: Option<String>,   // NEW: from config.server.instructions
) -> Self

BODY (only the changed ServerInfo construction):
    let server_info = ServerInfo {
        server_info: Implementation {
            name: SERVER_NAME.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            ..Default::default()
        },
        capabilities: ServerCapabilities::builder().enable_tools().build(),
        // Use config-supplied instructions when present; fall back to compiled default.
        // None means "not configured" — use the developer-authored default.
        instructions: Some(
            instructions.unwrap_or_else(|| SERVER_INSTRUCTIONS_DEFAULT.to_string())
        ),
        ..Default::default()
    };
```

---

## Doc Comment Updates (SR-05 Checklist Items)

Three doc comments in `server.rs` reference `context_retrospective`. These are
updated as part of the tool-rename component. Listed here for completeness:

```
// Line ~65 (the context_cycle accumulator field doc):
// BEFORE: /// context_retrospective handler (drains on call).
// AFTER:  /// context_cycle_review handler (drains on call).

// Line ~147 (feature bucket eviction comment):
// BEFORE: /// features that complete without calling context_retrospective or context_cycle.
// AFTER:  /// features that complete without calling context_cycle_review or context_cycle.

// Line ~207 (PendingEntriesAnalysis doc):
// BEFORE: /// Shared with UDS listener; drained by context_retrospective handler.
// AFTER:  /// Shared with UDS listener; drained by context_cycle_review handler.
```

These changes are coordinated with `tool-rename.md`.

---

## Test Impact

The existing test `test_get_info_instructions` in `server.rs`:
```
async fn test_get_info_instructions() {
    let server = make_server().await;
    let info = rmcp::ServerHandler::get_info(&server);
    assert!(info.instructions.is_some());
    let instructions = info.instructions.unwrap();
    assert!(instructions.contains("knowledge engine"));
    assert!(instructions.contains("search for relevant patterns"));
}
```

This test constructs a server with `make_server()`. `make_server()` must be updated to
pass `None` as the `instructions` parameter to `UnimatrixServer::new`. When `None` is
passed, the compiled default is used — so `instructions.contains("knowledge engine")`
and `instructions.contains("search for relevant patterns")` still pass. No change to
test assertions needed (AC-01, SR-07).

---

## Key Test Scenarios

1. **None instructions uses compiled default** (AC-01):
   - `UnimatrixServer::new(..., None)` → `server_info.instructions` contains the default text.
   - Existing `test_get_info_instructions` continues to pass without modification.

2. **Some instructions uses config value** (AC-05):
   - `UnimatrixServer::new(..., Some("Test domain guidance.".to_string()))`.
   - `server_info.instructions` == `Some("Test domain guidance.")`.
   - Integration test: server started with `instructions = "Test domain guidance."` in config;
     assert that string appears in the MCP `initialize` response.

---

## Error Handling

`UnimatrixServer::new` is infallible. The `instructions` parameter is a plain
`Option<String>` — no validation at construction time. Validation (length cap and
injection scan) is performed in `validate_config` at config load time, before
`UnimatrixServer::new` is called.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` — no patterns found specific to server instructions externalization.
- Deviations from established patterns: none.
