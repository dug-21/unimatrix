# Component: `run_carry_forward_loop` + `CarrySummary`

**Crate / location:** `unimatrix-server/src/mcp/tools.rs`, a **sibling of `run_redirect_loop`**
(`tools.rs:4660`). `pub(super)` for test visibility (mirrors `run_redirect_loop`). Place the
`CarrySummary` struct and the function adjacent to `RedirectSummary` / `run_redirect_loop` so the
two post-correction loops read side-by-side.

## Purpose

Orchestrate step 8b′: query A's eligible outgoing edges and copy each onto the new corrected
entry B, accumulating the `edges_carried` count. Owns its own write loop (ADR-003 / R-08) — it
CANNOT delegate the batch to `validate_and_write_edges`, which discards the per-edge bool. Same
warn-and-continue posture as `run_redirect_loop`: never aborts or rolls back the committed
correction (ADR-002 / NFR-01).

## Data structures

```rust
/// Accumulator for the outgoing carry-forward loop (vnc-035, step 8b′).
/// Returned BY VALUE (not Option) so the handler always observes found/failed for logging.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct CarrySummary {
    /// Eligible outgoing rows returned by query_outgoing_edges (the loop's input count).
    found: usize,
    /// write_graph_edge `true` returns — genuinely new carried edges. THE edges_carried ack value.
    /// For a Contradicts logical edge (two rows), this counts ONE (the forward direction only).
    carried: usize,
    /// Distinguished SQL-error writes — the SR-01 observable failure signal (ADR-002).
    failed: usize,
}
```

Field accessors: the handler in the same module reads `summary.carried` (and `found`/`failed`
for the completion log). If the fields are private to a sub-module, expose `pub(super)` getters
or make the fields `pub(super)`; mirror whatever `RedirectSummary` does for its handler read.

## Function signature

```rust
pub(super) async fn run_carry_forward_loop(
    store: &unimatrix_core::Store,
    original_id: u64,   // A — the entry being corrected (deprecated)
    new_entry_id: u64,  // B — correct_result.corrected_entry.id (freshly Active)
) -> CarrySummary
```

## Pseudocode body

```
run_carry_forward_loop(store, original_id, new_entry_id) -> CarrySummary:

    use unimatrix_engine::graph::RelationType
    use crate::mcp::edge_write::EDGE_SOURCE_AGENT

    // ── Query eligible outgoing edges (single-source predicate lives in the SQL) ──────────
    rows = match store.query_outgoing_edges(original_id).await:
        Err(e):
            // ADR-002 / SR-01: correction already committed. Warn, return empty summary.
            // Mirrors run_redirect_loop returning None on query failure.
            warn!(entry_id = original_id, error = %e,
                  "vnc-035: query_outgoing_edges failed; skipping carry-forward")
            return CarrySummary { found: 0, carried: 0, failed: 0 }
        Ok(rows) if rows.is_empty():
            // Zero eligible outgoing edges — nothing to carry; no log (mirrors vnc-017 silence).
            return CarrySummary { found: 0, carried: 0, failed: 0 }
        Ok(rows):
            rows

    // NO CEILING (FR-09 / AC-09 — deliberately unlike run_redirect_loop's REDIRECT_CEILING).
    // All eligible rows carry; eligibility (the SQL predicate) is the sole degree bound (SR-04).

    found   = rows.len()
    carried = 0
    failed  = 0

    // created_at = NOW (correction timestamp). NOT the source row's created_at (ADR-004 / R-11).
    now = current_unix_secs()   // SystemTime::now()... .as_secs(); reuse the handler's `now`
                                // pattern (tools.rs:1128) — or accept `now` as a param (see Note A).

    // ── Per-edge write loop — OWNS the loop to capture each bool (ADR-003 / R-08) ─────────
    for row in rows:

        rel = RelationType::from_str(&row.relation_type)
        // The stored relation_type came from a prior validated write; from_str should succeed.
        // Defensive: if it somehow does NOT resolve, skip this row (do not panic, NFR-05).
        if rel is None:
            warn!(target_id = row.target_id, relation_type = %row.relation_type,
                  "vnc-035: carried edge has unresolvable relation_type; skipping")
            continue   // not counted as carried or failed — it was never a valid eligible edge

        // ── Forward direction write — the counted direction ──────────────────────────────
        inserted = carry_write_edge(           // SEAM — see §Fault-injection seam
            store, new_entry_id, row.target_id, rel.as_str(),
            1.0,                // weight (ADR-004)
            now,                // created_at = now, NOT preserved (ADR-004 / R-11)
            EDGE_SOURCE_AGENT,  // source AND created_by = "agent" (FR-11)
            "",                 // metadata = "" (ADR-004)
        ).await

        match classify(inserted):              // see §Counting — distinguishing the two false cases
            Inserted:    carried += 1           // `true` → genuinely new carried edge (SR-02)
            UniqueConflict: { /* false, Ok-path: already on B from 8b or duplicate row; not counted, not failed */ }
            SqlError:    failed += 1            // `false`, Err-path: write_graph_edge already warned internally

        // ── Contradicts: write the reverse direction too — ONE logical edge, NOT counted ──
        // Reuse the same bidirectional STRUCTURE as validate_and_write_edges (edge_write.rs:211),
        // written inline here so we keep the forward bool for counting (ADR-005).
        // Source validation is NOT needed: the carry source is B, freshly Active (ADR-005 §3).
        if rel == RelationType::Contradicts:
            _reverse = carry_write_edge(
                store, row.target_id, new_entry_id, "Contradicts",
                1.0, now, EDGE_SOURCE_AGENT, "",
            ).await
            // Reverse is the bidirectional partner of one logical Contradicts edge.
            // Per ADR-005 it is NOT added to `carried` (edges_carried counts logical edges).
            // A reverse SQL-error is the accepted partial-write posture (ADR-003 vnc-015):
            // warn already fired inside write_graph_edge; do NOT increment `failed` for the
            // reverse (failed tracks logical-edge copy failures via the forward write).

    // ── Completion log (mirrors run_redirect_loop's single post-loop info log) ────────────
    info!(entry_id = original_id, new_entry_id = new_entry_id,
          found = found, carried = carried, failed = failed,
          "vnc-035: carry-forward loop complete")

    return CarrySummary { found, carried, failed }
```

