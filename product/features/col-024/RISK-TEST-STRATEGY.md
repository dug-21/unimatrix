# Risk-Based Test Strategy: col-024

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `cycle_ts_to_obs_millis` helper bypassed or duplicated — raw `* 1000` literal in query construction silently produces wrong window boundaries | High | Med | Critical |
| R-02 | `enrich_topic_signal` not applied at one of the four write sites — observations from that path carry `topic_signal: None` and are invisible to the primary lookup | High | Med | Critical |
| R-03 | Empty primary-path result treated as definitive by caller — legacy fallback not activated, producing an empty retrospective report for a valid cycle | High | Low | High |
| R-04 | `enrich_topic_signal` overrides an explicit extracted signal — observations attributed to the wrong feature when input contains a feature ID | High | Low | High |
| R-05 | Three-step algorithm runs across multiple `block_sync` calls — double `block_in_place` panic or nested-runtime creation at runtime | Med | Med | High |
| R-06 | Open-ended window (`cycle_start`, no `cycle_stop`) includes observations from a subsequent cycle that reuses the same session | Med | Med | High |
| R-07 | `load_cycle_observations` returns error instead of `Ok(vec![])` on no-cycle-events condition — legacy fallback never activates, `context_cycle_review` returns error to caller | Med | Low | Med |
| R-08 | Fallback log event (ADR-003) missing or at wrong level — attribution failures are silent post-deploy with no diagnostic signal | Med | Med | Med |
| R-09 | Step 3 Rust window-filter omitted — all observations from discovered sessions returned regardless of window membership, including observations from outside cycle windows | Med | Low | Med |
| R-10 | `parse_observation_rows` bypassed in new method — security bounds (64 KB input limit, JSON depth check) not applied to new query results | Med | Low | Med |
| R-11 | Multi-window deduplication not applied — session IDs appearing in multiple windows queried multiple times, observations duplicated in result | Low | Med | Low |
| R-12 | Enrichment applied to test/batch-import paths (out of scope) — `topic_signal` written in unexpected contexts changes behavior of unrelated paths | Low | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: Raw `* 1000` literal in window boundary construction
**Severity**: High
**Likelihood**: Med
**Impact**: Window boundaries are off by 1000×, matching zero observations or matching the entire table. Every `load_cycle_observations` call returns empty, silently activating the legacy fallback for all col-024+ features. Historical evidence: #3372 (ADR-002 col-024) was created specifically because this class of bug is invisible to the type system.

**Test Scenarios**:
1. Insert a `cycle_start` at a known timestamp `T`. Insert an observation at `T + 60s` (within the window). Assert `load_cycle_observations` returns that observation. If `* 1000` is missing, no observation is returned.
2. Insert an observation at `T - 1s` (before window start). Assert it is excluded. If conversion is applied twice (`* 1000000`), the in-window observation is excluded instead.
3. Code review: grep `services/observation.rs` for raw `* 1000` in window-binding code; must find zero occurrences.

**Coverage Requirement**: AC-01, AC-13. Both a positive inclusion test and a boundary exclusion test must pass; the helper must be the only multiplication site.

---

### R-02: `enrich_topic_signal` missing at a write site
**Severity**: High
**Likelihood**: Med
**Impact**: Observations written via that path have `topic_signal: None`. The primary lookup (Step 2) cannot discover the session. Silent attribution miss — the legacy fallback activates, masking the gap. Historical parallel: #981 and #756 — `sessions.feature_cycle` NULL failures were silent and discovered only during retrospective review.

**Test Scenarios**:
1. RecordEvent path: register session with feature `col-024`, send RecordEvent with no explicit `topic_signal`. Assert stored observation has `topic_signal = "col-024"`.
2. RecordEvents batch path: register session, send batch of three events, none with explicit `topic_signal`. Assert all three stored observations have `topic_signal = "col-024"`.
3. ContextSearch path: register session with feature, send ContextSearch with a non-feature-ID query. Assert stored observation has `topic_signal = "col-024"`.
4. Rework candidate path: same as scenario 1 applied to the rework handler.

