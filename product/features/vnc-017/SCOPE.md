# vnc-017: Auto-Redirect Incoming Edges on context_correct

## Problem Statement

When `context_correct` supersedes entry A with entry B, any third-party entry C that
holds an edge pointing at A (e.g. `C → A` via `Prerequisite`, `Supports`, etc.) is now
stale: it points at a deprecated entry. Correcting that requires a subsequent
`context_edge(mode="redirect")` call targeting every such edge. In practice agents never
make that call. The result is a growing set of stale edges that:

1. Cause false-positive `DependencyOnDeprecated` detections for edges that the agent
   intended to point at the live knowledge.
2. Degrade PPR traversal quality — stale edges participate in graph scoring with a
   deprecated endpoint.
3. Create invisible graph rot that compounds across correction chains.

The fix is to make `context_correct` automatically redirect all incoming edges from the
deprecated original to the new terminal-active entry as part of the same MCP operation,
requiring no separate agent action.

Affected party: every agent that stores entries with graph edges and later corrects those
entries. The defect is silent — no error is produced, no warning is surfaced.

Why now: vnc-015 shipped `context_edge(mode="redirect")` and vnc-016 shipped the
`DependencyOnDeprecated` detection rule. The infrastructure to detect and redirect stale
edges now exists; the remaining gap is automating what no agent does manually.

## Goals

1. `context_correct` automatically queries all incoming `graph_edges` rows whose
   `target_id` equals the deprecated original and redirects them to the new terminal-active
   entry (the correction result), as part of the correction flow.
