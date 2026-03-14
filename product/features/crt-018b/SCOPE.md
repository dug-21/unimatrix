# crt-018b: Effectiveness-Driven Retrieval

## Problem Statement

crt-018 delivered a complete effectiveness analysis pipeline: every active entry is now
classified (Effective, Settled, Unmatched, Ineffective, Noisy) on every `context_status`
call. crt-019 established real confidence spread (target >= 0.20 across the active population)
and introduced an adaptive blend weight (`clamp(spread * 1.25, 0.15, 0.25)`).

Despite both foundations being in place, the effectiveness classifications are **read-only
observability** — they are reported in `context_status` but never acted on. Search re-ranking
uses similarity and confidence only. Briefing assembly ranks by confidence only. Ineffective
and Noisy entries continue to compete equally with Effective entries at query time.

The result is a retrieval quality gap: entries that are demonstrably harmful (Ineffective —
injected many times, sessions fail) or garbage (Noisy — auto-sourced, zero helpfulness,
injected but never voted helpful) rank identically to entries that have proven utility. The
system knows which entries work and which do not, but does not use that knowledge when serving
agents.

## Goals

1. Add a utility signal to search re-ranking that boosts entries classified Effective and
   penalizes entries classified Ineffective and Noisy, improving retrieval precision for agents
2. Rank `context_briefing` convention and semantic results by proven utility (incorporating
   effectiveness category) rather than confidence alone, so briefing token budget is spent on
   empirically useful entries
3. Automatically quarantine entries that have been consistently Ineffective or Noisy across N
   consecutive background maintenance ticks, reducing manual toil once the classification system
   has proven reliable
4. Expose the auto-quarantine threshold configuration so operators can tune or disable it during
   initial rollout

## Non-Goals

1. **Not a new MCP tool** — No new tools are added. This feature modifies existing search and
   briefing pipelines and adds background automation to the maintenance tick.
2. **Not a schema migration** — All data needed (injection_log, sessions, entries, existing
   effectiveness classification types) already exists. No new tables or columns.
3. **Not changing classification logic** — The five-category classification in
   `unimatrix-engine::effectiveness` is unchanged. crt-018b reads classifications, it does not
   redefine them.
