# Component: redirect_loop

## Purpose

The redirect loop is a new block (~20 lines) inserted into the `context_correct`
handler in `tools.rs`. It runs after Phase B declared-edge writes (step 8b) and
before confidence recompute (step 9). It queries incoming edges on the deprecated
original entry, validates each source, calls `redirect_graph_edge` per edge, and
accumulates counts into a `RedirectSummary` consumed by `response_format`.

## File Location

`crates/unimatrix-server/src/mcp/tools.rs`

Inserted as step 8c, immediately after the closing brace of the Phase B block
(line ~1077 in the current file). No new helper function required unless the
handler total exceeds 500 lines after insertion (NFR-07); the current handler is
~145 lines plus ~20 lines = ~165 lines, well within the limit.

## Constant: REDIRECT_CEILING

Define at module level (top of `tools.rs`) alongside other module-level constants:

```
/// Maximum incoming edges to auto-redirect per context_correct call (SR-01 ceiling).
/// Entries with more than this many incoming edges emit tracing::warn! and redirect
/// only the first REDIRECT_CEILING rows. See ADR-004 vnc-017.
const REDIRECT_CEILING: usize = 50;
```

## Insertion Point

Current `context_correct` handler step sequence (relevant excerpt):

```
// 8b. Phase B — edge writes on corrected entry (vnc-015).
if !correct_edges_slice.is_empty() {
    ...
    if let Err(e) = validate_and_write_edges(...).await {
        return Err(rmcp::ErrorData::invalid_params(e.to_string(), None));
    }
}

// [INSERT STEP 8c HERE]

// 9. Confidence for both entries (fire-and-forget, via ConfidenceService)
self.services.confidence.recompute(&[...]);

// 10. Format response
Ok(format_correct_success(...))
```

## Step 8c Pseudocode

```
// 8c. Auto-redirect incoming edges (vnc-017).
//
// Query all graph_edges rows pointing at the deprecated original entry
// (Supersedes excluded at SQL level — ADR-002).
// NOTE: read_pool() and write_pool_server() currently alias the same pool
// (db.rs:294). Use canonical accessor names per C-07 vnc-017.
let redirect_summary: Option<RedirectSummary> =
    match self.entry_store.query_incoming_edges(original_id).await {

        Err(e) => {
            // query_incoming_edges SQL failure: log warn, skip loop entirely.
            // Correction has already committed; do not propagate this error.
            tracing::warn!(
                entry_id = original_id,
                error = %e,
                "vnc-017: query_incoming_edges failed; skipping auto-redirect"
            );
            None
        }

        Ok(incoming) if incoming.is_empty() => {
            // Zero-edge path: no summary, no log (FR-13, ADR-004).
            None
        }

        Ok(incoming) => {
            // One or more non-Supersedes incoming edges found.

            // --- Ceiling check (ADR-004 SR-01) ---
            let total_raw = incoming.len();
            let truncated = total_raw > REDIRECT_CEILING;
            if truncated {
                tracing::warn!(
                    entry_id = original_id,
                    total_found = total_raw,
                    ceiling = REDIRECT_CEILING,
                    "vnc-017: incoming edge fan-in exceeds ceiling; \
                     redirecting only first {} of {} edges",
                    REDIRECT_CEILING,
                    total_raw
                );
            }
            // Take only up to REDIRECT_CEILING rows.
            let edges_to_process = if truncated {
                &incoming[..REDIRECT_CEILING]
            } else {
                &incoming[..]
            };

            // --- Per-edge loop ---
            let mut skipped: usize = 0;
            let mut redirected: usize = 0;
            let mut failed: usize = 0;

            // new_entry_id is always correct_result.corrected_entry.id —
            // terminal-active by definition (ADR-001 vnc-017). No find_terminal_active.
            // No TypedGraphState read lock (NFR-05).
            let new_entry_id = correct_result.corrected_entry.id;

            for edge in edges_to_process {
                // Source-validation guard (FR-06, ADR-003 SR-06).
                // Skip Quarantined or Deprecated sources without incrementing failed.
                match self.entry_store.get(edge.source_id).await {
                    Ok(src) if src.status == Status::Quarantined
                               || src.status == Status::Deprecated => {
                        tracing::warn!(
                            source_id = edge.source_id,
                            relation_type = %edge.relation_type,
                            status = ?src.status,
                            "vnc-017: skipping incoming edge from invalid source \
                             (quarantined or deprecated)"
                        );
                        skipped += 1;
                        continue;
                    }
                    Ok(_) => {
                        // Source is Active (or other valid status): proceed.
                    }
                    Err(e) => {
                        // Source lookup failed (entry deleted? pool error?).
                        // Treat as skipped (source unverifiable) — do not count as failed.
                        tracing::warn!(
                            source_id = edge.source_id,
                            error = %e,
                            "vnc-017: source entry lookup failed; skipping edge"
                        );
                        skipped += 1;
                        continue;
                    }
                }

                // Call redirect_graph_edge — one RAII transaction per edge (NFR-03).
                // Caller contract satisfied: new_entry_id is a freshly inserted Active entry.
                // No re-validation of new_entry_id needed (ADR-001, ADR-003).
                match crate::mcp::edge_write::redirect_graph_edge(
                    &self.entry_store,
                    edge.source_id,
                    original_id,
                    new_entry_id,
                    &edge.relation_type,
                    edge.created_at,
                )
                .await
                {
                    Ok(()) => {
                        // Covers both: physical move AND UNIQUE-conflict idempotent ignore.
                        // Both count as success (ADR-003 return contract table).
                        redirected += 1;
                    }
                    Err(e) => {
                        // SQL infrastructure failure — warn and continue (ADR-003).
                        tracing::warn!(
                            source_id = edge.source_id,
                            target_old = original_id,
                            target_new = new_entry_id,
                            relation_type = %edge.relation_type,
                            error = %e,
                            "vnc-017: redirect_graph_edge failed; edge left pointing at \
                             deprecated entry"
                        );
                        failed += 1;
                    }
                }
            }

            // Summary info log (FR-09) — emitted once after loop, only when found > 0.
            let found = edges_to_process.len();
            tracing::info!(
                entry_id = original_id,
                new_entry_id = new_entry_id,
                found = found,
                redirected = redirected,
                skipped = skipped,
                failed = failed,
                truncated = truncated,
                total_raw = total_raw,
                "vnc-017: auto-redirect loop complete"
            );

            Some(RedirectSummary {
                found,
                skipped,
                redirected,
                failed,
                truncated,
                total_raw,
            })
        }
    };
```

