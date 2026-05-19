# SPECIFICATION: vnc-017 — Auto-Redirect Incoming Edges on context_correct

## Objective

When `context_correct` supersedes entry A with entry B, any `graph_edges` row pointing
at A becomes stale. This feature automatically redirects all such incoming edges to B as
part of the same `context_correct` MCP call, eliminating silent graph rot and
false-positive `DependencyOnDeprecated` detections without requiring any agent action.
The redirect uses the existing `redirect_graph_edge` infrastructure, runs inline after
the correction transaction commits, and follows the established ADR-003 partial-write
failure posture.

---

## Functional Requirements

**FR-01** — `context_correct` must, after the correction transaction commits and after
Phase B declared-edge writes complete, query all `graph_edges` rows where
`target_id = original_id` and process each as a redirect to `new_entry.id`.

**FR-02** — The redirect target is always `new_entry.id` (the direct correction result).
No cache traversal or `find_terminal_active` call is made. `context_correct` can only
be called on an Active entry; therefore `new_entry.id` is terminal-active by definition
at creation time.

**FR-03** — A new read function `query_incoming_edges(target_id: u64)` must be added to
`read.rs` in the `unimatrix-store` crate. It returns `Vec<(u64, String, u64)>` — tuples
of `(source_id, relation_type, created_at)` — using a `WHERE target_id = ?` query
against the `graph_edges` table over `read_pool()`.

**FR-04** — `Supersedes` relation type rows must be excluded from the redirect loop.
`entries.supersedes` is the authoritative source for Supersedes relationships;
`graph_edges` Supersedes rows are a derived representation rebuilt on the next tick.
Redirecting them would assert incorrect semantic claims. The exclusion must be implemented
at the SQL query level in `query_incoming_edges` (`WHERE relation_type != 'Supersedes'`),
not as a loop-level filter, accompanied by an explanatory comment (ADR-002).

**FR-05** — For each incoming edge (excluding `Supersedes`), the redirect loop must call
`redirect_graph_edge(store, source_id, original_id, new_entry.id, relation_type,
created_at)` from `edge_write.rs`.

**FR-06** — Before calling `redirect_graph_edge` for any edge, the redirect loop must
check whether the source entry is in a `Quarantined` or `Deprecated` status. If the
source entry is quarantined or deprecated, the loop must skip that edge and log a
`tracing::warn!` message identifying the source entry ID and the reason for skipping.
This is the resolution of SR-06: skip-with-warn, consistent with ADR-003.

**FR-07** — `redirect_graph_edge` returns `Result<(), EdgeRedirectError>`. The redirect
loop must handle both variants:

| Return value | Meaning | Loop treatment |
|---|---|---|
| `Ok(())` | Edge redirected, OR UNIQUE conflict (new edge already exists — idempotent) | Count as redirected (success) |
| `Err(e)` | SQL infrastructure error | Log `tracing::warn!`, increment failure counter, continue |

`Ok(())` covers both the "deleted + inserted" and the UNIQUE-conflict cases at the SQL
layer. The loop sees only success or error. `Err` must never abort the correction.
Note: lesson #4041 documents the `Ok(bool)` return of `write_graph_edge` — that is a
different function; `redirect_graph_edge` returns `Result<(), EdgeRedirectError>` (ADR-003).

**FR-08** — Redirect failures (SQL `Err`) must be logged with `tracing::warn!` per
individual edge and must not cause `context_correct` to return an error to the MCP
caller. The correction always succeeds if the two-entry atomic operation succeeded.

**FR-09** — After the redirect loop completes, a `tracing::info!` summary line must be
emitted with: total edges found, edges redirected (success), edges failed (SQL error),
edges skipped (quarantined/deprecated source), edges skipped (Supersedes exclusion).

**FR-10** — The `context_correct` response text is conditionally appended per the
following authoritative format table:

| Condition | Text appended |
|-----------|---------------|
| `found == 0` | *(no append — response text unchanged)* |
| `found > 0`, `truncated == false`, `skipped == 0` | `"Redirected N incoming edges (M failed, see logs)"` |
| `found > 0`, `truncated == false`, `skipped > 0` | `"Redirected N incoming edges (K skipped — invalid source, M failed, see logs)"` |
| `truncated == true` | `"Redirected N incoming edges (truncated from M, see logs)"` |

