# SPECIFICATION — crt-053 Active-Only PPR Expansion Seeds

**Feature ID**: crt-053
**GH Issue**: #717
**Source scope**: `product/features/crt-053/SCOPE.md` (LOCKED, rescoped 2026-06-10 after ass-073 #720, ass-074 #721)
**Scope risk basis**: `product/features/crt-053/SCOPE-RISK-ASSESSMENT.md`

> Scope is LOCKED. This specification describes ONLY the active-only seed filter and its
> precise boundary. It deliberately specifies no excluded behavior (injection-side
> redirect, penalty changes, steepness, deprecated-absence-in-Flexible). See "NOT in scope".

---

## Objective

Restrict the PPR / `graph_expand` expansion seed set to **Active** entries so graph
expansion anchors only on current knowledge. Today the expander seeds from the full
Flexible candidate pool — including Deprecated and superseded entries — and
`graph_expand` applies no status filter, so a deprecated entry can serve as an expansion
seed and inject its neighbors at full weight (the latent seed-side leak ass-074 #721
identified). The fix is a single predicate on the seed collection in
`crates/unimatrix-server/src/services/search.rs` Phase 0 (~`:915`); everything downstream
of the BFS, and the entire HNSW ranking path, is unchanged.

---

## Domain Models / Ubiquitous Language

| Term | Definition |
|------|------------|
| **Status** | Entry lifecycle enum (`unimatrix-store::schema::Status`): `Active`, `Deprecated`, `Proposed`, `Quarantined`. The seed predicate is defined against this enum, never a string compare. |
| **Active** | `Status::Active`. The only status permitted into the expander seed set by this feature. |
| **Deprecated / superseded** | A non-active entry. "Deprecated" = `Status::Deprecated`. "Superseded" = an entry whose `superseded_by` field is set (it points to a newer entry). For the seed predicate, the discriminator is **`status == Active`**; a superseded-but-still-`Active` entry is not a target of this filter — only non-`Active` statuses are excluded. |
| **Seed / seed set** | `seed_ids` — the IDs collected from `results_with_scores` (post Step 6a, post Step 6b) that are handed to `graph_expand` as BFS roots. This feature narrows this set. |
| **Expander / `graph_expand`** | The Phase 0 BFS (crt-042) that walks **Outgoing positive edges** forward from each seed to collect reachable neighbor IDs, bounded by `expansion_depth` and `max_expansion_candidates`. It excludes `Supersedes` and `Contradicts` edge types and applies **no status filter** of its own. Direction is forward: seed B with edge B→X surfaces X; it does not change as a result of this feature. |
| **6b terminal-active head** | An entry injected by Step 6b (crt-010/crt-014) as the terminal active resolution of a superseded chain. These are already `Status::Active` by construction (`search.rs:814`) and therefore pass the seed predicate unchanged. |
| **PPR injection** | Expanded neighbors added to the candidate pool by the expander and scored by PPR. The leak this feature closes is a *deprecated seed* anchoring such injection. |
| **Flexible mode** | Search-path admission: deprecated/superseded entries enter the candidate set, receive the HNSW topology penalty, and **remain visible** in results (penalize-but-keep, ADR-001 #481). |
| **Strict mode** | Briefing-path admission: deprecated/superseded entries are **evicted**. Unchanged by this feature. |
| **`ppr_expander_enabled`** | Feature flag. Default `false`. The entire seed-filter code path lives inside the `if self.ppr_expander_enabled` branch. |

---

## Functional Requirements

- **FR-01 — Active-only seed predicate.** When `ppr_expander_enabled = true` and
  `use_fallback = false`, the `seed_ids` set passed to `graph_expand` MUST contain only
  IDs of entries whose `status == Status::Active`. Entries with any other status
  (`Deprecated`, `Proposed`, `Quarantined`) MUST be excluded from the seed set before the
  BFS runs.

- **FR-02 — Predicate is enum-based.** The filter MUST test the `Status` enum value
  (`status == Status::Active`), never a string comparison of a status field. (SR-02.)

- **FR-03 — Terminal-active heads are retained.** 6b-injected terminal-active heads and
  all HNSW-sourced active entries MUST remain in the seed set. The predicate excludes only
  non-active entries; it MUST NOT drop any legitimately active seed. (SR-02.)

- **FR-04 — Downstream of the BFS unchanged.** No behavior after the `graph_expand` call
  (dedup guard, sorted iteration, candidate insertion, PPR scoring, penalty application,
  truncation to `k`) is modified. The filter narrows the seed input only.

- **FR-05 — Traversal semantics unchanged.** `graph_expand` direction, depth, candidate
  ceiling, excluded edge types, and determinism ordering are unchanged. The feature
  narrows *which seeds* the existing forward BFS starts from; it does not alter *how* the
  BFS walks. (SR-06.)

- **FR-06 — HNSW ranking path untouched.** Deprecated and superseded entries still enter
  the candidate set via HNSW, still receive their topology penalty, and still appear in
  Flexible (search) results. No admission stage, scoring path, penalty, or mode other than
  the expander seed set is modified. (C-01, C-03.)

- **FR-07 — Default-off equivalence.** When `ppr_expander_enabled = false`, the
  seed-filter code path adds zero behavioral change: no BFS, no extra fetch, no allocation
  or cost leaking into shared seed/candidate structures used by the off path. The off path
  is bit-for-bit identical to pre-crt-053. (C-02, SR-04.)

- **FR-08 — Single production edit site.** The active-only filter is the only production
  change for status behavior, located strictly inside the `if self.ppr_expander_enabled`
  branch in `services/search.rs` Phase 0 seed collection. No other file or stage is edited
  for status behavior. (C-01.)

---

## Non-Functional Requirements

- **NFR-01 — Default-off zero-overhead.** With `ppr_expander_enabled = false`, runtime
  cost is unchanged from baseline (no added iteration, allocation, or branch on the off
  path). Verified by FR-07 / AC-02.

- **NFR-02 — Negligible enabled-path overhead.** With the expander enabled, the filter is
  a single linear pass (or filter-in-place) over `results_with_scores` (bounded by the
  HNSW k=20 + 6b injections seed cardinality — small). No additional I/O, no lock
  acquisition, no async call. The Phase 0 combined candidate ceiling (270) is unchanged.

- **NFR-03 — Determinism preserved.** Seed ordering into `graph_expand` and the existing
  sorted-expanded processing remain deterministic; the filter MUST NOT introduce
  nondeterministic ordering.

- **NFR-04 — No new locks / no I/O in the filter.** The predicate reads the already-loaded
  `status` field on in-memory `EntryRecord`s in `results_with_scores`; it acquires no lock
  and performs no store/vector fetch.

- **NFR-05 — Behavior-based validation only.** Acceptance is asserted by seed
  inclusion/exclusion and ranking/presence outcomes, never penalty constants (C-04,
  crt-013 #703). There is **no eval-harness metric gate** for this feature (SR-01, R1):
  the platform cannot measure graph-relational effectiveness, so no P@5/MRR gate is
  required or permitted as acceptance.

---

## Acceptance Criteria

All criteria are behavior-based (assert seed-inclusion / ranking / presence outcomes,
never penalty constants — C-04, crt-013 #703). Verification routes to the nan-018 fixture
corpus (with the positive-edge revision ass-073 requested) or the Python integration suite
over raw `entries` JSON — **not** an eval-harness metric.

| AC-ID | Criterion | Verification Method |
|-------|-----------|---------------------|
| **AC-01** | The seed filter excludes deprecated/superseded entries from the expander seed set. Given a fixture pool containing one Active entry and one Deprecated entry, **both with positive out-edges**, the BFS expands from the Active seed and NOT from the Deprecated seed: an entry reachable **only** via the deprecated seed's out-edge is NOT injected; an entry reachable via the active seed's out-edge IS injected. | Behavior assertion on the injected candidate set (presence of the active-only neighbor, absence of the deprecated-only neighbor) over the nan-018 fixture corpus or Python integration suite. Maps SCOPE Validation #1, SR-05. |
| **AC-02** | Bit-for-bit unchanged when the expander is off. With `ppr_expander_enabled = false`, search results (entries, order, scores) are identical to the pre-crt-053 baseline for the same fixture/query set. | Baseline-equivalence assertion with the flag off (existing default-off tests pass untouched; no new injection occurs). Maps SCOPE Validation #2, C-02, FR-07. |
| **AC-03** | HNSW ranking is unchanged. All existing search and penalty tests pass untouched; deprecated entries still appear in Flexible results and are still penalized (ranked below a comparable active on the penalized path). | Existing search/penalty test suite passes with no modification; presence-of-deprecated-in-Flexible assertion holds. Maps SCOPE Validation #3, C-01, C-03, FR-06. |
| **AC-04** | Terminal-active heads survive the filter. Given a fixture with a superseded chain whose terminal active head is 6b-injected, that terminal-active head remains in the seed set and its out-edge neighbors are still eligible for expansion. | Behavior assertion that the 6b terminal-active head appears as an expansion anchor (its active-only neighbor is injected). Maps SR-02, FR-03. |
| **AC-05** | Supersession false-positive guard. Given Deprecated entry A superseded by Active entry B (both with positive out-edges), the BFS expands from B's path and NOT from A's path. | Behavior assertion: neighbor reachable only via A is absent; neighbor reachable via B is present. Maps SCOPE Validation #1 / SR-05; the supersession-chain variant of AC-01. |

**Explicitly forbidden acceptance criterion (anti-AC):** No test may assert that
deprecated entries are *absent* from Flexible (search) results. Such an assertion
contradicts the two-mode design (Flexible = penalize-but-keep-visible; ADR-001 #481) and
MUST NOT be added. (SCOPE Validation note, C-03.)

---

## User / Agent Workflows

1. **Agent search query (Flexible mode), expander enabled.** Agent issues a search. HNSW
   produces candidates; Step 6a/6b run; the active-only filter selects active seeds; the
   forward BFS expands from those active seeds only; PPR scores the widened pool;
   deprecated entries still present from HNSW are penalized and ranked below comparable
   actives. Net effect: graph-expanded neighbors are anchored on current knowledge, not on
   stale entries.

2. **Agent search query, expander disabled (default).** No expansion runs; results are
   identical to baseline. The filter code path is inert.

3. **Briefing query (Strict mode).** Deprecated/superseded entries are evicted as before;
   the feature does not alter Strict-mode behavior.

---

## Constraints

Mirrors SCOPE.md (LOCKED):

- **C-01** — The active-only filter is the **only** production change. No other
  file/stage is edited for status behavior.
- **C-02** — The `ppr_expander_enabled = false` path stays bit-for-bit identical.
- **C-03** — Flexible/Strict mode semantics are untouched (ADR-001 #481).
- **C-04** — Status tests assert ranking/presence outcomes, never penalty constants
  (crt-013 #703).

Locked decisions carried verbatim from SCOPE.md (do not reopen, do not "complete"):
two modes stand; the bar is "deprecated must not outweigh a comparable active," not
"deprecated must be absent" in Flexible; no steepness work (Q6/Q8 dropped); no
injection-side redirect or penalty-map extension; the vnc-017 50-edge redirect ceiling is
not this feature's problem.

---

## Dependencies

- **`crates/unimatrix-server/src/services/search.rs`** — Phase 0 seed collection
  (~`:915`), inside the `if self.ppr_expander_enabled` branch (~`:911`). Sole production
  edit site.
- **`crates/unimatrix-store/src/schema.rs::Status`** — the enum the predicate tests
  (`Status::Active`).
- **`crates/unimatrix-engine/src/graph_expand.rs::graph_expand`** — the BFS consumer of
  `seed_ids` (forward, Outgoing-only, status-agnostic). Unchanged.
- **`EntryRecord.status`** — in-memory field read by the predicate (already loaded in
  `results_with_scores`; no fetch added).
- **nan-018 fixture corpus** (with the ass-073 positive-edge revision) and/or the Python
  integration suite — acceptance verification surface.
- **Research provenance (read-only):** ass-073 (#720), ass-074 (#721).

---

## NOT in Scope (explicit exclusions)

- **Injection-side redirect.** Do NOT add `find_terminal_active` resolution to injected
  entries.
- **Penalty machinery.** Do NOT extend `penalty_map` to injected entries; do NOT change
  any penalty magnitude or steepness (Q6/Q8 dropped).
- **Deprecated absence in Flexible.** Do NOT make deprecated entries absent from search
  results, and do NOT add a test asserting such absence (contradicts two-mode design).
- **Eval-harness gate.** No P@5/MRR (or any eval-harness metric) acceptance gate
  (SR-01 — unmeasurable; #500 / soft-GT P@5 trap).
- **vnc-017 50-edge redirect ceiling.** Pre-existing, separately tracked. Leave it. The
  residual case (a deprecated *neighbor* of an active seed reachable only via the redirect
  ceiling) is knowingly accepted.
- **#585 edge-graph hygiene** (write-time exclusion of deprecated edge targets) — separate
  concern, tracked separately.
- **#406 multi-hop terminal-active injection** — does not reproduce; a test/snapshot
  artifact to investigate, not a retrieval fix here.
- **#405 deprecated confidence flake** — split out; independent timing flake.
- **Traversal-direction / mode / scoring changes** of any kind beyond narrowing the seed
  set.

---

## Open Questions

- **OQ-1 (tester/architect):** Does the nan-018 fixture corpus currently contain a
  deprecated entry with a positive out-edge to a non-active-reachable neighbor? SCOPE.md
  notes this requires the "positive-edge revision ass-073 requested." If the revision is
  not yet in the corpus, AC-01/AC-05 must be satisfied via the Python integration suite or
  the fixture must be extended first. Confirm which surface delivers AC-01/AC-05.
- **OQ-2 (architect):** Confirm `results_with_scores` at ~`:915` is the sole seed source
  for `graph_expand` inside the enabled branch (SCOPE assumption, asserted confirmed at
  `:915`). If any other path contributes seeds, FR-01 scope must include it.

---

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — returned PPR/graph_expand decisions (crt-030 #3750/#3744 traversal direction, crt-042 #4050/#4051 Phase 0 ordering, ass-074 measurement-gap basis). Confirmed forward Outgoing-only BFS framing for SR-06 and the enum-based predicate (SR-02). No conflicting prior spec convention found. Read-only tier — no storage.
