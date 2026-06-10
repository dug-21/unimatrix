# crt-053 Architecture — Active-Only PPR Expansion Seeds

**Feature**: crt-053 — Active-Only PPR Expansion Seeds (Surgical Search-Heuristic Correctness)
**GH Issue**: #717
**SCOPE.md**: `product/features/crt-053/SCOPE.md` (LOCKED — non-negotiable product judgment)
**Risk basis**: `product/features/crt-053/SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-06)
**Research basis**: ass-073 (#720), ass-074 (#721)

---

## System Overview

Unimatrix search (`crates/unimatrix-server/src/services/search.rs`, `SearchService::search`)
is a multi-stage admission + scoring pipeline. The stages relevant to this feature:

```
Step 4   query embedding bound (`embedding`)
Step 6a  HNSW candidate pool -> penalty_map built ONCE over that pool
Step 6b  supersession terminal-active injection (status == Active by construction, :814-821)
Step 6d  PPR expansion (crt-030/crt-042) — runs only when use_fallback == false
  Phase 0 [crt-042]   graph_expand BFS — runs only when ppr_expander_enabled == true
                      seed_ids built from results_with_scores (:915) -> BFS -> append neighbors
  Phase 1..5          personalization vector, PPR scoring, PPR-only injection, truncation
Step 7   final = fused_score * penalty_map.get(id).unwrap_or(1.0)
```

ass-074 (#721) confirmed the PPR expander is enabled in production and injects in ~48% of
queries. ass-074's structural finding (Unimatrix #4887): `penalty_map` is computed once at
6a; every later admit-stage (6b, Phase 0, Phase 5) appends candidates that bypass the map and
receive penalty `1.0` at Step 7. Phase 0 also seeds `graph_expand` from the **full** candidate
pool, which includes deprecated/superseded entries. Today the leak is **latent** because the
production graph is Active→Active only; it becomes live the moment a deprecated entry acquires a
positive out-edge.

**This feature closes exactly one path**: the PPR Phase 0 seed set. It does not touch the
penalty bypass, the two-mode design, the redirect ceiling, or edge hygiene. Those are
deliberately left as-is (see Locked Decisions, carried verbatim below).

## The Change (single surgical edit)

Inside the `if self.ppr_expander_enabled` branch, at the `seed_ids` build (~`search.rs:915`),
filter the seed set to `entry.status == Status::Active` **before** `graph_expand` walks from it.

Conceptually:

```rust
// BEFORE (:915)
let seed_ids: Vec<u64> =
    results_with_scores.iter().map(|(e, _)| e.id).collect();

// AFTER (crt-053): seed graph_expand from ACTIVE entries only.
// Drops deprecated/superseded/proposed/quarantined seeds; 6b terminal-active
// heads (status == Active, :814-821) pass by construction (SR-02).
let seed_ids: Vec<u64> = results_with_scores
    .iter()
    .filter(|(e, _)| e.status == Status::Active)
    .map(|(e, _)| e.id)
    .collect();
```

This is the entire production delta. `results_with_scores` itself is **not** filtered —
deprecated/superseded entries remain in the pool, keep their HNSW penalty, and keep appearing
in Flexible results (C-03, Locked Decision 1). Only the *seed list handed to `graph_expand`* is
narrowed. Everything downstream of the BFS is unchanged.

## Component Breakdown

| Component | Responsibility | Change |
|-----------|----------------|--------|
| `SearchService::search` (search.rs) | Orchestrates the pipeline | **One filter added** on `seed_ids` build inside the `ppr_expander_enabled` branch |
| `graph_expand` (graph.rs / graph submodule) | BFS-forward traversal from seeds over positive edges | **Unchanged.** Receives a narrower seed slice; traversal semantics, depth, ceilings untouched |
| `Status` enum (store schema.rs) | Entry lifecycle state | **Unchanged.** Consumed as the predicate's typed comparator |
| `penalty_map` / Step 7 scoring | Topology penalty on ranking path | **Unchanged.** No injection-side penalty added (Locked Decision 4) |
| Step 6b terminal-active injection | Supersession redirect | **Unchanged.** Already `status == Active`; preserved by construction |

No new components, modules, structs, traits, config flags, or files. No new function.

## Component Interactions / Data Flow

```
results_with_scores : Vec<(EntryRecord, f32)>   (post 6a + 6b; mixed statuses)
        |
        |  crt-053 FILTER: keep only e.status == Status::Active
        v
