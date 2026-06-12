# vnc-035 — context_correct: Carry Outgoing Edges Forward by Default

> Source: GitHub issue #730 (`bug`, `goal:self-learning`). The corrective decision was
> already settled on 2026-06-10 (flip default to **persist**). This SCOPE turns that
> decision into a designable contract and surfaces the remaining OPEN design questions
> for the human to resolve in the design session.

## Problem Statement

`context_correct(A → B)` repairs the typed knowledge graph **asymmetrically**:

- **Incoming** edges (`E→A`) are auto-redirected to the new head (`E→B`) up to
  `REDIRECT_CEILING = 50` (vnc-017, `tools.rs:1144`, `run_redirect_loop`).
- The corrected entry's own **outgoing** edges (`A→X`) are **never carried forward**.
  Only edges explicitly passed in the `edges` param are written, and they attach to the
  **new** entry's id (vnc-015, `tools.rs:1121–1142`, "Edges attach to the NEW corrected
  entry's ID").

So correcting an edged entry **silently drops its outgoing relationships** unless the
caller re-declares every one of them in `edges`. The asymmetry is insidious: preserving
incoming edges lulls callers into assuming full preservation.

**Confirmed live:** the `personal-cloud` and `proactive-delivery` goal entries lost their
`Advances → vision_root` edge through correction chains (both outgoing). A goal's only edge
is outgoing, so a goal correction **always** orphans the goal from the vision graph unless
`edges` is re-passed. Restored manually on 2026-06-10 via `context_edge`; uni-zero hit this
firsthand. This degrades graph-relational retrieval (`goal:self-learning`, architectural
principle 4 — the typed graph "surfaces what vector search alone cannot").

**Original rationale (why it is currently this way):** a deliberate vnc-015 judgment call —
"don't carry forward edges we aren't sure are still valid after a content change; a revised
entry might no longer `Supports`/`DependsOn` what the original did" (vnc-015 SCOPE Non-Goal:
"No auto-retarget on supersession… edges attach to the new entry. No auto-transfer.").

## Decision Already Made (2026-06-10, issue #730)

Flip the default to **persist**: `context_correct` carries the original's outgoing edges
forward to the new entry **by default**; an agent explicitly sheds any edge that no longer
holds via `context_edge remove`/`redirect` (already supported). Rationale: **"correction =
persist" is what every agent assumes; opt-out is safer than silent loss.** The vnc-015
"don't carry what might be stale" concern is reframed as an *explicit shed*, not a *silent
drop* — the safer default inverts which failure mode is silent.

This SCOPE does **not** reopen that decision. It scopes its implementation; the
sub-questions the decision did not pin down are now resolved (see **Resolved Decisions**).

**Framing principle — scope boundary.** vnc-035 is scoped to fixing the **PRODUCT** to the
correct behavior. Repairing the existing corpus (the already-orphaned historical
corrections) and any agent-behavior changes in this repo are **DISTINCT, SEPARATE**
concerns handled via follow-up issues — explicitly **out of scope** here.

## Goals

1. `context_correct` copies the original entry's eligible **outgoing** edges to the new
   (corrected) entry by default, with no `edges` param required.
2. The "shed a stale edge during correction" path is preserved and documented as the
   supported opt-out: `context_edge remove`/`redirect` against the new entry id.
3. The treatment is symmetric in spirit with the existing incoming `REDIRECT_CEILING`
   path (vnc-017) — same Supersedes exclusion, same warn-and-continue infra-failure
   posture, same "correction never aborts on edge-copy failure" guarantee.
4. Carried edges attach to the **new** corrected entry id (consistent with vnc-015 AC-02).
5. Goal corrections preserve `Advances → vision_root` (the confirmed-live regression) with
   no manual re-declaration.
6. Documentation that currently instructs agents to manually re-declare edges on correction
   is updated within this feature — specifically the `uni-zero` SKILL goal-curation
   guidance and any agent docs carrying the same warning.

## Non-Goals

- **Not** reopening the persist-by-default decision (settled 2026-06-10).
- **No** carry-forward of **derived/auto-generated** edge classes — `Supersedes`
  (rebuilt from `entries.supersedes` by the graph tick) and the tick-regenerated
  `CoAccess`/`Informs` classes are out of scope to copy; they are not agent-declared
  relationships and re-materialize on their own. (Mirrors vnc-017 excluding `Supersedes`
  from the incoming redirect.)
