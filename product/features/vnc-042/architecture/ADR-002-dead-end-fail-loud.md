## ADR-002: Dead-end chains return the originally-requested entry with a loud non-active flag

### Context

Resolution reuses `follow_to_current(store, id) -> Option<u64>`
(`graph_read_neighbors.rs:36-55`) with no new chain-walk (AC-05, C-1). The primitive
returns **`None`** — discarding the id it stopped at — in three cases:

- the chain terminates on an **orphaned deprecated** entry (`superseded_by IS NULL`,
  `status != Active`);
- the chain terminates on a **quarantined** entry;
- the chain exceeds the **50-hop** cap, or a store lookup errors.

AC-04 requires: a chain terminating on a non-active entry returns a result with a **loud
non-active flag — never empty, never silent** (mirrors `current`-mode's R-20 guard). But
AC-04's wording ("returns that entry") is ambiguous about *which* entry, because
`follow_to_current` does not hand back the stop-id (OQ-2). Two options:

- **(a)** return the **originally-requested** entry with the loud flag — pure
  `follow_to_current`, no new walk, AC-05-clean.
- **(b)** return the **non-active terminal** it stopped at — requires surfacing the stop-id
  via a helper tweak or a second query, brushing against AC-05's "no new chain-walk."

### Decision

**Adopt option (a): on `follow_to_current → None`, set `effective_id = id` (the
originally-requested id), fetch it, and attach a loud `DeadEnd { requested: id }` flag.**

```
follow_to_current(&self.store, id).await:
    Some(t) && t != id → effective_id = t;  ResolutionNote::Followed{from:id, to:t}
    Some(t) && t == id → effective_id = id;  no note (clean passthrough)
    None               → effective_id = id;  ResolutionNote::DeadEnd{requested:id}
```

The dead-end flag renders per ADR-003 (loud `⚠` line in text; structured
`{"status":"no_active_successor","requested_id":id}` in json). The response is never empty
and never silent — it returns the entry the caller actually named and states plainly that
the chain dead-ends on a non-active entry.

Rationale:

- **AC-05 clean.** No helper modification, no second query, no reimplemented walk. The
  50-hop cap and `status=0` terminal guard inside the reused primitives stay load-bearing
  and untouched (C-3, #4538).
- **AC-04 intent satisfied.** The intent is fail-loud, not silent-empty (vision principle
  #5: graceful degradation, not broken behavior). The requested id is the honest anchor:
  the caller asked for it, we return it, and we flag that no active successor exists.
- **Option (b) buys little at real cost.** Surfacing the stop-id would either fork
  `follow_to_current`'s signature (breaking its other callers) or add a second CTE query —
  scope creep against a "surgical single-tool" change, for a marginally different id in an
  edge case the flag already explains.

A store error inside `follow_to_current` collapses into the same `None` path and is
therefore also fail-loud via the flag — no silent success.

### Consequences

- **Easier:** the dead-end path is a trivial fallback (`effective_id = id`); no new store
  API, no signature churn on a shared primitive.
- **Harder:** the returned entry in the dead-end case is the *requested* (possibly
  deprecated) entry, not the non-active terminal. Acceptable — the flag makes the situation
  explicit, and the requested id is what the caller can act on. If a future need arises to
  inspect the exact stop node, `context_graph` mode `chain` already covers full-chain
  lookback (NG-2).
- Interacts with ADR-003 for rendering the flag; interacts with ADR-001's escape hatch
  (with `follow_supersessions=false` the walk is skipped entirely and the deprecated footer
  applies instead of the dead-end flag).
