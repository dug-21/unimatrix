# Risk-Based Test Strategy: col-002

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | JSONL parsing silently drops records due to malformed lines | High | Med | High |
| R-02 | Feature attribution misattributes records when sessions work on multiple features | High | Med | High |
| R-03 | Timestamp parsing produces incorrect epoch values for edge-case dates | Med | Med | Medium |
| R-04 | MetricVector bincode serialization breaks on field addition (col-002b) | High | Low | Medium |
| R-05 | DetectionRule trait does not accommodate future rule categories | Med | Low | Medium |
| R-06 | OBSERVATION_METRICS table addition causes regression in Store::open | Med | Low | Medium |
| R-07 | Hook scripts fail silently, producing no telemetry without user awareness | Med | Med | Medium |
| R-08 | File cleanup deletes files that are still being written to by active sessions | High | Low | Medium |
| R-09 | context_retrospective returns inconsistent results when called concurrently | Med | Low | Medium |
| R-10 | Permission retries detection produces false positives from legitimate tool rejections | Low | Med | Low |
| R-11 | Phase name extraction from task subjects fails on non-standard naming | Low | Med | Low |
| R-12 | Observation directory permissions prevent hook scripts from writing | Med | Med | Medium |
| R-13 | Large session files (100K+ records) cause excessive memory usage during analysis | Med | Low | Medium |
| R-14 | StatusReport struct extension breaks existing test compilation | Low | High | Low |

## Risk-to-Scenario Mapping

### R-01: JSONL Parsing Silently Drops Records
**Severity**: High
**Likelihood**: Med
**Impact**: Missing records cause incomplete analysis. Hotspot detection may miss patterns. Metric values are systematically understated.

**Test Scenarios**:
1. Parse a file with 50% malformed lines -- verify valid lines are parsed correctly and count matches
2. Parse a file with trailing garbage after valid JSON -- verify the valid portion is parsed
3. Parse an empty file -- verify empty result, no error
4. Parse a file with only malformed lines -- verify empty result, no error

**Coverage Requirement**: Unit tests in `unimatrix-observe::parser` module. Verify both successful parse count and that malformed lines produce skip (not error).

### R-02: Feature Attribution Misattributes Records
**Severity**: High
**Likelihood**: Med
**Impact**: Wrong records in a feature's retrospective. Hotspots from feature A appear in feature B's report. Metrics are meaningless if attribution is wrong.

**Test Scenarios**:
1. Single-feature session -- all records attributed to that feature
2. Two-feature session with clear switch point -- verify partition at switch
3. Session with no feature signals -- verify excluded from all retrospectives
4. Session where feature ID appears in file path vs. task subject vs. git command -- verify all signal types work
5. Records before first feature ID -- attributed to first feature found
6. Three sessions, two with feature A and one with feature B -- verify correct session selection for feature A

**Coverage Requirement**: Unit tests in `unimatrix-observe::attribution` module. Integration test with synthetic multi-session multi-feature data.

### R-03: Timestamp Parsing Produces Incorrect Epoch Values
**Severity**: Med
**Likelihood**: Med
**Impact**: Records may sort incorrectly. Session timeout detection (>2 hour gaps) may fire spuriously or miss real gaps. Duration metrics are wrong.

**Test Scenarios**:
1. Parse timestamps at epoch boundaries: `1970-01-01T00:00:00Z`, `2038-01-19T03:14:07Z`
2. Parse a leap year date: `2024-02-29T12:00:00Z`
3. Parse midnight and end-of-day: `2024-01-01T00:00:00Z`, `2024-12-31T23:59:59Z`
4. Reject invalid format: `2024/01/01 12:00:00` (slash separator, missing T/Z)
5. Reject non-UTC timezone: `2024-01-01T12:00:00+05:00` if not supported

**Coverage Requirement**: Unit tests in `unimatrix-observe::parser` with known epoch values verified against a reference.

### R-04: MetricVector Bincode Serialization Breaks on Field Addition
**Severity**: High
**Likelihood**: Low
**Impact**: Stored MetricVectors from col-002 cannot be deserialized after col-002b adds fields. Data loss or crash on retrieval.

