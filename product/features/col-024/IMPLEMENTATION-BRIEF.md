# col-024 Implementation Brief
# Cycle-Events-First Observation Lookup and Topic Signal Enrichment

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/col-024/SCOPE.md |
| Architecture | product/features/col-024/architecture/ARCHITECTURE.md |
| Specification | product/features/col-024/specification/SPECIFICATION.md |
| Risk Strategy | product/features/col-024/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-024/ALIGNMENT-REPORT.md |

---

## Goal

Redesign `context_cycle_review` to use `cycle_events` timestamps as the authoritative
primary observation lookup path, replacing the unreliable `sessions.feature_cycle` fast
path that is populated asynchronously and can be NULL, stale, or absent. Simultaneously,
enrich `topic_signal` at write time in `listener.rs` by falling back to the session
registry's in-memory `feature` when `extract_topic_signal` returns `None`, so that
observations written after `context_cycle(start)` carry attribution even when their input
text contains no recognizable feature ID pattern.

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| `ObservationSource` trait (`unimatrix-observe/src/source.rs`) | pseudocode/observation-source-trait.md | test-plan/observation-source-trait.md |
| `SqlObservationSource::load_cycle_observations` (`unimatrix-server/src/services/observation.rs`) | pseudocode/load-cycle-observations.md | test-plan/load-cycle-observations.md |
| `cycle_ts_to_obs_millis` helper (`unimatrix-server/src/services/observation.rs`) | pseudocode/load-cycle-observations.md | test-plan/load-cycle-observations.md |
| `enrich_topic_signal` helper (`unimatrix-server/src/uds/listener.rs`) | pseudocode/enrich-topic-signal.md | test-plan/enrich-topic-signal.md |
| `context_cycle_review` lookup order (`unimatrix-server/src/mcp/tools.rs`) | pseudocode/context-cycle-review.md | test-plan/context-cycle-review.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode and test-plan files are produced in Session 2 Stage 3a. The Component Map
lists expected components from the architecture — actual file paths are filled during delivery.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| All three SQL steps in `load_cycle_observations` run inside a single `block_sync` call | Single `block_sync` envelope containing Step 1 (cycle_events fetch), Step 2 (per-window session discovery loop with `.await`), and Step 3 (observation load + Rust filter). Matches pattern of `load_feature_observations` and `load_unattributed_sessions`. Prevents double-`block_in_place` panic and nested-runtime creation. | SR-02 | architecture/ADR-001-single-block-sync-entry.md |
| `cycle_events.timestamp` (seconds) to `observations.ts_millis` (milliseconds) unit conversion | Named helper `cycle_ts_to_obs_millis(ts_secs: i64) -> i64` using `ts_secs.saturating_mul(1000)`. No raw `* 1000` literals permitted in query-construction code. `saturating_mul` prevents i64 overflow on adversarial input. | SR-01 | architecture/ADR-002-named-timestamp-conversion-helper.md |
| Observability when primary path returns empty and legacy fallback activates | Emit `tracing::debug!` with structured fields `cycle_id` and `path` at both fallback transitions: primary-to-legacy and legacy-to-content-scan. Suppressed in production by default; visible with `RUST_LOG=debug`. | SR-06 | architecture/ADR-003-structured-log-on-primary-path-fallback.md |
| Single shared helper for topic_signal enrichment across all four write sites | Private free function `enrich_topic_signal(extracted, session_id, registry)` in `listener.rs`. Returns `extracted` unchanged when `Some(_)`. When `None`, reads `session_registry.get_state(session_id)?.feature`. Called at RecordEvent, RecordEvents batch (per-event in map), rework candidate, and ContextSearch sites. Override applied to `ObservationRow.topic_signal` after `extract_observation_fields` to avoid mutating immutable `ImplantEvent`. | SR-05 | architecture/ADR-004-shared-enrich-topic-signal-helper.md |
| Open-ended window stop boundary for `cycle_start` with no `cycle_stop` | Use `unix_now_secs()` at call time. No additional max-age cap. MCP handler timeout (`MCP_HANDLER_TIMEOUT`) already bounds scan duration. Over-inclusion for abandoned cycles is accepted behavior, documented in function doc comment per ADR-005. Cap is a forward-compatible enhancement. | SR-03 | architecture/ADR-005-open-ended-window-cap.md |