- **No** retroactive backfill / migration job to repair already-orphaned historical
  corrections. The two known goal entries were already restored manually on 2026-06-10.
  The corpus repair sweep is tracked by a **separate follow-up GitHub issue** (OQ-05),
  per the framing principle — distinct from this product fix.
- **No** change to `context_store` edge semantics (store has no "original" to carry from).
- **No** change to the incoming-redirect path (vnc-017) beyond reusing its conventions.
- **No** TypedRelationGraph eager rebuild — carried edges become visible to DB-backed
  reads immediately and to BFS path-mode after the next tick, identical to every other
  edge write today (lesson #4526). Not a regression introduced here.
- **No** new edge ceiling tuning knob exposed to agents.

## Background Research (grounded in code)

**Where the gap lives — `crates/unimatrix-server/src/mcp/tools.rs`** (note: issue says
`unimatrix-engine`; the actual handler is in `unimatrix-server`):

- `context_correct` handler: `tools.rs:1015`. Edge handling is split Phase A (pre-correct
  validation, `:1054–1088`) / Phase B (post-correct write, `:1121–1142`).
- Phase B writes only `params.edges` via `edge_write::validate_and_write_edges`, attaching
  them to `correct_result.corrected_entry.id` (the new id). **This is the only outgoing
  edge write — there is no copy of the original's existing outgoing edges.**
- Incoming redirect: `run_redirect_loop` (`tools.rs:4660`) reads
  `store.query_incoming_edges(original_id)` and redirects each via
  `edge_write::redirect_graph_edge`. Ceiling = `REDIRECT_CEILING = 50` (`tools.rs:44`),
  truncates with a `warn!` past the ceiling, **never aborts the correction**.

**Edge storage / schema** — `crates/unimatrix-store/src/read.rs`:

- `graph_edges` columns: `source_id, target_id, relation_type, weight, created_at,
  created_by, source, bootstrap_only, metadata` (`GraphEdgeRow`, `read.rs:1743`).
- `query_incoming_edges(target_id)` (`read.rs:1694`) selects
  `source_id, relation_type, created_at WHERE target_id=?1 AND relation_type != 'Supersedes'`,
  returns `IncomingEdgeRow`. **There is no symmetric `query_outgoing_edges(source_id)`
  for this use case** — a new store query (or reuse of an existing neighbor query) is the
  natural build surface. `Supersedes` exclusion is the established precedent.

**Edge write helpers** — `crates/unimatrix-server/src/mcp/edge_write.rs`:

- `validate_and_write_edges(store, source_id, edges, created_at)` (`:152`) — the agent-edge
  write path; writes `source = EDGE_SOURCE_AGENT ("agent")`, `weight = 1.0`,
  `bootstrap_only = 0`; handles bidirectional `Contradicts`.
- `redirect_graph_edge(...)` (`:300`) — RAII-transactional DELETE-old + INSERT-new; the
  same primitive the incoming loop uses.
- `delete_graph_edge(...)` (`:238`) — idempotent; the `context_edge remove` primitive.
- Infra-failure posture is **warn-and-continue, entry never rolled back** (ADR-003 vnc-015).

**Opt-out path — `context_edge`** (`tools.rs:3081`): modes `add`/`remove`/`redirect`.
**Constraint:** the source entry must be **Active** (`tools.rs:3121–3135` — Quarantined or
Deprecated source is rejected as "frozen"). After a correction the **new** entry is Active
and the **original** is Deprecated, so the opt-out must target the **new** entry id — which
is exactly where carried edges land. This is consistent and must be stated in docs.

**Edge type taxonomy** — `crates/unimatrix-engine/src/graph.rs:139`: agent-declarable
relationship types include `Contradicts, Supports, Prerequisite, Informs, Advances,
Motivates, RelatedTo` (+ others); `Supersedes` is derived, `CoAccess`/`Informs` are
tick-generated. Carry-forward eligibility filters to genuinely agent-declared
relationship edges (resolved — OQ-02 in Resolved Decisions).

**Prior art rationale (read):** vnc-015 SCOPE (`product/features/vnc-015/SCOPE.md`) Non-Goal
line 88 + AC-02/§218 deliberately chose "no auto-transfer". vnc-017 SCOPE established the
incoming-redirect, the ceiling, the `Supersedes` exclusion, and warn-and-continue. This
feature extends vnc-017's symmetry to the outgoing direction and consciously supersedes the
vnc-015 "no auto-transfer" stance for outgoing edges.

**Knowledge base:** the 2026-06-10 carry-forward decision is **not** yet stored in
Unimatrix — it lives only in issue #730. Related stored ADRs: #4460/#4463 (vnc-017 redirect
+ ceiling), #4426/#4439/#4420 (vnc-015 edge validation/posture), #4472 (stale-edge pattern
post-correct). Lesson #4526: `context_edge`/edge writes do not trigger graph rebuild —
expected staleness, not a defect.

