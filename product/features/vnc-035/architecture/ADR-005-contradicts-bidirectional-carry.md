## ADR-005: `Contradicts` Bidirectional Carry and Disjointness from Incoming Redirect

### Context

`Contradicts` is bidirectional. AC-06 requires carried `Contradicts` outgoing edges to carry
forward with both directions consistent, reusing existing helpers. Two integration hazards:

- **Double-write / orphan (SR-06):** the carry loop (8b′) and the incoming-redirect loop (8c)
  must not both act on the same `Contradicts` pair in one correction. vnc-017's
  `redirect_graph_edge` writes 4 rows atomically for `Contradicts`; carry uses
  `validate_and_write_edges`, which writes both `A→B` and `B→A` (`edge_write.rs:211`). If both
  loops touch one pair, edges could be doubly written or a reverse orphaned (#4459).
- **Source validation:** carried `Contradicts` writes a reverse edge *originating from the
  carry target* — the new entry B, which is freshly Active. The vnc-017 source-validation guard
  (#4459) exists because incoming redirect can have Quarantined/Deprecated sources; for carry
  the source is always B (Active by construction).

### Decision

**1. Reuse `validate_and_write_edges` for `Contradicts` bidirectional handling.** It already
writes both directions (`B → X` and `X → B`) for `Contradicts` (`edge_write.rs:211-223`) using
`write_graph_edge` + `INSERT OR IGNORE`, the accepted partial-write posture (ADR-003 vnc-015).
The carry loop does not re-implement bidirectionality.

Reconciling with ADR-003 (carry owns its write loop for counting): the carry loop iterates
eligible rows and, **per row, delegates the actual write to the shared primitive** —
`write_graph_edge` directly for unidirectional relations (capturing the `true`/`false` for the
count), and for `Contradicts` it writes the forward direction via `write_graph_edge` (counted)
then the reverse direction via `write_graph_edge` (the bidirectional partner, **not** counted
toward `edges_carried` — `edges_carried` counts logical edges, and a `Contradicts` is one
logical edge materialized as two rows). This mirrors `validate_and_write_edges`' own structure
rather than calling it as a black box, so the count stays exact (ADR-003).

**2. Disjointness guarantee (SR-06).** Carry (8b′) reads A's **outgoing** edges
(`query_outgoing_edges(A)` → rows where `source_id = A`); incoming redirect (8c) reads A's
**incoming** edges (`query_incoming_edges(A)` → rows where `target_id = A`). For a single
`Contradicts` pair `(A, X)` stored as rows `A→X` and `X→A`:

- `A→X` is in A's **outgoing** set → handled by carry (8b′), re-homed as `B→X` (+ reverse `X→B`).
- `X→A` is in A's **incoming** set → handled by redirect (8c), redirected to `X→B`.

These are different rows handled by different loops; **no row is touched by both**. The only way
a row could be in both sets is a self-loop `A→A`, which is impossible — self-referential edges
are rejected at write time (ADR-001/002 vnc-015). The carry runs before redirect (ADR-001), and
because they operate on disjoint row sets the ordering is for clarity, not correctness. The net
`Contradicts(B, X)` pair ends up consistent: carry establishes `B↔X` and redirect points the
old `X→A` at `X→B` (a UNIQUE conflict with the reverse carry already wrote — idempotent).

**3. Source validation is unnecessary for carry's writes.** Every carried edge originates from
B, which is freshly Active (terminal-active by construction, ADR-001 vnc-017). The #4459 guard
(skip Quarantined/Deprecated sources) does not apply — there is no invalid source on the carry
side. No source-status check is needed in the carry loop. (The incoming-redirect loop keeps its
own #4459 guard unchanged.)

### Consequences

Easier: `Contradicts` bidirectionality reuses the shipped, tested primitive; no new
bidirectional logic. Carry and redirect provably never collide because they read disjoint
row sets. No source-validation guard needed in carry (source is always Active B).

Harder: the carry loop must special-case `Contradicts` (two `write_graph_edge` calls, count one
logical edge) to keep `edges_carried` honest — slightly more loop logic than a blind delegate.
The reverse `Contradicts` rows written by carry (8b′) and by redirect (8c) overlap by design and
rely on `INSERT OR IGNORE` idempotency to converge — a `Contradicts` carry test (AC-06) must
assert both final directions exist exactly once.

Related: ADR-001 (8b′ before 8c), ADR-003 (count = logical edges), ADR-002 (posture). Reuses
`validate_and_write_edges` (vnc-015 AC-06, `edge_write.rs:211`). Patterns #4459 (Contradicts
source-validation), #4041 (rows-affected). SR-06.
