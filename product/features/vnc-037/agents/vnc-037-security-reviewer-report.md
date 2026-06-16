# Security Review: vnc-037-security-reviewer

## Risk Level: low

## Summary
A read-path addition to `context_get` surfacing ranked depth-1 typed edges. All SQL uses positional binds with a single static canonicalization CTE; no input is ever concatenated into SQL. Byte-identity for list-view tools and the `context_graph` neighbors contract are both preserved. No blocking findings; approved for security.

## Findings

### F1 — SQL injection surface (the primary concern): clean
- **Severity**: informational
- **Location**: `crates/unimatrix-store/src/graph_queries_ranked.rs` (CANON_CTE, query_ranked_neighbors, count_neighbors_split); `crates/unimatrix-server/src/mcp/get_edges.rs::fetch_titles_batch`
- **Description**: Ranked select and split count bind the anchor as `?1` and the cap as `?2` (← `GET_EDGE_DISPLAY_LIMIT`, never literal 3). The symmetric-type `IN (...)` set, `!= 'Supersedes'` filter, canonicalization `CASE`, `ORDER BY`, and `LIMIT` are static SQL. The batched title join generates `IN (?, …)` placeholders from arity only and binds each id positionally — no id is string-interpolated, list bounded to ≤cap.
- **Recommendation**: None.
- **Blocking**: no

### F2 — Input validation / deserialization: sound
- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs` (GetParams.include_edges), `graph_queries_*` mappers
- **Description**: New `include_edges: Option<bool>` with `#[serde(default)]`; backward compatible. No `.unwrap()`/`.expect()` on any read path — every sqlx/try_get error maps to `StoreError::Database`. `source` is `NOT NULL DEFAULT ''` so `try_get::<String>` is safe; `entries.confidence` handled as `Option<f64>` with `NULLS LAST` (dangling/LEFT-JOIN miss = None).
- **Recommendation**: None.
- **Blocking**: no

### F3 — Access control: no new boundary
- **Severity**: informational
- **Location**: `context_get` handler (tools.rs ~976) + `build_edges_view`
- **Description**: Single-project-scoped store (1-client:1-project). Edge queries read only `graph_edges`/`entries` within the store the primary `entry_store.get` already authorized. No cross-project leakage introduced.
- **Blocking**: no

## Blast Radius Assessment
Read-only path on the hottest read tool. Fail-loud: a post-primary-read edge/count/title error is mapped identically to the primary read and returned — no degrade, no silent omission. Worst realistic case is `context_get` returning an error instead of partial data: safe (no corruption, no disclosure, no DoS — queries anchored + capped, totals are one 4-scalar aggregate, hub fan-out never materialized). Two defensive no-panic fallbacks (`map_direction`→both, `render_json_edges`→[]) over a closed SQL-emitted direction set are benign.

## Regression Risk
- Byte-identity: `format_single_entry` gained `edges: Option<&EdgesView>`; `None` is structural (key/section/digest never emitted). The other production caller (lookup-by-id, tools.rs:647) passes `None` ⇒ unchanged. Pinned by `test_none_json_byte_identical_to_base_object`.
- Neighbors contract: `RawEdgeRow` gained two additive public fields; plain path sets `target_confidence: None`; only two in-crate construction sites, both updated; `query_ranked_neighbors` never mutates `query_direct_neighbors`.
- #744 inbound-degree integrity preserved (`↔` counted once in `both`, never folded into `inbound`).

## Dependencies & Secrets
No new dependencies (workspace-only). No hardcoded credentials/keys/tokens. Python harness change is additive opt-in plumbing.

## PR Comments
- Posted 1 review comment on PR #764 (`gh pr review --comment`).
- Blocking findings: no.

## Knowledge Stewardship
- Stored: nothing novel to store — Unimatrix MCP is disconnected this session, and the patterns observed (positional binds, arity-only IN-lists, structural None-as-absent seam) are already established conventions in the codebase with no new generalizable anti-pattern surfaced.
