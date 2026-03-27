## ADR-004: Lock Ordering for Three `Arc<RwLock<_>>` Handles on the Hot Path

### Context

`SearchService` holds three `Arc<RwLock<_>>` handles that are read on every search
call:

1. `EffectivenessStateHandle` — per-entry effectiveness classification (crt-018b)
2. `TypedGraphStateHandle` — pre-built typed relation graph (crt-021)
3. `PhaseFreqTableHandle` — phase-conditioned frequency table (col-031, new)

The background tick writes to all three. If a search call were to hold one lock while
attempting to acquire another, and the background tick were doing the same in a
different order, a deadlock could occur.

The existing codebase for handles 1 and 2 already documents the rule: each lock is
acquired, data is extracted (cloned or noted), and the lock is released before any
other lock is acquired or before scoring work begins. The `EffectivenessState` module
header explicitly states: "Readers hold short-lived read locks and release them before
acquiring any other lock (R-01 lock ordering)."

col-031 adds a third handle. SR-06 (SCOPE-RISK-ASSESSMENT.md) flags lock ordering
as a high-severity design-time concern. The architecture must document the ordering
explicitly so both the search hot path and the background tick implement it correctly.

No deadlock has been observed in the existing two-handle system because each lock is
released before the next is acquired. The same structural pattern must be applied to
the third handle.

### Decision

**Acquisition order for all callers (both search hot path and background tick):**

```
1. EffectivenessStateHandle   — read (search) or write (tick)
2. TypedGraphStateHandle      — read (search) or write (tick)
3. PhaseFreqTableHandle       — read (search) or write (tick)
```

**Structural enforcement rules:**

1. Each handle's lock acquisition, data extraction, and lock release is a separate
   lexical scope (`{ let guard = ...; let data = guard.clone_or_read(); }` — guard
   dropped at scope end). Scopes are strictly sequential, never nested.

2. In `SearchService::handle_search`: acquire EffectivenessStateHandle first, extract
   the effectiveness snapshot (or generation check), release; then acquire
   TypedGraphStateHandle, extract graph + entries, release; then acquire
   PhaseFreqTableHandle, extract `use_fallback` flag + table reference for scoring,
   release — all before the scoring loop begins.

3. In `run_single_tick` (background tick): the TypedGraphState swap block completes
   (lock acquired, `*guard = new_state`, lock released) before the PhaseFreqTable
   swap block begins (lock acquired, `*guard = new_state`, lock released). These are
   non-nested sequential blocks, consistent with the existing pattern for
   EffectivenessState and TypedGraphState.

4. All lock acquisitions use `.unwrap_or_else(|e| e.into_inner())` for poison
   recovery — consistent with all other handle acquisitions in the codebase.

5. The background tick NEVER holds a `PhaseFreqTableHandle` write lock while holding
   any other lock. The write is the last step in the tick's handle-update sequence.

### Consequences

**Easier:**
- Deadlock is structurally impossible: no code path holds two locks simultaneously.
- The pattern is identical to the existing EffectivenessState / TypedGraphState
  implementation; implementers have concrete examples to follow.
- Auditable: any `let guard = handle.write()` not followed immediately by `*guard =`
  and scope close is a violation detectable in code review.

**Harder:**
- The scoring loop cannot hold the `PhaseFreqTableHandle` read lock while iterating
  candidates. The lock must be released before the loop starts, meaning either the
  table data must be cloned (expensive for large tables) or the table is accessed via
  a reference held through the lock (requires the guard to live for the loop duration,
  violating the ordering rule).

  **Resolution:** The lock is held for a short duration at the start of
  `handle_search`, `use_fallback` is read, and if `use_fallback = false`, the table
  reference within the guard is used to pre-clone only the specific `(phase, category)`
  Vec needed for the current query. This is O(1) HashMap lookup and Vec clone rather
  than a full table clone. The guard is then released. The cloned Vec is used in the
  scoring loop without holding any lock. See ADR-001 for why bucket sizes are small
  enough to make this cheap.

  If `phase` is not present in `params.current_phase`, neither the lock nor any
  table data needs to be read — `phase_explicit_norm = 0.0` immediately.
