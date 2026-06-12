# Test Plan — `run_carry_forward_loop` + `CarrySummary` (carry orchestrator)

> Component: new `pub(super)` sibling of `run_redirect_loop` in
> `crates/unimatrix-server/src/mcp/tools.rs`. Tests live inline in `mod tests`
> (~line 9626), imported by path: `use crate::mcp::tools::{run_carry_forward_loop, CarrySummary};`.
> Extend the vnc-017 helpers `open_store_and_insert_active` / `insert_edge` — do not duplicate.
>
> Risks owned: **R-01** (Critical — warn-and-continue failure path), **R-02** (count contract),
> **R-05** (Contradicts bidirectional), **R-06** (self-loop disjointness), **R-08** (owns its
> write loop), **R-11** (created_at = now).

## Signature under test

```rust
pub(super) async fn run_carry_forward_loop(
    store: &Store, original_id: u64, new_entry_id: u64,
) -> CarrySummary;  // { found, carried, failed }
```
Count keys off `write_graph_edge` `true` (insert); `false` = UNIQUE conflict OR SQL error —
neither counted in `carried` (ADR-003, pattern #4041). Contradicts = two rows, counted once.

---

## ⚠️ R-01 (Critical) — MANDATORY, verified BY NAME at Gate 3b

### `test_carry_forward_continues_on_edge_copy_failure` (AC-07 / R-01 / SR-01 / lesson #4473) — MANDATORY

This test **must exist by name**. Warn-and-continue produces **no behavioral signal** if it
is absent — the feature behaves identically without it. vnc-017's identical AC was omitted and
**FAILed Gate 3b** (#4473). Gate 3b verifies presence **by name**, not by inferring from
passing happy-path tests.

- **Arrange**: seed original A with **≥2** eligible outgoing edges (`Supports → X`,
  `Supports → Y`). Insert a new Active entry B (the corrected entry). Engage the
  fault-injection seam so the carry write succeeds for the first edge and fails (`Err`/
  SQL-error → `write_graph_edge` `false`) for a later edge **mid-loop**.
  - Seam: extend the vnc-017 precedent `test_redirect_loop_correction_succeeds_when_redirect_fails`
    (tools.rs:10197) — rename `graph_edges`→`graph_edges_broken`, `CREATE VIEW graph_edges`
    so SELECT (`query_outgoing_edges`) succeeds, DML (INSERT) fails. Because the plain-view
    seam fails **every** write, achieving "edges before the failure persist" (assertion 3)
    requires either writing the first edge via a path engaged before the view is created, or a
    counted fault seam in the loop that fails only the Nth write. The implementation MUST
    expose whichever seam makes assertion 3 observable (brief constraint).
- **Act**: invoke the carry path — either `run_carry_forward_loop(&store, A.id, B.id)`
  directly, or the full `context_correct(A→B)` so the "correction returns success" assertion
  is end-to-end. (Prefer the end-to-end handler call for assertions 1–2; a direct loop call
  is acceptable for assertions 3–4 if the seam is loop-local.)
- **Assert (all four — by name in the test body)**:
  1. **Correction returns success** — `context_correct` returns `Ok`/success envelope; the
     carry failure never propagates as an error to the caller.
  2. **New entry Active + original Deprecated** — B status == Active, A status == Deprecated;
     the correction transaction is intact, not rolled back.
  3. **Edges copied before the failure persist** — the edge(s) written before the failing one
     remain on B in `graph_edges` (`source_id = B.id`).
  4. **`CarrySummary.failed` incremented + `tracing::warn!` fired** — `summary.failed >= 1`,
     and a `tracing::warn!` was emitted (assert via a `tracing` test subscriber / captured
     logs, mirroring how the redirect-fail test observes the warn posture).

### `test_carry_query_err_returns_empty_summary` (R-01 #2, failure-mode table) — REQUIRED
- **Arrange**: drive `query_outgoing_edges` itself to return `Err` (full table-rename-to-view
  seam — SELECT also fails if the view is malformed, or use the broken-table seam variant).
- **Act**: `run_carry_forward_loop(&store, A.id, B.id)`.
- **Assert**: returns `CarrySummary { found: 0, carried: 0, failed: 0 }` (mirrors
  `run_redirect_loop` returning `None`); a `tracing::warn!` fired; the correction (if called
  end-to-end) still succeeds. No panic, no propagated `Err`.

### `test_correction_committed_before_carry` (R-01 #3 / NFR-01) — REQUIRED
- **Arrange**: end-to-end `context_correct(A→B)` with the carry write seam forced to fail.
- **Act**: call the handler.
- **Assert**: B exists Active and A exists Deprecated in the store **even though** carry
  failed — proving the correction commit (step 8) precedes 8b′ and a carry failure cannot
  reach or undo it. (Overlaps assertion 2 of the mandatory test; kept as an explicit ordering
  guard.)

---

## R-02 — `edges_carried` count contract (count `true` inserts only)

### `test_carry_count_keys_off_true_only` (R-02 #2, R-08) — REQUIRED
- **Arrange**: seed A with N eligible outgoing edges, none pre-existing on B.
- **Act**: `run_carry_forward_loop(&store, A.id, B.id)`.
- **Assert**: `summary.carried == N`, `summary.found == N`, `summary.failed == 0`; exactly N
  rows with `source_id = B.id` in `graph_edges`. Doubles as the **R-08 guard**: an exact count
  is only possible if the loop owns its write loop and captures each `write_graph_edge` bool
  (cannot delegate the batch to `validate_and_write_edges`, which discards the bool —
  `edge_write.rs:152`).

### `test_carry_count_idempotent_repass` (R-02 #1, AC-08a) — REQUIRED
- **Arrange**: pre-write one of A's eligible triples onto B first (simulating 8b's
  `params.edges` write of the same triple), then seed A's outgoing edges.
- **Act**: `run_carry_forward_loop(&store, A.id, B.id)`.
- **Assert**: the already-present triple's carry write returns `false` (UNIQUE conflict) →
  **not** counted in `carried`; `carried` excludes it; one row only for that triple (no
  duplicate). Confirms a `false`-UNIQUE-conflict is not a carry and not a failure.

### `test_carried_edge_metadata_is_fresh_agent` (R-11, design-mandated check) — REQUIRED
- **Arrange**: seed A with one eligible outgoing edge whose `created_at` is a distinct **past**
  timestamp and `created_by`/`source` set to some original author; record a "now" boundary
  before the carry.
- **Act**: `run_carry_forward_loop(&store, A.id, B.id)`.
- **Assert**: the carried row on B has `created_at >= now_boundary` (the correction timestamp,
  **NOT** the source row's past `created_at`); `created_by`/`source == EDGE_SOURCE_AGENT
  ("agent")`; `weight == 1.0`; `bootstrap_only == 0`; `metadata == ""`. Byte-indistinguishable
  from a fresh agent declaration — guards against accidental preservation re-introducing a
  provenance marker (ADR-004 / FR-11).

---

## R-05 / R-06 — `Contradicts` bidirectional + self-loop disjointness

### `test_carry_contradicts_both_directions_exactly_once` (R-05 #1, AC-06) — REQUIRED
- **Arrange**: seed A with outgoing `Contradicts → X`.
- **Act**: `run_carry_forward_loop(&store, A.id, B.id)`.
- **Assert**: both `B→X` and `X→B` `Contradicts` rows exist on B, **exactly once each** — no
  duplicate, no orphaned reverse. Reuses `validate_and_write_edges` bidirectional structure
  (writes both rows) per ADR-005.

### `test_carry_contradicts_counts_one` (R-05 #3) — REQUIRED
- **Arrange**: seed A with one outgoing `Contradicts → X` (no other eligible edges).
- **Act**: `run_carry_forward_loop`.
- **Assert**: `summary.carried == 1`, **not 2** — one logical `Contradicts` = two rows but
  counts once (forward counted, reverse not). Confirms the loop special-cases Contradicts
  while keeping the count honest (ADR-003/ADR-005).

### `test_carry_redirect_contradicts_converge` (R-05 #2) — REQUIRED (loop-level half)
- **Arrange**: seed `Contradicts(A,X)` as `A→X` (A-outgoing) **and** `X→A` (A-incoming).
- **Act**: run `run_carry_forward_loop(A,B)` then `run_redirect_loop(A,B)` in pipeline order.
- **Assert**: carry re-homes `A→X` → `B→X` (+reverse `X→B`); redirect re-homes `X→A` → `X→B`;
  the `X→B` written by **both** converges via `INSERT OR IGNORE` to **one** row. No duplicate,
  no orphan. Validates disjoint row sets (A-outgoing vs A-incoming) + idempotent convergence.

### `test_self_referential_edge_rejected_at_write` (R-06 #1) — REQUIRED
- **Arrange/Act**: attempt to write an edge `A→A` (`source_id == target_id`).
- **Assert**: rejected at write time (the invariant ADR-005's disjointness proof rests on).
  This is a regression guard on the existing vnc-015 self-ref rejection, not new behavior.

### `test_carry_redirect_no_double_process_on_self_loop` (R-06 #2, defensive) — REQUIRED
- **Arrange**: if a self-loop somehow exists in `graph_edges` (force-insert bypassing the
  guard, or document why unreachable), run both loops.
- **Assert**: carry and redirect do not double-process or panic; the loops terminate cleanly.
  Defensive — documents that the disjointness guarantee degrades safely if the invariant is
  ever violated.

---

## Notes
- All tests `#[tokio::test]`, Arrange/Act/Assert, deterministic (no sleeps/races — see #4881).
- `tracing::warn!` assertions use a capturing subscriber, mirroring the vnc-017 warn-posture
  tests; do not assert on log strings beyond the warn-level firing.
- Handler-level concerns (ack envelope omission, pipeline order, shed, no-ceiling, tick
  visibility) live in **context_correct_handler.md**.
