# SPECIFICATION: crt-018b — Effectiveness-Driven Retrieval

## Objective

Activate the effectiveness classifications produced by crt-018 inside the search re-ranking
and briefing assembly pipelines so that Effective entries are preferred and Ineffective/Noisy
entries are penalized at query time. Introduce a background-tick-driven auto-quarantine
mechanism that removes persistently bad entries after N consecutive maintenance cycles without
manual intervention.

## Functional Requirements

### FR-01 — EffectivenessState Cache

The system shall maintain a shared in-memory cache (`EffectivenessState`) holding:
- A per-entry classification map: `categories: HashMap<u64, EffectivenessCategory>`
- A per-entry consecutive bad cycle counter: `consecutive_bad_cycles: HashMap<u64, u32>`

The cache is initialized empty on server startup and is written exclusively by the background
maintenance tick loop. It must not be written by `context_status` MCP calls or any query-time
path.

**Testable via**: AC-01, AC-06, AC-09

### FR-02 — EffectivenessStateHandle Type

`EffectivenessStateHandle` is defined as `Arc<RwLock<EffectivenessState>>`. It is the sole
mechanism by which `SearchService` and `BriefingService` access effectiveness classifications.
It is a required (non-optional) constructor parameter on both services.

**Testable via**: AC-02, AC-07 (compile error if omitted)

### FR-03 — Background Tick Write

After `maintenance_tick()` calls `compute_report()`, it shall extract
`report.effectiveness` and acquire a write lock on `EffectivenessState` to:
1. Replace `categories` with the new per-entry classification map from the report
2. Update `consecutive_bad_cycles` according to FR-08 semantics
3. Execute auto-quarantine checks according to FR-10

This write occurs after `compute_report()` returns and before `run_maintenance()` is called.

**Testable via**: AC-01, AC-09, AC-10

### FR-04 — Utility Constants

Three constants shall be defined in `unimatrix-engine::effectiveness`:
- `UTILITY_BOOST: f64 = 0.05` — additive bonus for Effective entries
- `SETTLED_BOOST: f64 = 0.01` — additive bonus for Settled entries
- `UTILITY_PENALTY: f64 = 0.05` — additive penalty magnitude for Ineffective and Noisy entries

`SETTLED_BOOST` (0.01) must be strictly less than the co-access boost maximum (0.03) so it
does not overwhelm the established co-access signal hierarchy.

**Testable via**: AC-03, AC-05

### FR-05 — Utility Delta Function

The utility delta for an entry is computed from its `EffectivenessCategory` as follows:

```
utility_delta(category) =
    Effective   =>  +UTILITY_BOOST      (+0.05)
    Settled     =>  +SETTLED_BOOST      (+0.01)
    Ineffective => -UTILITY_PENALTY     (-0.05)
    Noisy       => -UTILITY_PENALTY     (-0.05)
    Unmatched   =>  0.0
    None (no classification present in EffectivenessState)  =>  0.0
```

**Testable via**: AC-04, AC-05, AC-06, AC-16

### FR-06 — Search Re-Ranking with Utility Signal

All four `rerank_score` call sites in `search.rs` (Steps 7 and 8, covering both the initial
sort pass and the co-access re-sort pass) shall apply the utility delta. The search pipeline
shall snapshot `EffectivenessState.categories` under a short read lock at the top of
`search()` before any downstream computation. The combined final score formula is:

```
confidence_weight = clamp(spread * 1.25, 0.15, 0.25)          -- from crt-019
similarity_weight = 1.0 - confidence_weight                     -- complement

base_score = similarity_weight * similarity + confidence_weight * confidence
           = (0.75..0.85) * similarity + (0.15..0.25) * confidence

final_score = base_score
            + utility_delta          -- [-0.05, +0.05], this feature
            + co_access_boost        -- [0.0, +0.03], crt-004
            + provenance_boost       -- [0.0, +0.02], lesson-learned entries
            + status_penalty         -- [-0.30, 0.0], Deprecated/Superseded multiplier
```

At the crt-019 minimum confidence weight (spread ~0.12 → weight = 0.15):
- `base_score = 0.85 * sim + 0.15 * conf`
- A `+0.05` utility boost remains meaningful relative to the 0.15 confidence term