## Proposed Approach

Add an outgoing-edge carry-forward step to `context_correct`, modeled on `run_redirect_loop`:

1. After the correction commits, query the original's eligible outgoing edges (new store
   query mirroring `query_incoming_edges`, with the same `Supersedes` exclusion plus
   filtering of tick-generated classes — **agent-declared edges only**, OQ-02).
2. Re-write each as an outgoing edge on the **new** entry id, reusing `write_graph_edge` /
   `validate_and_write_edges` primitives (preserving `relation_type`; `Contradicts`
   bidirectional handling already lives in those helpers). All eligible edges carry — no
   ceiling (OQ-02). Carried edges write `source = "agent"`, indistinguishable from freshly
   declared (OQ-03).
3. Compose with the explicitly-passed `edges` param **additively on the full edge triple**
   (`source`, `target`, `relation_type`) — upsert, idempotent on exact re-pass (OQ-01).
4. Apply warn-and-continue infra posture; never abort the correction (ADR-003 / vnc-017
   parity).
5. Return an `edges_carried` count in the response ack (count only, omitted when zero,
   AC-11) so agents become aware carry-forward happened.
6. Update `uni-zero` SKILL + agent docs to remove manual re-declaration guidance and
   document `context_edge remove/redirect` (against the new entry id) as the shed path —
   cleanup, since the ack now carries awareness.

## Acceptance Criteria

- AC-01: Correcting an entry that has eligible outgoing edges, with **no** `edges` param
  passed, results in those outgoing edges existing on the **new** corrected entry id
  (verifiable in `graph_edges`).
- AC-02: Carried outgoing edges attach to the new corrected entry id, never the deprecated
  original (consistent with vnc-015 AC-02).
- AC-03: A goal entry whose only edge is `Advances → vision_root`, when corrected, retains
  `Advances → vision_root` from the new entry with no manual re-declaration (the
  confirmed-live regression is closed).
- AC-04: Derived/auto-generated edge classes (`Supersedes`, and the tick-generated
  `CoAccess`/`Informs`) are **not** copied by carry-forward; only agent-declared edges are
  eligible (OQ-02).
- AC-05: An agent can still shed a carried edge during correction via `context_edge
  remove`/`redirect` against the new entry id, and the shed edge is absent afterward.
- AC-06: `Contradicts` outgoing edges carry forward with both directions consistent
  (reusing existing bidirectional handling).
- AC-07: An infrastructure failure while copying any single outgoing edge does **not** abort
  or roll back the correction (warn-and-continue; entry + already-copied edges persist).
- AC-08: The `edges` param composition is **additive on full edge identity**
  (`source`, `target`, `relation_type`): carry-forward is the baseline, and any passed
  `edges` upsert on that triple — an exact re-pass dedupes (idempotent), a genuinely new
  edge adds, and a changed target on the same relation produces a **second** edge (correct
  for legitimately multi-target relations). Removal is **only** via the shed path
  (`context_edge remove`), never via omission from `edges`. A test asserts each of these:
  idempotent exact re-pass, additive new edge, and the two-edge changed-target case.
- AC-09: There is **no outgoing ceiling** — **all eligible** outgoing edges always carry.
  Eligible = **agent-declared edges only**; `Supersedes` (derived) and the tick-generated
  classes (`CoAccess`/`Informs`) are excluded, which is precisely what bounds agent-declared
  degree and makes "no ceiling" safe. A test pins that all eligible edges carry with no
  truncation.
- AC-10: `uni-zero` SKILL goal-curation guidance (and any agent docs) no longer instruct
  manual edge re-declaration on correction; they document carry-forward as default and
  `context_edge remove/redirect` (against the new entry id) as the shed path. With the
  `edges_carried` ack (AC-11) delivering agent awareness, these doc updates are **cleanup,
  not load-bearing**.
- AC-11: `context_correct`'s response acknowledges carried outgoing edges via an
  `edges_carried` integer field — **count only** (no edge content or identities), and
  **omitted when zero**. This ack is how agents become aware carry-forward happened, and is
  the delivery mechanism for agent awareness in place of any DB provenance marker (see
  Resolved Decisions, OQ-03).

## Constraints

