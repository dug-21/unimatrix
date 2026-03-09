# col-017: Hook-Side Topic Attribution — Acceptance Map

## Legend

- **Component**: Implementation component from brief
- **FR**: Functional requirement from specification
- **AC**: Acceptance criterion from specification
- **Test**: Test case from risk-test strategy
- **Priority**: P0 = must pass before merge, P1 = critical path, P2 = important, P3 = edge cases

---

## C1: Extraction Facade (`unimatrix-observe/src/attribution.rs`)

| AC | Description | FR | Test | Priority | Verification |
|----|-------------|-----|------|----------|--------------|
| AC-01 | `extract_topic_signal()` returns correct result for file path input | FR-01.1 | T-01 | P0 | Unit: `"editing product/features/col-002/SCOPE.md"` → `Some("col-002")` |
| AC-02 | `extract_topic_signal()` returns correct result for feature ID pattern | FR-01.1 | T-01 | P0 | Unit: `"Working on col-002 design"` → `Some("col-002")` |
| AC-03 | `extract_topic_signal()` returns correct result for git branch | FR-01.1 | T-01 | P0 | Unit: `"git checkout -b feature/col-002"` → `Some("col-002")` |
| AC-04 | `extract_topic_signal()` respects priority order (path > pattern > git) | FR-01.1 | T-01 | P0 | Unit: input with both path AND pattern → path result (AR-1) |
| AC-05 | `extract_topic_signal()` returns None for no-signal input | FR-01.1 | T-01 | P0 | Unit: `"regular text with no features"` → `None` |
| AC-22 | Existing attribution unit tests pass unchanged | FR-01.3 | T-02 | P0 | `cargo test -p unimatrix-observe` — zero modifications to existing tests |
| — | False-positive rejection | R4 | T-03 | P0 | Unit: `"utf-8"`, `"x86-64"`, `"sha-256"` → `None` |

## C2: Wire Protocol Extension (`unimatrix-engine/src/wire.rs`)

| AC | Description | FR | Test | Priority | Verification |
|----|-------------|-----|------|----------|--------------|
| AC-06 | `ImplantEvent` deserializes without `topic_signal` → None | FR-02.2 | T-05 | P0 | Unit: JSON without field → `topic_signal` is `None` |
| AC-07 | `ImplantEvent` deserializes with `topic_signal` → Some | FR-02.3 | T-05 | P0 | Unit: JSON with `"topic_signal": "col-017"` → `Some("col-017")` |
| — | Deserialize with `null` → None | FR-02.2 | T-05 | P0 | Unit: `"topic_signal": null` → `None` |
| — | Serialize with None omits field | FR-02.4 | T-05 | P0 | Unit: `skip_serializing_if` verified |

## C3: Hook-Side Extraction (`unimatrix-server/src/uds/hook.rs`)

| AC | Description | FR | Test | Priority | Verification |
|----|-------------|-----|------|----------|--------------|
| AC-08 | `build_request()` extracts topic signal for PreToolUse with file path | FR-03.1, FR-03.2 | T-08 | P1 | Unit: PreToolUse with `tool_input` containing feature path → `Some` |
| AC-09 | `build_request()` extracts topic signal for SubagentStart with feature ID | FR-03.2 | T-08 | P1 | Unit: SubagentStart with `prompt_snippet` containing feature ID → `Some` |
| AC-10 | `build_request()` sets topic_signal None when no signal in input | FR-03.1 | T-08 | P1 | Unit: generic content → `None` |
| — | `generic_record_event()` extracts from stringified extra | FR-03.3 | T-09 | P1 | Unit: extra with feature path → `Some` |
| — | `generic_record_event()` rejects false-positive patterns | SR-2 | T-09 | P1 | Unit: extra with `"api-v2"` URL → `None` |
| — | Realistic hook payload canary | SR-7 | T-08 | P1 | Unit: actual Claude Code event shapes |

## C4: Session Accumulation (`unimatrix-server/src/infra/session.rs`)

| AC | Description | FR | Test | Priority | Verification |
|----|-------------|-----|------|----------|--------------|
| AC-12 | `record_topic_signal` increments count and updates last_seen | FR-05.2 | T-06 | P1 | Unit: record same signal twice → count=2, last_seen=latest |
| — | Record two different signals → two map entries | FR-05.2 | T-06 | P1 | Unit: two signals, two entries |
| — | Memory bounded: 100 signals same topic → 1 entry | R2 | T-06 | P1 | Unit: verify HashMap size |
| — | Non-monotonic timestamp handling | SR-5 | T-15 | P3 | Unit: record at t=200 then t=100, document `last_seen` behavior |