**Test Scenarios**:
1. Serialize MetricVector, deserialize, verify roundtrip
2. Serialize with all default values, deserialize, verify
3. Serialize with populated phase map, deserialize, verify phase names and values preserved
4. Verify `#[serde(default)]` annotations on all MetricVector fields
5. Simulate forward compatibility: serialize a subset of fields, deserialize with full struct (using bincode's behavior with serde)

**Coverage Requirement**: Unit tests in `unimatrix-observe::types`. Explicit test for the serde(default) contract.

### R-05: DetectionRule Trait Does Not Accommodate Future Rules
**Severity**: Med
**Likelihood**: Low
**Impact**: col-002b must redesign the framework instead of just adding rules. Rework cost.

**Test Scenarios**:
1. Implement a test rule that needs per-tool aggregation (like permission retries)
2. Implement a test rule that needs timestamp analysis (like session timeout)
3. Implement a test rule that needs regex matching (like sleep workarounds)
4. Verify all three patterns work through the same `DetectionRule` trait interface
5. Verify the engine collects findings from all rules without special-casing

**Coverage Requirement**: The 3 shipped rules themselves validate the trait. Add one additional mock rule in tests that returns a different category to verify extensibility.

### R-06: OBSERVATION_METRICS Table Addition Causes Regression
**Severity**: Med
**Likelihood**: Low
**Impact**: Store::open fails. All existing functionality broken.

**Test Scenarios**:
1. Open a fresh database -- verify 14 tables exist
2. Open an existing database (without OBSERVATION_METRICS) -- verify table is created
3. Verify all existing table tests still pass
4. Write and read back a metric entry

**Coverage Requirement**: Unit test in `unimatrix-store::db` (extend existing `test_open_creates_all_tables`). CRUD test for the new table.

### R-07: Hook Scripts Fail Silently
**Severity**: Med
**Likelihood**: Med
**Impact**: No telemetry collected. Retrospective returns "no data" with no indication of why.

**Test Scenarios**:
1. Pipe valid JSON to each hook script -- verify JSONL file created with correct content
2. Pipe invalid JSON -- verify script exits 0 (no JSONL line written, but no crash)
3. Pipe JSON with missing session_id -- verify graceful handling
4. Run script when observation directory does not exist -- verify directory created

**Coverage Requirement**: Shell integration tests. These test the collection layer independently of the Rust analysis.

### R-08: File Cleanup Deletes Active Session Files
**Severity**: High
**Likelihood**: Low
**Impact**: Data loss during active session. In-progress observation records are destroyed.

**Test Scenarios**:
1. Create files with age exactly at 60-day threshold -- verify deleted
2. Create files at 59 days -- verify NOT deleted
3. Create files at 61 days -- verify deleted
4. Verify cleanup uses file modified time (not creation time)

**Coverage Requirement**: Unit test in `unimatrix-observe::files` with mock filesystem timestamps. Active sessions have recent modified times so this risk is theoretical but must be tested.

### R-09: Concurrent Retrospective Calls
**Severity**: Med
**Likelihood**: Low
**Impact**: Race condition on MetricVector store/retrieve. Duplicate analysis work.

**Test Scenarios**:
1. Call context_retrospective from MCP -- verify single-threaded execution within the tool handler (rmcp handles one tool call at a time per server)
2. Two sequential calls to the same feature -- second returns cached result

**Coverage Requirement**: Integration test verifying sequential behavior. Document that rmcp's tool dispatch is single-threaded (no concurrent tool execution).

### R-10: Permission Retries False Positives
**Severity**: Low
**Likelihood**: Med
**Impact**: Noisy hotspot report. LLM discusses non-issues.

**Test Scenarios**:
1. Tool with 3 PreToolUse and 3 PostToolUse -- no finding (0 retries)
2. Tool with 5 PreToolUse and 2 PostToolUse -- finding (3 retries)
3. Multiple tools, only one exceeds threshold -- finding only for the offending tool

**Coverage Requirement**: Unit test in detection rule module.

### R-11: Phase Name Extraction Failure
**Severity**: Low
**Likelihood**: Med
**Impact**: Phase metrics have wrong keys. Baselines accumulate under wrong names.

**Test Scenarios**:
1. Standard format: `"3a: Pseudocode"` -> phase `"3a"`
2. No colon: `"Implementation work"` -> no phase extracted (uncategorized)
3. Multiple colons: `"3b: Code: implement parser"` -> phase `"3b"`
4. Empty prefix: `": Just a description"` -> empty string phase (or uncategorized)

**Coverage Requirement**: Unit test for phase extraction function.

### R-12: Observation Directory Permissions
**Severity**: Med
**Likelihood**: Med
**Impact**: Hook scripts cannot write. Silent collection failure.

**Test Scenarios**:
1. Create observation directory with 755 -- hooks write successfully
2. Missing parent directory (`~/.unimatrix/` does not exist) -- hook creates it

**Coverage Requirement**: Shell integration test.

### R-13: Large Session Files Cause Memory Issues
**Severity**: Med
**Likelihood**: Low
**Impact**: Analysis OOM or takes minutes for a single feature.

**Test Scenarios**:
1. Generate a 10,000-record JSONL file -- verify parsing completes in <2 seconds
2. Verify records are parsed line-by-line (not loaded as entire string first)

**Coverage Requirement**: Unit test with generated large input. Verify streaming parse pattern in code review.

### R-14: StatusReport Extension Breaks Existing Tests
**Severity**: Low
**Likelihood**: High
**Impact**: Existing tests fail to compile. Known churn, not a bug.

**Test Scenarios**:
1. All existing StatusReport construction sites updated with new fields
2. New fields default to 0 / empty in existing test helpers

**Coverage Requirement**: Compile-time verification. Update `make_status_report` helpers in test modules.

## Integration Risks

- **Store + Observe boundary**: MetricVector bytes flow from observe crate -> server -> store. If bincode config differs between serialize (observe) and the stored bytes, deserialization fails. Mitigation: both use `bincode::config::standard()` (workspace convention).
- **Server + Hook convention**: Hook scripts and the observe crate parser must agree on the JSONL record schema. If hooks add or rename fields, parsing breaks silently. Mitigation: define the record schema as a single reference (SPECIFICATION.md FR-01.6) and test hook output against parser.
- **Status format + StatusReport**: Adding 5 new fields to StatusReport changes all three format outputs (summary, markdown, json). All existing format tests must be updated. Mitigation: add fields with meaningful defaults, update test helpers.

## Edge Cases

- Empty observation directory (no files) -- graceful "no data" response
- Session file with 0 valid records (all malformed) -- treated as empty session
- Feature cycle string that never appears in any session -- "no data" error
- Session file being actively written to during analysis -- partial read is acceptable (JSONL is append-only, incomplete last line is skipped)
- Timestamp ordering: records within a session may not be perfectly ordered if hook execution is async -- sort by timestamp before attribution
- Multiple sessions with same session_id (should not happen per Claude Code design, but defensive handling: merge records)

## Security Risks

- **File path traversal in observation records**: Tool inputs may contain arbitrary file paths. The observe crate only reads these as strings for attribution pattern matching -- it does not open or follow them. No path traversal risk in analysis. Hook scripts write to a fixed directory with session_id as filename -- session_id is sanitized (alphanumeric + dash).
- **JSONL injection**: Malformed observation records could contain control characters or very long strings. The parser reads line-by-line with serde_json -- standard JSON parsing handles escaping. Response snippet truncation (500 chars in hooks) bounds per-record size.
- **Observation file disclosure**: Session files contain tool call details (file paths, command inputs). Files are stored in `~/.unimatrix/observation/` with user-owned permissions. No network exposure. 60-day cleanup limits retention.
- **Session ID validation in hooks**: The session_id from hook input is used as a filename. If it contains path separators or special characters, files could be written outside the observation directory. Mitigation: sanitize session_id in hook scripts (strip non-alphanumeric-dash characters).

## Failure Modes

- **Hook script error**: Script exits 0 regardless. No telemetry for that event. Analysis proceeds with whatever data was collected. No user notification of collection failure.
- **Parse error on entire file**: File is skipped. Other files are processed. Report includes data from successful parses.
- **Store write failure for MetricVector**: ServerError returned to caller. Analysis results are lost (not persisted). Caller can retry.
- **Observation directory missing**: Analysis returns "no observation data" error. context_status reports 0 files. No crash.
- **Disk full during hook write**: Partial JSONL line written. Next parse skips the malformed trailing line. No data corruption.

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (timestamp parsing edge cases) | R-03 | Constrained to single canonical format (`YYYY-MM-DDTHH:MM:SSZ`). Parser validates format strictly. Tested with boundary dates. |
| SR-02 (unbounded JSONL file growth) | R-13 | Response snippet truncated to 500 chars in hooks (FR-01.5). Line-by-line parsing avoids loading entire file. 60-day cleanup caps retention. |
| SR-03 (observe crate coupling) | R-04, R-05 | ADR-001 enforces crate independence. MetricVector serialization owned by observe crate (ADR-002). No shared types between observe and store. |
| SR-04 (MetricVector extensibility) | R-04 | `#[serde(default)]` on all fields. Roundtrip test validates. Forward-compatible by design. |
| SR-05 (trait extensibility for col-002b) | R-05 | Trait designed with all 21 rules in view. Three shipped rules cover three different detection patterns. Mock rule test validates extensibility. |
| SR-06 (hook testing gap) | R-07 | Shell integration tests pipe synthetic JSON and verify JSONL output. Tests exercise each script independently. |
| SR-07 (table addition regression) | R-06 | Follows OUTCOME_INDEX precedent. Extend existing table count test. |
| SR-08 (StatusReport test churn) | R-14 | Known churn. Update test helpers with default values for new fields. |
| SR-09 (attribution accuracy) | R-02 | Three signal types with priority order. Exhaustive unit tests for all attribution edge cases (multi-feature, no-feature, pre-feature records). |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 3 (R-01, R-02, R-08) | 14 scenarios |
| Medium | 7 (R-03, R-04, R-05, R-06, R-07, R-12, R-13) | 22 scenarios |
| Low | 4 (R-09, R-10, R-11, R-14) | 9 scenarios |
