# vnc-017 Pseudocode Overview

## Components Involved

| Component | File | Action |
|-----------|------|--------|
| `query_incoming_edges` | `crates/unimatrix-store/src/read.rs` | New function + struct |
| `redirect_loop` | `crates/unimatrix-server/src/mcp/tools.rs` | New block in `context_correct` handler (step 8c) |
| `response_format` | `crates/unimatrix-server/src/mcp/tools.rs` (append at step 10) | Conditional text append to `CallToolResult` |

`response/entries.rs` (`format_correct_success`) is NOT modified. The redirect summary is appended post-call in `tools.rs` — the preferred minimal-impact approach from the architecture.

## Data Flow Between Components

```
context_correct handler (tools.rs)
  │
  │  [step 8: correct_entry commits → correct_result.corrected_entry.id is the new active ID]
  │
  ├─► query_incoming_edges(original_id: u64)
  │     IN:  original_id (the entry being deprecated)
  │     OUT: Vec<IncomingEdgeRow> { source_id, relation_type, created_at }
  │          Supersedes rows excluded at SQL level (ADR-002)
  │
  ├─► redirect_loop block
  │     IN:  Vec<IncomingEdgeRow>, new_entry_id (= correct_result.corrected_entry.id)
  │     SIDE EFFECT: calls redirect_graph_edge per row (write_pool_server)
  │     OUT: RedirectSummary { found, skipped, redirected, failed, truncated, total_raw }
  │
  └─► response_format append
        IN:  RedirectSummary, CallToolResult from format_correct_success
        OUT: CallToolResult with optional redirect text appended to first Content item
```

## Shared Types (New)

### IncomingEdgeRow (in `unimatrix-store/src/read.rs`)

```
pub struct IncomingEdgeRow {
    pub source_id:     u64,
    pub relation_type: String,
    pub created_at:    u64,
}
```

`target_id` is implicit (the queried entry) and excluded from the struct.

### RedirectSummary (inline struct in `tools.rs` — not exported)

```
struct RedirectSummary {
    found:      usize,  // non-Supersedes rows returned (after ceiling cap)
    skipped:    usize,  // edges whose source was Quarantined or Deprecated
    redirected: usize,  // edges where redirect_graph_edge returned Ok(())
    failed:     usize,  // edges where redirect_graph_edge returned Err(_)
    truncated:  bool,   // true when raw row count exceeded REDIRECT_CEILING
    total_raw:  usize,  // raw count before truncation (for truncation message)
}
```

`RedirectSummary` may be an anonymous struct, a tuple, or a named struct — the
implementer's choice. The field names above are the logical names used in this
pseudocode.

### REDIRECT_CEILING (constant in `tools.rs`)

```
/// Maximum incoming edges to auto-redirect per context_correct call (SR-01 ceiling).
/// Entries with more than this many incoming edges emit tracing::warn! and redirect
/// only the first REDIRECT_CEILING rows. See ADR-004 vnc-017.
const REDIRECT_CEILING: usize = 50;
```

## Sequencing Constraints

1. `query_incoming_edges` must be built before `redirect_loop` can reference it.
   No other ordering constraint — `redirect_graph_edge` (used by `redirect_loop`)
   already exists in `edge_write.rs` and must not be modified.

2. `redirect_loop` must run AFTER `correct_result` is obtained (step 8) and AFTER
   Phase B declared-edge writes (step 8b). It runs BEFORE confidence recompute (step 9).

3. `response_format` appends to the `CallToolResult` returned by `format_correct_success`.
   It runs at step 10, after the redirect loop has completed and `RedirectSummary` is built.

## Existing Symbols Consumed (No Changes)

| Symbol | Crate / File | How Used |
|--------|-------------|----------|
| `Store::get(id)` | `unimatrix-store` | Source-status validation per edge in redirect_loop |
| `redirect_graph_edge(store, src, old, new, rel, ts)` | `edge_write.rs` | One call per validated incoming edge |
| `EdgeRedirectError` | `edge_write.rs` | Match arm on `Err` branch in redirect_loop |
| `Status::Quarantined`, `Status::Deprecated` | `unimatrix-core` | Source-validation guard |
| `format_correct_success(orig, corr, fmt)` | `response/entries.rs` | Called first; result mutated post-call |
| `CallToolResult` content vec | rmcp | `content[0]` text field extended for redirect summary |

## Integration Surface (Architecture §)

| Integration Point | Type | File |
|-------------------|------|------|
| `query_incoming_edges` | `async fn(&self, target_id: u64) -> Result<Vec<IncomingEdgeRow>>` | `read.rs` (new) |
| `IncomingEdgeRow` | `{ source_id: u64, relation_type: String, created_at: u64 }` | `read.rs` (new) |
| Redirect summary text append | appended to `CallToolResult.content[0].text` when `found > 0` | `tools.rs` (new) |
| `REDIRECT_CEILING` | `const usize = 50` | `tools.rs` (new) |