At the crt-019 maximum confidence weight (spread >= 0.20 → weight = 0.25):
- `base_score = 0.75 * sim + 0.25 * conf`
- A `±0.05` utility delta is proportional to a 0.20-point shift in confidence

The ±0.05 magnitude does not dominate in either extreme of the crt-019 spread range. An
Ineffective entry at sim=0.95, conf=0.60 (spread=0.20) scores approximately 0.861 after
penalty — still surfaces in results but ranked below an Effective entry at sim=0.75,
conf=0.60 scoring approximately 0.916.

**Testable via**: AC-04, AC-05, AC-06

### FR-07 — Briefing Injection History Sort

`BriefingService::process_injection_history` shall use a two-key sort:
- Primary: confidence descending (unchanged)
- Secondary: effectiveness category score descending, where the category ordering is:
  `Effective (3) > Settled (2) > Unmatched (1) = nil (1) > Noisy (0) = Ineffective (0)`

`BriefingService` must accept `EffectivenessStateHandle` as a required constructor parameter
and snapshot `categories` under a short read lock at the top of `assemble()`.

**Testable via**: AC-07, AC-08

### FR-08 — Briefing Convention Sort

The convention lookup sort in `BriefingService` shall apply the same two-key sort as FR-07:
primary confidence, secondary effectiveness category score. Feature-promoted entries retain
front-of-list position (unchanged); effectiveness acts as tiebreaker among non-feature entries.

**Testable via**: AC-08

### FR-09 — Consecutive Bad Cycle Counter

`EffectivenessState.consecutive_bad_cycles: HashMap<u64, u32>` shall be maintained as follows
on each background tick write:

- **Increment**: Entry's category is `Ineffective` or `Noisy` in the current tick's
  classifications → counter incremented by 1
- **Reset**: Entry's category is `Effective`, `Settled`, or `Unmatched` in the current tick's
  classifications → counter reset to 0
- **Remove**: Entry does not appear in the current tick's active classification set (already
  Quarantined, Deprecated, or deleted) → entry removed from `consecutive_bad_cycles`
- **Hold on tick error**: If `compute_report()` returns an error, no write is performed on
  `EffectivenessState`. Counters are not incremented, not reset, and not removed. The previous
  state is retained unchanged. A structured audit event is emitted with `operation = "tick_skipped"`
  and the error string, so operators can observe skipped ticks.

Server restart resets all counters to 0 (in-memory only; this is intentional per Constraint 6).

**Testable via**: AC-09, AC-12, AC-15

### FR-10 — Auto-Quarantine Trigger

In `maintenance_tick()`, after writing `EffectivenessState`, for each entry where
`consecutive_bad_cycles[entry_id] >= AUTO_QUARANTINE_CYCLES` and
`AUTO_QUARANTINE_CYCLES > 0`:

1. Call `store.quarantine_entry(entry_id, reason)` with `agent_id = "system"` inside a
   `spawn_blocking` block
2. Write an audit event per FR-11
3. Reset `consecutive_bad_cycles[entry_id]` to 0
4. Fire-and-forget confidence recompute via `services.confidence.recompute(&[entry_id])`
5. Append `entry_id` to `auto_quarantined_this_cycle` on `StatusReport.effectiveness`

Auto-quarantine shall not fire for Settled or Unmatched entries.
Auto-quarantine shall not fire when `AUTO_QUARANTINE_CYCLES = 0`.

**Testable via**: AC-10, AC-11, AC-12, AC-13, AC-14

### FR-11 — Auto-Quarantine Audit Event Schema

Every auto-quarantine write shall emit an audit event containing the following fields:

| Field | Value |
|-------|-------|
| `operation` | `"auto_quarantine"` |
| `agent_id` | `"system"` |
| `entry_id` | the quarantined entry's numeric ID |
| `entry_title` | the entry's title string at time of quarantine |
| `entry_category` | the entry's knowledge category (e.g., `"convention"`, `"decision"`) |
| `classification` | the triggering `EffectivenessCategory` (`"Ineffective"` or `"Noisy"`) |
| `consecutive_cycles` | the cycle count that triggered quarantine (u32) |
| `threshold` | the configured `AUTO_QUARANTINE_CYCLES` value at time of trigger |
| `reason` | human-readable string, e.g. `"auto-quarantine: 3 consecutive Ineffective classifications in background maintenance tick"` |

