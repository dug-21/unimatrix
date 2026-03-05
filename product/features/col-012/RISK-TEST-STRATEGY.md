# Risk-Based Test Strategy: col-012

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | ImplantEvent payload field extraction produces wrong mapping (tool_name vs agent_type) | High | Med | High |
| R-02 | Schema migration fails on existing v6 database leaving DB in inconsistent state | High | Low | Med |
| R-03 | ObservationRecord from SQL path differs from JSONL path for same event data | High | Med | High |
| R-04 | spawn_blocking write fails silently, observations lost without logging | Med | Med | Med |
| R-05 | SESSIONS.feature_cycle is NULL for sessions registered before col-010 | Med | High | High |
| R-06 | Timestamp conversion overflow: seconds * 1000 exceeds i64 range | Low | Low | Low |
| R-07 | Batch insert (RecordEvents) partially fails, some events persisted others not | Med | Low | Med |
| R-08 | JSONL removal breaks hook scripts that depend on observation directory existence | Med | Low | Low |
| R-09 | ObservationStats schema change breaks context_status JSON response consumers | Med | Med | Med |
| R-10 | Detection rules produce different results when input field is JSON string vs parsed Value | High | Med | High |

## Risk-to-Scenario Mapping

### R-01: ImplantEvent Payload Field Extraction

**Severity**: High
**Likelihood**: Medium
**Impact**: Wrong tool names in observations -> detection rules misfire or miss patterns

**Test Scenarios**:
1. Send PreToolUse event with `payload.tool_name = "Read"` -> verify `tool` column = "Read"
2. Send PostToolUse event with `payload.tool_name = "Edit"`, `payload.response_size = 1024` -> verify all fields
3. Send SubagentStart event with `payload.agent_type = "uni-pseudocode"`, `payload.prompt_snippet = "Design..."` -> verify tool = "uni-pseudocode", input = prompt text
4. Send SubagentStop event -> verify tool = NULL, input = NULL
5. Send event with missing `payload.tool_name` -> verify tool = NULL (not crash)

**Coverage Requirement**: Every hook type with all field combinations tested.

### R-02: Schema Migration Failure

**Severity**: High
**Likelihood**: Low
**Impact**: Database unusable, server fails to start

**Test Scenarios**:
1. Open v6 database -> verify migration to v7 creates observations table and indexes
2. Open v7 database -> verify no-op migration (idempotent)
3. Open fresh database -> verify observations table exists via create_tables()
4. Verify CURRENT_SCHEMA_VERSION = 7 in counters table after migration

**Coverage Requirement**: Migration tested against v6 database artifact.

### R-03: SQL-to-ObservationRecord Mapping Fidelity

**Severity**: High
**Likelihood**: Medium
**Impact**: Detection rules receive different data shape, produce different (wrong) findings

**Test Scenarios**:
1. Insert observation row with all fields populated -> map to ObservationRecord -> verify field equality
2. Insert observation row with NULL optional fields -> verify None in ObservationRecord
3. Insert SubagentStart row -> verify input is `Value::String` (not `Value::Object`)
4. Insert row with JSON object in input column -> verify deserialized to `Value::Object`
5. Verify ts_millis maps to ObservationRecord.ts without transformation (both millis)

**Coverage Requirement**: Round-trip test: write event via RecordEvent, read via ObservationSource, compare fields.

### R-04: spawn_blocking Write Failure

**Severity**: Medium
**Likelihood**: Medium
**Impact**: Observations silently lost

**Test Scenarios**:
1. Normal write succeeds and row appears in table
2. Verify write failure is logged at error level (tracing)
3. Verify UDS response is Ack regardless of write success/failure

**Coverage Requirement**: At least one error path test (e.g., concurrent exclusive lock simulated).

### R-05: NULL feature_cycle in SESSIONS

**Severity**: Medium
**Likelihood**: High (existing sessions from before col-010)
**Impact**: Observations from pre-col-010 sessions not returned for any feature

**Test Scenarios**:
1. Session with feature_cycle = "col-012" -> observations returned for "col-012"
2. Session with feature_cycle = NULL -> observations excluded from all feature queries
3. Multiple sessions, mixed NULL and set feature_cycle -> only matching sessions included
4. Empty result set (no matching sessions) -> empty Vec returned, no error

**Coverage Requirement**: Explicit test for NULL feature_cycle behavior.

### R-06: Timestamp Conversion Overflow

**Severity**: Low
**Likelihood**: Low
**Impact**: Negative or wrapped timestamps in observations table

**Test Scenarios**:
1. Normal timestamp (2024 epoch seconds) -> correct millis
2. Timestamp = 0 -> ts_millis = 0
3. Large timestamp (year 3000) -> verify no overflow in i64 range

**Coverage Requirement**: Boundary test for timestamp conversion.

### R-07: Batch Insert Partial Failure

**Severity**: Medium
**Likelihood**: Low
**Impact**: Some events persisted, others not -> inconsistent observation data

