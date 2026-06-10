## ADR-001: Active-Only PPR Expansion Seeds at Phase 0

### Context

The PPR expander (`SearchService::search`, Step 6d Phase 0 `graph_expand`, crt-030/crt-042)
builds its seed set `seed_ids` (`search.rs:~915`) from the **full** post-6a/6b candidate pool,
which mixes HNSW actives, 6b terminal-active injections, and deprecated/superseded entries that
the two-mode design (ADR-001 #481, C-03) deliberately keeps visible in Flexible search.

`graph_expand` applies no status filter to its seeds. ass-074 (#721) confirmed the expander is
enabled in production and injects in ~48% of queries. ass-074 also established (Unimatrix #4887)
that `penalty_map` is built once at Step 6a and that every later admit-stage — including Phase 0
neighbors — bypasses it at Step 7, receiving penalty `1.0`. Consequently a graph neighbor reached
*only because a deprecated entry was used as a seed* can enter the result set as a fresh,
unpenalized candidate.

This leak is **latent** today: the production graph is Active→Active only, so deprecated entries
have no positive out-edges to walk from. It becomes live the moment a deprecated entry acquires a
positive out-edge (#704). ass-073 (#720) confirmed penalty *magnitude* is not the lever — the
ranking path already ranks deprecated below comparable actives; the gap is purely this one
injection path. There is no tool that measures search-heuristic effectiveness (ass-074, #4888),
so this is a human-judgment correctness fix, not a tunable-validated one.

Alternatives considered and **rejected by locked human judgment** (SCOPE.md): adding
`find_terminal_active` redirect to injected entries; extending `penalty_map` to injected entries;
penalty-steepness tuning; raising the vnc-017 redirect ceiling; #585 edge-write hygiene. Each was
examined and deliberately left as-is — the most defensive option is not necessarily the most
helpful one, and none can be empirically validated as "better agent results."

### Decision

Filter the Phase 0 seed set to active entries only, **inside** the `if self.ppr_expander_enabled`
branch, at the `seed_ids` build:

```rust
let seed_ids: Vec<u64> = results_with_scores
    .iter()
    .filter(|(e, _)| e.status == Status::Active)
    .map(|(e, _)| e.id)
    .collect();
```

Properties of this decision:

- **Predicate is the typed `Status::Active` comparison**, not a string compare (`Status` is
  `#[repr(u8)]` + `PartialEq`, schema.rs:8-15). `== Active` drops Deprecated, Proposed, and
  Quarantined seeds.
- **Only the seed list is narrowed.** `results_with_scores` is unchanged — deprecated entries
  remain in the pool, keep their HNSW penalty, and keep appearing in Flexible results (C-03,
  two-mode design intact).
- **6b terminal-active heads pass by construction** — they are pushed only after a
  `terminal.status == Status::Active` guard (:814), so the filter never drops a legitimate active
  anchor.
- **Single production change (C-01).** No other file or stage is edited. No injection-side
  redirect, no penalty-map extension, no steepness work, no ceiling change, no edge hygiene.
- **Off-path bit-for-bit identical (C-02).** The filter is lexically inside the
  `ppr_expander_enabled` block and touches only the local `seed_ids` binding; the default-off
  path never enters the block, so it is structurally unchanged.

Acceptance is **behavior-based only** (C-04): a fixture with deprecated A (superseded_by active B,
both with positive out-edges) asserts the BFS expands from B and an entry reachable only via A is
not injected; default-off identity holds; existing penalty/ranking tests pass untouched. No
eval-harness gate; no soft-GT P@5 (the #500 trap).

### Consequences

**Easier:**
- The one PPR injection path by which a deprecated entry could outrank a comparable active is
  closed (#704 closes on this PR). PPR now anchors only on current knowledge.
- The C-01 boundary is structurally hard to cross: the change references only `results_with_scores`,
  `EntryRecord.status`, `Status::Active`, and the local `seed_ids` — touching any locked mechanism
  requires editing code this design never names.
- Future agents have a clear, behavior-asserted contract for what the filter does (narrows seeds)
  and does not do (no traversal-direction, penalty, mode, or ceiling change).

**Harder / accepted:**
- A deprecated **neighbor** of an active seed reachable only via the vnc-017 >50-edge redirect
  ceiling is still not redirected. **Knowingly accepted** (Locked Decision 4), not a bug here.
- Effectiveness is unmeasurable (SR-01) — we accept behavior assertions on specific IDs as the
  only validation lever; we cannot prove "better agent results."
- Value is partly belt-and-suspenders while the graph stays Active→Active (#585 keeps it so at
  write time). Still correct; lower urgency until deprecated entries gain positive out-edges.