**Coverage Requirement**: AC-05, AC-06, AC-07. Each of the four write sites must have its own test; a shared test covering only one site is insufficient.

---

### R-03: Empty primary path not forwarded to legacy fallback
**Severity**: High
**Likelihood**: Low
**Impact**: Features predating col-024 produce empty retrospective reports. Breaks AC-09 backward compatibility. All pre-existing feature reviews regress.

**Test Scenarios**:
1. No `cycle_events` rows for `cycle_id`: call `context_cycle_review` with sessions-table data present. Assert report is non-empty (legacy fallback fired).
2. `cycle_events` rows exist but no observations match `topic_signal`: assert legacy fallback activates (via the fallback log or mock path assertion).
3. Mock `ObservationSource`: verify `load_feature_observations` is called if and only if `load_cycle_observations` returns empty.

**Coverage Requirement**: AC-04, AC-09, AC-12. Existing `context_cycle_review` tests must pass without modification.

---

### R-04: Enrichment overrides explicit extracted signal
**Severity**: High
**Likelihood**: Low
**Impact**: Observation input containing a feature ID pattern (e.g., `bugfix-342`) is correctly extracted by `extract_topic_signal`, but the registry has `col-024` — if enrichment overrides, the observation is attributed to `col-024` instead of `bugfix-342`. Cross-cycle contamination.

**Test Scenarios**:
1. Session registered with feature `col-024`. Send RecordEvent whose input contains `bugfix-342` (matching `extract_topic_signal`). Assert stored `topic_signal = "bugfix-342"`, not `"col-024"`.
2. Unit test `enrich_topic_signal` directly: call with `extracted = Some("bugfix-342")` and registry having feature `"col-024"`. Assert returns `"bugfix-342"`.
3. Assert `tracing::debug!` fires with both `extracted` and `registry_feature` values when they differ. Log must be at `debug` level, not `info`.

**Coverage Requirement**: AC-08. Must be tested at the `enrich_topic_signal` unit level and at each call site. The mismatch debug log is a non-negotiable component of AC-08.

---

### R-05: Multiple `block_sync` calls in `load_cycle_observations`
**Severity**: Med
**Likelihood**: Med
**Impact**: If Step 2's per-window loop calls `block_sync` per iteration, and `load_cycle_observations` is invoked from within a Tokio runtime (the normal production case), each re-entry panics with "Cannot start a runtime from within a runtime". Server crashes on any `context_cycle_review` call for a col-024+ feature. Historical parallel: #735, #1688 — spawn_blocking mis-use produced silent failures and panics in prior features.

**Test Scenarios**:
1. Integration test: call `load_cycle_observations` with two windows (requires two Step 2 queries) from within a `#[tokio::test]` context. Assert no panic, returns correct observations.
2. Code review: verify exactly one `block_sync(async { ... })` call exists in the method body; all awaits are inside that single async block.

**Coverage Requirement**: NFR-01. The multi-window case must be exercised inside an async test runtime.

---

### R-06: Open-ended window over-includes from subsequent session reuse
**Severity**: Med
**Likelihood**: Med
**Impact**: A session continues after `cycle_start` with no `cycle_stop`. If observations for a subsequent cycle are written to the same session, and enrichment has set `topic_signal = "col-024"` for those later observations, they are included in a retrospective for `col-024`. Report content is contaminated. ADR-005 documents this as accepted behavior, but the scenario must be tested to confirm the known limitation is bounded.

**Test Scenarios**:
1. Insert `cycle_start` at `T`, no `cycle_stop`. Insert observations at `T+60s` with `topic_signal = "col-024"` (in-window). Insert observations at `T+3600s` with `topic_signal = "col-024"` (still in open-ended window — should be included). Assert all returned.
2. Insert a subsequent `cycle_start` for a different `cycle_id` at `T+7200s`. Observations after that point have `topic_signal = "col-024-v2"`. Assert they are not returned for `load_cycle_observations("col-024")`.
3. Document test: confirm the doc comment on `load_cycle_observations` names this as a known limitation per ADR-005.

