# vnc-017: Auto-Redirect Incoming Edges on context_correct — Architecture

## System Overview

When `context_correct` supersedes entry A with entry B, any third-party entry C that
previously declared an edge `C → A` now holds a stale edge pointing at a deprecated
entry. Left unaddressed, these stale edges cause false-positive `DependencyOnDeprecated`
detections (vnc-016 rule 23) and degrade PPR traversal quality.

vnc-017 makes `context_correct` automatically redirect all such incoming edges as part
of the same MCP call, closing the gap between what the correction semantics imply and
what the graph state reflects.

The feature adds one new store-layer read function, one new redirect loop inserted into
the existing `context_correct` handler, and modifies the response format to report the
redirect count. No new modules, no new tools, no protocol changes.

## Component Breakdown

### 1. `query_incoming_edges` (new — `unimatrix-store/src/read.rs`)

A read-only function on the `Store` type that returns all `graph_edges` rows whose
`target_id` equals a given entry ID. Uses `read_pool()` and the existing
`idx_graph_edges_target_id` index (created in migration v12→v13). Excludes `Supersedes`
rows at the SQL level (see ADR-002 and the explanatory comment requirement).

Signature:
```rust
pub async fn query_incoming_edges(
    &self,
    target_id: u64,
) -> Result<Vec<IncomingEdgeRow>>
```

Return type:
```rust
pub struct IncomingEdgeRow {
    pub source_id:     u64,
    pub relation_type: String,
    pub created_at:    u64,
}
```

The `Supersedes` exclusion is applied in the SQL `WHERE` clause, not in the loop, to
avoid fetching rows that will always be discarded (SR-04).

### 2. Redirect loop (modified — `crates/unimatrix-server/src/mcp/tools.rs`)

Inserted in `context_correct`, after Phase B edge writes (line ~1077) and before
confidence recompute (step 9). The loop:

1. Calls `query_incoming_edges(original_id)` — returns zero or more `IncomingEdgeRow`s.
2. If zero rows: skip entirely; response is unchanged (AC-08).
3. For each row: applies source-validation guard (see ADR-003 below).
4. For validated rows: calls `redirect_graph_edge(store, source_id, original_id,
   new_entry_id, relation_type, created_at)`.
5. Accumulates counts: total found, skipped (invalid source), redirected (Ok), failed (Err).
6. Logs a summary with `tracing::info!` after the loop.
7. Appends redirect summary to response text only when count > 0 (SR-05).

The new entry's ID (`correct_result.corrected_entry.id`) is used directly as the
redirect target — it is always terminal-active by definition (see ADR-001).

### 3. `redirect_graph_edge` (existing — `crates/unimatrix-server/src/mcp/edge_write.rs`)

No changes. Used as-is. The function opens its own RAII `sqlx::Transaction`, handles
`Contradicts` bidirectionality atomically (4 rows), and returns `Ok(())` on success or
`Err(EdgeRedirectError)` on SQL failure. The caller contract requires the caller to
validate the new target before calling — the target is `correct_result.corrected_entry.id`
(freshly inserted, active, non-quarantined by construction; no validation call needed).

### 4. `format_correct_success` (modified — `crates/unimatrix-server/src/mcp/response/entries.rs`)

The function gains an optional `redirected_count: Option<RedirectSummary>` parameter
(or the redirect summary is appended to the text before passing to the formatter).

Two acceptable approaches:
- Pass a `RedirectSummary { found: usize, skipped: usize, redirected: usize, failed: usize }`
  to `format_correct_success` and let it append to each format variant.
- Append to the `CallToolResult` text after `format_correct_success` returns.

The second approach requires no change to the formatter signature and is preferred for
minimal impact. The implementer may choose either; both are acceptable.

Response text appended only when `found > 0`:
```
Redirected N incoming edges (M failed, see logs)
```
or when skipped > 0:
```
Redirected N incoming edges (K skipped — invalid source, M failed, see logs)
```

## Component Interactions

```
context_correct handler (tools.rs)
  │
  ├─► StoreService.correct()          [Step 8 — atomic deprecate+insert]
  │     └─► store.correct_entry()     [write_pool, returns deprecated_original + new_correction]
  │
  ├─► validate_and_write_edges()      [Step 8b Phase B — declared edges on new entry]
  │
  ├─► [NEW] query_incoming_edges(original_id)   [read_pool — idx_graph_edges_target_id]
  │     └─► IncomingEdgeRow: source_id, relation_type, created_at
  │
  ├─► [NEW] redirect loop
  │     ├─ source_validation_guard()  [store.get(source_id) — read_pool]
  │     └─► redirect_graph_edge()     [write_pool_server — RAII txn per edge]
  │
  ├─► services.confidence.recompute() [Step 9 — fire-and-forget]
  │
  └─► format_correct_success()        [Step 10 — with optional redirect summary]
```

## Technology Decisions

See ADR files in this directory.