Where: N = redirected count; M = failed count (non-truncated) or total_raw count
(truncation variant); K = skipped count (Quarantined/Deprecated source).
The `found == 0` path produces no summary log and no append (SR-05 resolution).

**FR-11** — The `context_correct` response contract fields (`deprecated_original`,
`corrected_entry`) must remain unchanged. The text append in FR-10 is additive and does
not modify existing fields or their values.

**FR-12** — `context_edge(mode="redirect")` must remain operational and its existing
tests must continue to pass. The auto-redirect does not replace or interfere with the
manual redirect tool.

**FR-13** — If `query_incoming_edges` returns an empty result, the redirect loop body is
skipped entirely (zero-overhead path). No summary log line or response append is
emitted.

---

## Non-Functional Requirements

**NFR-01** — The redirect loop executes synchronously and inline in the `context_correct`
handler, after Phase B edge writes and before confidence recompute. No `tokio::spawn`
or fire-and-forget task is permitted. This ensures failures are visible and logged in
the same call context.

**NFR-02** — No new async tasks, background workers, or tick-cycle jobs are introduced
by this feature. All execution is on the MCP call path.

**NFR-03** — Each `redirect_graph_edge` call opens and commits its own RAII
`sqlx::Transaction` (one transaction per edge). No batched transaction spanning multiple
redirects is required for correctness. The implementation must not refactor
`redirect_graph_edge` to share a transaction across edges unless the architect
explicitly decides to do so as an optimization.

**NFR-04** — `query_incoming_edges` uses `read_pool()`. Write operations use
`write_pool_server()`. These must be called by their canonical accessor names even
though they currently reference the same underlying pool (`db.rs:294`). SR-03
mitigation: a comment citing this implementation detail must appear at the call site.

**NFR-05** — The redirect loop must not hold any lock (read or write) on the typed graph
state cache at any point. The `find_terminal_active` function and `TypedGraphState` are
not accessed; no read-lock dependency is introduced.

**NFR-06** — The `read.rs` 500-line rule applies to any new function added. The function
is ~10 lines; no module split is required. If unrelated changes push `read.rs` over the
limit during this feature, the function may remain — the rule applies to new modules,
not additions to existing large files (OQ-04 resolution).

**NFR-07** — The `context_correct` handler currently totals ~145 lines. The redirect
block adds ~20 lines inline. If the total exceeds 500 lines, the redirect block must be
extracted to a named helper function consistent with Phase B edge writes pattern.

**NFR-08** — ADR-003 partial-write posture is mandatory: no single transaction may span
the correction entry operation plus the redirect loop. Partial redirect (some edges
redirected, some not) is an acceptable degraded state.

**NFR-09** — The implementation must not modify `redirect_graph_edge`, `write_graph_edge`,
`build_typed_relation_graph`, `TypedGraphState`, or `context_edge` handler behavior.

---

## Acceptance Criteria

**AC-01** — After `context_correct(A → B)`, no `graph_edges` row with `target_id = A`
and a non-Supersedes relation type exists in the database.
Verification: integration test seeds multiple edge types pointing at A; calls
`context_correct(A → B)`; queries `graph_edges WHERE target_id = A AND
relation_type != 'Supersedes'` and asserts zero rows.

**AC-02** — After `context_correct(A → B)`, all edges that formerly pointed at A
(excluding Supersedes) now point at B (`target_id = B`).
Verification: integration test asserts `graph_edges WHERE target_id = B` contains
the expected source/relation rows.

**AC-03** — The redirect always targets `new_entry.id` (the direct correction result),
not a cached chain traversal result.
Verification: unit test creates a two-hop chain (A → B → C) then calls
`context_correct(A → B)` and verifies redirect targets B, not C. No call to
`find_terminal_active` or access to `TypedGraphState` occurs (structural assertion
in code review; behavioral assertion via the unit test).

**AC-04** — `context_correct` returns a success response even when one or more redirect
calls return `Err`. The returned `deprecated_original` and `corrected_entry` fields are
present and correct regardless of redirect outcome.
Verification: unit test injects a failing `redirect_graph_edge` stub and asserts
the handler returns a well-formed success response, not an error.

**AC-05** — `query_incoming_edges(target_id)` is implemented in `read.rs`, returns
`Vec<(u64, String, u64)>` (source_id, relation_type, created_at), and uses
`read_pool()`.
Verification: Rust unit test seeds three edge rows with the same `target_id` and
one with a different `target_id`; calls `query_incoming_edges`; asserts exactly
three tuples are returned with correct values.