This schema provides sufficient context for an operator to identify and restore a
falsely-quarantined entry using `context_quarantine` restore path.

**Testable via**: AC-13

### FR-12 — Auto-Quarantine Configuration

`AUTO_QUARANTINE_CYCLES` shall be read from the `UNIMATRIX_AUTO_QUARANTINE_CYCLES` environment
variable at server startup, defaulting to 3. A value of 0 disables auto-quarantine entirely.
Any positive integer N requires N consecutive bad background ticks (minimum N × 15 minutes of
wall time) before auto-quarantine triggers.

**Testable via**: AC-11, AC-12

### FR-13 — Tick Error Audit Event

When `compute_report()` returns an error during `maintenance_tick()`, the tick shall emit a
structured audit event with:
- `operation = "tick_skipped"`
- `reason` containing the error string

`EffectivenessState` is not modified. `consecutive_bad_cycles` counters are held, not
incremented.

**Testable via**: SR-07 integration test path

### FR-14 — StatusReport Visibility Field

`StatusReport.effectiveness` shall include an `auto_quarantined_this_cycle: Vec<u64>` field
listing entry IDs quarantined in the most recent background tick. This field is populated by
`maintenance_tick()` after auto-quarantine writes complete and is surfaced in `context_status`
output.

**Testable via**: AC-13 (indirectly via status report inspection)

## Non-Functional Requirements

### NFR-01 — Lock Acquisition Budget

The read-lock acquisition and `HashMap` clone for `EffectivenessState.categories` in
`search()` and `assemble()` must complete in under 1ms for up to 500 active entries (~32KB
clone). Lock must not be held during SQL execution or embedding computation.

### NFR-02 — Write Lock Duration

The write lock on `EffectivenessState` in `maintenance_tick()` must be held only for the
duration of the in-memory map update (HashMap replace + counter update). It must be released
before any SQL write (auto-quarantine) is issued.

### NFR-03 — No Additional SQL on Search Path

The utility delta lookup uses only the in-memory `EffectivenessState` snapshot. No SQL queries
are added to the `search()` or `assemble()` hot paths.

### NFR-04 — Stored Formula Invariant Preserved

`W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST = 0.92` is unchanged.
The utility delta is a query-time additive adjustment, not a stored weight component.

### NFR-05 — Auto-Quarantine Within spawn_blocking Budget

Auto-quarantine SQLite writes are synchronous and must be performed inside `spawn_blocking`.
Bulk quarantine events (e.g., multiple entries crossing threshold in the same tick) must
complete within the existing maintenance tick timeout budget.

### NFR-06 — Cold-Start Safety

Before the first background tick fires (~15 minutes after server start), all utility deltas
are 0.0 and all consecutive counters are 0. Search and briefing behavior is identical to
pre-crt-018b. No guard or fallback logic is required — the empty map produces correct 0.0 deltas.

### NFR-07 — No Retroactive Quarantine on Deployment

Because `consecutive_bad_cycles` initializes to 0 at server start, no entry is quarantined on
first deployment regardless of its current classification history. Entries must accumulate N
consecutive bad ticks post-deployment before auto-quarantine triggers.

## Acceptance Criteria

### AC-01 — EffectivenessState Write Source

`EffectivenessState` is written by the background tick loop in `background.rs` after each
`compute_report()` call using the `EffectivenessReport` already present in
`StatusReport.effectiveness`. `context_status` MCP calls do NOT write to `EffectivenessState`.

**Verification**: Integration test — call `context_status` N times, confirm `categories` map
remains empty until background tick fires; after tick fires, confirm map is populated.

### AC-02 — SearchService Snapshot

`SearchService::search()` snapshots `EffectivenessState.categories` under a short read lock
at the top of the search pipeline, before embedding computation or SQL execution, using the
same pattern as `confidence_weight` in crt-019.

**Verification**: Code review of `search.rs` snapshot placement; unit test confirming utility
delta is applied to all four `rerank_score` call sites.

### AC-03 — Utility Constants Defined

`UTILITY_BOOST`, `SETTLED_BOOST`, and `UTILITY_PENALTY` are defined as public `f64` constants
in `unimatrix-engine::effectiveness`. Default values: `UTILITY_BOOST = 0.05`,
`SETTLED_BOOST = 0.01`, `UTILITY_PENALTY = 0.05`.

