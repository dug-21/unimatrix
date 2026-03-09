# col-017: Risk-Based Test Strategy

## Risk Inventory

17 risks identified: 7 from scope (R1–R7), 3 from architecture (ADR-derived), 7 from specification analysis.

### Scope Risk Traceability

| Scope Risk | Description | ADR | Functional Reqs | Acceptance Criteria | Architecture Mitigation | Test Coverage |
|------------|-------------|-----|-----------------|---------------------|------------------------|---------------|
| R1: Cross-crate API surface | Making attribution extractors public across crate boundary | ADR-017-001 | FR-01.1, FR-01.2 | AC-01–AC-05, AC-22 | Facade pattern — single `extract_topic_signal()` pub fn | T-01, T-02, T-03 |
| R2: SessionState memory growth | Vec grows unbounded over session lifetime | ADR-017-002 | FR-05.1, FR-05.2 | AC-12 | HashMap<String, TopicTally> bounds to O(unique topics) | T-06, T-07 |
| R3: Migration coordination | Three parallel features (col-017/018/019) sharing v9→v10 | ADR-017-003 | FR-08.1–FR-08.6 | AC-19, AC-20 | col-017 owns migration shell; others append | T-14, T-15, T-16 |
| R4: False-positive attribution | `extract_feature_id_pattern` matches non-feature text | — | FR-01.1, FR-03.2 | AC-02, AC-05, AC-04 | Priority chain + majority vote + `is_valid_feature_id` | T-03, T-04, T-05 |
| R5: Wire protocol compat | New field breaks old server or old hook | — | FR-02.1–FR-02.4 | AC-06, AC-07 | `serde(default)` + `skip_serializing_if` | T-08, T-09 |
| R6: SessionClose race | Signals arrive after topic resolution | — | FR-06.2–FR-06.3 | AC-16 | UDS ordering; fire-and-forget write | T-12, T-13 |
| R7: Fallback performance | Content-based attribution slow on large sessions | — | FR-06.2, FR-07.1 | AC-17, AC-18 | Fallback only when no hook signals; ~100ms typical | T-17 |

---

## Architecture-Derived Risks

### AR-1: Facade Priority Inversion (from ADR-017-001) — MEDIUM

**What**: `extract_topic_signal()` encapsulates the priority chain (path > pattern > git). If the ordering is wrong or a future change inverts it, low-confidence signals (pattern matches) override high-confidence signals (file paths).

**Impact**: Wrong topic attribution for sessions where multiple signal types are present. Silently incorrect — no error, just wrong data.

**Test**: Verify priority ordering with input containing signals at multiple confidence levels.

### AR-2: TopicTally Tie-Breaking Determinism (from ADR-017-002) — MEDIUM

**What**: HashMap iteration order is non-deterministic. The majority vote tie-breaker uses `last_seen` timestamp, but if timestamps are identical (same-second events), the spec requires lexicographic fallback. Without explicit deterministic fallback, results vary between runs.

**Impact**: Flaky tests and non-reproducible attribution in edge cases.

**Test**: Verify tie-breaking with identical timestamps falls back to lexicographic ordering.

### AR-3: Migration Backfill Corrupts Actively-Running Sessions (from ADR-017-003) — LOW-MEDIUM

**What**: Migration backfills `feature_cycle` for sessions where `feature_cycle IS NULL`. The spec says `AND ended_at IS NOT NULL` (closed sessions only), but if this filter is missing or wrong, active sessions get backfilled with potentially wrong attribution while they're still accumulating signals.

**Impact**: Active session gets a stale/wrong topic that's never corrected (SessionClose sees feature already set and skips resolution).

**Test**: Verify backfill only touches closed sessions. Verify SessionClose still resolves topic even if feature_cycle already has a value (or explicitly does not).

---

## Specification-Derived Risks

### SR-1: ObservationRow/INSERT Column Count Mismatch — HIGH

**What**: `insert_observation()` and `insert_observations_batch()` have hardcoded SQL with positional parameters (`?1` through `?7`). Adding `topic_signal` requires updating to `?8` in both functions plus the `ObservationRow` struct. Any mismatch between struct fields, SQL columns, and parameter bindings causes runtime insertion failures.

**Impact**: Every observation write fails. Silent data loss (fire-and-forget path) or crash.

**Likelihood**: HIGH (manual SQL string maintenance, two functions to update in sync)

**Test**: Integration test inserting observations with and without topic_signal, verifying roundtrip.

### SR-2: `generic_record_event()` Over-Extraction — MEDIUM

**What**: FR-03.3 specifies extracting from `serde_json::to_string(&input.extra)` in `generic_record_event()`. This stringifies the entire JSON payload, which may contain incidental feature-ID-like patterns in keys, URLs, or error messages.