### Note A — where `now` comes from

Two acceptable shapes (developer picks; `run_redirect_loop` reuses `edge.created_at`, but carry
MUST re-stamp `now`, ADR-004): (a) compute `now` inside the loop function via the same
`SystemTime::now()...as_secs()` expression the handler uses at `tools.rs:1128`; or (b) add a
`created_at: u64` parameter and pass the handler's already-computed `now`. Prefer (b) if the
handler computes `now` for 8b anyway, so the carried edges and any `params.edges` share one
correction timestamp. The CONTRACT is: `created_at` written onto B is the correction's `now`,
never `row.created_at`.

## Counting — distinguishing `false`-UNIQUE from `false`-SQL-error (ADR-003)

`write_graph_edge` collapses both to `false` (`true`=insert; `false`=UNIQUE conflict, no warn;
`false`=SQL error, warns internally — pattern #4041). The loop cannot see which `false` it got
from the bool alone. ADR-003 accepts two implementations; **`carried` is exact regardless**
(keys strictly off `true`). For `failed` precision pick ONE:

- **(a) Recommended — approximate `failed`, exact `carried`, no signature change.** Treat every
  `false` as "not newly inserted." `carried` counts `true` only. For `failed`, rely on the
  fault-injection seam (§below): in production a `false` is overwhelmingly a UNIQUE conflict, and
  the SR-01 guarantee (correction + prior carries persist) does NOT depend on `failed` being
  exact — `write_graph_edge`'s internal `warn!` is the durable failure signal. The AC-07 test
  drives the seam, so under test the SQL-error `false` is known and `failed` is asserted exactly.
  Under (a), `classify` returns `SqlError` only when the test seam fired; otherwise `UniqueConflict`.
- **(b) Exact `failed` always — add a thin three-case wrapper.** A `pub(super)` helper around
  `write_graph_edge` returning an enum `{ Inserted, Conflict, SqlError }` (e.g. by re-running the
  insert through a variant that surfaces the `Result` instead of collapsing it). Costs a new
  primitive; do NOT change `write_graph_edge`'s public signature (R-08 / NFR-03).

**Recommendation: (a).** It satisfies every AC without touching the shared write primitive.

## Fault-injection seam (AC-07 — MANDATORY, R-01 / SR-01 / lesson #4473)

The loop MUST let a test drive exactly ONE mid-loop edge write to a SQL-error (`false`-Err path)
without changing production behavior. The carry write therefore goes through an indirection
`carry_write_edge(...)` rather than calling `write_graph_edge` directly:

```
// Production path: carry_write_edge delegates straight to write_graph_edge.
async fn carry_write_edge(store, src, tgt, rel, w, now, source, meta) -> bool {
    #[cfg(test)]
    if let Some(fail_on) = CARRY_FAIL_INJECT.take_if_matches(src, tgt, rel) {
        // Simulate write_graph_edge's SQL-error false-path: emit the same warn, return false.
        warn!(source_id = src, target_id = tgt, relation_type = rel,
              "write_graph_edge: failed to write graph edge (fault-injected, vnc-035 AC-07)")
        return false;   // false-as-SQL-error → loop increments `failed`
    }
    write_graph_edge(store, src, tgt, rel, w, now, source, meta).await
}
```

Seam requirements (developer may substitute an equivalent — the CONTRACT is fixed):
- `#[cfg(test)]`-gated only; **zero** production overhead/branch when not compiled for test.
- Lets the test target ONE specific edge mid-loop (e.g. by `(target_id, relation_type)` or an
  Nth-call counter) so edges BEFORE it write successfully and persist on B.
- The injected failure returns `false` via the SQL-error path → `classify` yields `SqlError` →
  `failed += 1`, and a `warn!` fires. The loop CONTINUES (edges after it still attempt).
- Under (a) counting, the seam is also what makes `classify` return `SqlError` deterministically
  in the test (the only place a `false`-SQL-error is known to the loop).

Acceptable alternative seams: a thread-local / `OnceLock<Mutex<..>>` injection registry; a
trait-object write-fn swapped in tests; a feature-flagged store wrapper. Any seam that drives one
mid-loop write to `false`-SQL-error and lets the rest succeed satisfies AC-07.

## Data flow

- **Input:** `store`, `original_id` (A), `new_entry_id` (B). Called by `context_correct` at 8b′.
- **Internal:** `store.query_outgoing_edges(A)` → `Vec<OutgoingEdgeRow>`; per row →
  `carry_write_edge` (→ `write_graph_edge`) onto B.
- **Output:** `CarrySummary { found, carried, failed }`. The handler reads `carried` for the ack.

## Error handling (warn-and-continue — never rolls back, NFR-01 / ADR-002)

| Failure | Behavior |
|---------|----------|
| `query_outgoing_edges` returns `Err` | `warn!`; return `CarrySummary{0,0,0}`; correction stands. |
| Per-edge forward write SQL-error (`false`-Err) | `failed += 1`; `warn!` (internal); continue loop. |
| Per-edge UNIQUE conflict (`false`-Ok) | not counted, not failed; idempotent; continue (SR-02). |
| `Contradicts` reverse write SQL-error | accepted partial-write (ADR-003 vnc-015); warn-and-continue; not added to `failed`. |
| Unresolvable `relation_type` on a row (defensive) | `warn!`; skip row; not counted; continue. |

The function returns `CarrySummary` by value and **never** returns `Err` — there is no error
channel back to the handler, structurally guaranteeing no rollback (NFR-01).

## Key test scenarios (hints — not the test plan)

- **`test_carry_forward_continues_on_edge_copy_failure` (AC-07 / R-01 — MANDATORY, BY NAME):**
  fault-inject one mid-loop write to `false`-SQL-error. Assert: (1) `context_correct` returns
  success; (2) B Active, A Deprecated; (3) edges copied BEFORE the failing one persist on B;
  (4) `CarrySummary.failed` incremented AND a `tracing::warn!` fired. Gate 3b verifies present by name.
- **Happy path (AC-01):** A with eligible outgoing edges, `edges` omitted → all carried onto B;
  `carried == found`; rows exist with `source_id = B`.
- **Attach to B not A (AC-02):** no carried row has `source_id = A` (Deprecated).
- **Goal regression (AC-03):** A with only `Advances → vision_root` → `(B, vision_root, Advances)` exists.
- **Eligibility (AC-04):** mix incl. `Supersedes`/`CoAccess`/`Informs` → only agent-declared carry.
- **Contradicts (AC-06 / R-05):** outgoing `Contradicts → X` → both `B→X` and `X→B` exist exactly
  once; `carried` increments by **1** for the logical edge, not 2.
- **No ceiling (AC-09):** >50 eligible edges → all carry, no truncation, no ceiling warn.
- **created_at = now (R-11):** a carried edge's `created_at == now`, NOT the source row's; `source`/`created_by` == "agent".
- **query-Err path (R-01 #2):** force `query_outgoing_edges` to `Err` → `CarrySummary{0,0,0}`,
  correction still succeeds.
- **Idempotent re-pass count (R-02 #1):** an `edges` triple in 8b identical to a carried triple →
  8b′ write returns `false` (UNIQUE) → `carried` not incremented for it.
