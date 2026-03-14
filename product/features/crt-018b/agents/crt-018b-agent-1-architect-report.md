# Agent Report: crt-018b-agent-1-architect

## Artifacts Produced

### ARCHITECTURE.md
`/workspaces/unimatrix/product/features/crt-018b/architecture/ARCHITECTURE.md`

### ADR Files
- `/workspaces/unimatrix/product/features/crt-018b/architecture/ADR-001-generation-counter-for-snapshot-cache.md` — Unimatrix #1543
- `/workspaces/unimatrix/product/features/crt-018b/architecture/ADR-002-tick-error-semantics-consecutive-bad-cycles.md` — Unimatrix #1544
- `/workspaces/unimatrix/product/features/crt-018b/architecture/ADR-003-utility-delta-inside-penalty-multiplication.md` — Unimatrix #1545
- `/workspaces/unimatrix/product/features/crt-018b/architecture/ADR-004-effectiveness-handle-required-briefing-constructor.md` — Unimatrix #1546

## Key Design Decisions

### ADR-001: Generation counter for snapshot cache (#1543)
`EffectivenessState` carries a `generation: u64` counter incremented on each write. `SearchService`
and `BriefingService` each hold an `Arc<Mutex<EffectivenessSnapshot>>` with the last-seen
generation and cached `HashMap`. The clone is skipped on the common path (no state change since
last call) and only occurs once per background tick. Lock ordering: release the
`EffectivenessState` read lock before acquiring the cache mutex to prevent deadlock.

### ADR-002: Hold consecutive_bad_cycles on tick error (#1544)
If `compute_report()` returns an error, `consecutive_bad_cycles` is held at its current value —
not incremented, not reset. The write lock is never acquired on the error path. A structured
`operation = "tick_skipped"` audit event is emitted so operators can detect frozen counters.
This prevents false-positive auto-quarantine from transient SQLite lock failures.

### ADR-003: Utility delta inside the penalty multiplication (#1545)
The `utility_delta` (+0.05 Effective, -0.05 Ineffective/Noisy, +0.01 Settled, 0.0 Unmatched/None)
is added inside the status-penalty multiplicand alongside provenance boost and co-access boost:
`final = (rerank_score(sim, conf, cw) + utility_delta + prov + boost) * status_penalty`.
This is consistent with all existing additive signals and preserves the logical invariant that
deprecated/superseded entries are penalized regardless of effectiveness history.

### ADR-004: EffectivenessStateHandle non-optional on BriefingService (#1546)
`BriefingService::new()` takes `EffectivenessStateHandle` as a required parameter — incomplete
wiring is a compile error. `ServiceLayer::with_rate_config()` creates the handle once and clones
it to `SearchService`, `BriefingService`, and `spawn_background_tick`. The accessor
`ServiceLayer::effectiveness_state_handle()` mirrors the existing `confidence_state_handle()`.

## Resolved Risk Items

| Risk | Resolution |
|------|-----------|
| SR-02: HashMap clone cost per search | ADR-001: generation-counter cache, clone only on tick |
| SR-03: Audit event richness for auto-quarantine | Audit detail includes title, category, cycle count, topic, entry_id |
| SR-06: BriefingService wiring not compile-enforced | ADR-004: non-optional constructor param |
| SR-07 (Critical): Tick error increments bad cycles | ADR-002: hold on error, emit tick_skipped audit |
| SR-04: Utility delta vs adaptive confidence weight | ADR-003 range analysis: delta displaces ties, does not suppress high-similarity entries |
| SR-01: Restart resets consecutive counters | Documented in ARCHITECTURE.md; in-memory only, intentional per SCOPE constraint 6 |

## Resolved Decisions Table

| ADR | Title | Unimatrix ID |
|-----|-------|-------------|
| ADR-001 | Generation Counter in EffectivenessState | #1543 |
| ADR-002 | Hold consecutive_bad_cycles on Tick Error | #1544 |
| ADR-003 | Utility Delta Inside Penalty Multiplication | #1545 |
| ADR-004 | EffectivenessStateHandle Required on BriefingService | #1546 |

## New Files Required

| File | Type | Purpose |
|------|------|---------|
| `crates/unimatrix-server/src/services/effectiveness.rs` | New | `EffectivenessState`, `EffectivenessStateHandle`, `EffectivenessSnapshot` types |

## Modified Files

| File | Modification |
|------|-------------|
| `crates/unimatrix-engine/src/effectiveness/mod.rs` | Add `UTILITY_BOOST`, `SETTLED_BOOST`, `UTILITY_PENALTY` constants; add `auto_quarantined_this_cycle: Vec<u64>` to `EffectivenessReport` |
| `crates/unimatrix-server/src/services/mod.rs` | Add `EffectivenessStateHandle` field to `ServiceLayer`; wire to search, briefing, background tick |
| `crates/unimatrix-server/src/services/search.rs` | Add `effectiveness_state` + cached snapshot; apply `utility_delta` in Steps 7+8 |
| `crates/unimatrix-server/src/services/briefing.rs` | Add `effectiveness_state` constructor param; sort tiebreaker in injection history and convention lookup |
| `crates/unimatrix-server/src/background.rs` | Write `EffectivenessState` after `compute_report()`; auto-quarantine scan; `tick_skipped` audit; add handle to `spawn_background_tick` signature |
| `crates/unimatrix-server/src/main.rs` | Pass `EffectivenessStateHandle` to `spawn_background_tick` |

## Open Questions

None. All SCOPE open questions were resolved before design. All SCOPE-RISK-ASSESSMENT concerns
have been addressed by the four ADRs and the ARCHITECTURE.md error boundary table.