seed_ids : Vec<u64>   (active anchors only)
        |
        v
graph_expand(&typed_graph, &seed_ids, depth, max)  -> expanded_ids : HashSet<u64>
        |   (BFS-forward; seed B with edge B->X surfaces X — SR-06)
        v
per expanded_id: in_pool dedup -> fetch -> quarantine check (:950) -> embedding -> cosine
        |
        v
results_with_scores.push((entry, cosine_sim))   (unchanged admission of NEIGHBORS)
```

**Behavioral contract**: the filter narrows *which entries anchor the walk*. It does **not**
change traversal direction, depth, or the admission/scoring of neighbors. A neighbor reachable
**only** via a deprecated seed is no longer injected; a neighbor reachable via an active seed is
unaffected. Verify by outcome (entry presence/absence), never by inspecting a `Direction::` enum
(SR-06).

## Off-Path Equivalence Guarantee (C-02 / SR-04)

The filter lives **strictly inside** `if self.ppr_expander_enabled` (post `:914`). It touches
only the local `seed_ids` binding, which exists nowhere else. When `ppr_expander_enabled = false`
the entire block — including the new filter — is never entered, so the path is **bit-for-bit
identical** to pre-crt-053. No shared helper, no shared struct, no allocation, no cost leaks into
the default-off path. This is structurally guaranteed by lexical scope, not by discipline.

## Predicate Design (SR-02 / SR-05)

- Predicate is `e.status == Status::Active` — a typed enum comparison, never a string compare
  (`Status` is `#[repr(u8)]` with `PartialEq`, schema.rs:8-15).
- `Status` has four variants: `Active`, `Deprecated`, `Proposed`, `Quarantined`. `== Active`
  drops the latter three from the **seed set**. This is the intended, SCOPE-mandated semantics
  ("from ACTIVE entries only").
- 6b terminal-active heads are pushed at :821 only after the :814 guard asserts
  `terminal.status == Status::Active`. They therefore pass the new predicate **by construction** —
  the filter cannot drop a legitimate active anchor (SR-02).
- A deprecated entry superseded by an active entry: the deprecated entry is dropped as a seed;
  the active terminal (injected by 6b) remains a seed and anchors the walk. This is the exact
  case SCOPE Validation / SR-05 require a fixture for.

## Validation Strategy (SR-01 — behavior-based only)