**Impact**: Higher false-positive rate than event-type-specific extraction. Pollutes accumulator.

**Likelihood**: MEDIUM (JSON often contains paths with version-like segments)

**Test**: Verify extraction from generic events with JSON containing false-positive patterns.

### SR-3: `update_session_feature_cycle` — New Store Method Not Tested Independently — LOW-MEDIUM

**What**: FR-06.4 requires a new `Store::update_session_feature_cycle()` method. If this method has a SQL bug (wrong WHERE clause, missing parameter binding), topic resolution silently fails.

**Impact**: Sessions never get attributed despite correct signal accumulation.

**Test**: Unit test for the store method directly — update and read back.

### SR-4: Backfill Attribution Uses Different Code Path Than SessionClose — MEDIUM

**What**: Migration backfill calls `attribute_sessions()` (batch, content-based). SessionClose calls `majority_vote()` (signal-based) with fallback to `attribute_sessions()`. These are two different code paths producing the same output column. If they produce inconsistent results for the same session, behavior depends on timing.

**Impact**: Non-deterministic attribution results based on whether session was backfilled or closed after upgrade.

**Likelihood**: LOW (both paths use the same underlying extraction functions)

**Test**: Verify that backfill and SessionClose fallback produce the same result for the same observation set.

### SR-5: `record_topic_signal` Timestamp Comparison — Non-Monotonic Clocks — LOW

**What**: FR-05.2 says "updates last_seen if the timestamp is newer". Hook events use Unix seconds. If the system clock goes backward (NTP adjustment, VM migration), a signal with an earlier timestamp could fail to update `last_seen`, affecting tie-breaking.

**Impact**: Tie-breaking uses stale timestamp. Wrong topic in tied cases only.

**Likelihood**: LOW

**Test**: Verify `record_topic_signal` behavior with out-of-order timestamps.

### SR-6: Accumulation Without Observation Persistence Coupling — LOW

**What**: FR-05.3 accumulates signals in SessionState and FR-04.3 persists to observations independently. If observation insert fails (DB error) but signal accumulation succeeds, the session has a topic from signals that aren't backed by persisted observations. Retrospective audit trail is incomplete.

**Impact**: Minor: topic is correct but audit trail has gaps. No functional breakage.

**Test**: Not tested (accepted risk). Document as known divergence.

### SR-7: `build_request()` Input Field Access Patterns — LOW-MEDIUM

**What**: FR-03.2 accesses `input.extra["tool_input"]`, `input.extra["prompt_snippet"]`, and `input.prompt` per event type. These field names come from Claude Code's hook schema. If the field names change upstream, extraction silently returns None — no error, just no signals.

**Impact**: Gradual regression — topic attribution stops working with no error indicator.

**Likelihood**: LOW (Claude Code hook schema changes are rare)

**Test**: Canary test: verify extraction works for each event type with realistic hook payloads.

---

## Risk Severity Ranking

| # | Risk | Severity | Likelihood | Priority |
|---|------|----------|------------|----------|
| 1 | SR-1: Column count mismatch in INSERT | HIGH | HIGH | **P0** |
| 2 | AR-1: Facade priority inversion | MEDIUM | MEDIUM | **P1** |
| 3 | AR-2: Tie-breaking determinism | MEDIUM | MEDIUM | **P1** |
| 4 | R3/AR-3: Migration coordination + backfill safety | MEDIUM | MEDIUM | **P1** |
| 5 | SR-2: generic_record_event over-extraction | MEDIUM | MEDIUM | **P1** |
| 6 | SR-4: Backfill vs SessionClose path divergence | MEDIUM | LOW | **P2** |
| 7 | R4: False-positive attribution | LOW-MEDIUM | MEDIUM | **P2** |
| 8 | SR-3: New store method untested | LOW-MEDIUM | LOW | **P2** |
| 9 | R1: Cross-crate API surface | LOW | HIGH | **P2** |
| 10 | SR-7: Input field name fragility | LOW-MEDIUM | LOW | **P2** |
| 11 | R5: Wire protocol compat | LOW | LOW | **P3** |
| 12 | R2: SessionState memory | LOW | LOW | **P3** |
| 13 | SR-5: Non-monotonic timestamps | LOW | LOW | **P3** |
| 14 | R6: SessionClose race | LOW | LOW | **P3** |
| 15 | R7: Fallback performance | LOW | LOW | **P3** |
| 16 | SR-6: Accumulation/persistence decoupling | LOW | LOW | **P4** |

---

## Test Plan

### P0 Tests — Must Pass Before Merge