**Verification**: Unit test asserts constant values; `SETTLED_BOOST < 0.03` (co-access max)
assertion in test suite.

### AC-04 — Utility Delta Applied at All Call Sites

All four `rerank_score` call sites in `search.rs` apply the utility delta:
`+UTILITY_BOOST` for Effective, `+SETTLED_BOOST` for Settled, `-UTILITY_PENALTY` for
Ineffective or Noisy, `0.0` for Unmatched or absent classification.

**Verification**: Unit tests exercise each category value against the scoring function and
assert correct delta application.

### AC-05 — Effective Outranks Near-Equal Ineffective

An Effective entry with sim=0.75, conf=0.60 ranks above an Ineffective entry with sim=0.76,
conf=0.60, all else equal (same `confidence_weight`, no co-access or provenance adjustment).

**Verification**: Unit test with fixed inputs asserts result ordering.

### AC-06 — Unclassified Entry Receives Zero Delta

An entry with no record in `EffectivenessState.categories` (e.g., newly inserted, not yet
seen by the background tick) receives `utility_delta = 0.0`. No panic, no default-to-penalty.

**Verification**: Unit test with empty `EffectivenessState` confirms 0.0 delta.

### AC-07 — BriefingService Injection History Sort

`BriefingService` accepts `EffectivenessStateHandle` as a required constructor parameter (not
`Option<EffectivenessStateHandle>`). Injection history sort uses effectiveness category as
tiebreaker: same confidence → Effective ranks above Ineffective/Noisy.

**Verification**: Unit test with two entries at equal confidence, differing categories, asserts
Effective appears first in briefing output.

### AC-08 — Convention Sort Tiebreaker

Convention lookup sort in briefing uses effectiveness as tiebreaker when feature-sort does not
differentiate entries: same confidence, non-feature-tagged → Effective before Ineffective.

**Verification**: Unit test asserts ordering of convention entries with identical confidence
and no feature tag.

### AC-09 — Consecutive Bad Cycle Counter Semantics

`EffectivenessState.consecutive_bad_cycles` increments for entries classified Ineffective or
Noisy on a background tick write; resets to 0 for entries reclassified to any other category;
is not incremented on `context_status` calls; is not incremented when `compute_report()`
returns an error (hold-on-error semantics).

**Verification**: Unit test simulates three tick writes with known category sequences and
asserts counter values after each write.

### AC-10 — Auto-Quarantine Trigger

When `consecutive_bad_cycles[entry_id] >= AUTO_QUARANTINE_CYCLES` (and
`AUTO_QUARANTINE_CYCLES > 0`), `maintenance_tick()` calls the store quarantine path for that
entry with `agent_id = "system"` and a reason string that includes the cycle count.

**Verification**: Integration test — seed entry as Ineffective-yielding, simulate N background
ticks, confirm entry status becomes Quarantined after tick N.

### AC-11 — Auto-Quarantine Threshold Configuration

`AUTO_QUARANTINE_CYCLES` is read from `UNIMATRIX_AUTO_QUARANTINE_CYCLES` env var at startup,
defaulting to 3. Setting to any positive integer N requires exactly N consecutive bad ticks
before triggering.

**Verification**: Integration test with `UNIMATRIX_AUTO_QUARANTINE_CYCLES=2` confirms
quarantine on second consecutive bad tick, not first.

### AC-12 — Auto-Quarantine Disable

Setting `UNIMATRIX_AUTO_QUARANTINE_CYCLES=0` disables auto-quarantine entirely. No entries
are quarantined regardless of classification or cycle count.

**Verification**: Unit test with counter at 100 and threshold 0 confirms no quarantine call
is issued.

### AC-13 — Auto-Quarantine Audit Event

Auto-quarantine writes an audit event containing `operation = "auto_quarantine"`,
`agent_id = "system"`, `entry_title`, `entry_category`, `classification`, `consecutive_cycles`,
`threshold`, and `reason` string.

**Verification**: Integration test reads the audit log after auto-quarantine fires and asserts
all required fields are present with correct values.

### AC-14 — Auto-Quarantine Category Restriction

Auto-quarantine only fires for entries classified Ineffective or Noisy. Settled and Unmatched
entries are never auto-quarantined regardless of counter value.

