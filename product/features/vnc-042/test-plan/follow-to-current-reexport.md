# Test Plan — `follow_to_current` visibility widen + re-export

**Component:** `crates/unimatrix-server/src/mcp/graph_read_neighbors.rs` (`:36-55`, `pub(super)` →
`pub(crate)`) + `crates/unimatrix-server/src/mcp/graph_read.rs` (re-export). **ONLY a visibility
change** — no behavioral edit to the primitive.
**Owns risks:** R-05 (Med). Supports AC-05 (reuse, no new walk).

This component has no new logic to unit-test; coverage is build/gate + regression + call-site.

---

## Build / gate expectations

- **BLD-01 (R-05):** `cargo build --workspace` **and** `cargo clippy --workspace -- -D warnings`
  green after the `pub(super)→pub(crate)` widen + re-export. No `dead_code` / unused-import /
  unreachable-`pub` warnings that would fail a `-D warnings` gate.
- Re-export path resolves: the handler calls `crate::mcp::graph_read::follow_to_current` and it
  compiles (the re-export from `graph_read.rs` exists and is correct).

## Call-site correctness (BLD-02, AC-05)

- **Grep assertion:** handler invokes the **canonical** `crate::mcp::graph_read::follow_to_current`
  (`graph_read_neighbors.rs:36` copy, Pattern #4436 fully-qualified path), NOT the duplicate at
  `graph_read_supersession.rs:122`. The duplicate is NOT consolidated in this feature (out of
  scope) — the risk is calling the wrong copy, so assert the path explicitly.
- `handle_current` (`graph_read_supersession.rs:86-103`) is **NOT** called — it errors on orphaned
  terminals and would violate AC-04.

## Regression — existing callers unchanged (BLD-03, R-05, C-3)

- **`graph_queries_tests.rs` hop-cap / orphan-guard suite MUST stay green, unchanged** — remains the
  authority for chain-walk correctness (50-hop cap + `status=0` active-terminal guard, load-bearing,
  #4538). vnc-042 adds only `context_get`-level exercises; it does NOT re-test chain correctness
  here.
- Existing `follow_to_current` callers (neighbors / subgraph modes) behave identically — their
  tests stay green with no edits. Widening visibility must not change the primitive's behavior for
  any caller.

## Coverage requirement (RISK §R-05)
Clean build with warnings-as-errors; canonical call-site confirmed by grep; existing
supersession-walk tests unchanged and green. No behavioral test is added at this layer — the
primitive is reused verbatim, and its correctness is already covered upstream.

## Explicitly out of scope
- Consolidating the two `follow_to_current` copies — flagged for future cleanup, not this feature.
- Any change to `handle_current` or `graph_read_supersession.rs`.
