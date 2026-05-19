# vnc-017: Auto-Redirect Incoming Edges on context_correct — Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-017/SCOPE.md |
| Architecture | product/features/vnc-017/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-017/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-017/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-017/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| query_incoming_edges | pseudocode/query_incoming_edges.md | test-plan/query_incoming_edges.md |
| redirect_loop | pseudocode/redirect_loop.md | test-plan/redirect_loop.md |
| response_format | pseudocode/response_format.md | test-plan/response_format.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode and test-plan files are produced in Session 2 Stage 3a. The Component Map lists expected components from the architecture — actual file paths are filled during delivery. The Cross-Cutting Artifacts section tracks files that don't belong to a single component but are consumed by specific stages.

---

## Goal

When `context_correct` supersedes entry A with entry B, all `graph_edges` rows pointing at A become stale and silently cause false-positive `DependencyOnDeprecated` detections and degraded PPR traversal quality. This feature makes `context_correct` automatically redirect all such incoming non-Supersedes edges to the new active entry as part of the same MCP call, using the existing `redirect_graph_edge` infrastructure, with warn-and-continue failure posture so the correction always succeeds even if some redirects fail.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| Terminal-active resolution strategy | Always use `correct_result.corrected_entry.id` directly; never call `find_terminal_active` or acquire a read lock on `TypedGraphState`. `context_correct` can only be called on an Active entry, making the new entry terminal-active by definition at creation time. | SCOPE OQ-05 / ARCHITECTURE | product/features/vnc-017/architecture/ADR-001-terminal-active-resolution.md |
| Supersedes exclusion implementation level | Exclude `Supersedes` rows at the SQL level in `query_incoming_edges` via `AND relation_type != 'Supersedes'` with an explanatory comment. Not at loop level. | SCOPE OQ-03 / ARCHITECTURE | product/features/vnc-017/architecture/ADR-002-supersedes-exclusion-at-sql-level.md |
| Failure posture for redirect errors | Warn-and-continue per ADR-003 partial-write posture. Before calling `redirect_graph_edge`, validate source entry status; skip-with-warn for Quarantined/Deprecated sources (does not increment failure counter). SQL errors log `tracing::warn!` and increment failure counter. Correction always succeeds if `correct_entry` commits. | SCOPE SR-06 / SPEC FR-06–FR-08 | product/features/vnc-017/architecture/ADR-003-redirect-loop-failure-posture.md |
| `redirect_graph_edge` return contract | `Result<(), EdgeRedirectError>`. `Ok(())` covers both the successful insert and the UNIQUE-conflict case (idempotent). `Err(EdgeRedirectError)` covers SQL infrastructure failures only. `redirected++` on `Ok(())`. No `Ok(bool)` variant exists. SPEC FR-07 is correct and matches ADR-003. | ADR-003 / RISK R-01 | product/features/vnc-017/architecture/ADR-003-redirect-loop-failure-posture.md |
| Fan-in ceiling | Truncate and warn at N=50. When `query_incoming_edges` returns more than 50 rows, emit `tracing::warn!` and process only the first 50. Response text uses truncation variant: `"Redirected N incoming edges (truncated from M, see logs)"`. Ceiling constant `REDIRECT_CEILING: usize = 50` defined in `tools.rs`. | SCOPE SR-01 | product/features/vnc-017/architecture/ADR-004-fan-in-ceiling-and-response-text.md |
| Zero-edge response text | Omit the redirect summary line entirely when no non-Supersedes incoming edges are found (`found == 0`). No summary log emitted. Response text identical to current `format_correct_success` output. | SCOPE SR-05 | product/features/vnc-017/architecture/ADR-004-fan-in-ceiling-and-response-text.md |
| `query_incoming_edges` module placement | Add to `read.rs` in `unimatrix-store`. `read.rs` is 3,465 lines; no new module required. The 500-line rule applies to new modules, not additions to existing large files. | SCOPE OQ-04 | product/features/vnc-017/architecture/ADR-002-supersedes-exclusion-at-sql-level.md |

---

## Files to Create/Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-store/src/read.rs` | Modify | Add `query_incoming_edges(target_id: u64)` read function and `IncomingEdgeRow` struct (~15 lines) |
| `crates/unimatrix-server/src/mcp/tools.rs` | Modify | Insert auto-redirect loop block (~20 lines) in `context_correct` handler after Phase B edge writes (step 8b), before confidence recompute (step 9); define `REDIRECT_CEILING` constant |
| `crates/unimatrix-server/src/mcp/response/entries.rs` | Modify (optional) | Extend `format_correct_success` with optional redirect summary, or append text post-call in `tools.rs` — implementer's choice |
| `crates/unimatrix-server/tests/` (or `unimatrix-store/tests/`) | Modify | Add Rust unit tests for `query_incoming_edges` and redirect loop (AC-05, AC-14, AC-03, AC-04, AC-08, AC-09, AC-11, AC-13, R-01, R-05, R-06) |
| `infra-001` Python integration test suite | Modify | Add new test cases for AC-06, AC-07, AC-12, AC-16 |

