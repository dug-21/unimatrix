## ADR-003: Enforce eager ⊆ tick as an Executable Invariant

### Context

Two delete passes now touch `graph_edges` with different keys: the eager delete keys on entry id + provenance (`(source_id=? OR target_id=?) AND source='agent'`); the tick keys on status (`source_id NOT IN Active OR target_id NOT IN Active`, all sources). Multi-pass cleanup on the same table with divergent scope is a known ghost-record / divergence bug class — bugfix-458 (#3910), bugfix-879 (#5417). The load-bearing safety property is **eager ⊆ tick**: every edge the eager path removes must also be removed by the tick, so the eager path can only ever do a subset of the backstop's work early.

The property holds because, after the flip, the deprecated entry is non-Active with no successor: tick Phase 1 repoints nothing (no successor), tick Phase 2 deletes every edge touching the entry (all sources), and the eager delete removes only the `agent` subset. It breaks in exactly one case: if the eager delete runs on a Deprecated entry **with a successor**, Phase 1 would repoint (keep) an inbound agent edge the eager delete destroys. The chokepoint prevents this structurally — `context_deprecate` → `deprecate_with_audit` never sets `superseded_by`; `correct_entry` is excluded.

Asserting this only in prose is what SR-02 flags as High risk. It must be testable and must catch a future widening of the eager predicate **or** narrowing of the tick.

### Decision

Make the invariant executable via a test that runs **both real functions** over parallel fixtures — do not re-implement either predicate in the test:

1. Seed entry `e` with one edge per (direction × source): inbound/outbound × {`agent`, `nli`, `co_access`, `cosine_supports`, `S1`, `S2`, `S8`}.
2. Fixture A: bare-deprecate `e`, run the eager helper, capture the removed tuple set `R`.
3. Fixture B (identical seed): bare-deprecate `e`, run `run_orphaned_edge_compaction`, capture the removed tuple set `T`.
4. Assert `R ⊆ T` **and** `R` equals exactly the two `agent` edges (inbound + outbound).

Companion structural assertion: `context_deprecate` leaves `superseded_by` NULL after the flip, so the eager path never sees a successor-bearing entry. Do **not** add a runtime `superseded_by IS NULL` clause to the eager SQL — the predicate stays LOCKED per ADR-002; the structural guarantee plus this test are the enforcement.

Rationale: because step 4 uses the actual eager helper and the actual compaction, widening the eager predicate to a machine source breaks the exact-set assertion, and narrowing the tick so it keeps agent edges breaks `R ⊆ T`. Either divergence fails the test rather than shipping green. The per-source seeding also discharges SR-03 (proves exactly which sources are eagerly removed).

### Consequences

Easier: any future edit to either predicate that would cause divergence fails a behavioral test rooted in real state, not a call-count or string check (#5427). The subset relationship is documented once and enforced continuously.

Harder: the test depends on both the eager helper and the tick being invokable against a shared fixture (test infrastructure is cumulative — extend existing background-tick fixtures). If the tick predicate is legitimately changed in future, this test must be re-derived, not deleted — that re-derivation is the intended re-check point (SR-02, SR-05).