- **Code location:** handler is `unimatrix-server`, not `unimatrix-engine` (issue
  mis-states the crate). Store queries live in `unimatrix-store`.
- **Posture parity (ADR-003 vnc-015 / vnc-017):** edge-copy failures warn and continue; the
  correction transaction is already committed and must never be rolled back by edge work.
- **Opt-out targets the new entry:** `context_edge` requires an Active source; only the new
  (corrected) entry qualifies. Docs must say so to avoid agents trying to edit the
  Deprecated original.
- **Graph rebuild staleness (lesson #4526):** carried edges are immediately visible to
  DB-backed reads; BFS path-mode sees them after the next tick. Expected, not in scope to
  change.
- **No `unsafe`, no `.unwrap()` in non-test code; ≤500 lines/file** (workspace rules) — the
  new store query likely warrants its own module rather than growing `read.rs`.
- **Test infra is cumulative** — extend the existing vnc-015/vnc-017 correction + edge
  fixtures, do not scaffold isolated harnesses.
- **Backward compatibility** needs no caller changes (OQ-04 dissolved): additive-on-triple
  upsert makes exact re-passes idempotent, so existing callers that re-pass `edges` neither
  double-write nor conflict.

## Resolved Decisions

> The five design-session questions are resolved. Recorded here for traceability; reflected
> in Goals, Non-Goals, Acceptance Criteria, and Constraints above. Not reopened.

- **OQ-01 — `edges` param semantics → RESOLVED: additive on full edge identity.** Composition
  is **additive on the full edge triple** (`source`, `target`, `relation_type`).
  Carry-forward is the baseline; passed `edges` upsert on that triple — exact re-passes
  dedupe (idempotent), genuinely new edges add, and a *changed target* on the same relation
  produces **two** edges (correct for legitimately multi-target relations). Removal is
  **only** via the shed path (`context_edge remove`), never via omission from `edges`.
  Because re-passes are idempotent, legacy agent habits need no fixing. (See AC-08.)
- **OQ-02 — Outgoing ceiling + eligible classes → RESOLVED: no ceiling, agent-declared
  only.** **No outbound ceiling** — all eligible edges always carry. Eligible =
  **agent-declared edges only**; `Supersedes` (derived) and the tick-generated classes
  (`CoAccess`/`Informs`) are excluded. That exclusion is precisely what makes "no ceiling"
  safe — it bounds agent-declared degree. If any defense is ever wanted later it is a
  **high-threshold observability warning that still carries every edge** — never a
  truncating cap. (See AC-09.)
- **OQ-03 — Provenance of carried edges → RESOLVED: simplest, no schema marker.** Carried
  edges write `source = "agent"`, **indistinguishable from freshly declared**. No DB
  provenance marker, no preserved original `created_at`/`created_by`. Agent awareness is
  delivered by the response **ack** (`edges_carried`, AC-11), not by schema.
- **OQ-04 — Migration / back-compat → DISSOLVED.** The new default **is** the
  defined-correct behavior; agents adapt via the ack. No caller audit and no caller changes
  inside this feature.
- **OQ-05 — Historical repair sweep → OUT OF SCOPE.** A **follow-up GitHub issue** tracks the
  corpus repair sweep over already-orphaned historical corrections; the decision is deferred
  to it. This reinforces the existing Non-Goal (no retroactive backfill/migration here).

## Dependencies

- **vnc-015** (#595) — `edges` param + `validate_and_write_edges` + edge-validation posture
  (ADR-001/002/003). Carry-forward composes with this write path.
- **vnc-017** (#606) — incoming `run_redirect_loop`, `REDIRECT_CEILING`, `Supersedes`
  exclusion, warn-and-continue posture. The outgoing path mirrors it.
- **vnc-016** (#603) — `context_edge` add/remove/redirect (the documented opt-out path).
- **unimatrix-store** `graph_edges` schema + `query_incoming_edges` (the model for a new
  outgoing query).
- **uni-zero SKILL** + agent docs — doc updates land **within** this feature (interim
  guidance was intentionally not committed to avoid riding the nan-016 branch; it lands here).
- Lesson #4526 (graph-rebuild staleness) — informs expected post-correct visibility, no code
  dependency.

## Tracking

- GitHub Issue: https://github.com/dug-21/unimatrix/issues/730 (`goal:self-learning`) —
  reframed bug → vnc-035 feature; body synced to the finalized design.
- Follow-up: #745 — corpus repair sweep over already-orphaned historical corrections (OQ-05).