---

## Files to Create/Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-observe/src/source.rs` | Modify | Add `load_cycle_observations(&self, cycle_id: &str) -> Result<Vec<ObservationRecord>>` method to the `ObservationSource` trait. |
| `crates/unimatrix-server/src/services/observation.rs` | Modify | Implement `load_cycle_observations` on `SqlObservationSource`; add `cycle_ts_to_obs_millis` private helper; add unit tests for AC-01, AC-02, AC-03, open-ended window, exclusion outside window. |
| `crates/unimatrix-server/src/mcp/tools.rs` | Modify | Restructure `context_cycle_review` observation-loading block to three-path order: primary (`load_cycle_observations`), legacy-1 (`load_feature_observations`), legacy-2 (`load_unattributed_sessions` + `attribute_sessions`). Add `tracing::debug!` events on each fallback transition. |
| `crates/unimatrix-server/src/uds/listener.rs` | Modify | Add private `enrich_topic_signal` helper function; apply it at four observation write sites (RecordEvent ~line 684, rework candidate ~line 592, RecordEvents batch ~line 784, ContextSearch ~line 842). |

---

## Data Structures

### TimeWindow (logical, not a new type — constructed inline in `load_cycle_observations`)

```rust
// Pair of (start_ms, stop_ms) where both values are Unix epoch milliseconds.
// Derived from cycle_events rows: start from cycle_start, stop from cycle_stop
// or unix_now_secs() * 1000 if no cycle_stop exists.
(i64, i64)  // (start_ms, stop_ms)
```

### cycle_events columns (existing, read-only in col-024)

| Column | Type | Semantics |
|--------|------|-----------|
| `cycle_id` | TEXT | Feature identifier (e.g., `col-024`) |
| `event_type` | TEXT | `cycle_start` / `cycle_phase_end` / `cycle_stop` |
| `timestamp` | INTEGER | Unix epoch **seconds** (written by `unix_now_secs()`) |
| `seq` | INTEGER | Advisory monotonic counter; tie-breaks on timestamp equality |

### observations columns used by new query (existing, read-only for lookup)

| Column | Type | Semantics |
|--------|------|-----------|
| `session_id` | TEXT | Agent session identifier |
| `ts_millis` | INTEGER | Unix epoch **milliseconds** |
| `topic_signal` | TEXT (nullable) | Feature cycle ID; written at ingestion, enriched by col-024 |
| `hook`, `tool`, `input`, `response_size`, `response_snippet` | various | Parsed by `parse_observation_rows` unchanged |

---

## Function Signatures

### New: `ObservationSource` trait method

```rust
// unimatrix-observe/src/source.rs
fn load_cycle_observations(&self, cycle_id: &str) -> Result<Vec<ObservationRecord>>;
```

### New: `SqlObservationSource` implementation