#### T-01: extract_topic_signal facade correctness (AC-01–AC-05, AR-1)
- **Type**: Unit
- **Location**: `crates/unimatrix-observe/src/attribution.rs`
- **Cases**:
  - File path input: `"editing product/features/col-002/SCOPE.md"` → `Some("col-002")`
  - Pattern input: `"Working on col-002"` → `Some("col-002")`
  - Git branch: `"git checkout -b feature/col-002"` → `Some("col-002")`
  - No signal: `"regular text"` → `None`
  - **Priority ordering**: input with both path AND pattern → returns path result (AR-1)
  - **Priority ordering**: input with both pattern AND git → returns pattern result (AR-1)

#### T-02: Existing attribution tests unchanged (AC-22)
- **Type**: Unit (existing)
- **Location**: `crates/unimatrix-observe/src/attribution.rs`
- **Verification**: `cargo test -p unimatrix-observe` — all existing tests pass with no modifications

#### T-03: False-positive rejection (R4)
- **Type**: Unit
- **Location**: `crates/unimatrix-observe/src/attribution.rs`
- **Cases**:
  - `"encoding utf-8"` → `None` (rejected by `is_valid_feature_id`)
  - `"architecture x86-64"` → `None`
  - `"hash sha-256"` → `None`
  - `"version v2-1"` → `None`

#### T-04: Observation insert with topic_signal (SR-1, AC-11)
- **Type**: Integration
- **Location**: `crates/unimatrix-server` integration tests
- **Cases**:
  - Insert observation with `topic_signal: Some("col-017")` — query back, assert column value matches
  - Insert observation with `topic_signal: None` — query back, assert NULL
  - Batch insert: mix of Some/None topic_signals — all roundtrip correctly
  - **Verify column count**: SQL has 8 positional parameters matching 8 struct fields

#### T-05: Wire protocol backward compatibility (R5, AC-06, AC-07)
- **Type**: Unit
- **Location**: `crates/unimatrix-engine/src/wire.rs`
- **Cases**:
  - Deserialize JSON without `topic_signal` field → `topic_signal` is `None`
  - Deserialize JSON with `"topic_signal": "col-017"` → `topic_signal` is `Some("col-017")`
  - Deserialize JSON with `"topic_signal": null` → `topic_signal` is `None`
  - Serialize with `topic_signal: None` → field absent or null (either acceptable)

### P1 Tests — Critical Path

#### T-06: SessionState::record_topic_signal (AC-12, R2)
- **Type**: Unit
- **Location**: `crates/unimatrix-server/src/infra/session.rs`
- **Cases**:
  - Record one signal → count=1, last_seen=timestamp
  - Record same signal twice → count=2, last_seen=latest
  - Record two different signals → two entries in map
  - Record 100 signals for same topic → count=100, memory = 1 HashMap entry

#### T-07: majority_vote correctness (AC-13–AC-15, AR-2)
- **Type**: Unit
- **Location**: `crates/unimatrix-server/src/uds/listener.rs` (or `session.rs`)
- **Cases**:
  - Clear winner: `{col-017: 5, col-018: 2}` → `Some("col-017")`
  - Tie broken by recency: `{a: 3, b: 3}`, last_seen `{a: 100, b: 200}` → `Some("b")`
  - **Deterministic tie with same timestamp**: `{a: 3, b: 3}`, last_seen `{a: 100, b: 100}` → lexicographic smallest (AR-2)
  - Single topic: `{col-017: 1}` → `Some("col-017")`
  - Empty map → `None`

#### T-08: build_request extraction per event type (AC-08–AC-10, SR-7)
- **Type**: Unit
- **Location**: `crates/unimatrix-server/src/uds/hook.rs`
- **Cases**:
  - PreToolUse with `tool_input` containing `product/features/col-002/SCOPE.md` → `topic_signal: Some("col-002")`
  - SubagentStart with `prompt_snippet` containing `"implement col-017"` → `topic_signal: Some("col-017")`
  - UserPromptSubmit with prompt containing `"fix the nxs-002 bug"` → `topic_signal: Some("nxs-002")`
  - Event with no feature-identifying content → `topic_signal: None`
  - **Canary**: realistic hook payload shapes from actual Claude Code events (SR-7)

#### T-09: generic_record_event extraction (SR-2)
- **Type**: Unit
- **Location**: `crates/unimatrix-server/src/uds/hook.rs`
- **Cases**:
  - Extra containing `{"tool_input": "read product/features/col-017/SCOPE.md"}` → extracts `col-017`
  - Extra containing `{"url": "https://api-v2.example.com"}` → does NOT extract `api-v2` (false positive guard)
  - Extra containing `{"error": "timeout in step-3"}` → does NOT extract `step-3`

### P2 Tests — Important Coverage

#### T-10: SessionClose end-to-end with signals (AC-16)
- **Type**: Integration
- **Location**: `crates/unimatrix-server` integration tests
- **Flow**:
  1. Register session
  2. Send 5 RecordEvents with `topic_signal: Some("col-017")`
  3. Send 2 RecordEvents with `topic_signal: Some("col-018")`
  4. Send SessionClose
  5. Query `sessions` table → `feature_cycle = "col-017"`