**AC-06** — The Python infra-001 integration test demonstrates the full auto-redirect
flow: store C, store A, add edge `C → A` (type = Prerequisite), call
`context_correct(A → B)`, assert `graph_edges` contains `C → B` and no row with
`target_id = A` for non-Supersedes types.
Verification: infra-001 Python test suite, new test case.

**AC-07** — `Contradicts` edges are correctly handled by the existing 4-row bidirectional
logic inside `redirect_graph_edge`. The integration test includes at least one
`Contradicts` edge in the seeded data and asserts both the forward and reverse rows
are updated to the new target.
Verification: AC-06 integration test extended with a Contradicts edge pair; asserts
both `C → B` and `B → C` (reverse) rows exist post-redirect.

**AC-08** — When the source entry of a Contradicts (or any) edge is quarantined or
deprecated, that edge is skipped, a `tracing::warn!` is emitted, and the failure counter
is NOT incremented (skipped ≠ failed).
Verification: unit test seeds an incoming edge whose source has status Quarantined;
calls the redirect loop; asserts the edge row is unchanged, no failure count, and
`tracing::warn!` is present in captured logs.

**AC-09** — A UNIQUE conflict (new edge already exists — idempotent redirect) returns
`Ok(())` and is counted as redirected (success). No warning is logged.
Verification: unit test pre-inserts `C → B` then calls the redirect loop for
`C → A → B`; asserts failure count = 0, redirected count = 1, and no warn log.

**AC-10** — `Supersedes` edges in `graph_edges` are excluded from the redirect loop.
They remain in the database unchanged by `context_correct`.
Verification: integration test seeds a Supersedes row pointing at A; calls
`context_correct(A → B)`; asserts the Supersedes row still has `target_id = A` and
no corresponding Supersedes row with `target_id = B` was inserted by the redirect.

**AC-11** — When no incoming non-Supersedes edges exist, the response text is identical
to the current `context_correct` response (no append). The summary log line is also
omitted.
Verification: unit test calls `context_correct` on an entry with no incoming edges;
asserts response text does not contain "Redirected" and no info log is emitted.

**AC-12** — When incoming non-Supersedes edges exist and all redirect successfully,
the response text is appended with `"Redirected N incoming edges (0 failed, see logs)"`.
Verification: integration test with two Prerequisite edges; asserts response text
contains "Redirected 2 incoming edges (0 failed, see logs)".

**AC-13** — When incoming edges exist and some fail (SQL error), the response text is
appended with the correct failure count: `"Redirected N incoming edges (M failed,
see logs)"` where M > 0.
Verification: unit test with a partial-failure stub; asserts response text reflects
actual counts.

**AC-14** — A Rust unit test in `unimatrix-server` (or `unimatrix-store`) exercises the
redirect loop end-to-end: seeds an edge row pointing at original_id, calls
`context_correct`, and verifies the edge row now points at new_entry.id.
Verification: Rust test using in-memory SQLite database, asserting `graph_edges`
state before and after.

**AC-15** — `context_edge(mode="redirect")` tool remains operational. All existing tests
for that tool pass without modification.
Verification: existing test suite passes unchanged.

**AC-16** — `DependencyOnDeprecated` detection does not fire for edges that were
successfully auto-redirected, after the next graph tick rebuilds the typed relation graph.
Verification: SR-07 integration test — after AC-06 flow, trigger a graph state tick
and assert no DependencyOnDeprecated event is raised for the redirected edge.

**AC-17** — When all incoming edges are skipped (Quarantined/Deprecated sources), the
response text uses the skipped-count variant and does not read as "nothing happened."
Verification: unit test seeds 3 incoming edges from Quarantined sources; calls
`context_correct`; asserts response text contains
`"Redirected 0 incoming edges (3 skipped — invalid source, 0 failed, see logs)"` and
failure count = 0.

---

## Domain Models

### Entities

**Entry** — A knowledge record in the `entries` table. Relevant statuses for this
feature: `Active`, `Deprecated`, `Quarantined`. `context_correct` can only be called
on an `Active` entry. An entry is terminal-active if its `superseded_by` field is `None`
and its status is `Active`.

