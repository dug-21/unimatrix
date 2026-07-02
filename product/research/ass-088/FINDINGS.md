# FINDINGS: Edge-consistency under correction — synchronous-redirect vs read-time resolution vs background-tick convergence

**Spike**: ass-088
**Date**: 2026-07-02
**Approach**: investigation + design (with a snapshot-derived back-of-envelope degree probe)
**Confidence**: directional

---

## Executive summary (read this first)

The working hypothesis holds **with two corrections**:

1. **The residual is worse than "stale pointer."** The per-tick `run_orphaned_edge_compaction`
   job (`background.rs:832`) issues `DELETE FROM graph_edges WHERE ... target_id NOT IN
   (SELECT id FROM entries WHERE status=Active)`. A Deprecated entry still has a row but is
   not `Active`, so **every edge pointing at a corrected entry that the synchronous redirect
   loop did not reach is physically DELETED at the next tick** — the referrer→terminal link is
   *lost*, not left dangling. #744's past-ceiling referrers therefore don't accumulate orphan
   pointers; they silently **lose their edges**. Convergence must **repoint before compaction
   deletes**, or hot-node referrers past 50 are dropped on every correction.

2. **Reads do not fully self-heal today.** `context_get` resolves the anchor (vnc-042,
   default-on) but its surfaced edge *targets* are rendered raw (NG-1, confirmed in
   `get_edges.rs`), and `context_graph`'s `resolve_supersessions` is **default-OFF**
   (`graph_read_subgraph.rs:158`, `graph_read_neighbors.rs:164`). The "reads self-heal"
   premise is only true for `context_get`'s anchor. The hybrid requires *finishing* the read
   cover, not merely relying on it.

With those corrections, the recommended division of labor is: **write stays cheap · reads
resolve on demand (completed) · a bounded per-tick sweep converges the stored graph, running
before compaction so links are repointed not deleted.** The sweep is scan-driven with a `LIMIT`
batch, which subsumes **#744** and the **inbound** half of the historical corpus for free.
**#745's outbound half does NOT fully collapse** (deleted rows cannot be repointed — see Q6).

---

## Findings

### Q: "Enumerate the full failure surface. Catalog every way an edge goes stale under correction/quarantine ... State which are live (going-forward) vs historical (existing-corpus) drift."

**Answer** — catalog below. The single most important, previously-unstated fact: the residual
for unhandled inbound edges is **deletion at the next compaction tick**, not a persistent stale
pointer. `run_orphaned_edge_compaction` deletes any edge whose source or target is not `Active`.