#### T-11: SessionClose fallback — no signals (AC-17, R7)
- **Type**: Integration
- **Location**: `crates/unimatrix-server` integration tests
- **Flow**:
  1. Register session
  2. Send RecordEvents with no topic_signal (None) but with observation content containing feature paths
  3. Send SessionClose
  4. Verify fallback to content-based attribution → `feature_cycle` populated

#### T-12: update_session_feature_cycle store method (SR-3)
- **Type**: Unit/Integration
- **Location**: `crates/unimatrix-store` or server integration tests
- **Cases**:
  - Update existing session → read back, assert feature_cycle matches
  - Update non-existent session → no crash (0 rows affected)

#### T-13: Migration v9→v10 (AC-19, AC-20, AR-3)
- **Type**: Integration
- **Location**: `crates/unimatrix-store/src/migration.rs`
- **Cases**:
  - Run migration: assert `topic_signal` column exists on `observations` table
  - Schema version bumped to 10
  - **Backfill safety (AR-3)**: Create one closed session (ended_at IS NOT NULL, feature_cycle IS NULL) and one active session (ended_at IS NULL, feature_cycle IS NULL). Run migration. Assert closed session gets backfilled. Assert active session is NOT backfilled.

#### T-14: Backfill vs SessionClose consistency (SR-4)
- **Type**: Integration
- **Location**: `crates/unimatrix-server` integration tests
- **Cases**:
  - Create a session with observations containing feature paths
  - Run content-based attribution (backfill path) → record result A
  - Reset feature_cycle to NULL
  - Trigger SessionClose fallback → record result B
  - Assert A == B

### P3 Tests — Edge Cases

#### T-15: Non-monotonic timestamps (SR-5)
- **Type**: Unit
- **Location**: `crates/unimatrix-server/src/infra/session.rs`
- **Cases**:
  - Record signal at t=200, then same signal at t=100 → last_seen stays 200 (if "newer" check) or becomes 100 (if always-update). Document chosen behavior.

#### T-16: Multi-topic session resolution (R4)
- **Type**: Unit
- **Location**: majority_vote tests
- **Cases**:
  - 3 topics with counts {a: 10, b: 8, c: 2} → `Some("a")`
  - Verify with many unique topics (10+) — HashMap handles correctly

#### T-17: Retrospective end-to-end (AC-21)
- **Type**: End-to-end
- **Location**: `crates/unimatrix-server` integration tests
- **Flow**:
  1. Register session, send events with topic signals for "col-017"
  2. SessionClose → feature_cycle populated
  3. Call `context_retrospective` for "col-017"
  4. Assert non-empty results

---

## Coverage Map

| Component | Risks Covered | Tests | Priority |
|-----------|--------------|-------|----------|
| `extract_topic_signal()` facade | R1, R4, AR-1 | T-01, T-02, T-03 | P0 |
| `ObservationRow` + `insert_observation()` | SR-1 | T-04 | P0 |
| `ImplantEvent` serde | R5 | T-05 | P0 |
| `SessionState` accumulation | R2 | T-06 | P1 |
| `majority_vote()` | AR-2 | T-07 | P1 |
| `build_request()` extraction | SR-7 | T-08 | P1 |
| `generic_record_event()` | SR-2 | T-09 | P1 |
| SessionClose dispatch | R6, R7 | T-10, T-11 | P2 |
| `update_session_feature_cycle()` | SR-3 | T-12 | P2 |
| Migration v9→v10 | R3, AR-3 | T-13 | P2 |
| Backfill consistency | SR-4 | T-14 | P2 |
| Timestamp edge cases | SR-5 | T-15 | P3 |
| Multi-topic resolution | R4 | T-16 | P3 |
| Retrospective pipeline | — | T-17 | P3 |

---

## Risks Not Tested (Accepted)

| Risk | Rationale |
|------|-----------|
| SR-6: Accumulation/persistence decoupling | Low impact — topic is correct even if observation insert fails. No functional breakage. Monitoring-only concern. |
| R6: Late-arriving subagent signals | UDS ordering guarantees make this near-impossible. Cost of testing outweighs risk. |
| R7: Fallback performance under extreme load | Content scan is inherently bounded by observation count. No realistic scenario exceeds 100ms. |

---

## Test Infrastructure Notes

- Extend existing `TestDb` and hook test helpers (C-09). No new test scaffolding.
- Wire protocol tests use `serde_json::from_str` / `serde_json::to_string` directly.
- Migration tests follow existing patterns in `migration.rs` test suite.
- SessionClose integration tests use existing UDS test harness.
- All new test functions go in existing test modules — no new test files unless forced by crate boundaries.
