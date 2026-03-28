## ADR-005: PhaseFreqTableHandle as Required Non-Optional Constructor Parameter

### Context

Lesson #3216 (GH #311, dsn-001) documents the exact failure mode: a new Arc
handle is wired to `ServiceLayer` but `background.rs::run_single_tick` constructs
`StatusService` directly, bypassing `ServiceLayer`. The handle is accepted by
`spawn_background_tick` and `background_tick_loop` but silently dropped via a
`_underscore` stub before reaching `run_single_tick`. The serving path reverts to
`::default()`.

SR-01 identifies this as High / High for col-031: `run_single_tick` has an
established pattern of direct service construction that bypasses `ServiceLayer`.
`PhaseFreqTableHandle` must be threaded through the full chain:

```
spawn_background_tick
  → background_tick_loop
    → run_single_tick
      → PhaseFreqTable::rebuild
```

Pattern #3213 documents the full set of construction sites that must receive the
new parameter:

1. Sub-service struct field (SearchService)
2. Sub-service constructor (SearchService::new)
3. ServiceLayer::new and with_rate_config
4. run_single_tick (background.rs) — direct construction site, NOT via ServiceLayer
5. background_tick_loop parameter list
6. spawn_background_tick parameter list
7. All test helpers: server.rs, shutdown.rs, test_support.rs, listener.rs,
   eval/profile/layer.rs

Two options for enforcing wiring:

**Option A: Optional parameter (`Option<PhaseFreqTableHandle>`)**: Allows partial
wiring; `None` falls back to cold-start. This is the failure mode — a `None`
that should have been `Some` is silently correct at compile time.

**Option B: Required non-optional parameter**: Any construction site that omits
the parameter fails to compile. Missing wiring becomes a compile error, not a
silent regression.

### Decision

Use Option B: `PhaseFreqTableHandle` is a required, non-optional parameter in
every consuming constructor:

- `SearchService::new(…, phase_freq_table: PhaseFreqTableHandle)`
- `run_single_tick(…, phase_freq_table: &PhaseFreqTableHandle)`
- `background_tick_loop(…, phase_freq_table: PhaseFreqTableHandle)`
- `spawn_background_tick(…, phase_freq_table: PhaseFreqTableHandle)`

`ServiceLayer::with_rate_config` creates the handle once via
`PhaseFreqTable::new_handle()` and exposes it via
`ServiceLayer::phase_freq_table_handle()` for `main.rs` to pass to
`spawn_background_tick`.

Before declaring wiring complete, the implementer must grep for ALL direct
construction sites of `SearchService` in `background.rs` and confirm each
receives the handle.

### Consequences

**Easier**:
- Missing wiring is a compile error. The lesson #3216 failure mode cannot
  recur silently.
- Pattern is consistent with `TypedGraphStateHandle`, `EffectivenessStateHandle`,
  and `ConfidenceStateHandle` — reviewers recognize the convention.

**Harder**:
- Every test helper that constructs `SearchService` or calls
  `spawn_background_tick` must be updated. Pattern #3213 lists the known sites:
  `server.rs`, `shutdown.rs`, `test_support.rs`, `listener.rs`,
  `eval/profile/layer.rs`. The implementer must verify this list has not grown.
- The `background_tick_loop` and `run_single_tick` signatures already have
  `#[allow(clippy::too_many_arguments)]`; adding one more parameter does not
  change the suppression requirement.
