## ADR-005: Per-Slug Registry Isolation Does NOT Subsume #925 — #925 Is Retained as an Independent Fix on a Different Plane

### Context

SR-09 requires an explicit verdict: does per-slug-registry-by-construction (this feature) make
#925's cycle-review foreign-session sweep redundant, or is #925 retained as defense-in-depth?
The two look like the same family — both are "same-name cross-contamination in `cycle_review`" —
but they are on **different planes, at different granularities, through different mechanisms**:

| | vnc-046 INV-T2 | #925 |
|---|---|---|
| **Plane** | Transcript-**candidate** plane (in-memory `SessionRegistry`) | **Metrics** plane (SQL over the per-slug `observations` store) |
| **Boundary crossed** | Cross-**SLUG** (two projects, co-hosted) | Cross-**FEATURE** within **one** slug's store |
| **Mechanism** | Global registry commingled all slugs' held buffers; fold joins on `SessionState.feature` with no slug scoping | `load_cycle_observations` two-stage query drops the `topic_signal = ?cycle_id` predicate at Step 3, admitting a whole foreign session on one text match |
| **Fix** | Per-slug registries — each `take_transcripts_for_feature` scans only that slug's sessions (structural, by construction) | Per-record topic attribution (`attribute_sessions`) — count a record only if it is itself in-topic |

Confirmation from the issues themselves: #925 states "the transcript-candidate plane was **not**
affected (it filters by `SessionState.feature`, `infra/session.rs:482`)" — i.e. the exact plane
vnc-046 isolates per-slug is the plane #925 declares out of its scope. And #925's metrics plane
reads the per-slug `observations` store via SQL, which is **already** slug-isolated (vnc-034
per-slug DB + vnc-038 write funnel) — so the two overlapping sessions in #925's real-world case
(vnc-044 sweeping in nxs-014 records) are in the **same** project store; per-slug registries
give them the same registry and would not touch that leak.

### Decision

**Per-slug registry isolation does NOT subsume #925. #925 is retained as an independent,
still-open fix.** The two mechanisms are **orthogonal layers on different planes**, not two
overlapping mechanisms for one property:

- vnc-046 closes the **cross-slug transcript-candidate** case (INV-T2): a co-hosted slug B can
  never fold slug A's transcript, even under an identical `{phase}-{NNN}` name. Structural, by
  construction.
- #925 closes the **cross-feature metrics-plane** case **within** a slug: a concurrent session
  working feature Y in the same project cannot inflate feature X's metrics (hotspots,
  `mutation_spread`, `adr_count`, Phase Timeline) by referencing X's id once. Per-record topic
  attribution.

Neither's fix is reachable from the other's mechanism: per-slug registries do nothing for the
in-store SQL predicate, and per-record topic attribution does nothing for the in-memory
cross-slug registry commingling. Shipping both is **not** shipping two overlapping mechanisms —
they cover disjoint leak paths. This feature does **not** close, absorb, or block #925; #925
stays open on its own track. Per the standing norm (don't auto-file/close outward commitments),
the human owns any #925 close/keep call — this ADR's verdict is **keep**.

INV-T2's behavioral test (ADR-004) exercises the cross-**slug** transcript case only; it does
**not** cover #925's cross-feature metrics case, and must not be read as doing so (a false
subsumption would leave #925's metrics leak unguarded behind a green vnc-046 suite).

### Consequences

- **Easier:** the two fixes compose cleanly — after both land, `cycle_review` is isolated on
  both the transcript plane (across slugs) and the metrics plane (across features within a slug).
  No double-proof, no reconciliation debt.
- **Easier:** no wasted work — vnc-046 does not attempt a metrics-plane fix it cannot reach, and
  #925 is not deferred on a false "vnc-046 will handle it" assumption.
- **Harder:** two issues stay live where a naive reading hoped for one; the distinction
  (plane × granularity) must be stated in the PR so a reviewer does not close #925 as
  "subsumed."
- **Watch:** if #925's preferred fix (`attribute_sessions`) is later generalized to the
  transcript plane, revisit — but today the transcript plane is already correct per-slug after
  vnc-046, so there is nothing for #925 to add there.

Related: SR-09, #925 (metrics-plane per-record attribution), ADR-001/002 (the transcript-plane
per-slug isolation this is distinguished from), ADR-004 (INV-T2 scope boundary),
`infra/session.rs:482` (`SessionState.feature` fold key), `services/observation.rs`
(`load_cycle_observations` two-stage query).