**Verification**: Unit test with Settled entry at `consecutive_bad_cycles = 10` confirms no
quarantine is triggered.

### AC-15 — Already-Quarantined Entry Not Incremented

If an entry is already Quarantined when a background tick runs, it does not appear in the
active entry set fed to `load_entry_classification_meta`. Its counter is not incremented and
is removed from `consecutive_bad_cycles` on the next tick write that omits it.

**Verification**: Unit test with pre-quarantined entry confirms its ID is absent from the
counter map after one tick.

### AC-16 — Utility Delta Unit Test Coverage

Unit tests cover: delta for all five `EffectivenessCategory` values, delta for absent entry
(0.0), Effective-vs-Ineffective ordering at close similarity values, `SETTLED_BOOST < 0.03`
constant invariant.

**Verification**: Test file in `unimatrix-engine::effectiveness` or `search.rs` test module.

### AC-17 — Integration Test Coverage

Integration tests cover:
1. Background tick with known injection/session data produces correct utility deltas in search
   result ordering
2. Briefing injection history orders Effective above Ineffective at same confidence
3. Auto-quarantine fires after N consecutive background ticks with bad classification
4. crt-019 confidence spread is non-zero in the test fixture (prerequisite check confirming
   the crt-019 adaptive weight dependency is exercised, not defaulted to cold-start behavior)

**Verification**: Tests in existing integration test infrastructure using `TestDb` helper.

### AC-18 — No Regression

All existing calibration, effectiveness classification, and re-ranking pipeline tests pass
without modification.

**Verification**: `cargo test` on `unimatrix-engine`, `unimatrix-store`, and `unimatrix-server`
crates shows no previously-passing tests failing.

## Domain Models

### EffectivenessState

```
EffectivenessState {
    categories: HashMap<u64, EffectivenessCategory>
        -- entry_id -> last-known category from background tick
        -- absent key means: not yet classified (utility_delta = 0.0)

    consecutive_bad_cycles: HashMap<u64, u32>
        -- entry_id -> count of consecutive background ticks with
        --             Ineffective or Noisy classification
        -- absent key means: counter is 0
        -- in-memory only; resets to empty on server restart
}

EffectivenessStateHandle = Arc<RwLock<EffectivenessState>>
```

### EffectivenessCategory (from crt-018, unchanged)

| Category | Meaning | utility_delta |
|----------|---------|---------------|
| `Effective` | Injected repeatedly, sessions succeed, positive helpfulness | +0.05 |
| `Settled` | Historically served its topic; lower recent activity | +0.01 |
| `Unmatched` | Insufficient data to classify | 0.0 |
| `Ineffective` | Injected >= 3 times, sessions fail or are abandoned | -0.05 |
| `Noisy` | Auto-sourced, zero helpfulness, never voted helpful | -0.05 |
| (absent) | Not yet seen by background tick | 0.0 |

### Utility Delta

A query-time additive f64 value derived from `EffectivenessCategory`. Applied to all
`rerank_score` call sites. Does not modify stored confidence. Does not affect the
`W_BASE + ... + W_TRUST = 0.92` stored formula invariant.

### Auto-Quarantine

The automated process by which `maintenance_tick()` sets a persistently bad entry's status
to `Quarantined` after `consecutive_bad_cycles >= AUTO_QUARANTINE_CYCLES`. Triggered by the
background tick loop; performed via the existing synchronous `quarantine_entry` store path;
attributed to `agent_id = "system"`. Reversible via manual `context_quarantine` restore
operation.

### Consecutive Bad Cycle Counter

An in-memory u32 counter per entry tracking how many consecutive background maintenance ticks
that entry has been classified Ineffective or Noisy. Resets on category improvement or server
restart. The N-cycle guard ensures auto-quarantine requires sustained, recent bad classification
rather than a single transient event.

### Background Maintenance Tick

The periodic 15-minute task in `background.rs` that calls `compute_report()` and
`run_maintenance()`. The sole writer of `EffectivenessState`. One tick = one unit in the
`consecutive_bad_cycles` count. Three cycles = minimum 45 minutes of wall time.

### Tick-Skipped Event

A structured audit event emitted when `compute_report()` returns an error during
`maintenance_tick()`. Signals that the effectiveness state was NOT updated this cycle. Counters
are held (not incremented) to prevent false auto-quarantine on stale or failed data.