| # | Failure mode | Mechanism | Residual after next tick | Live / historical |
|---|---|---|---|---|
| F1 | **Inbound referrers past `REDIRECT_CEILING=50`** (#744) | `run_redirect_loop` (`tools.rs:5207`) caps at 50 (`tools.rs:45`), `warn!`s, truncates. | The 51+ edges point at Deprecated X until the next tick, then are **DELETED** by compaction. Referrer loses its link to X′ entirely. | **LIVE** |
| F2 | **Per-edge redirect SQL failure under the ceiling** | `redirect_graph_edge` `Err` → `failed++`, warn-and-continue (`tools.rs:5312`). | Edge left on Deprecated X → **DELETED** next tick. Same harm as F1, smaller N. | **LIVE** |
| F3 | **Redirect skips quarantined/deprecated *sources*** | `run_redirect_loop` skips sources not Active (`tools.rs:5266`). | Edge from a non-Active source is compaction-deleted (source not Active). Correct — no repoint target semantics. | LIVE, benign |
| F4 | **Outbound edge drops on correction** (#745) | Pre-vnc-035 `context_correct` did not carry A's outbound edges; they were dropped. | vnc-035 `run_carry_forward_loop` (`tools.rs:5543`) now carries **all** eligible outbound, **no ceiling** → **going-forward CLOSED**. Residual is pre-vnc-035 rows only. | **HISTORICAL** |
| F5 | **Stale neighbor *targets* in `context_get` edge list** (NG-1) | `build_edges_view` (`get_edges.rs:44`) renders stored `target_id` + raw title via `fetch_titles_batch`; **no supersession resolution**. | A resolved get of X′ lists edge targets that may themselves be Deprecated, with old titles. Label staleness (not a wrong anchor). | **LIVE** (discovery-label) |
| F6 | **`context_graph` renders unresolved endpoints by default** | `resolve_supersessions` defaults to `false` (`graph_read_subgraph.rs:158`, `graph_read_neighbors.rs:164`). | Any caller (traversal, briefing edge-walk, a gating decision) that omits the flag sees Deprecated endpoints/neighbors. | **LIVE** |
| F7 | **Entry quarantined *after* edges exist** | Quarantine sets status→Quarantined; inbound edges remain. New edges *to* a quarantined target are rejected at write validation (`tools.rs:1234`), but pre-existing ones are not touched. | Compaction deletes them (target not Active). Quarantine has no `superseded_by` → **no repoint target exists** → deletion is the only valid convergence. Acceptable. | LIVE, deletion-only |
| F8 | **Supersession cycle / self-loop** | Chain-walk capped at 50 hops via CTE (#4468); `typed_graph_rebuild` sets `use_fallback` on a detected cycle (`background.rs:884`). | `follow_to_current` caps → `None`; convergence must **skip** (no valid terminal), leaving the edge resolvable-at-read / compaction-deleted, never crash. | LIVE, bounded |
| F9 | **Chain dead-ends on a non-Active terminal** (vnc-042 ADR-002 case) | `superseded_by` chain ends on Deprecated/Quarantined. `follow_to_current` → `None`. | No valid repoint target. Convergence must skip; read side already fails loud (returns requested id). | LIVE, edge case |
| F10 | **Cross-slug edges** | Per-slug store = one DB per slug (vnc-034); `graph_edges` lives inside a slug DB; tick is per-slug (`jobs.rs` `PerSlugTickContext`). | **Cannot occur** — no cross-slug edge rows exist. Convergence is inherently per-slug. | Non-issue (confirm) |

**Live vs historical rollup:**
- **LIVE, needs the new mechanism:** F1, F2 (inbound ceiling/failure → link loss), F5, F6 (read-label staleness).
- **HISTORICAL, existing-corpus only:** F4 (#745 outbound drops).
- **Bounded / non-issue:** F3, F7 (deletion-only is correct), F8/F9 (skip-on-no-terminal), F10 (impossible).

**Evidence**: `run_redirect_loop` + `REDIRECT_CEILING` (`tools.rs:45,5207-5350`);
`run_orphaned_edge_compaction` delete-not-repoint (`background.rs:832-859`);
`run_carry_forward_loop` no-ceiling outbound carry (`tools.rs:5543`);
`build_edges_view` raw target render (`get_edges.rs:44-101`);
`resolve_supersessions` default-false (`graph_read_subgraph.rs:158`).

**Recommendation**: Treat the catalog as two workstreams — **stored-graph convergence** (F1, F2)
owned by the tick, and **read-label completion** (F5, F6) owned by the read path. F7/F8/F9 are
"skip when no valid terminal" rules the convergence code must encode; F10 needs only a per-slug
scope assertion in the design.

---

### Q: "Evaluate the three strategies against each failure mode — synchronous-at-write, read-time resolution, background-tick convergence — on correctness guarantee, cost profile, scale behavior, failure/partial-work modes, and interaction with the in-memory hot path."

**Answer**:

| Dimension | Synchronous-at-write | Read-time resolution | Background-tick convergence |
|---|---|---|---|
| **Correctness** | Immediate stored consistency — *when it completes*. Ceiling caps it (F1). | Eventual, on-demand; stored graph never changes (drift persists). | Eventual, bounded by SLO; stored graph actually converges. |
| **Who pays** | Writer, O(in-degree) per correction. | Reader, per-read resolution tax, forever. | Tick, amortized, bounded batch. |
| **Scale** | **Worst** for hot nodes — cost grows with in-degree; the ceiling exists *because* hot-node fan-out is expensive. Snapshot: 5 nodes already exceed 50 (max 110) at ~4k entries. | Neutral to corpus; scales with read volume. Cost is permanent. | Amortized; per-tick work is `LIMIT`-bounded regardless of corpus (addresses #625). |
| **Partial work** | Truncation (F1) / per-edge failure (F2) → link deleted next tick. Silent. | N/A (no mutation). | Graceful: incomplete batch drains next tick; interim edges stay resolvable-at-read (principle #5). |
| **Hot path (#7)** | None at write (rebuild is tick-side). | Read cost only. | Repointed edges feed `typed_graph_rebuild` — must run **before** rebuild in the same tick → one in-memory swap, no extra rebuild. |

**No single winner** — the intended answer is the mechanism-per-failure split:

- **Synchronous** cannot own correctness for hot nodes (its ceiling is the defect). Keep it, if
  at all, only as a best-effort fast-path for small fan-in; it must stop being the guarantee.
- **Read-time resolution** is the correct **interim-window cover** and the right home for
  discovery-label freshness (F5, F6) — but it is a permanent tax and, critically, **does not
  repair the stored graph**, so compaction still deletes hot-node links. It cannot be the sole
  home.
- **Background-tick** is the right home for **stored-graph convergence** (F1, F2): it is the
  only strategy that both keeps writes cheap *and* preserves the referrer→terminal link at
  enterprise scale.

**Evidence**: degree distribution from `snapshot-combined.db` (probe below); strategy costs
read directly off the three code paths; hot-path sequencing from the Job registry order
(`jobs.rs` Jobs 2→3→4).

**Back-of-envelope degree probe** (snapshot-derived, `product/research/ass-038/harness/snapshot-combined.db`, ~3,983 entries / 7,155 edges):
- Non-Supersedes in-degree: **max 110**, avg 8.07 over 872 targeted nodes.
- **5 nodes already exceed `REDIRECT_CEILING=50`** (110, 71, 61, 56, 52).
- Correcting the degree-110 node today: `run_redirect_loop` repoints 50, `warn!`-truncates 60;
  those 60 edges are DELETED at the next compaction tick. Heavy-tailed (power-law) distribution
  ⇒ at ~10× corpus expect the count of over-ceiling nodes to grow roughly linearly (order tens),
  and the max degree of hot nodes (`vision_root`, core ADRs) to grow super-linearly. #744's scale
  concern is **already realized**, not hypothetical.

**Recommendation**: Adopt the hybrid. Do not raise or make-configurable the ceiling (a #744
option) — that only defers the same failure. Move correctness off the write path.

---

### Q: "Recommend the division of labor. Confirm or refute the working hypothesis."

**Answer**: **Confirmed, with two required corrections.**

- **Write stays cheap** — `context_correct` deprecates + sets `superseded_by` (already atomic in
  `store_ops.correct`). The synchronous `run_redirect_loop` is **demoted from guarantee to
  optional fast-path**; correctness no longer depends on it. (Simplest: keep it uncapped-but-
  time-boxed for instant small-fan-in repoint, or remove it — an ADR choice, see Unanswered.)
- **Reads resolve on demand — but the cover must be COMPLETED** (this is correction #2):
  1. Flip `context_graph` `resolve_supersessions` to **default-on** (at minimum for the
     briefing/injection and any gating consumers), matching `context_get`'s vnc-042 default.
  2. Resolve edge-*target* labels in `build_edges_view` (NG-1 / F5) so a resolved get shows
     terminal neighbors.
- **A bounded per-tick sweep converges the stored graph** — repoints referrer edges to terminals,
  and (correction #1) **runs before `run_orphaned_edge_compaction`** so links are repointed, not
  deleted.

**Refutation of the naive form**: the hypothesis as written ("reads self-heal → eventual is
safe") is *incomplete* — `context_graph` default-off and NG-1 mean some consumers see stale data
today. The hybrid is right, but only after the read cover is finished.

**Evidence**: `store_ops.correct` audit+atomicity path; the read-path defaults cited in F5/F6;
compaction-before-rebuild ordering (`jobs.rs`).

**Recommendation**: Ship as three coordinated changes (write demotion, read-cover completion,
convergence sweep) under one ADR; they are only jointly correct.

---

### Q: "Rule the guarantee semantics. Is there any consumer that needs immediate stored-graph consistency? Define a convergence SLO."

**Answer**: **No consumer needs immediate *stored* consistency that cannot be served by read
resolution.** The guarantee shifts to eventual, safely.

Consumer audit:
- **`typed_graph_rebuild` / PPR substrate** — reads edges from DB and rebuilds the in-memory
  `Arc<RwLock<TypedRelationGraph>>` each tick. It is itself tick-driven; if convergence runs
  earlier in the *same* tick, the rebuild sees converged edges same-cycle. It does not need
  sub-tick consistency.
- **`context_graph` gating / briefing edge-walk** — resolvable at read *once* F6 is fixed
  (default-on). The apparent need for immediate consistency here is really the read-cover gap,
  not a write-side requirement.
- **`context_get`** — already resolves the anchor (vnc-042); F5 fixes its edge labels.

So eventual is safe **conditional on completing the read cover**. No path requires synchronous
stored redirect.

**Convergence SLO (concrete, testable, load-stable):**
- **Unit: ticks / corrections-processed**, NOT wall-time (wall-time SLOs flake — cf. tick-timing
  flakes #790/#833).
- **Statement:** *"After a correction, every non-Supersedes referrer edge to the deprecated
  entry is repointed to its terminal within **1 tick**, provided the tick's convergence batch
  budget `B` covers the pending backlog; under a burst exceeding `B`, the backlog drains
  monotonically within `⌈pending / B⌉` ticks. At every instant before convergence completes, the
  stale stored edge is resolvable-at-read, so no consumer observes a dangling or wrong
  reference."*
- **Testable, deterministic:** force N corrections, force one tick, assert zero non-Supersedes
  edges target any deprecated id whose fan-in ≤ `B`; for fan-in > `B`, assert the count strictly
  decreases each forced tick until zero. Tick-count assertions — no wall clock, no flake.
- **Degradation bound (principle #5):** the *stored* graph may lag by ≤ the drain window; the
  *observable* graph (through the completed read cover) never lags. Fail-loud-not-broken holds:
  an incomplete batch leaves resolvable-at-read state, never a worse one.

**Evidence**: Job ordering and `Arc<RwLock>` rebuild (`background.rs:865`, `jobs.rs` Job 4);
vnc-042 anchor resolution; flake history referenced in scope.

**Recommendation**: Adopt the tick-count SLO with `N=1` tick target for the common case. Set `B`
in the ADR (see Unanswered — needs a correction-rate measurement to tune; a conservative starting
`B` of a few hundred edges/tick/slug covers the observed max degree of 110 in a single tick).

---

### Q: "Design the tick (mechanism sketch, enough to cost it)."

**Answer**: **Fold into the existing per-slug tick.** Two viable shapes; recommend the first.

**Primary — extend compaction into repoint-then-compact (minimal change).**
Replace `run_orphaned_edge_compaction`'s single delete with a two-step, same-job pass:
1. **Repoint** (bounded): for each non-Supersedes edge whose target is Deprecated *with*
   `superseded_by` set, repoint to the CTE-resolved terminal, `LIMIT B`.
2. **Compact** (existing): delete the residual — edges whose target/source is non-Active *and
   has no valid terminal* (quarantined, dead-end chains F7/F9).
This guarantees repoint precedes delete structurally (they are the same job), and the job already
sits **before** `typed_graph_rebuild` (Job 4), so PPR sees converged edges same-tick.

**Alternative — new `EdgeConvergenceJob` inserted before `OrphanedEdgeCompactionJob`.** Same
effect, cleaner separation, one more job in the registry. Choose in the ADR.

Design points:
- **Placement / ordering:** convergence **before** compaction (else delete wins) and **before**
  typed-graph rebuild (else PPR lags a tick). Fits the existing invariant chain
  maintenance → compaction → promotion → rebuild (`jobs.rs` Jobs 1–4).
- **Idempotency:** reuse `redirect_graph_edge` (DELETE old + `INSERT OR IGNORE` new, RAII txn)
  — already idempotent via `UNIQUE(source_id, target_id, relation_type)` (`db.rs:963`). Re-running
  a converged edge is a no-op. Same infallible-per-direction contract as pattern #3897 /
  crt-034/035.
- **Bounded work — scan-driven with `LIMIT`:**
  `SELECT e.source_id, e.target_id, e.relation_type FROM graph_edges e JOIN entries t ON
  e.target_id = t.id WHERE t.status = Deprecated AND t.superseded_by IS NOT NULL AND
  e.relation_type != 'Supersedes' LIMIT B`. Covered by `idx_graph_edges_target_id` +
  `idx_entries_superseded_by` (both exist, `db.rs:972,985`). Never a full-corpus scan; drains
  over ticks (answers #625's unbounded-scan concern). Repoint-target computed via the SQL
  recursive CTE (#4468), **never** in-memory `find_terminal_active` (avoids the cold-cache
  staleness and lock dependency).
  - **Free historical backfill:** because the scan keys off *current* Deprecated-with-successor
    state, not correction date, it sweeps the **inbound** existing-corpus drift with zero extra
    mechanism — no separate one-shot pass for the inbound half of #745.
- **Alternative bounding — work-queue:** a correction enqueues its deprecated id; the tick pops
  ≤`B` ids and repoints each's referrers. Bounds work to actual corrections (tighter under low
  correction volume) but adds a queue table + write-path coupling. Recommend scan-driven as
  primary (no coupling, self-healing, backfills for free); queue is the enterprise-scale
  optimization if the scan's cost ever dominates.
- **Ordering vs concurrent corrections:** tick and `context_correct` both write `graph_edges`
  via `write_pool_server` (per-slug single-writer pool). A correction landing mid-tick simply
  seeds rows the next tick's scan finds. Per-edge RAII txns mean no cross-edge atomicity is
  needed; idempotent repoint makes double-processing safe.
- **Audit-log volume (principle #2) — KEY FINDING:** edge-table mutations are **not audited
  today**. `write_graph_edge`, `redirect_graph_edge`, and `run_orphaned_edge_compaction` append
  **zero** `audit_log` rows; only entry-level operations (store/correct/deprecate/quarantine via
  `store_ops` → `log_audit_event`) are audited. Edges are not integrity-chain state. The
  convergence tick, following existing precedent, writes **no per-edge audit**. If any audit
  signal is desired, emit **one summary event per tick per slug** ("converged N edges across M
  deprecated targets") — never per-edge. This satisfies "append-only and complete" at the
  operation granularity that already governs edges, with zero bloat.
- **Hash-chain (principle #1) — CONFIRMED CLEAN:** `graph_edges` is a standalone table
  (`db.rs:952`) with no FK to `entries`, no hash column, and no participation in the entry hash
  chain. Repointing is INSERT/DELETE on `graph_edges` only; it touches zero entry rows and zero
  chain state. Principle #1 is **not engaged**. (The only cross-table constraint is correctness:
  compute terminals via the CTE per #4468.)
- **In-memory hot path (#7):** converged edges are consumed by `typed_graph_rebuild` from DB
  each tick. Running convergence earlier in the same tick means one existing rebuild picks them
  up — **no separate invalidation, no extra rebuild**. Sequencing *is* the mechanism.

**Evidence**: `jobs.rs` registry + `background.rs:832,865`; `db.rs` DDL (edges standalone, indexes,
audit triggers on `audit_log` only); `redirect_graph_edge` (`edge_write.rs:305`); #3897, #4468.

**Recommendation**: Implement the repoint-then-compact extension of Job 2, scan-driven with
`LIMIT B`, CTE terminal resolution, no per-edge audit (optional per-tick summary), per-slug scope
asserted. This is an *extension of proven infra*, not greenfield.

---

### Q: "Reconcile #744, #745, and NG-1. Does the design retire them as standalone efforts? Give the existing-corpus backfill story."

**Answer**:

- **#744 (inbound ceiling) — RETIRED as a standalone effort.** The convergence sweep repoints
  *all* referrers with no ceiling, off the hot path. `REDIRECT_CEILING` stops being the
  guarantee; the issue's options (raise/configurable/surface-loud/accept-with-contract) become
  moot. Residual: decide whether to keep the write-side `run_redirect_loop` as a best-effort
  fast-path or delete it (ADR choice; not load-bearing either way).
- **#745 (outbound drops) — PARTIALLY subsumed; does NOT fully collapse.**
  - *Going-forward:* already CLOSED by vnc-035 (`run_carry_forward_loop`, no ceiling). Not this
    spike's mechanism.
  - *Historical inbound drift:* swept for free by the convergence scan (keys off current state).
  - *Historical OUTBOUND drops — the actual #745 case — are UNRECOVERABLE by any convergence
    scan.* Pre-vnc-035 corrections *deleted* the deprecated entry's outbound rows. A deleted row
    has nothing to repoint; the scan cannot reconstruct an edge that no longer exists. The two
    goal entries (`personal-cloud`, `proactive-delivery`) were hand-restored precisely because
    the source data was gone. **#745's outbound residual therefore needs a separate, optional,
    best-effort reconstruction** (from audit-log detail or embedding similarity), or a product
    decision to accept the two manual fixes and close it. This is the concrete challenge to
    "they collapse into this."
- **NG-1 (stale neighbor labels) — NOT retired by convergence; complementary.** It is a
  read-side label-resolution gap (F5/F6), orthogonal to stored convergence. Convergence *shrinks
  the window* during which labels are stale, but between correction and convergence — and for
  the rendering path itself — NG-1 needs the read-side fix (resolve edge-target labels in
  `build_edges_view`; flip `context_graph` default-on). Keep NG-1 as a read-path work item that
  ships *with* the convergence ADR.

**Backfill story (existing corpus):** the same scan-driven convergence sweeps all
**inbound** historical drift with no separate pass — it finds every Deprecated-with-successor
target regardless of when it was corrected. The only existing-corpus item the sweep *cannot*
address is the pre-vnc-035 **outbound** deletions (#745), which are data-loss, not drift.

**Evidence**: vnc-035 carry loop; the scan predicate (state-keyed, date-agnostic); the #745
issue framing (deleted rows, two hand-restored entries).

**Recommendation**: Re-scope #744 → close, folded into the convergence ADR. Re-scope #745 →
split: mark going-forward + inbound-historical as resolved by this design; open a small,
optional follow-up (or product-accept) for the outbound data-loss. NG-1 → keep as the read-cover
work item bundled with the ADR.

---

## Recommended SLN3 `done_when` rewording (mechanism-agnostic)

Current (synchronous, write-side): *"correcting a hot node carries/redirects its referrers and
accumulates no orphan edges."*

Proposed (eventual + read-cover, mechanism-agnostic):

> **After an entry is corrected:** (a) every referrer edge converges to the correction's terminal
> within the convergence SLO (≤ 1 tick per correction-batch budget), with **no permanent orphan
> edge and no silently-dropped referrer link**; and (b) at every instant before convergence
> completes, all read surfaces (`context_get` anchor *and* edge labels, `context_graph`,
> briefing/injection edge-walk) resolve the deprecated endpoint to its terminal, so **no consumer
> observes a dangling or stale reference.**

This separates the *eventual stored* guarantee (a) from the *immediate observable* guarantee (b),
and names no mechanism.

---

## Unanswered Questions

- **Batch budget `B` and cadence tuning.** The snapshot gives the degree *distribution* (max 110)
  but not the correction *rate*. `B` should be set from a real correction-rate measurement so the
  ≤1-tick common case holds; a conservative starting `B` (few hundred edges/tick/slug) covers the
  observed max in one tick. Needs a follow-up measurement, not resolvable from a static snapshot.
- **Keep or delete the write-side `run_redirect_loop` fast-path?** A pure ADR/design choice
  (instant small-fan-in repoint vs. one code path). Both are correct once convergence owns the
  guarantee; recommend deleting to avoid two mechanisms, but flagged for the steward.
- **#745 outbound reconstruction — worth any effort?** Requires a product call: attempt
  best-effort re-derivation (audit-log `detail` / embedding similarity) or accept the two manual
  restorations and close. Out of this spike's scope to decide.
- **PPR / retrieval consequence of repointing (Open Q5).** Directionally, repointing preserves
  `relation_type` and moves the endpoint to the *current form* of the same knowledge, so the
  PPR-positive neighborhood should be preserved or improved — but this is unmeasured. Flag as
  needs-validation before enabling convergence if a PPR regression is a concern.

## Out-of-Scope Discoveries

- **Compaction deletes-rather-than-repoints is a latent integrity defect, independent of this
  ADR.** `run_orphaned_edge_compaction` silently drops the inbound edges the synchronous loop
  did not reach, on *every* correction of a node with fan-in > 50 (and on any per-edge redirect
  failure). This means hot-node referrers lose links today, not just under enterprise scale.
  Worth a **GitHub issue** (per repo rule: code defects are GH issues, not lessons) even if the
  full convergence ADR is deferred — the repoint-before-delete ordering is a small, high-value
  fix on its own.
- **`context_graph` `resolve_supersessions` default-OFF vs `context_get` default-ON is an
  inconsistency** that affects briefing/injection and any gating consumer regardless of
  convergence. Worth aligning independently.
- **Clean power-law in-degree baseline** (max 110, avg 8.07, 5 nodes > ceiling at ~4k entries)
  from the ass-038 snapshot — a useful capacity-planning datum; extrapolates to order-tens of
  over-ceiling nodes at ~10× corpus.

---

## Recommendations Summary

- **Q1 (failure surface):** Two workstreams — stored-graph convergence (F1/F2, where the real
  harm is *link deletion at next tick*, not stale pointers) and read-label completion (F5/F6).
  F7–F9 are "skip when no valid terminal"; F10 (cross-slug) is impossible.
- **Q2 (strategies):** Hybrid, no single winner. Synchronous can't own hot-node correctness
  (ceiling is the defect); read-time is the interim cover but a permanent tax that doesn't fix
  stored drift; background-tick owns stored convergence at scale.
- **Q3 (division of labor):** Confirmed — write cheap · read resolve · tick converge — with two
  corrections: convergence must **repoint before compaction deletes**, and the read cover must be
  **completed** (`context_graph` default-on + NG-1 label resolution).
- **Q4 (guarantee):** Eventual is safe; no consumer needs immediate *stored* consistency once the
  read cover is complete. SLO in **ticks** (not wall-time): all referrers repointed within 1 tick
  per batch budget `B`, draining `⌈pending/B⌉` under bursts; stale edges always resolvable-at-read.
- **Q5 (tick):** Extend Job 2 into **repoint-then-compact**, scan-driven `LIMIT B`, CTE terminal
  resolution (#4468), before `typed_graph_rebuild`. Hash-chain **untouched** (edges standalone
  table); **no per-edge audit** (edges aren't audited today — optional per-tick summary);
  in-memory rebuild picks up converged edges same-tick.
- **Q6 (reconciliation):** #744 **retired** (folded in). #745 **partially** — going-forward
  (vnc-035) + inbound-historical resolved by the scan; **outbound historical drops are
  data-loss, not reparable by convergence** — needs a separate optional pass or product-accept.
  NG-1 **not retired** — complementary read-side fix, ship with the ADR. Inbound backfill is
  free (state-keyed scan); no separate one-shot pass for it.