**GraphEdge** — A row in the `graph_edges` table with columns `(source_id, target_id,
relation_type, created_at)`. Edges are directional: source → target. The table has
index `idx_graph_edges_target_id` for efficient lookup by target.

**IncomingEdge** — A `graph_edges` row where `target_id` matches a given entry ID. The
data contract for `query_incoming_edges` return values:

```
(source_id: u64, relation_type: String, created_at: u64)
```

`target_id` is implicit (the queried entry) and not included in the tuple.

**RelationType** — The type of directed relationship between two entries. For this
feature:
- `Supersedes` — excluded from the redirect loop (authoritative source is
  `entries.supersedes`).
- `Contradicts` — triggers bidirectional 4-row atomic redirect inside
  `redirect_graph_edge`.
- All other types — unidirectional 2-row redirect (DELETE old row, INSERT OR IGNORE
  new row).

**CorrectionResult** — The output of a `context_correct` operation: a deprecated
original entry and a new active correction entry. The `new_entry.id` is the redirect
target for this feature.

**RedirectOutcome** — Per-edge result of calling `redirect_graph_edge`:
- Success (true): edge was physically moved.
- Conflict (false): new row already existed; idempotent; treated as success.
- Failure (Err): SQL infrastructure error; logged; counted as failed.
- Skipped-source: source entry is Quarantined or Deprecated; not dispatched to
  `redirect_graph_edge`; counted as skipped, not failed.
- Skipped-supersedes: relation type is Supersedes; not dispatched; not counted in
  totals reported to caller.

### Ubiquitous Language

| Term | Definition |
|---|---|
| auto-redirect | The automatic `query → loop → redirect` performed by `context_correct` after the correction transaction commits |
| incoming edge | A `graph_edges` row whose `target_id` equals the deprecated original entry |
| redirect loop | The sequential per-edge loop in the `context_correct` handler that calls `redirect_graph_edge` for each incoming edge |
| terminal-active | The last non-deprecated, non-superseded entry at the end of a correction chain; for newly created entries, always `new_entry.id` itself |
| blast-radius posture | ADR-003: partial-write failures are logged, not propagated; the outer operation is not rolled back |
| skip-with-warn | The SR-06 resolution: skip a quarantined/deprecated source entry, log `tracing::warn!`, do not count as failure |

---

## User Workflows

### Workflow 1: Normal Correction (No Incoming Edges)

1. Agent calls `context_correct` with original_id and new content.
2. `correct_entry()` runs atomically: deprecates original, inserts new entry.
3. Phase B declared-edge writes execute (if any edges were declared in params).
4. `query_incoming_edges(original_id)` returns empty.
5. Redirect loop is skipped; no summary log emitted.
6. Confidence recompute runs.
7. Response returns with standard text (no append).

### Workflow 2: Correction With Incoming Edges

1. Agent calls `context_correct` with original_id and new content.
2. `correct_entry()` runs atomically.
3. Phase B declared-edge writes execute.
4. `query_incoming_edges(original_id)` returns N rows (Supersedes excluded at SQL level).
5. For each row:
   a. Look up source entry status. If Quarantined or Deprecated: log warn, increment
      skipped, continue.
   b. Call `redirect_graph_edge`. On `Ok(())`: increment redirected.
      On `Err`: log warn, increment failed.
6. Emit `tracing::info!` summary with all counts.
7. Confidence recompute runs.
8. Response returns with appended text:
   `"Redirected {redirected} incoming edges ({failed} failed, see logs)"`.

### Workflow 3: Manual Edge Repair (Unaffected)

Agents continue to call `context_edge(mode="redirect")` for corrections that are not
triggered by `context_correct` (e.g., data repair, third-party source edges). This
tool is unmodified.

---

## Constraints

**C-01** — The correction atomicity boundary (ADR-002 vnc-003) covers only the two-entry
operation (deprecate original + insert correction). The redirect loop is outside this
boundary. No single transaction may span both.

**C-02** — `redirect_graph_edge` must be called with a RAII `sqlx::Transaction` from
`write_pool_server()` per call. One transaction per edge redirect. Batching is not
required for correctness.

**C-03** — `context_correct` can only be called on an Active entry (enforced by existing
handler validation). This invariant is what makes `new_entry.id` always terminal-active.
The implementation must not attempt cache traversal and must not call
`find_terminal_active`.