**Coverage Requirement**: The open-ended window test scenario from the architecture test table. The boundary must be `unix_now_secs()`, not a fixed constant.

---

### R-07: Error returned instead of `Ok(vec![])` on missing cycle_events
**Severity**: Med
**Likelihood**: Low
**Impact**: `context_cycle_review` propagates the error to the MCP caller instead of activating the legacy fallback. Pre-col-024 features return error responses to agents rather than reports. Breaks backward compatibility for all legacy features.

**Test Scenarios**:
1. Unit test: `load_cycle_observations` called with a `cycle_id` that has no rows in `cycle_events`. Assert return is `Ok(vec![])`, not `Err(...)`.
2. Verify no `?` propagation from the cycle_events query when the result set is empty (empty result is not an error in sqlx).

**Coverage Requirement**: AC-03, FR-06. Explicit assertion on `Ok(vec![])` shape, not just that `unwrap()` doesn't panic.

---

### R-08: Fallback log missing or at wrong tracing level
**Severity**: Med
**Likelihood**: Med
**Impact**: Post-deploy enrichment gaps are invisible. Engineers cannot confirm whether the primary path is working for col-024+ features without the debug log. ADR-003 is the sole observability mechanism for this diagnostic.

**Test Scenarios**:
1. Use `tracing_test` or a log-capture fixture: call `context_cycle_review` for a feature with no `cycle_events` rows. Assert `TRACE`/`DEBUG`-level log line containing `feature_cycle` value and message substring `"primary path empty"` is emitted.
2. Assert log is at `debug` level (not `info` or `warn`), so it is suppressed in production by default.

**Coverage Requirement**: AC-14. Log capture test or structured event assertion.

---

### R-09: Step 3 Rust window-filter absent
**Severity**: Med
**Likelihood**: Low
**Impact**: SQL Step 3 fetches all observations for discovered sessions bounded by `[min_window_start, max_window_stop]` but does not apply per-window Rust filtering. For multiple disjoint windows, observations between windows (outside any window) are included. Report content is inflated with observations from gap periods.

**Test Scenarios**:
1. Two disjoint windows: `(T, T+1h)` and `(T+3h, T+4h)`. Insert observations at `T+30m` (window 1), `T+2h` (gap — must be excluded), `T+3h30m` (window 2). Assert result contains only `T+30m` and `T+3h30m` observations.
2. Assert observation count in the multi-window test equals exactly the in-window count, not the broader session count.

**Coverage Requirement**: AC-02. Requires disjoint-window test with explicit gap-period observation.

---

### R-10: `parse_observation_rows` bypassed in new method
**Severity**: Med
**Likelihood**: Low
**Impact**: Observations with oversized input (>64 KB) or deep JSON are returned raw to the detection pipeline. Downstream detection rules receive unvalidated data, potentially panicking or producing incorrect metrics. Breaks NFR-05.

**Test Scenarios**:
1. Code review: verify `load_cycle_observations` calls `parse_observation_rows` on the Step 3 query result, with the same 7-column SELECT shape as existing methods.
2. Unit test: insert an observation with `input` exceeding 64 KB. Call `load_cycle_observations`. Assert it either truncates or is excluded per the existing `parse_observation_rows` behavior (not that it panics or returns raw oversized content).

**Coverage Requirement**: NFR-05. Must be verified at both the code level and with a boundary-input test.

---

### R-11: Session ID deduplication skipped across windows
**Severity**: Low
**Likelihood**: Med
**Impact**: The same session ID is returned from Step 2 for both windows and queried twice in Step 3 (if IN-clause deduplication is not performed). Observations from that session appear twice in the result. Detection rule counts are inflated.

**Test Scenarios**:
1. Single session with observations in both window 1 and window 2 (session spans both windows). Assert observation count equals the actual number of distinct observations, not 2× that count.

**Coverage Requirement**: AC-02 (implicit). The multi-window test should verify exact observation counts to catch duplication.

---

