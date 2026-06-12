## ADR-001: Carry-Forward Step Placement and Composition Order

### Context

vnc-035 adds an outgoing-edge carry-forward to `context_correct` (`tools.rs:1015`). The
handler already has an ordered post-correction edge pipeline:

- Step 8 — `store_ops.correct()` commits (B created Active, A deprecated).
- Step 8b — Phase B: writes `params.edges` onto B via `validate_and_write_edges` (vnc-015).
- Step 8c — `run_redirect_loop`: redirects incoming `E→A` to `E→B` (vnc-017).
- Step 9 — `confidence.recompute`.

The carry-forward must copy A's eligible **outgoing** edges onto B. Two sequencing questions
must be locked: (a) where carry runs relative to the existing `params.edges` write (8b) and
the incoming redirect (8c); (b) how carry composes with the passed `edges`.

The composition rule is settled (SCOPE OQ-01 / AC-08): **additive on the full triple**
`(source, target, relation_type)` — carry-forward is the baseline, passed `edges` upsert on
top; exact re-pass is idempotent, new edge adds, changed target produces a second edge.
Removal is only via the shed path. This ADR locks the *mechanism and order* that realize it.

### Decision

Insert a new step **8b′** `run_carry_forward_loop(store, A, B)` **between** step 8b
(`params.edges` write) and step 8c (incoming redirect):

```
 8.  store_ops.correct()
 8b. validate_and_write_edges(store, B, params.edges, now)   [EXISTING]
 8b′ run_carry_forward_loop(store, A, B)         ◄── NEW
 8c. run_redirect_loop(store, A, B)              [EXISTING]
 9.  confidence.recompute
 10. format response + edges_carried ack
```

**Composition is the carry baseline written second**, not first. `params.edges` (8b) write
first; carry (8b′) writes A's outgoing edges onto B afterward. Both write to the same target
id B with `INSERT OR IGNORE` on `UNIQUE(source_id, target_id, relation_type)`. The ordering of
8b vs 8b′ is immaterial to the *final edge set* (set union is commutative under idempotent
insert), but it is material to **counting**: see ADR-003/ADR-004 — `edges_carried` counts only
the `true` (new-insert) returns from the carry loop, so an edge the passed `edges` already
wrote in 8b is a UNIQUE conflict in 8b′ (`false`) and is correctly **not** double-counted.

Rationale for 8b′ **after** 8b: the SCOPE frames carry as the baseline and `edges` as the
upsert *on* it. Realizing "additive on triple" requires both writes to land on the same id
(B) and dedupe by the DB UNIQUE constraint. Writing carry last means `edges_carried` reflects
genuinely *carried* edges not already supplied by the caller — the ack tells the agent what it
would have lost, not what it re-passed (SR-02, AC-11).

Rationale for 8b′ **before** 8c: outgoing carry reads A's *outgoing* edges; incoming redirect
reads A's *incoming* edges. These sets are disjoint (a self-loop A→A is impossible — self-ref
edges are rejected at write time). Running carry before redirect guarantees no `Contradicts`
pair is touched by both loops in one correction (ADR-005, SR-06), and keeps each loop's posture
independently testable.

`run_carry_forward_loop` is a sibling of `run_redirect_loop` — same module (`tools.rs`),
`pub(super)` for test visibility, same warn-and-continue posture (ADR-002). It returns a
`CarrySummary { found, carried, failed }` consumed by step 10 for the `edges_carried` ack.

### Consequences

Easier: carry is a self-contained additive step; no existing step changes behavior. The
"additive on triple" rule (AC-08) falls out of the DB UNIQUE constraint rather than custom
diff logic. Outgoing-carry and incoming-redirect stay independently reasoned and tested.

Harder: a third post-correction loop adds inline latency to `context_correct` (one
`query_outgoing_edges` + N writes per correction). Bounded by the agent-declared-only
eligibility filter (ADR-002) — no ceiling needed (AC-09). The handler's post-correct section
grows; the loop body lives in a sibling function to respect the 500-line rule.

Related: ADR-002 (eligibility + posture), ADR-003 (count contract), ADR-004 (upsert
composition), ADR-005 (`Contradicts`). Mirrors vnc-017 `run_redirect_loop` (ADR-001/004 vnc-017).