```rust
// unimatrix-server/src/services/observation.rs
fn load_cycle_observations(&self, cycle_id: &str) -> Result<Vec<ObservationRecord>> {
    let pool = self.store.write_pool_server();
    block_sync(async {
        // Step 1: fetch (event_type, timestamp) from cycle_events WHERE cycle_id = ?1 ORDER BY timestamp ASC, seq ASC
        // Step 2: for each (start, stop) window, SELECT DISTINCT session_id FROM observations
        //         WHERE topic_signal = ?1 AND ts_millis >= cycle_ts_to_obs_millis(start)
        //                                 AND ts_millis <= cycle_ts_to_obs_millis(stop)
        // Step 3: SELECT 7-col shape FROM observations WHERE session_id IN (...)
        //         AND ts_millis >= min_window_ms AND ts_millis <= max_window_ms
        //         ORDER BY ts_millis ASC; then Rust-filter to retain only in-window records.
        // Step 0: count-only pre-check — SELECT COUNT(*) FROM cycle_events WHERE cycle_id = ?1
        //   If zero: return Ok(vec![]) — "no cycle_events rows" case (distinguishable from "rows exist, no match")
        // Step 1: fetch (event_type, timestamp) from cycle_events WHERE cycle_id = ?1 ORDER BY timestamp ASC, seq ASC
        // Step 2: for each (start, stop) window, SELECT DISTINCT session_id FROM observations
        //         WHERE topic_signal = ?1 AND ts_millis >= cycle_ts_to_obs_millis(start)
        //                                 AND ts_millis <= cycle_ts_to_obs_millis(stop)
        // Step 3: SELECT 7-col shape FROM observations WHERE session_id IN (...)
        //         AND ts_millis >= min_window_ms AND ts_millis <= max_window_ms
        //         ORDER BY ts_millis ASC; then Rust-filter to retain only in-window records.
        // Return Ok(vec![]) if Step 2 finds no sessions ("rows exist but no match" case).
    })
}
```

### New: unit-conversion helper

```rust
// unimatrix-server/src/services/observation.rs (module-private)
#[inline]
fn cycle_ts_to_obs_millis(ts_secs: i64) -> i64 {
    ts_secs.saturating_mul(1000)
}
```

### New: topic-signal enrichment helper

```rust
// unimatrix-server/src/uds/listener.rs (module-private)
fn enrich_topic_signal(
    extracted: Option<String>,
    session_id: &str,
    session_registry: &SessionRegistry,
) -> Option<String>
```

Returns `extracted` unchanged when `Some(_)`. When `None`, reads
`session_registry.get_state(session_id)` and returns `state.feature.clone()` if present.
When `extracted` is `Some(x)` and `x` differs from the registry feature, emits
`tracing::debug!` with both values for attribution forensics.
Handles missing registry entry gracefully by returning `None` (no `.unwrap()`).

---

## Constraints

1. **Timestamp units**: `cycle_events.timestamp` is seconds; `observations.ts_millis` is
   milliseconds. All window boundary comparisons must use `cycle_ts_to_obs_millis(ts)`. No
   raw `* 1000` literals in query-construction code (AC-13, ADR-002).

2. **Sync trait**: `ObservationSource` is a sync trait. `load_cycle_observations` must not
   use `async fn`. All three steps must run inside a single `block_sync(async { ... })`
   closure (NFR-01, ADR-001).

3. **test fixture API**: Tests must use `SqlxStore::insert_cycle_event(...)` for
   `cycle_events` rows, not raw SQL inserts (SPEC constraint 3).

4. **No new index on `topic_signal`**: The Step 2 query is bounded by the existing
   `idx_observations_ts` index narrowing the scan to the cycle window. Deferral threshold:
   revisit if a single cycle window exceeds 20 K rows (NFR-03).

5. **parse_observation_rows reuse**: Step 3 must use the existing `parse_observation_rows`
   with the same 7-column SELECT shape (`session_id, ts_millis, hook, tool, input,
   response_size, response_snippet`). The 64 KB input limit and JSON depth check apply
   unchanged (NFR-05).

6. **No schema migration**: Schema version remains at 15. All required columns and indexes
   already exist (NFR-02).

7. **Enrichment scope**: `enrich_topic_signal` applies only to the four UDS listener write
   paths (RecordEvent, rework candidate, RecordEvents batch, ContextSearch). Test paths,
   batch-import paths, and any other write paths are explicitly excluded (SPEC constraint 6).

8. **Legacy fallback semantics**: `load_cycle_observations` must return `Ok(vec![])` (not
   an error) when no `cycle_events` rows exist for the `cycle_id`. The legacy fallback
   must NOT activate on `Err(...)` — only on `Ok(vec![])` (FM-01).

9. **write pool connection held**: All three SQL steps hold the `write_pool_server()`
   connection inside the single `block_sync`. `max_connections=1` on the write pool means
   concurrent observation writes are blocked during a retrospective call. Acceptable at
   current call frequency; document as a known limitation (S-04).