### R-12: Enrichment applied outside the four scoped write paths
**Severity**: Low
**Likelihood**: Low
**Impact**: Test helpers, batch-import paths, or other write paths pick up `topic_signal` values they should not have, corrupting test fixtures or production backfill data. Scope constraint 6 forbids enrichment outside the four UDS listener handlers.

**Test Scenarios**:
1. Code review: `enrich_topic_signal` is `fn` (not `pub`), scoped to `listener.rs`. Verify it is only called from the four handler sites.
2. Verify test helpers in `observation.rs` do not call enrichment — they insert raw `topic_signal` values directly.

**Coverage Requirement**: Scope constraint 6. Code-level verification.

---

## Integration Risks

**I-01: `ObservationSource` trait change breaks downstream consumers.**
`load_cycle_observations` is a new method on the trait. Any existing mock or test
implementation of `ObservationSource` (outside `SqlObservationSource`) must add the
method or fail to compile. The risk is a breaking change silently caught at test time
but missed in a partial compilation. Verify `cargo test` passes for both
`unimatrix-observe` and `unimatrix-server`. AC-10.

**I-02: `block_sync` bridge context at test time differs from production.**
Tests that call `load_cycle_observations` directly outside a Tokio runtime use the
transient-runtime branch of `block_sync`. Tests inside `#[tokio::test]` use
`block_in_place`. Both branches must be exercised; a test only in one context does not
validate production behavior. At minimum the multi-window test (R-05) must run inside
`#[tokio::test]`.

**I-03: `session_registry.get_state` returns `None` during rapid session creation.**
If a `RecordEvent` arrives for a session ID before `set_feature_force` completes (a
race in handler ordering), `enrich_topic_signal` silently returns `None`. This is
accepted behavior per FR-13, but a test should confirm the degraded-graceful path: an
unregistered session does not error, it produces `topic_signal: None`.

**I-04: `insert_cycle_event` test API vs raw SQL contract.**
Spec constraint 3 requires tests to use `SqlxStore::insert_cycle_event`. If that API's
column order or parameter binding is incorrect, all `cycle_events` test fixtures are
silently wrong. Unit test fixtures must verify at least one round-trip: insert via API,
query directly, assert row is present with expected values.

---

## Edge Cases

**E-01: Zero observations in a valid window.**
`cycle_events` has `(cycle_start, cycle_stop)` rows but zero observations carry
`topic_signal = cycle_id` within the window (enrichment was not deployed yet, or no
hooks fired). Step 2 returns no session IDs. Step 3 returns empty. Primary path returns
`Ok(vec![])`. Legacy fallback activates. Must not error. Test: insert cycle_events rows,
no matching observations; assert `Ok(vec![])` and fallback log emitted.

**E-02: `cycle_phase_end` rows between `cycle_start` and `cycle_stop`.**
The window-pairing algorithm must treat `cycle_phase_end` events as neither start nor
stop. Only `cycle_start` opens a window and `cycle_stop` closes it. Test: insert
`cycle_start`, `cycle_phase_end`, `cycle_stop` rows; assert one `(start, stop)` window
produced, not split by the phase-end.

**E-03: Multiple `cycle_start` rows without intervening `cycle_stop` (malformed event log).**
If two `cycle_start` rows appear in sequence (server restart mid-cycle writes a second
start), the algorithm must pair the first start with the first stop, or treat each
unmatched start as open-ended until a stop or end-of-events. Behavior must be defined
and tested; incorrect pairing could silently drop observations.

**E-04: `cycle_id` used for both `cycle_start` and a different feature's observations.**
Shared `cycle_id` namespace: if two features use the same string as `cycle_id`
(e.g., naming collision), Step 2 conflates their sessions. No runtime guard exists. Test:
two distinct sessions, both with `topic_signal = "col-024"` but attributed to different
features in registry. Assert `load_cycle_observations("col-024")` returns observations
from both, confirming the behavior is deterministic.