2. The redirect is performed using the existing `redirect_graph_edge` function from
   `edge_write.rs`, which already handles `Contradicts` bidirectionality and RAII
   transactions (lesson #2269).
3. Terminal-active resolution must handle chains longer than one hop: if A was corrected
   to B and then B is corrected to C, a subsequent correction of A redirects to C, not B.
   `find_terminal_active` (already implemented in `unimatrix-engine`) provides this.
4. The auto-redirect must not break the `context_correct` response contract — the returned
   `deprecated_original` and `corrected_entry` fields remain as today.
5. Agents retain the ability to call `context_edge(mode="redirect")` manually for
   non-correction-triggered redirects (the tool is not removed).

## Non-Goals

- Auto-redirecting edges when `context_deprecate` is called without a correction (no
  target to redirect to — out of scope for this feature).
- Redirecting the `Supersedes` edge written by `context_correct` itself — that edge is
  written by the correction flow and is always correct.
- Removing `context_edge(mode="redirect")` from the public tool surface — it serves
  legitimate use cases outside correction flows and remains available.
- Changing the `context_edge` handler or any of its modes.
- Modifying `redirect_graph_edge` behavior for `Contradicts` edges — the existing 4-row
  atomic logic already handles them correctly.
- Changing the graph rebuild path (`build_typed_relation_graph`) or the
  `TypedGraphState` tick.
- Redirecting edges for multi-hop chains in a single call: only the direct incoming edges
  to the deprecated original are redirected; chain traversal is used only for resolving
  the correct new target.

## Background Research

### context_correct Flow (Existing)

`store_correct.rs` delegates to `write_ext.rs:correct_entry()` which atomically:
1. Deprecates the original entry (`status = Deprecated`, `superseded_by = new_id`,
   `correction_count++`)
2. Inserts the new correction entry (`supersedes = original_id`)
3. Inserts tags and vector mapping for the new entry
4. Updates status counters

After the transaction commits, `store_correct.rs` fires an audit event (fire-and-forget),
inserts the HNSW embedding, and updates adaptation prototypes. Edge writes for explicitly
declared edges (`params.edges`) are written synchronously in Phase B via
`validate_and_write_edges`.

The auto-redirect step must be inserted after the `correct_entry` transaction commits, so
the new entry ID is known and exists in the DB before any edges are redirected to it.

### redirect_graph_edge (Existing)

`edge_write.rs:redirect_graph_edge` uses a RAII `sqlx::Transaction` (mandatory per
lesson #2269 — raw SQL BEGIN/COMMIT strings lose atomicity under multi-connection pools).

- Non-Contradicts: 2 SQL statements (DELETE old, INSERT OR IGNORE new)
- Contradicts: 4 SQL statements (DELETE A→B, DELETE B→A, INSERT A→B', INSERT B'→A)

Caller contract: validate new target existence and quarantine status before calling.
The function itself does not re-validate.

There is no existing function to bulk-redirect all incoming edges to an entry. A new
store-layer read helper is needed to query them.

### Incoming Edge Query (Missing)

No `query_incoming_edges` function exists in the store layer. The `graph_edges` table
has an `idx_graph_edges_target_id` index (created in migration v12→v13), making a
`WHERE target_id = ?` query efficient. The new function:

```sql
SELECT source_id, relation_type, created_at
FROM graph_edges
WHERE target_id = ?1
```

This is a read-only operation using `read_pool()`, consistent with all other
`query_*` functions in `read.rs`.

### Terminal Active Resolution (Existing)

`unimatrix_engine::graph::find_terminal_active(node_id, graph, entries)` traverses
outgoing `Supersedes` edges from `node_id` and returns the first active entry with
`superseded_by = None`, up to `MAX_TRAVERSAL_DEPTH`. It is called today by the search
hot path via `TypedGraphState`.

Critical constraint: `find_terminal_active` requires a pre-built `TypedRelationGraph` and
an entries snapshot — both live in the `TypedGraphState` tick cache. The correct handler
has access to `self.services` (which holds the typed graph state cache). Calling
`find_terminal_active` requires taking a short read lock on the typed graph state to
clone the needed snapshot.

Fallback: if the graph state is cold-start or the new entry is not yet in the cache (it
was just created), the immediate `new_entry.id` is used as the target — it is always
terminal-active by definition at creation time.

### Supersedes Edge Direction

`build_typed_relation_graph` derives Supersedes edges from `entries.supersedes`, NOT from
`GRAPH_EDGES` rows. Edge direction: `predecessor_id → new_id` (outgoing = toward newer
knowledge). `find_terminal_active` traverses outgoing Supersedes edges. A freshly created
correction entry will have `supersedes = original_id` in the entries table; the graph
tick will pick this up on the next rebuild. Since there is no need to wait for the tick
rebuild (the new entry IS the terminal active), the fallback to `new_entry.id` is correct.

### Blast Radius (ADR-003 vnc-015)

ADR-003 established that edge write failures after entry insert are treated as
infrastructure errors — logged, not rolled back. The same posture applies here: if
individual redirect operations fail (SQL error), they are logged and the correction
response succeeds. Partial redirect (some edges redirected, some not) is an acceptable
degraded state, consistent with the existing partial-write posture for declared edges.

### Atomicity Boundary (ADR-002 vnc-003)

ADR-002 established that `context_correct` atomicity covers the two-entry operation only
(deprecate original + insert correction). Edge operations are explicitly outside the
atomic boundary (ADR-003). The auto-redirect follows the same pattern: edge redirects
run after commit, using the same fire-and-forward sequential model as Phase B edge writes.

### context_edge(mode="redirect") Retention

The tool remains valuable for:
- Redirecting edges that were written before `context_correct` existed (data repair)
- Redirecting edges where the source entry belongs to a third party
- Arbitrary edge redirections unrelated to correction chains

Removing it would reduce agent capability without benefit.

### Synchronous vs Background Execution

Phase B edge writes in `context_correct` are executed synchronously inline. The same
model applies here — the redirect loop runs inline after the correction commits. The
number of incoming edges on any given entry is bounded in practice (no production entry
has been observed with more than a handful of incoming edges). Fire-and-forget via
`tokio::spawn` is not appropriate because failures would be invisible until the next tick
rebuild, and any per-edge error logging would require an owned copy of all edge data.

### W1B-2 / vnc-016 Shared Logic

vnc-016 added a Rust store-layer unit test for `query_stale_prerequisite_edges_for_cycle`
and extended the Python integration harness. No new chain-following logic was introduced
in vnc-016. The `find_terminal_active` function predates vnc-016 and lives in
`unimatrix-engine/src/graph.rs`. There is no shared logic to reuse from vnc-016 directly;
the relevant building blocks (`redirect_graph_edge`, `find_terminal_active`) preexist.

## Proposed Approach

**Location**: The redirect loop is added to `store_correct.rs` after Phase B edge writes
(or directly in the `context_correct` handler in `tools.rs`, consistent with how Phase B
edge writes are already inlined there).

**New store read function** (`read.rs`): `query_incoming_edges(target_id: u64)`
returning `Vec<(u64, String, u64)>` — `(source_id, relation_type, created_at)` — using
`read_pool()` and the existing `idx_graph_edges_target_id` index.

**Redirect loop** (in `context_correct` handler, after Phase B, before confidence
recompute):
1. Query incoming edges to `original_id` via `query_incoming_edges`.
2. Resolve terminal-active target: attempt `find_terminal_active` from typed graph state;
   fall back to `new_entry.id` if the cache is cold or the entry is not yet present.
3. For each incoming edge: call `redirect_graph_edge(store, source_id, original_id,
   terminal_id, relation_type, created_at)`. Log individual failures; do not abort.
4. Log a summary `tracing::info!` with counts (total found, redirected, failed).

**Posture**: same as ADR-003 — infrastructure failures are logged, not propagated as
call errors. The correction always succeeds if the entry operation succeeds.

**Response**: unchanged — the existing `format_correct_success` output is returned.
Optionally, a `redirected_edges: N` count is included in the response text (open
question).

## Acceptance Criteria

- AC-01: After `context_correct(A → B)`, all `graph_edges` rows with `target_id = A` are
  updated to `target_id = B` (or the terminal-active entry in the chain). No stale edges
  pointing at the deprecated A entry remain after the call.
- AC-02: The redirect always points edges at the direct correction target (`new_entry.id`),
  which is terminal-active by definition at creation time. Deep chain resolution (A→B→C)
  is the responsibility of subsequent corrections when those intermediate entries are
  corrected — not this flow. No cache traversal is attempted.
- AC-03: `context_correct` continues to return the same success response format. The
  correction is not aborted if the redirect loop encounters infrastructure failures.
- AC-04: A new `query_incoming_edges(target_id: u64)` read function exists in the store
  layer (`read.rs`) and is covered by a unit test that seeds known edge rows and asserts
  the returned tuples.
- AC-05: An integration test (infra-001 Python suite) demonstrates the auto-redirect:
  store C, store A, add edge `C → A`, call `context_correct(A → B)`, assert that
  `graph_edges` contains `C → B` and no row with `target_id = A` (for non-Supersedes
  types).
- AC-06: `Contradicts` edges are handled correctly by the existing
  `redirect_graph_edge` 4-row logic. The integration test includes at least one
  `Contradicts` edge to exercise bidirectional redirect.
- AC-07: `context_edge(mode="redirect")` tool remains operational and its existing tests
  continue to pass.
- AC-08: If no incoming edges exist, `context_correct` behaves identically to its current
  behavior (zero-overhead path: query returns empty, loop is skipped).
- AC-09: Redirect failures (SQL infrastructure errors) are logged with `tracing::warn!`
  and do not cause `context_correct` to return an error to the caller.
- AC-10: A Rust unit test in `unimatrix-server` (or `unimatrix-store`) exercises the
  redirect loop with a seeded edge row and verifies the edge is updated to the new target.
- AC-11: The terminal-active fallback is tested: when the typed graph state is cold
  (empty), the redirect uses the direct correction target (`new_entry.id`) rather than
  panicking or skipping.

## Constraints

- `redirect_graph_edge` requires a RAII `sqlx::Transaction` started from
  `write_pool_server()`. Each call opens and commits its own transaction. Batching
  multiple redirects into a single transaction would require refactoring
  `redirect_graph_edge` to accept `&mut Transaction` — acceptable if the designer
  decides to optimize, but not required for correctness.
- Terminal-active resolution always uses `new_entry.id` directly — no cache traversal.
  `context_correct` can only be called on an active entry, so the correction result is
  always terminal-active at creation time by definition. `find_terminal_active` is not
  called; the typed graph state cache is not accessed. This eliminates the read-lock
  dependency and the cold-cache edge case.
- `read.rs` 500-line rule: `query_incoming_edges` must fit within the current file or
  prompt a module split if adding it would push the file over the limit.
- The `context_correct` handler is already 145 lines. The redirect block adds
  ~20 lines inline. If this pushes it over 500 lines, the redirect logic should be
  extracted to a helper (similar to Phase B edge writes' current inline placement).
- ADR-003 partial-write posture is mandatory — no transaction wrapping the full
  correction + redirect operation.
- `write_pool_server()` and `write_pool` are the same pool in the current implementation
  (`db.rs:294`). This is an implementation detail; callers must use the correct accessor.
- SQLite WAL mode: individual small transactions (one per redirect) are efficient. No
  performance concern for typical edge cardinalities.
- The `Contradicts` bidirectional redirect is already handled inside
  `redirect_graph_edge` — no special-casing needed in the caller loop.

## Open Questions — RESOLVED

| OQ    | Decision |
|-------|----------|
| OQ-01 | Include count — append `Redirected N incoming edges (M failed, see logs)` to response text. MCP responses are informational text for human operators, not structured JSON; this is not a breaking change. |
| OQ-02 | Log only; count line in response covers human visibility. No `warnings` field — would create a schema obligation for agents to check. ADR-003 posture maintained. |
| OQ-03 | Exclude `Supersedes` from redirect loop. `entries.supersedes` is authoritative; `graph_edges` Supersedes rows are a derived representation rebuilt on next tick. Redirecting them would assert incorrect semantic claims (e.g. C supersedes B when only C superseded A). Exclude explicitly with an explanatory comment. |
| OQ-04 | `read.rs` — no new module. `read.rs` is already 3,465 lines; the 500-line concern is moot. No `edge_read.rs` exists in the store crate. A single ~10-line function does not justify a new module boundary. |
| OQ-05 | Always use `new_entry.id` — terminal-active by definition, always. `context_correct` can only be called on an active entry, so the new entry cannot have `superseded_by` set at creation time. The `find_terminal_active` cache path is complexity with no benefit for forward-going usage. Skip the cache attempt entirely and document why. |

## Tracking

https://github.com/dug-21/unimatrix/issues/606