10. **Abandoned-cycle open-ended window**: A `cycle_start` with no `cycle_stop` uses
    `unix_now_secs()` as the implicit stop. This may over-include observations from a
    subsequent reuse of the same session. Accepted behavior; document in function doc
    comment (ADR-005).

---

## Dependencies

| Dependency | Version / Location | Role |
|------------|--------------------|------|
| `unimatrix-observe` | workspace | `ObservationSource` trait definition; `extract_topic_signal` |
| `unimatrix-store` | workspace | `SqlxStore`, `write_pool_server()`, `insert_cycle_event` test API |
| `sqlx` | workspace | Async query execution inside `block_sync` |
| `tokio` | workspace | `block_in_place` underlying `block_sync` |
| `tracing` | workspace | `tracing::debug!` on fallback activation (ADR-003) |
| `cycle_events` table | schema v15 | Time windows source; `cycle_id`, `event_type`, `timestamp`, `seq` |
| `observations` table | schema v15 | `topic_signal`, `ts_millis`, `session_id` columns; existing indexes |
| `session_registry` | in-memory (`listener.rs`) | `get_state(sid)?.feature` for `enrich_topic_signal` |
| `parse_observation_rows` | `services/observation.rs` | Existing 7-column row parser; reused by new method |
| `block_sync` | `services/observation.rs` | Existing sync/async bridge; used by new method |

---

## NOT in Scope

- Changes to the `cycle_events` schema (no `session_id` column will be added).
- Changes to `ObservationRecord` in `unimatrix-core` (`topic_signal` is storage-level only).
- Changes to detection rules, metrics pipeline, or report format.
- Backfill of `topic_signal` for historical observations predating col-024.
- Changes to `sessions.feature_cycle` column or its async write path.
- Adding an index on `observations.topic_signal`.
- Enrichment for test paths, batch-import paths, or any write path outside the four UDS listener handlers.
- A composite index on `(topic_signal, ts_millis)` — deferred; revisit at 20 K rows/window.
- A maximum window cap or max-age limit for abandoned cycles.
- Runtime validation that `cycle_events.timestamp` is stored in seconds.
- A maximum window cap or max-age limit for abandoned cycles (documented known limitation).

---

## Alignment Status

ALIGNMENT-REPORT.md was not produced — the vision guardian agent did not run as part of
this Session 1 execution. The following notes are drawn from scope and specification
content; they are not a substitute for a formal vision alignment review.

No structural deviations were identified during synthesis:

- The feature is entirely within the Collective phase (col- prefix), addressing an
  attribution reliability defect in the retrospective pipeline. It makes no changes to
  public interfaces, schema, or tool signatures visible to external consumers.
- The three-path fallback preserves full backward compatibility with all features
  predating `cycle_events` (schema v15), consistent with the project's stated
  non-negotiable backward compatibility requirement.
- The enrichment mechanism relies solely on in-memory registry state already present
  at write time — no new async work, no new persisted state, no schema change.
- The observability addition (ADR-003 debug log) is at `debug` level and suppressed in
  production by default, consistent with the project convention of not adding info-level
  noise to hot paths.

Formal alignment review complete. ALIGNMENT-REPORT.md produced by vision guardian:
5 PASS, 1 WARN (minor — two architecture open questions that are now resolved), 0 variances.
No blocking items for Session 2.

---

## Resolved Design Questions

1. **AC-08 mismatch diagnostic**: `tracing::debug!` fires when extracted signal differs
   from registry feature. Explicit signal still wins (AC-08 unchanged). In scope.

2. **Empty-result disambiguation**: Count-only pre-check (Step 0) added to
   `load_cycle_observations`. Distinguishes "no cycle_events rows" from "rows exist but
   no match." Both cases return `Ok(vec![])` to caller. In scope (AC-15).

3. **ALIGNMENT-REPORT.md**: Vision guardian ran successfully. 5 PASS, 1 WARN (minor),
   0 variances. No blocking items.