No new crates, modules, or migration files. No schema changes.

---

## Data Structures

### IncomingEdgeRow (new)

```rust
pub struct IncomingEdgeRow {
    pub source_id:     u64,
    pub relation_type: String,
    pub created_at:    u64,
}
```

Returned by `query_incoming_edges`. `target_id` is implicit (the queried entry) and excluded from the struct.

### RedirectSummary (inline or named struct — implementer's choice)

Accumulator for the redirect loop:

```
found:     usize   // total non-Supersedes incoming edges (capped at REDIRECT_CEILING)
skipped:   usize   // edges whose source was Quarantined or Deprecated
redirected: usize  // edges where redirect_graph_edge returned Ok(())
failed:    usize   // edges where redirect_graph_edge returned Err(_)
truncated: bool    // whether found exceeded REDIRECT_CEILING before capping
total_raw: usize   // raw row count before ceiling truncation (for truncation message)
```

### REDIRECT_CEILING (constant)

```rust
/// Maximum incoming edges to auto-redirect per context_correct call (SR-01 ceiling).
/// Entries with more than this many incoming edges emit tracing::warn! and redirect
/// only the first REDIRECT_CEILING rows. See ADR-004 vnc-017.
const REDIRECT_CEILING: usize = 50;
```

Defined in `tools.rs`.

---

## Function Signatures

### query_incoming_edges (new)

```rust
// In unimatrix-store/src/read.rs, on the Store type:
pub async fn query_incoming_edges(
    &self,
    target_id: u64,
) -> Result<Vec<IncomingEdgeRow>>
```

SQL:

```sql
SELECT source_id, relation_type, created_at
FROM graph_edges
WHERE target_id = ?1
  AND relation_type != 'Supersedes'
  -- Supersedes rows are derived from entries.supersedes; redirecting them would assert
  -- incorrect semantic claims (e.g. C supersedes B when only C superseded A). They are
  -- rebuilt by the graph tick automatically on the next cycle. ADR-002 vnc-017.
```

Uses `read_pool()`. The `idx_graph_edges_target_id` index makes the `WHERE target_id = ?` filter efficient.

### redirect_graph_edge (existing — must not be modified)

```rust
// In unimatrix-server/src/mcp/edge_write.rs:
pub async fn redirect_graph_edge(
    store: &Store,
    source_id: u64,
    old_target_id: u64,
    new_target_id: u64,
    relation_type: &str,
    created_at: u64,
) -> Result<(), EdgeRedirectError>
```

Returns `Ok(())` on success or UNIQUE-conflict (idempotent). Returns `Err(EdgeRedirectError)` on SQL infrastructure failure. The `Contradicts` path atomically handles 4 rows (bidirectional). Caller must validate new target before calling — not required here because `new_entry.id` is a freshly inserted Active entry.

### Redirect loop insertion point in context_correct

Inserted as step 8c in `tools.rs`, after Phase B (`validate_and_write_edges`), before step 9 (confidence recompute):

```
Step 8c-1: let incoming = store.query_incoming_edges(original_id).await?  // or log+skip on Err
Step 8c-2: if incoming.is_empty() { skip }
Step 8c-3: if incoming.len() > REDIRECT_CEILING { warn!, truncate to first 50 }
Step 8c-4: for each IncomingEdgeRow { source_id, relation_type, created_at }:
             - look up store.get(source_id) — if Quarantined or Deprecated: warn!, skipped++, continue
             - redirect_graph_edge(store, source_id, original_id, new_entry_id, relation_type, created_at)
             - Ok(()) -> redirected++
             - Err(e) -> warn!(e), failed++
Step 8c-5: tracing::info! summary (found, skipped, redirected, failed)
Step 8c-6: if found > 0 { build redirect_summary string }
```

---

## Constraints