There is **no** measure of search-heuristic effectiveness (ass-074 #721, Unimatrix #4888); the
eval harness scores positive relevance only. Therefore correctness is asserted by **behavior on
specific entry IDs**, never by an eval-harness gate and never by penalty constants (C-04, crt-013
#703). Three assertions:

1. **Seed exclusion (primary, SR-05).** Fixture pool = active entry B + deprecated entry A (A
   superseded_by B; both have positive out-edges to distinct neighbors). Assert: the BFS expands
   from B's path and an entry reachable **only** via A is **not** injected.
2. **Off-path identity (C-02).** With `ppr_expander_enabled = false`, the seed-filter path adds
   zero behavior delta — existing default-config search behavior is unchanged.
3. **Ranking unchanged (C-03).** Existing HNSW/penalty tests pass untouched — deprecated entries
   still appear and are still penalized in Flexible.

**Do NOT** add tests asserting deprecated *absence* in Flexible (contradicts the two-mode design,
Locked Decision 1). **Do NOT** gate on soft-GT P@5 (the #500 trap — correctly demoting stale
entries mechanically drops soft-GT P@5; ass-073 #720). Testable on the nan-018 fixture corpus
(with ass-073's positive-edge revision) or the Python integration suite.

## Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| `results_with_scores` | `Vec<(EntryRecord, f32)>` | local in `SearchService::search`, post 6a/6b |
| `EntryRecord.status` | `pub status: Status` | `unimatrix-store/src/schema.rs:57` |
| `Status` enum | `Active=0, Deprecated=1, Proposed=2, Quarantined=3` (`#[repr(u8)]`, `PartialEq`) | `unimatrix-store/src/schema.rs:8-15` |
| `seed_ids` (the edit site) | `Vec<u64>` | `search.rs:~915`, inside `if self.ppr_expander_enabled` |
| `graph_expand` | `fn(&TypedRelationGraph, &[u64], depth, max) -> HashSet<u64>` | graph submodule (called at `search.rs:919`); **signature unchanged** |
| `ppr_expander_enabled` | `bool` field on `SearchService` | `SearchService` (guards the whole block at `:911`) |
| Quarantine enforcement | `SecurityGateway::is_quarantined(&entry.status)` | `search.rs:950` — per-expanded-entry; **unchanged**, NOT the seed filter |

Downstream agents: do not introduce new names. The edit reuses `results_with_scores`,
`EntryRecord.status`, `Status::Active`, and `seed_ids`. No new symbol is created.

## Locked Decisions — carried VERBATIM as binding architecture constraints (SR-03 / R3)

These are non-negotiable product judgment from SCOPE.md "Human-judged decisions (LOCKED)". The
architecture is designed so crossing the C-01 single-change boundary requires touching code this
design never references. Re-litigating any of these is a scope-creep failure (precedent: vnc-018
inverted PPR negative tests without an ADR, Unimatrix #4495).

1. **Two modes stand.** Flexible (search) = penalize-but-keep-visible; Strict (briefing) = evict.
   Returning deprecated entries in search is **not an error**. Not in scope to change.
2. **The bar is "deprecated must not outweigh a comparable active," not "deprecated must be
   absent" (in Flexible).** The HNSW topology penalty already meets that bar on the ranking path;
   the active-only seed filter closes the one path (PPR injection) where it didn't. Sufficient by
   judgment.
3. **No steepness work.** Penalty magnitude is not the lever (ass-073/ass-074). crt-053 is not a
   tuning feature. Q6/Q8 dropped entirely.
4. **No injection-side redirect or penalty machinery.** Do **not** add `find_terminal_active` to
   injected entries; do **not** extend `penalty_map` to injected entries. The active-only seed
   filter is the chosen mechanism. The residual case it does not cover (a deprecated *neighbor* of
   an active seed reachable only via the vnc-017 >50-edge redirect ceiling) is **knowingly
   accepted**, not a bug to patch here.
5. **The vnc-017 redirect ceiling (50) is not this feature's problem.** Pre-existing,
   separately-tracked. Leave it.

### Explicitly NOT designed (out of bounds)

`#585` edge-generation hygiene; `#406` multi-hop test (does not reproduce — test-artifact
investigation, not a retrieval fix); `#405` confidence flake (split); injection-side penalty
re-application from Unimatrix #4887 (real, but knowingly out of scope here).

## Constraints (from SCOPE.md)

- **C-01**: Active-only filter is the **only** production change. No other file/stage edited for
  status behavior.
- **C-02**: `ppr_expander_enabled = false` path stays bit-for-bit identical.
- **C-03**: Flexible/Strict mode semantics untouched (ADR-001 #481).
- **C-04**: Status tests assert ranking/presence outcomes, never penalty constants (crt-013 #703).

## Open Questions

1. **Fixture host** — SCOPE permits either the nan-018 fixture corpus (needs ass-073's
   positive-edge revision) or the Python integration suite. Tester/Delivery picks; both satisfy
   the behavior contract. Not an architecture blocker.
2. **#406 reproduction signal** — if the deprecated-superseded-by-active fixture *does* reproduce
   #406, that signals the delivery fixture differs from ass-073's eval graph. Per SCOPE: **raise,
   do not patch retrieval.**

## ADRs

- `architecture/ADR-001-active-only-ppr-seeds.md` — Active-only seed predicate at Phase 0,
  inside the expander branch.