**C-04** — `redirect_graph_edge` caller contract (from vnc-015): the caller must validate
the new target exists and is not quarantined before calling. The redirect loop satisfies
this because `new_entry.id` was just created (existence guaranteed) and the correction
flow only produces Active entries (not quarantined). Source validation (FR-06) is the
caller's responsibility and must be implemented in the loop.

**C-05** — `Supersedes` rows in `graph_edges` must not be redirected. The authoritative
source is `entries.supersedes`. Redirecting would produce incorrect semantic claims.

**C-06** — The 500-line-per-file rule applies to new modules, not existing large files.
`read.rs` is 3,465 lines; adding `query_incoming_edges` does not require a new module.
`context_correct` handler must not exceed 500 lines total after this change.

**C-07** — `write_pool_server()` and `write_pool` currently share the same pool
(`db.rs:294`). Callers must use the canonical accessor name. A comment must be present
at the `query_incoming_edges` call site documenting this detail.

---

## Dependencies

### Existing Functions (Must Not Be Modified)

| Function | Location | Role |
|---|---|---|
| `correct_entry()` | `write_ext.rs` | Atomic two-entry correction transaction |
| `redirect_graph_edge()` | `edge_write.rs` | Per-edge redirect with Contradicts bidirectional logic |
| `validate_and_write_edges()` | `edge_write.rs` or `tools.rs` | Phase B declared-edge writes |
| `format_correct_success()` | `tools.rs` or `store_correct.rs` | Response text builder (must be extended for FR-10) |

### New Function Required

| Function | Location | Signature |
|---|---|---|
| `query_incoming_edges` | `read.rs` (unimatrix-store) | `async fn query_incoming_edges(pool: &SqlitePool, target_id: u64) -> Result<Vec<(u64, String, u64)>>` |

### External Crates / Libraries (No New Dependencies)

- `sqlx` 0.8 — existing; used for `read_pool()` queries and RAII transactions.
- `tracing` — existing; used for `warn!` and `info!` log emission.

### Existing Infrastructure

- `graph_edges` table — `idx_graph_edges_target_id` index exists (migration v12→v13).
- `redirect_graph_edge` — shipped in vnc-015.
- `DependencyOnDeprecated` detection rule — shipped in vnc-016.
- `context_edge(mode="redirect")` tool — shipped in vnc-015, retained as-is.

### Integration Test Harness

- infra-001 Python suite — must be extended with new test cases for AC-06, AC-07,
  AC-16.

---

## NOT In Scope

- Auto-redirecting edges when `context_deprecate` is called without a correction.
  There is no target to redirect to.
- Redirecting `Supersedes` edges in `graph_edges`. These are derived and rebuilt
  on tick; redirecting them would produce incorrect graph semantics.
- Removing `context_edge(mode="redirect")` from the public MCP tool surface.
- Modifying `redirect_graph_edge` Contradicts logic or its transaction model.
- Modifying `build_typed_relation_graph` or `TypedGraphState` tick behavior.
- Chain traversal to find a multi-hop terminal-active target. `new_entry.id` is
  always used directly.
- Batching multiple edge redirects into a single transaction (optional optimization
  explicitly deferred to architect discretion).
- Adding a structured `redirected_edges` field to the MCP response schema. Only
  the response text is augmented (FR-10).
- Redirecting edges for entries other than the one being corrected in the current call.

---

## Open Questions — RESOLVED

**OQ-01** — Resolved by ADR-002: Supersedes exclusion is at the SQL level in
`query_incoming_edges` (`WHERE relation_type != 'Supersedes'`). No loop-level filter.

**OQ-02** — Resolved by ADR-003: `redirect_graph_edge` returns `Result<(), EdgeRedirectError>`.
`Ok(())` covers both inserted and UNIQUE-conflict cases; redirected counter always
increments on `Ok(())`. There is no `Ok(false)` to distinguish.

---

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — returned 9 entries. Most relevant: entry
  #4459 (graph-edges pattern: source-validation posture for Contradicts redirect loops),
  entry #4420 (ADR-003 vnc-015: partial-write blast-radius posture), entry #92 (ADR-002
  vnc-003: correction chain atomicity). All three directly informed the specification.
  Entry #4439 (ADR-001 vnc-015: validate all edge inputs before entry insert) confirmed
  the caller-validates contract for `redirect_graph_edge`. Entry #4459 was pre-staged as
  a pattern specifically for this feature — applied as FR-06 and AC-08.