## State Machine: Per-Edge Disposition

Each `IncomingEdgeRow` in `edges_to_process` follows exactly one path:

```
IncomingEdgeRow
    │
    ├─ [store.get(source_id) → Quarantined or Deprecated]
    │     → warn!, skipped++, continue
    │
    ├─ [store.get(source_id) → Err(_)]
    │     → warn!, skipped++, continue    (source unverifiable — treated same as invalid)
    │
    └─ [store.get(source_id) → Ok(Active or other valid)]
          │
          ├─ redirect_graph_edge → Ok(())
          │     → redirected++
          │
          └─ redirect_graph_edge → Err(EdgeRedirectError)
                → warn!, failed++
```

Skipped edges are never dispatched to `redirect_graph_edge`. Failed edges are
edges where the store write itself errored. The distinction matters for the
response text (FR-10, AC-17).

## Data Flow

- Inputs from calling scope:
  - `original_id: u64` — the deprecated entry (captured earlier in the handler)
  - `correct_result.corrected_entry.id: u64` — the new active entry
  - `self.entry_store: &Store` — the store reference already held by the handler
- Side effects: up to `REDIRECT_CEILING` calls to `redirect_graph_edge`, each
  opening and committing its own `write_pool_server()` RAII transaction
- Output: `Option<RedirectSummary>` consumed by the response_format step

## Error Handling

| Condition | Behavior |
|-----------|----------|
| `query_incoming_edges` returns `Err` | Log warn; `redirect_summary = None`; handler continues |
| `query_incoming_edges` returns `Ok(empty)` | Skip loop; `redirect_summary = None` |
| `query_incoming_edges` returns `Ok(>50 rows)` | Warn, truncate to first 50; process truncated slice |
| Source `store.get` returns `Quarantined` or `Deprecated` | Log warn; `skipped++`; skip edge |
| Source `store.get` returns `Err` | Log warn; `skipped++`; skip edge |
| `redirect_graph_edge` returns `Ok(())` | `redirected++` (covers both insert and conflict) |
| `redirect_graph_edge` returns `Err(e)` | Log warn; `failed++`; continue |

The outer handler (`context_correct`) never returns `Err` due to redirect failures.
The correction itself is the atomic success unit (C-01).

## Key Test Scenarios

**AC-04 — Partial failure does not abort correction (unit test)**
- Inject a failing `redirect_graph_edge` stub for one edge
- Assert handler returns well-formed success response
- Assert `deprecated_original` and `corrected_entry` fields are present and correct
- Assert `failed == 1`

**AC-08 — Quarantined source skipped, not failed (unit test)**
- Seed incoming edge where source has `status = Quarantined`
- Call redirect loop
- Assert: edge row in `graph_edges` unchanged
- Assert: `skipped == 1`, `failed == 0`
- Assert: `tracing::warn!` with source_id and status present in logs

**AC-08 variant — Deprecated source (unit test)**
- Repeat AC-08 with `status = Deprecated`
- Same assertions

**AC-09 — UNIQUE conflict counts as success (unit test)**
- Pre-insert `C → B` in `graph_edges`
- Call redirect loop for `C → A → B`
- Assert: `redirect_graph_edge` returns `Ok(())`; `redirected == 1`, `failed == 0`
- Assert: no `tracing::warn!` emitted

**R-05 — Ceiling truncation (unit test)**
- Seed 55 incoming edges in deterministic order
- Call redirect loop
- Assert: `tracing::warn!` emitted with `total_found=55`
- Assert: exactly 50 redirects attempted
- Assert: `truncated == true`, `total_raw == 55`
- Assert: 5 unredirected edges still have `target_id = original_id` in `graph_edges`

**AC-14 — End-to-end redirect (unit test, in-memory SQLite)**
- Seed edge row pointing at `original_id`
- Call `context_correct` (not the loop in isolation)
- Assert: edge row now has `target_id = new_entry.id`

**R-06 / AC-07 — Contradicts bidirectional success (integration test)**
- Seed `C → A` (Contradicts) where C is Active
- Call `context_correct(A → B)`
- Assert: `C → B` and `B → C` both exist in `graph_edges` post-redirect

**R-06 variant — Mixed Contradicts (Active + Quarantined sources)**
- Seed one valid Contradicts (Active source) + one invalid Contradicts (Quarantined source)
- Assert: valid one redirects (both directions); invalid one skipped (skipped++, failed=0)

**query_incoming_edges Err path (unit test)**
- Inject a pool error on `query_incoming_edges`
- Assert: handler returns success response
- Assert: `tracing::warn!` with error emitted
- Assert: `redirect_summary == None` (no text appended)