## C5: Observation Persistence (`unimatrix-server/src/uds/listener.rs`)

| AC | Description | FR | Test | Priority | Verification |
|----|-------------|-----|------|----------|--------------|
| AC-11 | `ObservationRow` includes topic_signal, persisted to DB | FR-04.1–FR-04.3 | T-04 | P0 | Integration: insert with `Some("col-017")`, query back, assert match |
| — | Insert with topic_signal None → NULL in DB | FR-04.3 | T-04 | P0 | Integration: insert with None, query back, assert NULL |
| — | Batch insert with mixed Some/None | FR-04.4 | T-04 | P0 | Integration: batch roundtrip |
| — | SQL parameter count = struct field count = 8 | SR-1 | T-04 | P0 | Integration: verify no runtime binding error |

## C6: SessionClose Resolution (`unimatrix-server/src/uds/listener.rs`)

| AC | Description | FR | Test | Priority | Verification |
|----|-------------|-----|------|----------|--------------|
| AC-13 | `majority_vote` returns clear winner | FR-06.1 | T-07 | P1 | Unit: `{col-017: 5, col-018: 2}` → `Some("col-017")` |
| AC-14 | `majority_vote` breaks tie by recency | FR-06.1 | T-07 | P1 | Unit: `{a: 3, b: 3}`, last_seen `{a: 100, b: 200}` → `Some("b")` |
| AC-15 | `majority_vote` returns None for empty | FR-06.1 | T-07 | P1 | Unit: empty map → `None` |
| — | Deterministic tie (same timestamp) → lexicographic | AR-2 | T-07 | P1 | Unit: same count + same timestamp → smallest string |
| AC-16 | SessionClose resolves + persists feature_cycle | FR-06.2–FR-06.3 | T-10 | P2 | Integration: register, send events with signals, SessionClose, query sessions |
| AC-17 | SessionClose falls back to content-based attribution | FR-06.2 | T-11 | P2 | Integration: events without signals, SessionClose, feature_cycle populated |
| AC-18 | Content-based attribution results persisted | FR-07.1–FR-07.2 | T-11 | P2 | Integration: retrospective attribution → sessions table updated |
| — | `update_session_feature_cycle` roundtrip | SR-3 | T-12 | P2 | Unit: update + read back |
| — | Backfill vs SessionClose consistency | SR-4 | T-14 | P2 | Integration: same observations → same result from both paths |
| — | Multi-topic session (3+ topics) | R4 | T-16 | P3 | Unit: 3 topics → highest count wins |

## C7: Schema Migration v9→v10 (`unimatrix-store/src/migration.rs`)

| AC | Description | FR | Test | Priority | Verification |
|----|-------------|-----|------|----------|--------------|
| AC-19 | Migration adds topic_signal column to observations | FR-08.1–FR-08.2 | T-13 | P2 | Migration test: run v9→v10, assert column exists |
| AC-20 | Migration backfills feature_cycle for closed sessions | FR-08.3 | T-13 | P2 | Migration test: closed session backfilled, active session untouched |
| — | Schema version bumped to 10 | FR-08.4 | T-13 | P2 | Migration test: query schema_version |
| — | Backfill only touches closed sessions (AR-3) | FR-08.3 | T-13 | P2 | Migration test: active session `feature_cycle` remains NULL |

## End-to-End

| AC | Description | FR | Test | Priority | Verification |
|----|-------------|-----|------|----------|--------------|
| AC-21 | `context_retrospective` returns non-empty for attributed feature | FR-06, FR-07 | T-17 | P3 | E2E: full pipeline → retrospective query returns results |

---

## Summary

| Priority | Count | Description |
|----------|-------|-------------|
| P0 | 15 | Must pass before merge — facade correctness, wire compat, observation persistence, existing tests |
| P1 | 13 | Critical path — hook extraction, accumulation, majority vote, event type coverage |
| P2 | 9 | Important — SessionClose integration, migration, store method, consistency |
| P3 | 3 | Edge cases — timestamps, multi-topic, retrospective E2E |
| **Total** | **40** | |

## Risks Not Tested (Accepted)

| Risk | Rationale |
|------|-----------|
| SR-6: Accumulation/persistence decoupling | Low impact — topic correct even if observation insert fails |
| R6: Late-arriving subagent signals | UDS ordering guarantees; cost of testing outweighs risk |
| R7: Fallback perf under extreme load | Content scan bounded by observation count; < 100ms typical |
