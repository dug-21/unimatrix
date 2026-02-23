## ADR-006: Lazy Embedding Model Initialization

### Context

`OnnxProvider::new()` downloads the embedding model from HuggingFace Hub on first use (~30+ seconds). The MCP protocol requires a timely response to the `initialize` handshake. Blocking MCP init on model download creates a poor first-run experience.

Options:
1. **Block on init**: Download model before starting MCP. Simple but 30+ second delay before any tool calls work.
2. **Lazy-load in background**: Start MCP immediately, load model asynchronously. Reads work immediately; embedding-dependent operations return "initializing" until ready.
3. **Pre-download via CLI**: Require `unimatrix download-model` before first use. Extra setup step.

### Decision

Lazy-load the embedding model in a background tokio task. The `EmbedServiceHandle` wraps the async initialization:

- Initial state: `Loading`
- Background task: `tokio::spawn(async { OnnxProvider::new(config) })`
- On success: transition to `Ready(adapter)`
- On failure: transition to `Failed(error_message)`

Tool handlers check the state:
- `context_lookup` and `context_get`: do not require embeddings, work immediately
- `context_search`: requires embeddings, returns structured error "Embedding model is initializing. Try again in a few seconds, or use context_lookup which does not require embeddings." with error code -32004
- `context_store`: if embedding is needed for near-duplicate detection, degrade gracefully (store without dedup check, log warning)

### Consequences

- **Easier:** MCP server starts instantly. Read-only operations work from second zero. Agents get actionable error messages explaining why search isn't available yet.
- **Easier:** No extra CLI step for model download. First `context_search` call after model loads works transparently.
- **Harder:** Adds state machine complexity to the embed service wrapper. Tool handlers must check readiness.
- **Harder:** First-run experience still has a "search not available yet" window. Mitigated by: the error message explains the situation and suggests alternatives.
