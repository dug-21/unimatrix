# Pseudocode — `SearchService::search` Phase 0 Seed Filter

**Component**: `SearchService::search` Phase 0 seed build
**File**: `crates/unimatrix-server/src/services/search.rs`
**Edit site**: line **915**, inside `if self.ppr_expander_enabled {` (line **911**)
**Maps**: FR-01..FR-08, AC-01..AC-05, SR-02/SR-05/SR-06, ADR-001 (#4917)

> LOCKED single-edit feature. The production delta is exactly one `.filter(...)` clause on the
> `seed_ids` build. This file documents that one edit, its lexical placement, the typed-enum
> predicate, and its integration surface. It introduces **no** new function, helper, config,
> redirect, or penalty logic — any such addition is a C-01/R-03 scope-creep failure.

---

## Purpose

Narrow the PPR Phase 0 expansion seed set so `graph_expand` anchors its forward BFS **only** on
`Status::Active` entries. This closes the one path (deprecated entry as BFS seed → its neighbors
injected at unpenalized weight 1.0) by which a deprecated entry could inject current-looking
neighbors into Flexible search results. `results_with_scores` itself is **not** filtered — only the
seed list handed to `graph_expand`.

---

## The Edit (the entire production delta)

```
# BEFORE (line 915)
let seed_ids: Vec<u64> = results_with_scores.iter().map(|(e, _)| e.id).collect();

# AFTER (crt-053)
let seed_ids: Vec<u64> = results_with_scores
    .iter()
    .filter(|(e, _)| e.status == Status::Active)   # crt-053: active-only seeds
    .map(|(e, _)| e.id)
    .collect();
```

Nothing else in the function changes. No import needed (`Status` already in scope, line 10).

---

## Pseudocode Body (in surrounding context — for orientation only; only the filter clause is edited)

```
function SearchService::search(...):                       # signature UNCHANGED
    ...                                                     # Steps 1..5: query embedding, HNSW pool
    # Step 6a: build penalty_map ONCE over HNSW pool       # UNCHANGED
    # Step 6b: inject terminal-active heads                 # UNCHANGED
    #          (each pushed only if terminal.status == Status::Active, line 814)
    #   => results_with_scores : Vec<(EntryRecord, f64)>, MIXED statuses

    if self.ppr_expander_enabled:                          # line 911 — guard UNCHANGED
        phase0_start = Instant::now()                      # UNCHANGED

        # ----- THE ONLY EDIT (line 915) -----
        # Collect seed IDs from results_with_scores (post 6a + 6b),
        # KEEPING ONLY entries whose status == Status::Active.
        seed_ids : Vec<u64> =
            results_with_scores
              .iter()
              .filter( (entry, _score) -> entry.status == Status::Active )   # FR-01, FR-02
              .map( (entry, _score) -> entry.id )
              .collect()
        # ------------------------------------

        expanded_ids : HashSet<u64> =                      # lines 919-924 UNCHANGED
            graph_expand(&typed_graph, &seed_ids, self.expansion_depth, self.max_expansion_candidates)

        in_pool : HashSet<u64> = seed_ids -> set           # line 929 UNCHANGED (inherits filter)
        sorted_expanded = sort(expanded_ids)               # determinism (NFR-03/NFR-04) UNCHANGED

        for expanded_id in sorted_expanded:                # neighbor admission UNCHANGED
            if in_pool.contains(expanded_id): continue
            entry = self.entry_store.get(expanded_id) ? else continue        # silent skip on err
            if SecurityGateway::is_quarantined(&entry.status): continue      # line 950 UNCHANGED (R-11)
            emb = self.vector_store.get_embedding(expanded_id) ? else continue
            cosine = cosine_similarity(&query_embedding, &emb)
            results_with_scores.push((entry, cosine))      # UNCHANGED

        tracing::debug!( seeds = seed_ids.len(), ... )     # UNCHANGED (R-10: debug! not info!)

    # Phases 1..5, Step 7 scoring, Step 10 floors, Step 11 — ALL UNCHANGED (FR-04, FR-06)
    ...
```

---

## Predicate Design (FR-02 / SR-02 / R-12)

- The predicate is the **typed enum comparison** `entry.status == Status::Active`. `Status` is
  `#[repr(u8)]` and derives `PartialEq` (schema.rs:8-15), so `==` is a discriminant comparison.
- **Never a string compare.** Do not match on `status.to_string()` or any text form — that would
  drift if the `Display` impl or serialization changes (R-12).
- The predicate is `== Active`, **not** `!= Deprecated`. This deliberately drops `Deprecated`,
  `Proposed`, **and** `Quarantined` from the seed set. A test must prove a non-Deprecated
  non-Active status (Proposed or Quarantined) is also excluded, to confirm the predicate is
  `== Active` (RISK-TEST Edge Cases; R-12).
- The closure binds the score to `_` — the score type (`f64`) is irrelevant to the predicate.

---

## State Machine

None. This is a stateless expression edit inside an existing function. No lifecycle, no new states.

---

## Initialization Sequence

None added. `ppr_expander_enabled` is an existing `bool` field on `SearchService` (line 384),
set at construction (line 552). The filter introduces no new field, config flag, or init step.

---

## Data Flow

| Stage | Input | Output | Transformation |
|-------|-------|--------|----------------|
| Pre-edit (lines 681..~821) | HNSW candidates + 6b injections | `results_with_scores : Vec<(EntryRecord, f64)>` (mixed statuses) | unchanged |
| **The filter (915)** | `results_with_scores` (read-only `.iter()`) | `seed_ids : Vec<u64>` (active anchors only) | `filter(status == Active).map(id)` |
| BFS (919-924) | `&seed_ids`, `&typed_graph`, depth, max | `expanded_ids : HashSet<u64>` | forward BFS over Outgoing positive edges — unchanged |
| Admission (936-969) | `expanded_ids`, dedup `in_pool` | mutated `results_with_scores` | per-neighbor fetch/gate/embed/cosine push — unchanged |

`results_with_scores` is read immutably by the filter (`.iter()`); the deprecated entries it holds
remain in the pool, keep their HNSW penalty, and keep appearing in Flexible results (C-03, FR-06).

---

## Error Handling

The filter clause itself **cannot fail**: it reads an already-loaded, in-memory `Status` enum on
entries already in `results_with_scores`. No I/O, no lock, no fallible call, no `unwrap`, no panic
path (NFR-04). Existing downstream error handling is unchanged:

- `entry_store.get` error → silent `continue` (line 945) — unchanged.
- `get_embedding` returns `None` → silent `continue` (line 960) — unchanged.
- `is_quarantined` true → silent `continue` (line 951) — unchanged; **NOT** the seed filter (R-11).

Boundary behavior — **all seeds deprecated**: `seed_ids` is empty ⇒ `graph_expand` walks zero
seeds ⇒ empty `expanded_ids` ⇒ no neighbors injected ⇒ no panic; HNSW + 6b results returned
normally (RISK-TEST Failure Modes). The empty-`Vec` path is already well-defined.

---

## Integration Surface (consumed, not modified)

| Symbol | Signature / Type | Relationship |
|--------|------------------|--------------|
| `results_with_scores` | `Vec<(EntryRecord, f64)>` | filter input (read via `.iter()`) |
| `EntryRecord.status` | `Status` | predicate field |
| `Status::Active` | enum variant (`#[repr(u8)]`, `PartialEq`) | predicate comparator |
| `seed_ids` | `Vec<u64>` | filter output; sole seed slice to `graph_expand` (OQ-2) |
| `graph_expand` | `fn(&TypedRelationGraph, &[u64], depth, max) -> HashSet<u64>` | downstream consumer — **unchanged**, receives narrower slice |
| `ppr_expander_enabled` | `bool` (SearchService field) | enclosing guard (line 911) |
| `SecurityGateway::is_quarantined` | `is_quarantined(&Status) -> bool` (line 950) | downstream per-entry gate — **unchanged, do not edit (R-11)** |

No new symbol is introduced. Downstream agents: reuse these exact names.

---

## Key Test Scenarios (hints for the tester — not the test plan)

All behavior-based (assert entry-ID presence/absence/rank), **never** penalty constants (C-04) and
**never** an eval-harness metric gate (SR-01 / NFR-05; the #500 soft-GT P@5 trap).

1. **AC-01 / AC-05 — seed exclusion with differential control (primary, R-04).** Fixture: Active
   entry B and Deprecated entry A (A `superseded_by` B), each with a positive out-edge to a
   **distinct** neighbor reachable by no other active path. Assert: neighbor of B is injected;
   neighbor reachable **only** via A is absent. **Control arm (R-04, mandatory):** with the filter
   removed (or A forced `Active`), A's neighbor reappears — proving absence is filter-caused, not
   unreachability (#4902 vacuous-pass guard). Both arms in one assertion set (#4902 truth table).

2. **AC-04 — terminal-active head survives (R-02 positive retention).** Fixture: superseded chain
   whose terminal active head is 6b-injected. Assert the 6b head anchors the walk and its
   active-only out-edge neighbor IS injected. Proves the filter retains legitimate actives, not
   only that it drops deprecated.

3. **AC-02 — off-path identity (C-02 / SR-04).** With `ppr_expander_enabled = false`, results
   (entries, order, scores) are identical to the pre-crt-053 baseline for the same fixture/query.

4. **AC-03 — ranking unchanged + anti-AC guard (C-03 / R-06).** Existing HNSW/penalty tests pass
   untouched; deprecated entries **still appear** in Flexible and are still penalized (ranked below
   a comparable active). **Forbidden:** no test asserting deprecated *absence* from Flexible.

5. **Direction-explicit (SR-06 / R-07).** Every seed-exclusion fixture states edge direction
   concretely (A→X, B→Y) and asserts on neighbor IDs — never by inspecting a `Direction::` enum.

6. **Predicate-is-`== Active` (R-12 edge case).** Include at least one `Proposed` or `Quarantined`
   seed and assert it is excluded — proving the predicate is `== Active`, not `!= Deprecated`.

7. **Empty-seed boundary.** All-deprecated pool ⇒ empty `seed_ids` ⇒ no injection, no panic; HNSW
   + 6b results returned.

8. **Superseded-but-still-Active retained.** An `Active` entry with `superseded_by` set still
   anchors expansion (discriminator is `status`, not the `superseded_by` field).

**Do not test (knowingly accepted residual, RISK-TEST Edge Cases):** a 6b head whose own neighbor
is deprecated and reachable only via the vnc-017 >50-edge redirect ceiling — Locked Decision 4/5,
not a test target.