4. **Not embedding tuning** — Using effectiveness labels as ML training signal (item 5 from
   issue #206) is a separate, higher-effort research track requiring data volume and ML
   infrastructure.
5. **Not the `context_retrospective` "knowledge-that-helped" feature** — Surfacing per-entry
   contribution in retrospective output (item 4 from issue #206) is a separate feature.
6. **Not modifying the confidence formula** — The stored six-factor confidence composite is
   unchanged. The utility signal is applied at query time as an additive boost/penalty, the same
   way co-access affinity and provenance boost work today.
7. **Not changing existing re-ranking weight sum invariant** — W_BASE + ... + W_TRUST = 0.92
   is a stored formula invariant. The utility signal is a query-time adjustment, not a stored
   weight.
8. **Not retroactive quarantine of all currently Ineffective/Noisy entries** — The N-cycle
   persistence guard means auto-quarantine only triggers after repeated classification. On first
   deployment, no entries are immediately quarantined.
9. **Not re-ranking UDS (Strict) path** — The Strict retrieval mode (UDS hook path) already
   hard-filters to Active-only. crt-018b applies only to the Flexible (MCP) path.

## Background Research

### What crt-018 Produced (Verified from Codebase)

**`unimatrix-engine::effectiveness` module** (`crates/unimatrix-engine/src/effectiveness/mod.rs`):
- `EffectivenessCategory` enum: `Effective`, `Settled`, `Unmatched`, `Ineffective`, `Noisy`
- `EntryEffectiveness` struct: `entry_id`, `title`, `topic`, `trust_source`, `category`,
  `injection_count`, `success_rate`, `helpfulness_ratio`
- `EffectivenessReport` struct: `by_category`, `by_source`, `calibration`, `top_ineffective`,
  `noisy_entries`, `unmatched_entries`, `data_window`
- `classify_entry(...)` — pure classification function
- `utility_score(success, rework, abandoned) -> f64` — weighted success rate (1.0/0.5/0.0)
- `INEFFECTIVE_MIN_INJECTIONS: u32 = 3` — minimum injection count before Ineffective verdict
- `NOISY_TRUST_SOURCES: &[&str] = &["auto"]` — trust sources eligible for Noisy classification

**Store layer** (`unimatrix-store::read`):
- `Store::compute_effectiveness_aggregates() -> Result<EffectivenessAggregates>` — 4 SQL
  queries (entry stats, active topics, calibration rows, data window)
- `Store::load_entry_classification_meta() -> Result<Vec<EntryClassificationMeta>>` — entry
  metadata for all active entries

**StatusService integration** — Phase 8 in `compute_report` (status.rs lines 661-736):
- Computes effectiveness report on every `compute_report()` call via `spawn_blocking`
- Populates `StatusReport.effectiveness: Option<EffectivenessReport>`
- **Key finding**: classifications are computed fresh each call and stored only in `StatusReport`.
  There is no shared in-memory cache. There is no history of "this entry was Ineffective last
  week and this week."

### What context_status Does (Verified — Critical Correction)

The MCP `context_status` handler (`mcp/tools.rs`) calls:
1. `status_svc.compute_report(params.topic, params.category, check_embeddings).await` — this
   runs Phase 8 (effectiveness) and returns the report
2. Populates tick metadata from `tick_metadata` mutex (last run, next scheduled, extraction stats)
3. Writes an audit event

**`context_status` makes no database writes and calls no maintenance operations.**
`run_maintenance()` is never called by `context_status`. It is called exclusively by the
background tick loop in `background.rs`.

The `maintain` parameter was removed from `context_status` in a prior feature. The handler
is now strictly read-only.

### How ConfidenceState Is Written (Verified — The Authoritative Pattern)

`ConfidenceState` (`services/confidence.rs`) is an `Arc<RwLock<ConfidenceState>>` holding
`{ alpha0, beta0, observed_spread, confidence_weight }`. This is the exact pattern crt-018b
should follow for `EffectivenessState`.

**Writer**: `StatusService::run_maintenance()`, Step 2b — after confidence refresh, the
maintenance pass computes empirical priors and observed spread, then acquires the write lock
and updates all four fields atomically.

**Reader**: `SearchService` — snapshots `confidence_weight` from `ConfidenceStateHandle` at
the top of `search()` under a short read lock.

**Trigger path**: `background.rs` `maintenance_tick()` calls:
1. `status_svc.compute_report(None, None, false).await` — gets report and active_entries
2. `status_svc.run_maintenance(&active_entries, &mut report, ...)` — here is where
   `ConfidenceState` is written (Step 2b)

The background tick fires every 15 minutes (`TICK_INTERVAL_SECS = 900`). `ConfidenceState`
is NOT written on `context_status` calls — only on the background tick.

### Where EffectivenessState Should Be Written

Phase 8 of `compute_report()` already computes all classifications. The background tick calls
`compute_report()` as the first step of `maintenance_tick()`, so the `StatusReport` returned
contains `report.effectiveness: Option<EffectivenessReport>` with fresh classifications.

**Correct pattern**: After `compute_report()` returns in `maintenance_tick()`, extract the
classification map from `report.effectiveness` and write it to `EffectivenessState` under a
write lock — before calling `run_maintenance()`. This mirrors how `run_maintenance()` writes
`ConfidenceState` from data computed in Phase 2b.

**Alternative**: Move the write into `run_maintenance()` by passing the effectiveness report
as a parameter, keeping all state writes co-located in one function.

**What does NOT happen**: `EffectivenessState` is NOT written on `context_status` calls.
`compute_report()` computes Phase 8 on every `context_status` call, but the resulting
classifications are only stored in `StatusReport.effectiveness` for display — they do not
flow to the shared `EffectivenessState`. The background tick is the sole writer.

This is consistent with `ConfidenceState`: both are background-tick-driven caches that make
computed-in-maintenance signals available to the query path without re-computation per query.

**Cold-start behavior**: `EffectivenessState` starts empty on server startup. The first
background tick (15 minutes after start) populates it. Before that, all utility deltas are
0.0 — search behaves identically to pre-crt-018b.

### How Search Re-Ranking Currently Works (Verified)

Pipeline (search.rs `search()` method):
1. Embed query, HNSW search, quarantine filter
2. Status filter/penalty (Flexible mode: penalties for Deprecated/Superseded)
3. Supersession injection (inject successor entries)
4. **Step 7**: `rerank_score(sim, confidence, confidence_weight)` — initial sort
5. **Step 8**: co-access boost — adds up to 0.03 to top-3 anchor entries
6. Truncate to k, apply floors, emit audit

The formula at full confidence weight (spread = 0.20):
```
final_score = 0.75 * similarity + 0.25 * confidence + co_access_boost + provenance_boost
```
Co-access boost max = 0.03. Provenance boost (lesson-learned) = 0.02.

**No effectiveness signal exists in this pipeline today.**

### How Briefing Assembly Currently Works (Verified)

`BriefingService::assemble()` (briefing.rs):
- Injection history: deduplicate by entry_id, keep highest confidence, **sort by confidence
  descending** within each category partition (decisions/injections/conventions)
- Convention lookup: `QueryFilter { category: "convention", status: Active }`, **sort by
  confidence descending** (feature-tagged entries promoted to front)
- Semantic search: delegates to `SearchService::search()` which already uses `rerank_score`

The injection history and convention paths sort purely by confidence. No effectiveness data
is consulted.

### How Quarantine Currently Works (Verified)

`quarantine_with_audit(entry_id, reason, audit_event)` on `UnimatrixServer` (server.rs):
- Sets status to `Status::Quarantined`, stores `pre_quarantine_status` for restore
- Writes audit event
- Calls `self.services.confidence.recompute(&[entry_id])` (fire-and-forget)
- Fully synchronous from caller's perspective; no async retry logic

**Current triggers for quarantine:**
1. Manual: `context_quarantine` MCP tool (Admin capability required)
2. No automated quarantine exists anywhere in the codebase

There is no persistence layer for historical effectiveness snapshots. crt-018 computes
classifications transiently on each `compute_report()` call with no memory of prior calls.

### Existing Patterns for Query-Time Signals

All query-time adjustments to re-ranking follow the additive-or-multiplicative pattern
established by prior features:
- Co-access affinity: additive boost `+0.03` max (applied in Step 8)
- Provenance boost (lesson-learned): additive `+0.02` (applied in Step 7 and 8)
- Deprecated/Superseded penalty: multiplicative (`0.7x` or `0.5x`, applied in Step 7/8)

A utility signal follows the same pattern. It does not touch the stored confidence formula.

### Confidence State Access Pattern (crt-019)

`SearchService` holds `ConfidenceStateHandle`. Readers snapshot the needed f64 values at the
top of `search()` under a short read lock. The maintenance tick (via `run_maintenance()`)
holds the write lock only for the four-field atomic update.

`EffectivenessState` must follow this same pattern: `Arc<RwLock<EffectivenessState>>` shared
between the background tick (writer) and `SearchService` + `BriefingService` (readers).

### Test Infrastructure

Current test count: 2169 unit + 16 migration integration + 185 infra-001 integration.
Test infrastructure uses `TestDb` helper. Effectiveness tests are in `read.rs` (store layer)
and `tests_classify.rs` / `tests_aggregate.rs` (engine layer). Integration tests for status
are in the existing status test infrastructure.

## Proposed Approach

### Change 1 — Effectiveness State Cache (new component)

Add `EffectivenessState` to hold a per-entry classification map:
```rust
pub struct EffectivenessState {
    /// entry_id -> EffectivenessCategory, populated by the background maintenance tick.
    pub categories: HashMap<u64, EffectivenessCategory>,
    /// consecutive cycles each entry has been Ineffective or Noisy (for auto-quarantine).
    pub consecutive_bad_cycles: HashMap<u64, u32>,
}
pub type EffectivenessStateHandle = Arc<RwLock<EffectivenessState>>;
```
Held by `UnimatrixServer` alongside `ConfidenceStateHandle`.

**Writer**: The background tick loop in `background.rs` — specifically, `maintenance_tick()`
after calling `compute_report()`. The Phase 8 effectiveness report is already computed and
returned in `StatusReport.effectiveness`. `maintenance_tick()` extracts the classification
map from `report.effectiveness` and writes it to `EffectivenessState` under a write lock
before passing control to `run_maintenance()`. No additional SQL queries are needed.

**Reader**: `SearchService` and `BriefingService` — snapshot `categories` at the top of
`search()` / `assemble()` under a short read lock (clone the needed HashMap snapshot).

**What does NOT trigger a write**: `context_status` MCP calls. Phase 8 runs inside every
`compute_report()` call (including `context_status`), but the resulting classifications are
only placed into `StatusReport.effectiveness` for display. They are NOT written to
`EffectivenessState`. Only the background tick writes `EffectivenessState`.

**Rationale**: Avoids a second `compute_effectiveness_aggregates()` SQL call on every search.
The background tick already pays that cost every 15 minutes. Mirrors the `ConfidenceState`
pattern exactly — background tick is the sole writer; query path is the reader.

### Change 2 — Utility Signal in Search Re-Ranking

Add an additive utility delta to `rerank_score` calls in search.rs:
```
utility_delta(category) = match category {
    Effective   =>  UTILITY_BOOST      (+0.05)
    Settled     =>  SETTLED_BOOST      (+0.01)
    Ineffective => -UTILITY_PENALTY    (-0.05)
    Noisy       => -UTILITY_PENALTY    (-0.05)
    Unmatched   =>  0.0
    // None (not yet classified) => 0.0
}
final_score = rerank_score(sim, conf, confidence_weight) + utility_delta + co_access_boost + ...
```

Constants exposed in `unimatrix-engine::effectiveness`:
- `UTILITY_BOOST: f64 = 0.05`
- `SETTLED_BOOST: f64 = 0.01`
- `UTILITY_PENALTY: f64 = 0.05`

Applied in all four `rerank_score` call sites in search.rs (Steps 7 and 8), using the
`EffectivenessState` snapshot taken at the top of `search()`.

**Rationale**: Additive delta matches the co-access and provenance boost patterns already
established. A 0.05 magnitude is large enough to meaningfully reorder ties but small enough
that a highly similar Ineffective entry (sim=0.95) still surfaces above a low-similarity
Effective entry (sim=0.50). The penalty for Noisy entries equals the Ineffective penalty —
both categories represent entries that should be displaced.

### Change 3 — Effectiveness-Weighted Briefing

Modify `BriefingService::process_injection_history` and the convention sort to incorporate
effectiveness category as a tiebreaker:
- Primary sort key: confidence descending (unchanged)
- Secondary sort key: effectiveness category score descending (Effective > Settled/Unmatched/nil > Ineffective > Noisy)

When `EffectivenessState` is passed to `BriefingService` (via constructor, following the
`ConfidenceStateHandle` pattern), the injection history and convention sorts can prefer
Effective entries over same-confidence Ineffective/Noisy entries.

The semantic search path through `SearchService` already benefits from Change 2.

### Change 4 — Auto-Quarantine with N-Cycle Guard

**Persistence layer**: `consecutive_bad_cycles: HashMap<u64, u32>` is held in
`EffectivenessState` (see Change 1). It is incremented on each background tick where an entry
remains Ineffective/Noisy; reset to 0 if the entry moves to any other category. Because
`EffectivenessState` is only written by the background tick, "N cycles" means N consecutive
background tick passes (not N `context_status` calls).

**Threshold constant**: `AUTO_QUARANTINE_CYCLES: u32 = 3` (tunable via env var
`UNIMATRIX_AUTO_QUARANTINE_CYCLES`). Default 3 means an entry must be classified Ineffective
or Noisy on 3 consecutive background maintenance ticks (minimum 45 minutes apart) before
auto-quarantine triggers.

**Trigger**: In `maintenance_tick()` (`background.rs`), after writing `EffectivenessState`:
- For each entry where `consecutive_bad_cycles >= AUTO_QUARANTINE_CYCLES`:
  - Call `store.quarantine_entry(entry_id, reason)` (existing synchronous write path)
  - Write audit event with `agent_id = "system"`, reason = "auto-quarantine: N consecutive
    Ineffective/Noisy classifications in background maintenance tick"
  - Reset counter to 0 (idempotent — already Quarantined next cycle)
  - Fire-and-forget confidence recompute

**Opt-out**: Setting `UNIMATRIX_AUTO_QUARANTINE_CYCLES=0` disables auto-quarantine entirely.
Default should be conservative (3 cycles) for initial rollout.

**Key constraint**: Auto-quarantine calls are synchronous SQLite operations, compatible with
`spawn_blocking` contexts. They should be performed inside a `spawn_blocking` block within
`maintenance_tick()`.

## Acceptance Criteria

- AC-01: `EffectivenessState` is written by the background tick loop in `background.rs` after
  each `compute_report()` call, using the `EffectivenessReport` already present in
  `StatusReport.effectiveness`. It holds `HashMap<u64, EffectivenessCategory>` for all active
  entries classified in the last background tick. `context_status` MCP calls do NOT write to
  `EffectivenessState`.
- AC-02: `SearchService::search()` snapshots `EffectivenessState.categories` under a short
  read lock at the top of the search pipeline (same pattern as `confidence_weight`).
- AC-03: `UTILITY_BOOST` and `UTILITY_PENALTY` constants are defined in
  `unimatrix-engine::effectiveness`. Default value: 0.05 each.
- AC-04: All four `rerank_score` call sites in search.rs (Steps 7 and 8, initial sort and
  co-access re-sort) apply the utility delta: `+UTILITY_BOOST` for Effective entries,
  `-UTILITY_PENALTY` for Ineffective or Noisy entries, 0.0 for all others.
- AC-05: An Effective entry with sim=0.75 and conf=0.60 ranks above an Ineffective entry with
  sim=0.76 and conf=0.60 (all else equal, including same confidence_weight). Verified by unit test.
- AC-06: An entry with no classification in `EffectivenessState` (e.g., newly inserted, not
  yet seen by the background tick) receives a 0.0 utility delta — no regression for
  unclassified entries.
- AC-07: `BriefingService` accepts `EffectivenessStateHandle` via constructor. Injection
  history sort uses effectiveness category as a tiebreaker (same confidence -> Effective ranks
  above Ineffective/Noisy). Verified by unit test.
- AC-08: Convention sort in briefing uses effectiveness category as a tiebreaker when feature
  sort does not differentiate entries.
- AC-09: `EffectivenessState` maintains a `consecutive_bad_cycles: HashMap<u64, u32>` counter.
  The counter increments for each entry that is Ineffective or Noisy in a background tick write;
  resets to 0 for entries that change to any other category. The counter is NOT incremented by
  `context_status` calls.
- AC-10: When `consecutive_bad_cycles[entry_id] >= AUTO_QUARANTINE_CYCLES` (default 3),
  `maintenance_tick()` calls the store quarantine path for that entry with `agent_id = "system"`
  and a reason string that includes the cycle count.
- AC-11: `AUTO_QUARANTINE_CYCLES` is configurable via `UNIMATRIX_AUTO_QUARANTINE_CYCLES` env
  var. Setting to 0 disables auto-quarantine. Setting to any positive integer N requires N
  consecutive bad background ticks before triggering.
- AC-12: Auto-quarantine is fully disabled when `AUTO_QUARANTINE_CYCLES = 0`. No entries are
  quarantined in this case regardless of classification. Verified by unit test.
- AC-13: Auto-quarantine writes an audit event with `operation = "auto_quarantine"`,
  `agent_id = "system"`, and a reason string. Verified by integration test.
- AC-14: Auto-quarantine does not trigger for Settled or Unmatched entries — only Ineffective
  and Noisy.
- AC-15: If an entry is already Quarantined when a background tick runs, its counter is not
  incremented (it is no longer in the active entry set fed to `load_entry_classification_meta`).
- AC-16: Unit tests cover: utility delta values for all five categories, newly-inserted entry
  receives 0.0 delta, Effective-vs-Ineffective ordering with close similarities.
- AC-17: Integration tests cover: background tick with known injection/session data produces
  correct utility deltas in search results; briefing injection history orders Effective above
  Ineffective at same confidence; auto-quarantine fires after N background ticks.
- AC-18: All existing calibration and regression pipeline tests continue to pass.

## Constraints

1. **No stored confidence formula change** — `W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR +
   W_TRUST = 0.92` invariant is unchanged. Utility delta is query-time only.
2. **No new database tables or columns** — `EffectivenessState` is in-memory, held in
   `Arc<RwLock<_>>`. The consecutive-cycle counter is in-memory only (resets on server restart).
   This is a deliberate constraint: the counter is meant to require recent, persistent bad
   classifications, not historical ones that may be stale.
3. **Performance budget**: `EffectivenessState` snapshot in `search()` is a read-lock + clone
   of a `HashMap<u64, EffectivenessCategory>`. For 500 active entries, this is ~32KB clone. Must
   not exceed 1ms for the lock acquisition.
4. **Cold-start behavior**: Before the first background maintenance tick (~15 minutes after
   server start), `EffectivenessState` is empty. All utility deltas are 0.0. Search behaves
   identically to pre-crt-018b behavior. This is safe and expected.
5. **Auto-quarantine cycle semantics**: `AUTO_QUARANTINE_CYCLES = 3` means 3 consecutive
   background tick passes, not 3 `context_status` calls. Because the background tick fires
   every 15 minutes, 3 cycles = minimum 45 minutes of persistent bad classification before
   automation acts.
6. **Server restart resets consecutive counters**: The in-memory counter resets on restart.
   This is intentional: a restart is itself a change event; the N-cycle guard should be freshly
   earned post-restart before auto-quarantine fires.
7. **Auto-quarantine is spawn_blocking-compatible**: The store quarantine write path is
   synchronous SQLite. It is called from within a `spawn_blocking` block in `maintenance_tick()`.
   No async calls in the auto-quarantine path.
8. **Test infrastructure**: Extend existing `TestDb` helper. Extend existing effectiveness
   tests in `read.rs` and `tests_classify.rs`. Extend existing search pipeline tests.
   Do not create isolated test scaffolding.
9. **Utility delta magnitude**: 0.05 is the proposed default. It must be small enough that
   a highly similar Ineffective entry (sim=0.95, conf=0.60, delta=-0.05) does not completely
   disappear from results (it still surfaces with final_score ~= 0.86), but large enough to
   meaningfully reorder closely competing entries.

## Resolved Decisions

1. **UTILITY_BOOST and UTILITY_PENALTY are symmetric** — 0.05/0.05. Equal magnitude for
   boost and penalty.
2. **Settled entries receive a small positive boost** — `+SETTLED_BOOST` (0.01). Settled
   entries historically served their topic well and should be preferred over unclassified
   entries of equal confidence.
3. **Briefing semantic search path benefits automatically from Change 2** — it already
   delegates to `SearchService::search()`. No additional briefing-specific change needed.
4. **EffectivenessStateHandle wired into BriefingService via constructor** — same pattern
   as `ConfidenceStateHandle` into `SearchService`. 2-line change in `server.rs`.
5. **`auto_quarantined_this_cycle: Vec<u64>` added to `StatusReport.effectiveness`** —
   gives operators visibility into which entries were auto-quarantined in the last tick.

## Open Questions

None — all questions resolved.

## Tracking

https://github.com/dug-21/unimatrix/issues/206
https://github.com/dug-21/unimatrix/issues/262
