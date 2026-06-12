# SPECIFICATION — vnc-035: context_correct Outgoing-Edge Carry-Forward

> Source: `product/features/vnc-035/SCOPE.md` (Goals, Non-Goals, AC-01..AC-11, Resolved
> Decisions OQ-01..OQ-05, Constraints) + `SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-08).
> Decision settled 2026-06-10 (issue #730): flip the `context_correct` default to **persist**
> outgoing edges. This spec scopes the implementation; it does not reopen the decision.

## Objective

`context_correct(A → B)` currently redirects A's **incoming** edges to B (vnc-017) but
**silently drops** A's **outgoing** edges unless the caller re-declares each one in the
`edges` param. vnc-035 makes `context_correct` carry the original entry's eligible outgoing
graph edges forward to the new corrected entry **by default**, with no `edges` param
required, composing additively with any passed `edges`, under a warn-and-continue posture
that never rolls back the correction. The new entry's `edges_carried` response count makes
agents aware the carry happened; the supported opt-out for a stale edge is `context_edge
remove`/`redirect` against the **new** entry id.

---

## Domain Models / Ubiquitous Language

| Term | Definition |
|------|------------|
| **Carry-forward** | Copying the original entry's eligible outgoing edges onto the new corrected entry's id during `context_correct`, by default, with no `edges` param required. |
| **Outgoing edge** | A `graph_edges` row whose `source_id` is the entry under correction (`A→X`). Distinct from **incoming** edges (`E→A`) handled by vnc-017's `run_redirect_loop`. |
| **Eligible edge** | An **agent-declared** outgoing relationship edge. Excludes derived `Supersedes` (rebuilt from `entries.supersedes` by the graph tick) and tick-generated `CoAccess`/`Informs` classes. The eligibility predicate is the single source of truth that bounds "no ceiling" safety (SR-03/SR-04). |
| **Shed** | An agent explicitly dropping a carried (or any) outgoing edge that no longer holds, via `context_edge remove`/`redirect` against the **new** (Active) entry id. The supported opt-out — replaces the former "manually re-declare on correction" practice. |
| **Additive-on-triple** | The composition rule between carry-forward (baseline) and the passed `edges` param: upsert keyed on the full edge triple `(source_id, target_id, relation_type)`. Exact re-pass is idempotent; a genuinely new triple adds; a changed target on the same relation produces a **second** edge. Removal is **never** via omission from `edges` — only via the shed path. |
| **`edges_carried` ack** | An integer field in the `context_correct` response — **count only** (no edge identities/content), **omitted when zero** — reporting how many outgoing edges were carried forward. The delivery mechanism for agent awareness in place of any DB provenance marker (OQ-03). |
| **Warn-and-continue** | The infra-failure posture (ADR-003 vnc-015 / vnc-017): an edge-copy failure emits `tracing::warn!`, increments a failure counter, and continues. The already-committed correction is **never** rolled back. |

---

## Functional Requirements

Each requirement is testable; verification methods are detailed in Acceptance Criteria.

- **FR-01 — Carry by default.** After the correction commits, `context_correct` queries the
  original entry's eligible outgoing edges and writes each as an outgoing edge on the new
  corrected entry id, with **no `edges` param required**. (AC-01)

- **FR-02 — Attach to the new entry.** Carried outgoing edges attach to
  `correct_result.corrected_entry.id` (the new id), never to the Deprecated original
  (`vnc-015` AC-02 parity; `vnc-017` ADR-001 terminal-active resolution). (AC-02)

- **FR-03 — Goal regression closed.** Correcting a goal entry whose only edge is
  `Advances → vision_root` retains that edge from the new entry with no manual
  re-declaration. (AC-03, Goal 5)

- **FR-04 — Eligibility = agent-declared only.** Carry-forward excludes derived `Supersedes`
  and tick-generated `CoAccess`/`Informs`; only agent-declared relationship edges are
  carried. The exclusion mirrors `query_incoming_edges`' `Supersedes`-at-SQL-level
  precedent (`vnc-017` ADR-002) and must be defined **once** so outgoing and incoming
  exclusion sets cannot diverge (SR-03). (AC-04, AC-09)

- **FR-05 — Shed via context_edge against the new id.** An agent can drop any carried edge
  during/after correction via `context_edge remove`/`redirect` targeting the **new** entry
  id (the only Active source post-correction). Omission from `edges` does **not** shed.
  (AC-05, AC-08; SR-08)

- **FR-06 — Contradicts bidirectional carry.** Outgoing `Contradicts` edges carry forward
  with both directions consistent, reusing the existing `validate_and_write_edges`
  bidirectional handling. Outgoing-carry and incoming-redirect must not double-write or
  orphan the reverse direction of the same `Contradicts` pair within one correction (SR-06).
  (AC-06)

- **FR-07 — Warn-and-continue on copy failure.** An infrastructure failure copying any single
  outgoing edge does **not** abort or roll back the correction. The entry and all
  already-copied edges persist; the failure emits `tracing::warn!` and increments a failure
  counter. (AC-07; ADR-003 vnc-015/vnc-017; SR-01)

- **FR-08 — Additive-on-triple composition.** Carry-forward is the baseline; the passed
  `edges` param upserts on the full triple `(source_id, target_id, relation_type)`:
  - exact re-pass of an already-carried edge is **idempotent** (no duplicate, not
    double-counted);
  - a genuinely new triple **adds**;
  - a **changed target** on the same `(source_id, relation_type)` produces a **second**
    edge (correct for legitimately multi-target relations).

  Removal is **only** via the shed path, never via omission from `edges`. (AC-08; OQ-01)

- **FR-09 — No outgoing ceiling.** **All eligible** outgoing edges always carry; there is no
  truncating cap and no agent-facing ceiling knob. The safety invariant: "no ceiling" is
  valid **only** while eligibility = agent-declared-only (FR-04), which bounds agent-declared
  degree. Any future defense is a high-threshold observability warning that **still carries
  every edge**, never a truncating cap. (AC-09; OQ-02; SR-04)

- **FR-10 — `edges_carried` ack.** The `context_correct` response includes an
  `edges_carried` integer = the count of outgoing edges actually carried (actual inserts,
  not attempted writes — SR-02), **omitted when the count is zero**, carrying **no** edge
  identities or content. (AC-11; OQ-03)

- **FR-11 — Carried edge attribution.** Carried edges write `source = EDGE_SOURCE_AGENT
  ("agent")` (binding both `source` and `created_by` per `vnc-015` ADR-008), `weight = 1.0`,
  `bootstrap_only = 0` — **indistinguishable from freshly declared edges**. No DB provenance
  marker; no preservation of the original's `created_at`/`created_by`. (OQ-03; FR-10
  delivers awareness instead)

- **FR-12 — Documentation cleanup.** The `uni-zero` SKILL goal-curation guidance and any
  agent docs that instruct manual edge re-declaration on correction are updated **within this
  feature** to (a) document carry-forward as the default and (b) document `context_edge
  remove`/`redirect` against the **new** entry id as the shed path, explicitly noting the
  Deprecated original cannot be edited (frozen-source rejection). (AC-10; SR-05, SR-08)

---

## Non-Functional Requirements

- **NFR-01 — Posture parity / no rollback.** Edge-copy work runs **after** the correction
  transaction has committed and must never roll it back. Correction success depends only on
  `correct_entry` succeeding (ADR-003 vnc-015 / vnc-017). Measurable: a forced edge-copy
  infra failure leaves the new Active entry and the original Deprecated entry intact (AC-07).

- **NFR-02 — Eligibility predicate single-definition.** The agent-declared eligibility
  filter is defined once (shared with / mirroring the `query_incoming_edges` `Supersedes`
  precedent), so the outgoing and incoming exclusion sets cannot drift (SR-03). Measurable:
  one named predicate/SQL clause; AC-04 + AC-09 tests pin the exclusion set.

- **NFR-03 — Count semantics against rows-affected.** `edges_carried` counts **actual
  inserts**. A UNIQUE-conflict on `(source_id, target_id, relation_type)` (`write_graph_edge`
  returns `false`, not `Err`, pattern #4041) is **not** counted as a new carry and is **not**
  treated as a failure. Measurable: idempotent exact re-pass yields the same `edges_carried`
  as a single carry (AC-08).

- **NFR-04 — Graph-rebuild staleness is expected, not a regression.** Carried edges are
  visible to DB-backed reads immediately and to BFS path-mode only after the next graph tick
  (lesson #4526). This is the existing behavior of every edge write and is **not** introduced
  or worsened here. Any path-mode acceptance test must force a tick/drain before asserting
  (SR-07; patterns #4517, #4114).

- **NFR-05 — Workspace code rules.** No `unsafe`; no `.unwrap()`/`.expect()` in non-test
  code. Each touched/new source file stays ≤500 lines — the new outgoing store query likely
  warrants its own module rather than growing `read.rs` (SCOPE Constraints).

- **NFR-06 — Cumulative test infra.** Extend the existing vnc-015/vnc-017 correction + edge
  fixtures and helpers; do not scaffold isolated harnesses (SCOPE Constraints).

- **NFR-07 — Backward compatibility.** No caller changes required. Existing callers that
  re-pass `edges` neither double-write nor conflict, because additive-on-triple upsert makes
  exact re-passes idempotent (FR-08 / NFR-03). (OQ-04 dissolved)

---

## Acceptance Criteria

Each maps a SCOPE AC to a concrete, verifiable test with an explicit verification method.
IDs are preserved end-to-end (AC-01..AC-11). **AC-07 is MANDATORY and flagged "easy to omit"
— see the callout below.**

| AC | Requirement | Verification Method |
|----|-------------|---------------------|
| **AC-01** | Correcting an entry with eligible outgoing edges, **no `edges` param**, carries those edges to the new entry. | Integration test: seed entry A with eligible outgoing edges (e.g. `Supports`, `Advances`); call `context_correct(A→B)` with `edges` omitted; assert `graph_edges` rows exist with `source_id = B.id` for each original eligible relation/target. |
| **AC-02** | Carried edges attach to the new id, never the Deprecated original. | Same test as AC-01: assert **no** carried `graph_edges` row has `source_id = A.id` (post-correct A is Deprecated); all carried rows have `source_id = B.id`. |
| **AC-03** | Goal with only `Advances → vision_root`, when corrected, retains it from the new entry, no manual re-declaration. | Integration test reproducing the confirmed-live regression: seed a goal entry with a single `Advances → vision_root` edge; correct it with `edges` omitted; assert `graph_edges` has `(B.id, vision_root, Advances)`. |
| **AC-04** | Derived/auto-generated classes (`Supersedes`, tick-generated `CoAccess`/`Informs`) are **not** copied; only agent-declared edges are eligible. | Integration test: seed A with a mix — agent-declared (`Supports`) plus a `Supersedes` row and a `CoAccess`/`Informs` row; correct; assert carried edges include the agent-declared edge and **exclude** `Supersedes`, `CoAccess`, `Informs`. Unit test on the eligibility predicate asserts the exclusion set. |
| **AC-05** | An agent can shed a carried edge via `context_edge remove`/`redirect` against the **new** entry id; the edge is absent afterward. | Integration test: correct A→B (carries edge E); call `context_edge remove` with `source_id = B.id` for E; assert E absent from `graph_edges`. Assert the shed path targets the **new** Active id (a remove against the Deprecated original id is rejected as frozen-source — SR-08). |
| **AC-06** | `Contradicts` outgoing edges carry forward with both directions consistent; no double-write or reverse-orphan vs. the redirect path. | Integration test: seed A with an outgoing `Contradicts → X`; correct A→B; assert the bidirectional pair is consistent on B (B↔X), and the carry did not duplicate or orphan the reverse direction that incoming-redirect may also touch (SR-06). |
| **AC-07** ⚠️ **MANDATORY — easy to omit** | An infra failure copying any single outgoing edge does **not** abort/roll back the correction; entry + already-copied edges persist. | Integration test that **forces** the per-edge-copy write to return `Err` (e.g. fault-inject the store edge-write for one edge mid-loop). Assert: (1) `context_correct` returns success; (2) the new entry is Active and the original Deprecated; (3) edges copied **before** the failing one persist; (4) a `tracing::warn!` + failure-counter increment occurred. **This test must exist by name** — see callout. |
| **AC-08** | `edges` composition is additive on `(source, target, relation_type)`: idempotent exact re-pass, additive new edge, two-edge changed-target; removal only via shed. | Three sub-assertions in one test (or three named tests): (a) **idempotent** — pass an `edges` entry identical to a carried triple; assert one row, `edges_carried` not inflated; (b) **additive** — pass a genuinely new triple; assert it is added; (c) **changed-target** — pass same `(source, relation)` with a new target; assert **two** edges exist. Plus: assert omission of a carried edge from `edges` does **not** remove it. |
| **AC-09** | **No outgoing ceiling** — all eligible edges carry with no truncation; eligible = agent-declared only. | Integration test: seed A with more eligible outgoing edges than vnc-017's `REDIRECT_CEILING` (>50); correct; assert **all** eligible edges carry, no truncation, no ceiling warn. Couples to AC-04: ineligible classes excluded. |
| **AC-10** | `uni-zero` SKILL + agent docs no longer instruct manual re-declaration; document carry-forward default + `context_edge remove`/`redirect` (new entry id) as shed. | Doc review: the `uni-zero` SKILL goal-curation section and any agent docs carrying the "re-declare edges on correction" warning are updated; assert the shed path is documented against the **new** entry id and the Deprecated-original-frozen note is present. **Coupled with AC-11** — neither ships without the other (SR-05). |
| **AC-11** | Response includes `edges_carried` integer — **count only**, **omitted when zero**. | Test the response envelope: (a) a correction carrying N>0 edges returns `edges_carried = N` (actual inserts, NFR-03); (b) a correction carrying zero edges **omits** the field entirely; (c) the field carries no edge identities/content. **Coupled with AC-10.** |

### ⚠️ AC-07 — MANDATORY, easy to omit (SR-01 / lesson #4473)

Warn-and-continue on a side-effect failure produces **no signal**: the feature behaves
identically whether or not the failure-path test exists — no compile error, no test failure,
no behavior change. vnc-017's mirrored path **FAILed Gate 3b** for exactly this — its AC-04
(correction succeeds when the redirect side-effect returns `Err`) was absent; the gate caught
it only by explicit per-name AC checklist comparison.

**Requirement on downstream:**
- The test plan (uni-tester / uni-risk-strategist) MUST list AC-07 as a **named** test for
  the per-edge-copy `Err` path.
- The Gate 3b validator MUST verify AC-07's test is **present by name** — not inferred from
  passing happy-path behavior.
- The implementation must surface the failure path (fault injection or seam) so the test can
  drive an `Err` on a single edge copy mid-loop.

### AC coupling notes

- **AC-10 + AC-11 are one acceptance unit** (SR-05): the `edges_carried` ack is what makes
  the doc change non-load-bearing; neither ships without the other.
- **AC-04 + AC-09 share the eligibility predicate** (SR-03/SR-04): "no ceiling" is safe
  **only** while eligibility = agent-declared-only.

---

## User / Agent Workflows

1. **Default correction (no edges).** Agent calls `context_correct(original_id, ...)` with
   `edges` omitted. All eligible outgoing edges of the original appear on the new entry. The
   response shows `edges_carried = N` (when N>0), so the agent knows the carry happened — no
   manual re-declaration needed.

2. **Correction with additional edges.** Agent calls `context_correct` with `edges`
   containing new relationships. Carry-forward runs as the baseline; passed edges upsert
   additively on the triple. Re-passing an already-carried edge is harmless (idempotent).

3. **Shedding a stale edge.** After a correction, the agent decides a carried edge no longer
   holds. It calls `context_edge remove` (or `redirect`) with `source_id = <new entry id>`.
   The edge is dropped. (Targeting the Deprecated original id is rejected as frozen-source.)

4. **Multi-target relation.** Agent passes `edges` with the same `(source, relation)` but a
   different target than a carried edge. Both edges coexist (two targets), which is correct.

---

## Constraints

- **Code location:** handler is `crates/unimatrix-server/src/mcp/tools.rs` (issue #730
  mis-states `unimatrix-engine`). Store queries live in `crates/unimatrix-store`; the new
  outgoing query likely in its own module (NFR-05).
- **Posture parity (ADR-003 vnc-015 / vnc-017):** edge-copy failures warn and continue; the
  committed correction is never rolled back by edge work (NFR-01).
- **Opt-out targets the new entry:** `context_edge` requires an Active source; only the new
  entry qualifies. Docs must state the Deprecated original cannot be edited (FR-12, SR-08).
- **Graph-rebuild staleness (lesson #4526):** carried edges are immediately visible to
  DB-backed reads; BFS path-mode after the next tick. Expected, not in scope to change; path
  tests must tick/drain first (NFR-04, SR-07).
- **Count semantics (SR-02 / pattern #4041):** `write_graph_edge` returns `bool` (UNIQUE
  conflict → `false`, not `Err`); `edges_carried` counts actual inserts (NFR-03).
- **No `unsafe`, no `.unwrap()` in non-test code; ≤500 lines/file** (NFR-05).
- **Cumulative test infra:** extend vnc-015/vnc-017 correction + edge fixtures (NFR-06).
- **Backward compatibility:** no caller changes (NFR-07).

---

## Dependencies

- **vnc-015** (#595) — `edges` param, `validate_and_write_edges`, edge-validation posture
  (ADR-001/002/003), `EDGE_SOURCE_AGENT` (ADR-008). Carry-forward composes with this write
  path and reuses its constants/bidirectional handling.
- **vnc-016** (#603) — `context_edge` add/remove/redirect (the documented shed/opt-out path).
- **vnc-017** (#606) — incoming `run_redirect_loop`, `REDIRECT_CEILING`, `Supersedes`
  exclusion (ADR-002), terminal-active resolution (ADR-001), warn-and-continue (ADR-003). The
  outgoing path mirrors these conventions.
- **unimatrix-store** — `graph_edges` schema (`source_id, target_id, relation_type, weight,
  created_at, created_by, source, bootstrap_only, metadata`) and `query_incoming_edges` (the
  model for the new outgoing query). `write_graph_edge` rows-affected `bool` contract.
- **unimatrix-engine** `graph.rs` edge-type taxonomy — defines agent-declarable vs. derived /
  tick-generated classes; the eligibility predicate filters against it.
- **uni-zero SKILL + agent docs** — doc updates land **within** this feature (AC-10).
- **Lesson #4526** (graph-rebuild staleness) — informs expected post-correct visibility; no
  code dependency.

---

## NOT in Scope (explicit exclusions)

- **Reopening the persist-by-default decision** (settled 2026-06-10).
- **Carry-forward of derived/auto-generated edge classes** — `Supersedes`,
  tick-generated `CoAccess`/`Informs` are never copied (FR-04).
- **Retroactive backfill / migration** of already-orphaned historical corrections. The two
  known goal entries were restored manually 2026-06-10; the corpus repair sweep is a
  **separate follow-up issue** (OQ-05).
- **Changes to `context_store` edge semantics** (no "original" to carry from).
- **Changes to the incoming-redirect path** (vnc-017) beyond reusing its conventions.
- **TypedRelationGraph eager rebuild** — carried edges follow the standard tick-window
  visibility (NFR-04); not a regression introduced here.
- **A new edge ceiling tuning knob** exposed to agents.
- **DB provenance marker** distinguishing carried vs. freshly-declared edges (OQ-03; accepted
  one-way door — awareness via `edges_carried` ack only).
- **Per-edge agent attribution** beyond `source = "agent"` (vnc-015 ADR-008 deferral).

---

## Open Questions (for architect / human)

None blocking. The five SCOPE design questions (OQ-01..OQ-05) are resolved; this spec
specifies against them. Two items the architect should pin precisely during design (called
out, not reopened):

1. **Eligibility predicate placement (SR-03).** Where the single agent-declared eligibility
   predicate lives so the new outgoing store query and the `query_incoming_edges` precedent
   cannot drift. Recommend defining it once and reusing.
2. **Contradicts non-interference (SR-06).** Confirm outgoing-carry and incoming-redirect do
   not both act on the same `Contradicts` pair within one correction (ordering / dedup at the
   write seam). AC-06 must cover both directions.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced vnc-017 ADR-003 (warn-and-continue,
  #4462), vnc-017 redirect pattern (#4472), vnc-015 ADR-008 edge-source `"agent"` (#4425),
  vnc-015 ADR-003 partial-write blast radius (#4420), plus the SubagentStart-injected lesson
  #4473 (warn+continue failure-path AC silently omitted at Gate 3b). All applied to FR-04,
  FR-07, FR-11, NFR-01/03, and the AC-07 mandatory callout. Read-only tier — no storage.
