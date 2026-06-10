# Pseudocode Overview — crt-053 Active-Only PPR Expansion Seeds

**Feature**: crt-053 (#717) · **Scope**: single surgical edit, LOCKED
**Source of truth**: ARCHITECTURE.md, SPECIFICATION.md (FR-01..FR-08), RISK-TEST-STRATEGY.md, ADR-001 (Unimatrix #4917)

> This is a one-line production change. This overview exists to pin the edit site, the
> types in play, the off-path equivalence argument, and OQ-2 — not to expand the design.
> Anything beyond the single `seed_ids` filter is a C-01/R-03 scope-creep failure.

---

## The Single Component

There is **one** affected production component and **one** edit. No decomposition.

| Component | File | Change |
|-----------|------|--------|
| `SearchService::search` Phase 0 seed build | `crates/unimatrix-server/src/services/search.rs` | Add `.filter(\|(e, _)\| e.status == Status::Active)` to the `seed_ids` build (line **915**), inside `if self.ppr_expander_enabled` (line **911**) |

Per-component pseudocode: `pseudocode/search-seed-filter.md`.

`graph_expand`, `Status`, `penalty_map`/Step 7, and the Step 6b terminal-active injection are
**unchanged** and are documented here only as the integration surface the filter sits between.

---

## Verified Edit Site (against live source — #4886 caution honored)

The brief/architecture cite `~915`; verified line-exact at delivery snapshot:

- Line **911**: `if self.ppr_expander_enabled {`  (guards the whole Phase 0 block)
- Line **914**: comment "Collect seed IDs from current results_with_scores (post Steps 6a + 6b)."
- Line **915**: `let seed_ids: Vec<u64> = results_with_scores.iter().map(|(e, _)| e.id).collect();`  ← **edit here**
- Lines **919–924**: `let expanded_ids: HashSet<u64> = graph_expand(&typed_graph, &seed_ids, self.expansion_depth, self.max_expansion_candidates);`
- Line **929**: `let in_pool: HashSet<u64> = seed_ids.iter().copied().collect();` (dedup set, derived from `seed_ids`)
- Line **950**: `SecurityGateway::is_quarantined(&entry.status)` — per-expanded-entry security gate. **NOT the seed filter. Do not touch (R-11).**

---

## Data Flow Across the Boundary

```
results_with_scores : Vec<(EntryRecord, f64)>        (post 6a penalty_map + 6b terminal-active injection; MIXED statuses)
        |
        |   crt-053 FILTER (the only delta): keep only e.status == Status::Active
        v
seed_ids : Vec<u64>                                   (active anchors only)
        |
        v
graph_expand(&typed_graph, &seed_ids, depth, max) -> expanded_ids : HashSet<u64>
        |   forward BFS over Outgoing positive edges; seed B with edge B->X surfaces X (SR-06)
        v
per expanded_id: in_pool dedup -> store.get -> is_quarantined gate (:950) -> embedding -> cosine
        |
        v
results_with_scores.push((entry, cosine_sim))        (neighbor admission — UNCHANGED)
```

The filter narrows **which entries anchor the walk**. It does not change traversal direction,
depth, ceilings, neighbor admission, PPR scoring, penalty application, or truncation (FR-04, FR-05).

---

## Types In Play (all pre-existing — nothing created)

| Symbol | Type / Definition | Source (verified) |
|--------|-------------------|-------------------|
| `Status` | `enum { Active=0, Deprecated=1, Proposed=2, Quarantined=3 }`, `#[repr(u8)]`, derives `PartialEq, Eq, Copy` | `unimatrix-store/src/schema.rs:8-15`; re-exported via `unimatrix_core` |
| `EntryRecord.status` | `pub status: Status` | `unimatrix-store/src/schema.rs:57` |
| `results_with_scores` | `Vec<(EntryRecord, f64)>` — **note `f64`, not `f32`** (the brief/architecture say `f32`; live type is `f64`, line 681). Irrelevant to the predicate — the score is bound to `_` — but recorded for accuracy. | local in `SearchService::search` (line 681) |
| `seed_ids` | `Vec<u64>` | local, line 915, inside the enabled branch |
| `ppr_expander_enabled` | `bool` field on `SearchService` (default `false`) | line 384; guards block at line 911 |
| `graph_expand` | `fn(&TypedRelationGraph, &[u64], depth, max) -> HashSet<u64>` — **signature unchanged**, consumes a narrower slice | `unimatrix_engine::graph::graph_expand` (imported line 21) |

**Import status (refines the brief):** `Status` is **already in scope** — imported at line 10
(`use unimatrix_core::{ ... Status ... };`) and already used in production code at lines 718,
727, 737, 765, 814, 1125. **No import edit is required.** The brief's "import if needed" is moot;
the only permissible adjacent edit turns out to be unnecessary. C-01 is even tighter than stated:
the diff is the filter clause alone.

---

## Off-Path Equivalence Argument (C-02 / SR-04 / FR-07)

The filter lives **strictly inside** `if self.ppr_expander_enabled {` (line 911) and rebinds only
the local `seed_ids` (line 915), a binding that exists nowhere outside this block. When
`ppr_expander_enabled = false` (the production default) the entire block — including the new
filter clause — is never entered. Therefore:

- No added iteration, allocation, branch, fetch, or lock on the off path.
- No shared helper, no shared struct field mutated, no value leaking outside the branch.
- The `ppr_expander_enabled = false` path is **bit-for-bit identical** to pre-crt-053.

This is guaranteed by **lexical scope**, not by discipline (ARCHITECTURE "Off-Path Equivalence").
Validated behaviorally by AC-02 (flag off ⇒ results identical to baseline).

---

## OQ-2 Resolution (architect confirmation)

**Confirmed: `results_with_scores` is the SOLE seed source for `graph_expand` inside the enabled
branch.** Verified against live source:

- `seed_ids` (line 915) is built only from `results_with_scores`.
- `graph_expand` (lines 919–924) receives exactly `&seed_ids` as its seed slice — no other
  collection is passed or merged.
- `in_pool` (line 929) is derived **from `seed_ids`**, so the dedup guard inherits the filter.
- No second seed-collection path, no append to `seed_ids`, no alternate `graph_expand` call exists
  inside the branch.

⇒ FR-01 scope is complete with the single filter; **no additional seed path requires inclusion**
(R-09 cleared).

---

## Sequencing Constraint

The filter runs **after** Step 6a (`penalty_map` built) and Step 6b (terminal-active injection,
line ~821) have completed, and **before** `graph_expand` (line 919). 6b terminal-active heads are
pushed only after the `terminal.status == Status::Active` guard (line 814), so they are present
in `results_with_scores` at line 915 and pass the predicate by construction (FR-03 / SR-02). The
filter cannot drop a legitimate active anchor.