## User Workflows

### Workflow 1 — Steady-State Search with Effectiveness Signal

1. Agent calls `context_search` with a query
2. `SearchService::search()` acquires read lock on `EffectivenessState`, clones `categories`
3. HNSW search, quarantine filter, status filter execute as before
4. At each `rerank_score` call site, `utility_delta(categories.get(entry_id))` is added to
   the base score
5. Results returned to agent; Effective entries ranked ahead of equivalent-confidence
   Ineffective entries

### Workflow 2 — Briefing Assembly with Effectiveness Tiebreaker

1. Agent calls `context_briefing` with topic/agent parameters
2. `BriefingService::assemble()` acquires read lock on `EffectivenessState`, clones `categories`
3. Injection history sort: primary confidence, secondary effectiveness category score
4. Convention sort: primary confidence, secondary effectiveness category score
5. Semantic search path delegates to `SearchService` which already applies utility delta (FR-06)
6. Token budget is spent on empirically useful entries

### Workflow 3 — Background Tick Writing EffectivenessState

1. `maintenance_tick()` calls `compute_report(None, None, false).await`
2. On success: extract `StatusReport.effectiveness` classification map
3. Acquire write lock on `EffectivenessState`
4. Replace `categories` map with fresh classifications
5. Update `consecutive_bad_cycles` per FR-09 semantics
6. Release write lock
7. For each entry with `consecutive_bad_cycles >= AUTO_QUARANTINE_CYCLES > 0`: execute
   auto-quarantine (FR-10), emit audit event (FR-11)
8. Continue to `run_maintenance()`
9. On error at step 1: emit tick-skipped audit event; do not modify `EffectivenessState`

### Workflow 4 — Operator Diagnosing Auto-Quarantine

1. Operator notices an entry has disappeared from retrieval results
2. Calls `context_status` to view `auto_quarantined_this_cycle` field
3. Reads audit log for `operation = "auto_quarantine"` events with full field set (FR-11)
4. Reviews `entry_title`, `entry_category`, `classification`, `consecutive_cycles` to
   determine if classification was accurate
5. If false positive: calls `context_quarantine` restore operation to reinstate entry
6. Optionally sets `UNIMATRIX_AUTO_QUARANTINE_CYCLES=0` to disable auto-quarantine during
   investigation

## Constraints

1. **No stored confidence formula change** — `W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR +
   W_TRUST = 0.92` invariant is unchanged. Utility delta is query-time only.
2. **No new database tables or columns** — `EffectivenessState` is in-memory. Consecutive
   counters are in-memory only, resetting on server restart. This is intentional: the counter
   must be freshly earned post-restart.
3. **No new MCP tools** — This feature modifies existing search, briefing, and maintenance
   pipelines only.
4. **No classification logic changes** — The five-category `classify_entry()` function in
   `unimatrix-engine::effectiveness` is unchanged.
5. **SETTLED_BOOST < co-access boost max** — `SETTLED_BOOST = 0.01 < 0.03 = co-access max`.
   Settled boost does not displace co-access as the dominant query-time differentiator.
6. **EffectivenessStateHandle is non-optional** — `BriefingService::new()` takes
   `EffectivenessStateHandle` as a required parameter, not `Option<_>`. Incomplete wiring is a
   compile error.
7. **Lock held only for in-memory updates** — Write lock on `EffectivenessState` is released
   before any SQL write (auto-quarantine). Read lock in `search()` is released before SQL or
   embedding computation.
8. **Hold on tick error** — `compute_report()` failure does not increment `consecutive_bad_cycles`.
   Old state is retained. Tick-skipped audit event is emitted.
9. **Auto-quarantine is spawn_blocking-compatible** — All store quarantine writes are
   synchronous SQLite, called from within `spawn_blocking` in `maintenance_tick()`.
10. **Test infrastructure is cumulative** — Extend existing `TestDb` helper, `tests_classify.rs`,
    `read.rs`, and search pipeline tests. Do not create isolated scaffolding.
11. **crt-019 adaptive weight must be active in integration test fixture** — The fixture must
    confirm non-zero confidence spread so the utility delta is exercised against real
    `confidence_weight` variation, not only the cold-start default.

## Dependencies