**E-05: `saturating_mul` overflow guard in `cycle_ts_to_obs_millis`.**
A malformed `cycle_events.timestamp` near `i64::MAX / 1000` would overflow without
`saturating_mul`. Test: insert a cycle_events row with `timestamp = i64::MAX`. Assert
`load_cycle_observations` does not panic; window boundary is clamped to `i64::MAX`.

**E-06: Empty `cycle_id` string.**
`load_cycle_observations("")` should return `Ok(vec![])` since no `cycle_events` rows
will match an empty cycle_id. Must not panic or produce a SQL error.

---

## Security Risks

**S-01: `cycle_id` as SQL bind parameter.**
Step 1 and Step 2 bind `cycle_id` as `?1` in parameterized queries. No SQL injection
risk from this input. Verify: `cycle_id` is never interpolated into the query string via
format macros. Code review: zero `format!` or string concatenation near cycle_id in
query construction.

**S-02: `topic_signal` written to `observations` from session registry.**
`enrich_topic_signal` reads `state.feature` from in-memory registry. The feature string
was originally set by `set_feature_force` from a `context_cycle(start)` MCP call. That
MCP input is admin-gated, but its value flows into `topic_signal` without length or
character validation. A very long feature ID or one containing special characters could
produce oversized `topic_signal` values. Blast radius: `topic_signal` is used only as a
filter in Step 2 SQL (parameterized); no injection risk. However, it is stored in every
observation. Verify `topic_signal` is subject to the same size constraints as other
`TEXT` columns, or add a length cap in `enrich_topic_signal`.

**S-03: `parse_observation_rows` security bounds apply to new query path.**
The 64 KB input size check and JSON depth check in `parse_observation_rows` apply when
Step 3 uses the existing parser. If the implementation accidentally uses a different row
mapper, those bounds are bypassed. Blast radius: oversized or deeply-nested observation
`input` fields reach detection rules. Covered by R-10 test scenarios.