1. **Atomicity boundary** — The two-entry `correct_entry` transaction (deprecate original + insert new) is the only atomic unit. The redirect loop runs after commit. No single transaction may span the correction and the redirects (C-01, ADR-003 vnc-003).
2. **One transaction per redirect** — `redirect_graph_edge` opens its own RAII `sqlx::Transaction`. No batching. No refactoring of `redirect_graph_edge` (NFR-03).
3. **Terminal-active = new_entry.id always** — No `find_terminal_active` call. No TypedGraphState read lock. `context_correct` can only be called on an Active entry; the new entry is terminal-active by definition (C-03, ADR-001).
4. **Supersedes exclusion at SQL level** — `WHERE relation_type != 'Supersedes'` in the query, not a loop-level filter (C-05, ADR-002).
5. **Pool accessor names** — `query_incoming_edges` uses `read_pool()`; redirect transactions use `write_pool_server()`. Both currently alias the same pool (`db.rs:294`). A comment citing this implementation detail must appear at the call site (C-07, NFR-04).
6. **No TypedGraphState access** — The redirect loop must not hold any lock on the typed graph state cache (NFR-05).
7. **No modification of existing functions** — `redirect_graph_edge`, `write_graph_edge`, `build_typed_relation_graph`, `TypedGraphState`, `context_edge` handler must not be changed (NFR-09).
8. **Handler line count** — `context_correct` handler is ~145 lines; adding ~20 lines stays well under 500. If it exceeds 500, extract the redirect block to a named helper.
9. **Source validation** — Before calling `redirect_graph_edge`, check source entry status. Skip with `tracing::warn!` for Quarantined or Deprecated sources; do not count as failure (FR-06, ADR-003).

---

## Dependencies

### Internal (existing — no changes)

| Symbol | Crate | File |
|--------|-------|------|
| `correct_entry()` | `unimatrix-store` | `write_ext.rs` |
| `redirect_graph_edge()` | `unimatrix-server` | `mcp/edge_write.rs` |
| `EdgeRedirectError` | `unimatrix-server` | `mcp/edge_write.rs` |
| `validate_and_write_edges()` | `unimatrix-server` | `mcp/tools.rs` or `edge_write.rs` |
| `format_correct_success()` | `unimatrix-server` | `mcp/response/entries.rs` |
| `graph_edges` table | SQLite | schema migration v12→v13 |
| `idx_graph_edges_target_id` index | SQLite | schema migration v12→v13 |

### External Crates (no new dependencies)

| Crate | Usage |
|-------|-------|
| `sqlx` 0.8 | `read_pool()` queries; RAII transactions for redirects |
| `tracing` | `warn!` per failed/skipped edge; `info!` summary |

### Integration Test Infrastructure

| Suite | Role |
|-------|------|
| infra-001 Python | Extend with AC-06, AC-07, AC-12, AC-16 test cases |
| Rust unit tests (`unimatrix-store` or `unimatrix-server`) | New tests for AC-05, AC-14, AC-03, AC-04, AC-08, AC-09, AC-11, AC-13 |

---

## NOT in Scope

- Auto-redirecting edges when `context_deprecate` is called without a correction (no target to redirect to).
- Redirecting `Supersedes` edges in `graph_edges` — derived from `entries.supersedes`, rebuilt on tick.
- Removing `context_edge(mode="redirect")` from the MCP tool surface.
- Modifying `redirect_graph_edge` Contradicts logic or its RAII transaction model.
- Modifying `build_typed_relation_graph` or `TypedGraphState` tick behavior.
- Multi-hop chain traversal to find a terminal-active target beyond `new_entry.id`.
- Batching multiple edge redirects into a single transaction.
- Adding a structured `redirected_edges` field to the MCP response schema.
- Redirecting edges for entries other than the one being corrected in the current call.

---

## Alignment Status

Status: **PASS with two warnings** (from ALIGNMENT-REPORT.md).

### WARN-1 — SPEC FR-07 return-contract table (RESOLVED)

SPECIFICATION FR-07 was corrected before the vision guardian ran: the `Ok(true)/Ok(false)` table was replaced with `Result<(), EdgeRedirectError>`. SPEC FR-07 and ADR-003 are now consistent. No action required for delivery.

### WARN-2 — Fan-in ceiling N=50 is a new design choice not in SCOPE.md

SCOPE.md SR-01 asked the architect to "document the acceptable edge-cardinality ceiling." Architecture ADR-004 introduced N=50 as a concrete truncate-and-warn ceiling with documented latency rationale. The guardian recommended accepting it.

**Resolution**: Accepted per ADR-004. The N=50 ceiling is well-justified, consistent with the partial-write posture, and observable via `tracing::warn!`. No action required.

### Vision alignment

The feature directly serves the hash-chain integrity non-negotiable (correcting stale graph edges that undermine PPR traversal and DependencyOnDeprecated detection) and is narrowly scoped to the correction side-effect without introducing new tools, background workers, or architectural layers. Milestone fit is Wave 1B; all dependencies (vnc-015 `redirect_graph_edge`, vnc-016 DependencyOnDeprecated rule) are shipped.
