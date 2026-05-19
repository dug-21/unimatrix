# Agent Report: vnc-017-agent-1-pseudocode

## Task

Produce per-component pseudocode files for vnc-017 (Auto-Redirect Incoming Edges
on context_correct): three components — query_incoming_edges, redirect_loop,
response_format.

## Deliverables

| File | Status |
|------|--------|
| `product/features/vnc-017/pseudocode/OVERVIEW.md` | Complete |
| `product/features/vnc-017/pseudocode/query_incoming_edges.md` | Complete |
| `product/features/vnc-017/pseudocode/redirect_loop.md` | Complete |
| `product/features/vnc-017/pseudocode/response_format.md` | Complete |

## Components Covered

1. **query_incoming_edges** — new `pub async fn` on `Store` in `read.rs`; new
   `IncomingEdgeRow` struct. SQL excludes Supersedes at query level (ADR-002).
   Uses `read_pool()` with C-07 comment requirement.

2. **redirect_loop** — new step 8c block in `context_correct` handler in `tools.rs`.
   Defines `REDIRECT_CEILING = 50`. Handles: empty path, truncation path,
   source-validation guard (Quarantined/Deprecated → skipped++), redirect call
   (`Ok(()) → redirected++`, `Err → warn+failed++`), info summary log.

3. **response_format** — replaces the current single-line `Ok(format_correct_success(...))`
   at step 10 with a two-step pattern: build result, conditionally append redirect
   summary text. Four-variant format table per SPEC FR-10 + AC-17.

## Open Questions / Gaps Found

**OQ-1: `content.raw.text` mutation path**
The pseudocode for `response_format` shows `content.raw.text.push_str(...)` but the
exact field path into `Content`'s internal struct depends on the rmcp 0.16.0 API.
The implementer must inspect `rmcp::model::Content` to confirm the mutation path
(or use the `as_text()` reconstruction alternative documented in the pseudocode).
This is a known minor gap — it cannot be resolved from source inspection alone
without compiling.

**OQ-2: `store.get` return on source-lookup failure**
The redirect loop pseudocode skips (skipped++) when `store.get(source_id)` returns
`Err`, treating it identically to a Quarantined/Deprecated source. The architecture
and spec only specify skip-with-warn for Quarantined and Deprecated status. The `Err`
path (entry deleted between query and validation — extreme TOCTOU race) is not
explicitly specified. The pseudocode uses skipped++ as the conservative choice.
If the implementer prefers failed++ for the Err path, this is a valid alternative
and the test for AC-08 should be updated accordingly.

No other gaps found. All OQs from SCOPE.md are resolved in the ADRs and
IMPLEMENTATION-BRIEF.md.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search(query='graph edge redirect patterns conventions', category='pattern')` — found entries #4459 (source-validation posture for Contradicts redirect loops), #4458 (context_correct auto-redirect pattern), #4078 (co-access back-fill pattern). #4459 and #4458 directly applied.
- Queried: `mcp__unimatrix__context_search(query='vnc-017 architectural decisions', category='decision', topic='vnc-017')` — found entries #4463 (ADR-004: fan-in ceiling), #4460 (ADR-001: terminal-active), #4461 (ADR-002: Supersedes SQL exclusion). All three applied directly.
- Deviations from established patterns: none. All pseudocode follows the read.rs `query_*` pattern (sqlx::query + read_pool() + .map_err(|e| StoreError::Database(e.into()))), the redirect_loop warn-and-continue pattern from ADR-003, and the `format_correct_success` post-call append approach from the architecture.