**S-04: `block_sync` holds a write-pool connection for three SQL steps.**
All three steps run inside a single `block_sync` against `write_pool_server()`. The
write pool has `max_connections=1` (lesson #2130 — SQLite WAL BUSY_SNAPSHOT). Holding
the write pool for the entire three-step block during a retrospective call could block
concurrent observation writes for the duration. Severity is low because
`context_cycle_review` is an infrequent call, but the risk of write starvation during
a large retrospective should be acknowledged. No code change required; document as a
known limitation.

---

## Failure Modes

**FM-01: `load_cycle_observations` errors on SQL failure.**
Expected behavior: propagate `ObserveError` to `context_cycle_review`, which returns
an MCP error response. The legacy fallback must NOT activate on a SQL error — only on
`Ok(vec![])`. Verify: mock that returns `Err(...)` causes error propagation, not
fallback activation.

**FM-02: `block_sync` panics inside Tokio runtime.**
If Step 2's loop is accidentally placed outside the `block_sync` closure, calling
`block_sync` a second time panics. The server process crashes for that request;
other requests are unaffected (Tokio task isolation). Mitigated by R-05 test.

**FM-03: `context_cycle_review` timeout fires during large retrospective.**
`spawn_blocking_with_timeout` wraps `context_cycle_review`. If `load_cycle_observations`
scans a large window (e.g., abandoned open-ended cycle with 50K+ observations), the
timeout fires and returns `ERROR_HANDLER_TIMEOUT` to the caller. Expected behavior:
timeout error propagated cleanly; no partial results returned. No test required beyond
verifying the timeout wrapper is present.

**FM-04: Session registry unavailable during `enrich_topic_signal`.**
`session_registry.get_state` acquires a Mutex. If the registry is poisoned (a prior
thread panicked while holding the lock), `get_state` may panic. The architecture
describes this as a microsecond read with no expected contention, but poisoning is
possible. The `enrich_topic_signal` function should handle registry errors gracefully
(return `None` rather than panic). Verify the implementation does not `.unwrap()` on
the registry read.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (timestamp unit mismatch) | R-01 | Addressed: ADR-002 mandates `cycle_ts_to_obs_millis` helper; raw `* 1000` forbidden in query construction. R-01 tests verify the helper is the sole conversion site. |
| SR-02 (block_sync multi-step blocking) | R-05 | Addressed: ADR-001 mandates single `block_sync` entry enclosing all three steps. R-05 tests verify no double-entry panic in multi-window case. |
| SR-03 (open-ended window over-inclusion) | R-06 | Partially mitigated: ADR-005 documents `unix_now_secs()` cap with no max-age limit; over-inclusion for abandoned cycles is accepted. R-06 tests verify the known limitation boundary. |
| SR-04 (explicit signal vs registry mismatch) | R-04 | Addressed: FR-14 and ADR-004 enforce "explicit signal wins"; `enrich_topic_signal` returns `extracted` unchanged when `Some`. Mismatch IS logged via `tracing::debug!` with both values (AC-08). R-04 scenario 3 verifies the debug log fires. |
| SR-05 (per-site enrichment drift) | R-02 | Addressed: ADR-004 introduces `enrich_topic_signal` shared helper. R-02 tests each write site independently. |
| SR-06 (empty primary path indistinguishable from enrichment gap) | R-03, R-08 | Fully mitigated: ADR-003 adds `tracing::debug!` on fallback activation. AC-15 count pre-check (Step 0) now distinguishes "no cycle_events rows" from "rows exist but no match." R-08 test verifies log is emitted; R-03 tests verify legacy fallback activates; AC-15 tests verify count pre-check distinguishes the two empty cases. |
| SR-07 (topic_signal scan cost at scale) | — | Accepted: NFR-03 records scale assumption (20 K rows/window threshold for revisit). No test coverage needed; operational monitoring responsibility. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | 7 scenarios (3 for R-01, 4 for R-02) |
| High | 4 (R-03, R-04, R-05, R-06) | 9 scenarios |
| Med | 6 (R-07–R-12) | 10 scenarios |
| Low | 2 (R-11, R-12) | 3 scenarios |
| **Total** | **14** | **≥ 29 scenarios** |

Non-negotiable tests (failure invalidates the feature):
- `load_cycle_observations` with correct window boundary returns in-window observation (R-01 / AC-01 / AC-13)
- All four enrichment write sites return `topic_signal` from registry when extracted is `None` (R-02 / AC-05–07)
- `load_cycle_observations` returns `Ok(vec![])` for missing cycle_events (R-07 / AC-03)
- `enrich_topic_signal` returns explicit signal unchanged when `extracted = Some(x)` AND emits debug log on mismatch (R-04 / AC-08)
- `load_cycle_observations` count pre-check distinguishes "no rows" from "rows exist, no match" (AC-15)
- Multi-window excludes gap-period observations (R-09 / AC-02)
- Legacy fallback activates when primary path is empty (R-03 / AC-04, AC-09)

---

## Knowledge Stewardship
- Queried: /uni-knowledge-search for lesson-learned failures gate rejection -- found #1203 (gate validation), #2758 (non-negotiable test names); relevant context: #981/#756 (NULL feature_cycle silent failure pattern, directly informs R-02/R-03 severity)
- Queried: /uni-knowledge-search for risk patterns observation attribution sync trait -- found #3367 (topic_signal enrichment pattern, col-024), #755 (dependency inversion pattern)
- Queried: /uni-knowledge-search for timestamp unit mismatch -- found #3372 (ADR-002 col-024, confirms SR-01 is addressed via helper)
- Queried: /uni-knowledge-search for cycle_events topic_signal session attribution -- found #3373/#3374 (ADRs 003/004, fallback log and shared helper), confirming architecture addresses SR-05/SR-06
- Queried: /uni-knowledge-search for fire-and-forget spawn_blocking -- found #735 (pool saturation lesson), #1688 (spawn_blocking_with_timeout lesson), elevating R-05 to High priority based on historical evidence
- Stored: nothing novel to store -- risk patterns for this feature are architecture-specific to col-024; the timestamp-unit and enrichment-site patterns are already stored as #3372, #3373, #3374; the silent-attribution failure pattern is already captured in #981/#756