| Decision | ADR |
|----------|-----|
| Terminal-active resolution: always use new_entry.id directly | ADR-001 |
| Supersedes exclusion: at SQL level, not loop level | ADR-002 |
| Failure posture for redirect errors: warn+continue (ADR-003 posture) | ADR-003 |
| SR-06 source-validation: skip-with-warn for quarantined/deprecated sources | ADR-003 |
| SR-02 return contract: Ok(false) treated as success, not warning | ADR-003 |
| SR-01 fan-in ceiling: warn+truncate at N=50 | ADR-004 |
| SR-05 zero-edge response: omit redirect line entirely | ADR-004 |

## Integration Points

- `unimatrix-store/src/read.rs` — `query_incoming_edges` added (store layer)
- `unimatrix-server/src/mcp/tools.rs` — `context_correct` handler extended (~20 lines)
- `unimatrix-server/src/mcp/edge_write.rs` — `redirect_graph_edge` called; no changes
- `unimatrix-server/src/mcp/response/entries.rs` — `format_correct_success` may receive
  redirect summary (or text is appended post-call; implementer's choice)

## Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|-----------------|--------|
| `query_incoming_edges` | `async fn(&self, target_id: u64) -> Result<Vec<IncomingEdgeRow>>` | `unimatrix-store/src/read.rs` (new) |
| `IncomingEdgeRow` | `{ source_id: u64, relation_type: String, created_at: u64 }` | `unimatrix-store/src/read.rs` (new) |
| `redirect_graph_edge` | `async fn(store: &Store, source_id: u64, old_target_id: u64, new_target_id: u64, relation_type: &str, created_at: u64) -> Result<(), EdgeRedirectError>` | `edge_write.rs` (existing) |
| `EdgeRedirectError` | `TargetNotFound { target_id }` / `TargetQuarantined { target_id }` / `TransactionError(sqlx::Error)` | `edge_write.rs` (existing) |
| `format_correct_success` | `fn(original: &EntryRecord, correction: &EntryRecord, format: ResponseFormat) -> CallToolResult` | `response/entries.rs` (existing, unchanged or lightly extended) |
| Redirect summary text | `"Redirected N incoming edges (M failed, see logs)"` — appended to response when found > 0 | `tools.rs` (new logic) |

### query_incoming_edges SQL

```sql
SELECT source_id, relation_type, created_at
FROM graph_edges
WHERE target_id = ?1
  AND relation_type != 'Supersedes'
  -- Supersedes rows are derived from entries.supersedes; redirecting them would assert
  -- incorrect semantic claims (e.g. C supersedes B when only C superseded A). They are
  -- rebuilt by the graph tick automatically on the next cycle. OQ-03.
```

## Execution Order within context_correct

```
Step 1-7:  [unchanged] identity, validation, entry fetch, Phase A edge validation
Step 8:    [unchanged] StoreService.correct() — atomic deprecate + insert
Step 8b:   [unchanged] Phase B — validate_and_write_edges() for declared edges
Step 8c:   [NEW]       auto-redirect loop
             8c-1: query_incoming_edges(original_id)
             8c-2: if found == 0, skip
             8c-3: if found > REDIRECT_CEILING (50), warn + truncate to first 50
             8c-4: for each edge row:
                     - if source entry is quarantined or deprecated: tracing::warn!, skip
                     - call redirect_graph_edge(store, source_id, original_id,
                                                new_entry_id, relation_type, created_at)
                     - Ok(())     -> redirected++
                     - Err(e)     -> tracing::warn!(e), failed++
             8c-5: tracing::info! summary (found, skipped, redirected, failed)
             8c-6: if found > 0, build redirect_summary string
Step 9:    [unchanged] confidence.recompute()
Step 10:   [modified]  format_correct_success + optional redirect_summary append
```

## Key Constraints

1. `redirect_graph_edge` caller contract: validate new target before calling. The new
   target is `correct_result.corrected_entry.id` — freshly inserted Active entry by
   construction. No validation call required.

2. `redirect_graph_edge` returns `Result<(), EdgeRedirectError>`, not `Result<bool, _>`.
   The UNIQUE-conflict idempotency is handled by `INSERT OR IGNORE` inside the function;
   the caller never receives a conflict signal — only `Ok(())` or `Err`. SR-02 applies
   to `write_graph_edge` (which returns `bool`), not to `redirect_graph_edge`. The loop
   handles: `Ok(())` = success, `Err(_)` = warn+continue.

3. No transaction wraps the full correction + redirect operation. Each redirect call
   opens and commits its own transaction (RAII per lesson #2269). ADR-003 partial-write
   posture is mandatory.

4. `write_pool_server()` and `write_pool` share the same underlying pool (`db.rs:294`).
   Use canonical accessor names; add a comment citing this implementation detail.

5. `read.rs` is 3,465 lines. Adding `query_incoming_edges` (~15 lines) does not trigger
   a module split.

6. `tools.rs` context_correct handler is 145 lines. Adding ~20 lines for the redirect
   block stays well under 500. No extraction to a helper required unless the implementer
   prefers it.

## Open Questions

None. All OQs from SCOPE.md are resolved. SR-06 is resolved by ADR-003 (skip-with-warn).
SR-02 is resolved by the `redirect_graph_edge` return contract clarification in the
Integration Surface section above and in ADR-003. SR-01 is resolved by ADR-004 (ceiling
at N=50). SR-05 is resolved by ADR-004 (omit redirect line when found=0).