### Internal Crates

| Crate | Component | Usage |
|-------|-----------|-------|
| `unimatrix-engine` | `effectiveness::{EffectivenessCategory, EffectivenessReport}` | Classification types; new constants defined here |
| `unimatrix-engine` | `classify_entry()`, `utility_score()` | Unchanged; read by status service |
| `unimatrix-store` | `Store::compute_effectiveness_aggregates()` | Called inside `compute_report()`, unchanged |
| `unimatrix-store` | `Store::load_entry_classification_meta()` | Called inside `compute_report()`, unchanged |
| `unimatrix-store` | `quarantine_entry()` | Called by auto-quarantine path in `maintenance_tick()` |
| `unimatrix-server` | `services/confidence.rs` `ConfidenceState` / `ConfidenceStateHandle` | Structural pattern to mirror for `EffectivenessState` |
| `unimatrix-server` | `services/search.rs` `SearchService` | Receives `EffectivenessStateHandle`; apply utility delta |
| `unimatrix-server` | `services/briefing.rs` `BriefingService` | Receives `EffectivenessStateHandle`; apply category tiebreaker |
| `unimatrix-server` | `services/status.rs` `StatusService` | `compute_report()` already returns `StatusReport.effectiveness` |
| `unimatrix-server` | `background.rs` `maintenance_tick()` | New write path for `EffectivenessState` |
| `unimatrix-server` | `server.rs` `UnimatrixServer` | Holds `EffectivenessStateHandle`; wires into constructors |

### External / Prior Features

| Feature | Component | Dependency |
|---------|-----------|------------|
| crt-018 | `EffectivenessCategory`, `EffectivenessReport`, classification pipeline | Must be merged; provides all classification types and store queries |
| crt-019 | Adaptive confidence weight `clamp(spread * 1.25, 0.15, 0.25)` | Must be merged; integration test fixture must show non-zero spread |
| crt-004 | Co-access boost pattern (max +0.03) | Structural reference for additive query-time signal pattern |

### Environment Variables

| Variable | Default | Behavior |
|----------|---------|----------|
| `UNIMATRIX_AUTO_QUARANTINE_CYCLES` | `3` | Number of consecutive bad ticks before auto-quarantine; `0` disables |

## NOT in Scope

1. **New MCP tools** — No new tools added. `context_search`, `context_briefing`, and the
   background tick are the only modified surfaces.
2. **Schema migration** — No new tables, no new columns, no schema version bump.
3. **Classification logic changes** — `classify_entry()` and all five `EffectivenessCategory`
   definitions are read-only from this feature's perspective.
4. **Embedding/ML training** — Using effectiveness labels as ML training signal is a separate
   research track (issue #206, item 5).
5. **Retrospective "knowledge-that-helped" surfacing** — Per-entry contribution in retrospective
   output is a separate feature (issue #206, item 4).
6. **Persistent consecutive counter storage** — `consecutive_bad_cycles` is in-memory only.
   Durability across restarts is not in scope.
7. **Retroactive quarantine of existing Ineffective/Noisy entries** — Entries must accumulate
   N consecutive bad ticks post-deployment.
8. **UDS (Strict) path re-ranking** — The Strict retrieval mode hard-filters to Active-only
   and is not affected by this feature.
9. **Auto-quarantine undo tool** — Restore uses the existing `context_quarantine` restore
   operation; no new undo primitive is added.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for effectiveness re-ranking utility signal — returned #215
  (co-access boosting outcome), #703 (behavior-based status penalty tests), #485 (ADR-005
  deprecated/superseded penalty multipliers), #724 (behavior-based ranking test pattern
  asserting ordering not scores)
- Queried: /uni-query-patterns for background tick consecutive counter error semantics —
  returned #1542 (Background Tick Writers: Define Error Semantics for Consecutive Counters
  Before Implementation — directly applicable), #1366/#732 (Tick Loop Error Recovery:
  Extract-and-Catch Pattern)
- Queried: /uni-query-patterns for quarantine audit event operator recovery — returned #601
  (ADR-002: Restore fallback to Active), vnc-010 quarantine schema decisions
- Queried: /uni-query-patterns for ConfidenceState Arc RwLock pattern — returned #255
  (ADR-004: Batched Confidence Recomputation), #1480 (parameter-passing over shared state)
