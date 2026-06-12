## ADR-002: `query_outgoing_edges`, Single-Source Eligibility Predicate, and Warn-and-Continue Posture

### Context

No symmetric `query_outgoing_edges(source_id)` exists (SR-03). Carry-forward needs one, and it
must replicate the incoming-redirect's eligibility precedent so the two cannot drift and
silently carry ineligible edges. The eligible set is settled (SCOPE OQ-02 / AC-04 / AC-09):
**agent-declared edges only** — exclude derived `Supersedes` and the tick-generated
`CoAccess`/`Informs`. There is **no outgoing ceiling**; all eligible edges always carry.

`query_incoming_edges` (`read.rs:1694`) excludes only `Supersedes` at the SQL level (ADR-002
vnc-017), because `CoAccess`/`Informs` are not relevant to redirecting incoming edges at a
correction target. For the **outgoing** direction from a hub entry, `CoAccess`/`Informs` *are*
present and must be excluded — so the outgoing predicate is a **superset** exclusion, not a
copy. This legitimate difference is exactly the drift trap SR-03 warns about: it must be
expressed once, in one place, with the rationale documented so a future reader does not
"fix" it into false symmetry.

vnc-017's warn-and-continue posture (ADR-003/004 vnc-017) and the lesson that its failure-path
test was silently omitted (lesson #4473, SR-01) both apply here.

### Decision

**1. New `query_outgoing_edges` in `unimatrix-store`, mirroring `query_incoming_edges`:**

```rust
pub async fn query_outgoing_edges(&self, source_id: u64) -> Result<Vec<OutgoingEdgeRow>>

pub struct OutgoingEdgeRow {
    pub target_id: u64,
    pub relation_type: String,
    pub created_at: u64,
}
```

SQL — eligibility enforced **at the SQL level** (mirroring vnc-017 ADR-002), the **single
source of truth** for the outgoing predicate:

```sql
SELECT target_id, relation_type, created_at
FROM graph_edges
WHERE source_id = ?1
  AND relation_type NOT IN ('Supersedes', 'CoAccess', 'Informs')
```

Bind `source_id as i64` (the codebase convention). Use `read_pool()`. The predicate's three
exclusions are documented inline with the same depth as `query_incoming_edges`' `Supersedes`
comment, and explicitly note: **this is a superset of the incoming exclusion — the difference
is intentional, not drift** (`CoAccess`/`Informs` are outgoing-relevant from hub entries but
not incoming-relevant to a correction target).

**2. Eligibility is the safety basis for "no ceiling" (AC-09, SR-04).** Documented invariant:
the absence of an outgoing ceiling is valid **only** while eligibility = agent-declared-only.
The exclusion list bounds agent-declared out-degree. Any future defense is a **high-threshold
observability warning that still carries every edge** — never a truncating cap (OQ-02). A new
agent-declarable type added to the engine taxonomy (`graph.rs:139`) carries automatically
(accepted, SCOPE Assumptions).

**3. Warn-and-continue posture parity (ADR-003 vnc-015 / vnc-017), SR-01.**
`run_carry_forward_loop` never aborts or rolls back the correction:

- `query_outgoing_edges` returns `Err` → `warn!`, return `CarrySummary { found:0, carried:0,
  failed:0 }`. Correction already committed (mirrors `run_redirect_loop` returning `None`).
- A per-edge write fails (SQL error → `write_graph_edge` returns `false` after warning
  internally) → increment `failed`, continue the loop. The correction and all
  already-carried edges persist.

**The failure path is observable and testable** (SR-01, lesson #4473): the `failed` counter and
the returned `CarrySummary` give the test a signal to assert. The spec MUST name an explicit
per-edge-copy-failure test asserting "correction + already-carried edges persist when one carry
write fails." This is the highest-probability gate rejection (#4473 precedent).

### Consequences

Easier: one SQL predicate is the sole definition of outgoing eligibility; it cannot diverge
from a parallel Rust filter because none exists. "No ceiling" rests on a stated, documented
invariant. Posture is identical to vnc-017, so reviewers reason about it once.

Harder: the outgoing and incoming predicates legitimately differ (superset), so a careless
reader might "align" them and reintroduce `CoAccess`/`Informs` carry — mitigated by the inline
rationale comment. The failure-path test produces no behavioral signal if omitted (SR-01) — it
must be verified by name at the gate.

Related: ADR-001 (placement), ADR-003 (count), ADR-005 (`Contradicts`). Mirrors vnc-017
ADR-002 (Supersedes-at-SQL), ADR-003 (source-validation posture), ADR-004 (ceiling — here
*absent* by design). Lessons #4473 (missing failure test), #4526 (tick staleness).