**Test Scenarios**:
1. Batch of 5 valid events -> all 5 inserted
2. Batch in single transaction -> if one fails, none are committed (atomicity)

**Coverage Requirement**: Transaction atomicity test for batch path.

### R-08: Hook Script Breakage After JSONL Removal

**Severity**: Medium
**Likelihood**: Low
**Impact**: Hooks fail to forward events to UDS

**Test Scenarios**:
1. Hook scripts run without errors after JSONL write lines removed
2. `unimatrix-server hook` receives event data correctly from modified scripts
3. No reference to observation directory in hook scripts

**Coverage Requirement**: Manual script review + hook CLI integration test.

### R-09: ObservationStats Schema Change

**Severity**: Medium
**Likelihood**: Medium
**Impact**: context_status consumers (agents, human) see unexpected field names

**Test Scenarios**:
1. context_status JSON includes `observation_record_count` instead of `observation_file_count`
2. `total_size_bytes` field handling (removed or zero-valued)
3. Backward compatibility: consumers that ignore unknown fields continue working

**Coverage Requirement**: context_status integration test with updated response assertions.

### R-10: Input Field Type Mismatch in Detection Rules

**Severity**: High
**Likelihood**: Medium
**Impact**: Detection rules that parse input field fail or miss patterns

**Test Scenarios**:
1. Input stored as JSON string `"{\"file_path\":\"/tmp/test\"}"` -> deserialized to Value::Object
2. Input stored as plain string (SubagentStart prompt) -> deserialized to Value::String
3. Detection rules that call `input.get("command")` work on SQL-sourced data
4. Detection rules that match tool input patterns produce same findings as JSONL-sourced data

**Coverage Requirement**: At least 3 representative detection rules tested with SQL-sourced data.

## Integration Risks

- **SESSIONS <-> observations join correctness**: If session_id values differ between tables (e.g., truncation, encoding), the join produces empty results. Must verify session_id is stored identically in both tables.
- **ObservationSource trait boundary**: If the trait API is insufficient for a specific retrospective feature, the trait must be extended (breaking all implementors). Keep the initial trait minimal and verify against all current usage.
- **context_retrospective code path branches**: The current implementation has both JSONL and structured-events code paths. After migration, ensure no conditional branch falls back to the removed JSONL path.

## Edge Cases

- Empty observations table -> retrospective returns valid report with zero hotspots
- Session with zero observations -> session appears in discover_sessions but load returns empty vec
- Very long input strings (>10KB tool input JSON) -> stored and retrieved without truncation
- Concurrent RecordEvent writes from multiple hooks in same session -> no SQLITE_BUSY (WAL mode)
- Observations for a feature with 50+ sessions -> query performance within NFR-04 (500ms)

## Security Risks

- **Input field injection**: The `input` column stores arbitrary JSON from hook stdin. This is agent-provided data, not user-controlled. However, if the input contains SQL-like strings, parameterized queries prevent injection. All SQL uses `rusqlite::params![]` (parameterized). Risk: Low.
- **Response snippet content**: May contain sensitive file contents (first 500 chars of tool output). Same exposure as current JSONL path. No change in blast radius.
- **Observation retention**: 60-day retention limits data exposure window. Same as current JSONL policy.

## Failure Modes

- **Migration failure**: Server fails to start. Error propagated via Store::open() -> main. User sees startup error. Recovery: manually delete DB and restart (fresh DB at v7).
- **Write failure (spawn_blocking)**: Event silently lost. Logged at error level. Retrospective data incomplete but not incorrect.
- **Query failure (ObservationSource)**: context_retrospective returns error to caller. No silent degradation.
- **Hook script failure**: Hook exits 0 (FR-03.7). Claude Code session unaffected. Event lost.

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (schema migration corruption) | R-02 | Migration is idempotent CREATE TABLE IF NOT EXISTS; no data transformation needed |
| SR-02 (WAL write contention) | R-04 | spawn_blocking + WAL mode + busy_timeout=5000ms handles concurrency |
| SR-03 (unbounded table growth) | FR-07 | 60-day retention cleanup in context_status maintain path |
| SR-04 (ADR-001 trait abstraction) | ADR-002 | Minimal 3-method trait; dependency inversion preserves independence |
| SR-05 (same results verification) | R-03, R-10 | Round-trip tests + detection rule integration tests |
| SR-06 (silent event loss) | ADR-003 | Accepted; EventQueue provides retry for transient failures |
| SR-07 (dual code paths) | Integration risk | Audit all retrospective entry points; remove JSONL fallback |
| SR-08 (status response fields) | R-09 | ObservationStats revised; integration test validates |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 3 (R-01, R-03, R-10) | 14 scenarios |
| Medium | 4 (R-04, R-05, R-07, R-09) | 10 scenarios |
| Low | 3 (R-02, R-06, R-08) | 7 scenarios |
| **Total** | **10** | **31 scenarios** |
