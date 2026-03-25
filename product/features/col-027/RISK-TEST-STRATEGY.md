# Risk-Based Test Strategy: col-027 — PostToolUseFailure Hook Support

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `extract_error_field()` absent or miscalled: `PostToolUseFailure` arm calls `extract_response_fields()` instead, silently returning `(None, None)` — error content lost with no test failure signal | High | Med | Critical |
| R-02 | Partial two-site differential fix: `friction.rs` and `metrics.rs` updated in separate commits — metric and rule diverge, reporting contradictory signals from identical data | High | Med | Critical |
| R-03 | `extract_observation_fields()` wildcard fall-through: `"PostToolUseFailure"` arm missing or mis-spelled — records stored with `tool = None`, making them invisible to all per-tool rules | High | Low | High |
| R-04 | `PermissionRetriesRule` still fires for fully-terminal failure sessions: `PostToolUseFailure` records not counted in terminal bucket — false friction findings continue after col-027 | High | Low | High |
| R-05 | `build_request()` wildcard routing: explicit arm absent — `tool_name` not extracted, `event_type` stored as whatever the wildcard produces, ToolFailureRule never fires | Med | Low | Med |
| R-06 | `ToolFailureRule` threshold boundary error: fires at count == 3 (should be > 3) or not at count == 4 — AC-08 and AC-09 produce inverse results | Med | Med | Med |
| R-07 | `ToolFailureRule` missing `source_domain == "claude-code"` guard — records from non-claude-code domains inflate failure counts or cause spurious findings | Med | Low | Med |
| R-08 | Hook exit code non-zero on malformed/empty failure payload — violates FR-03.7, breaks all future hook invocations for that Claude Code session | Med | Low | Med |
| R-09 | `extract_event_topic_signal()` falls through to generic stringify for `PostToolUseFailure` — `topic_signal` populated with raw JSON blob instead of clean tool_input content | Low | Med | Low |
| R-10 | `response_size` set to non-None for failure records — consumers of `total_context_loaded_kb` that filter on `POSTTOOLUSE` accidentally widen to include failure records if filter removed in future | Low | Low | Low |
| R-11 | `hook_type::POSTTOOLUSEFAILURE` constant value misspelled or mismatched — all string comparisons in rules silently miss every failure record | Med | Low | Med |
| R-12 | `permission_friction_events` underflow: if failure count arithmetic uses signed subtraction rather than `saturating_sub`, large failure counts could produce negative metric values | Low | Low | Low |
| R-13 | `ToolFailureRule` not registered in `default_rules()` in `detection/mod.rs` — rule exists but never executes during retrospectives | Med | Low | Med |
| R-14 | Settings.json `PostToolUseFailure` registration uses wrong command pattern or wrong event key casing — hook binary never invoked by Claude Code on tool failure | High | Low | High |

---

## Risk-to-Scenario Mapping

### R-01: Silent error content loss from wrong extractor call

**Severity**: High
**Likelihood**: Med
**Impact**: Every `PostToolUseFailure` record stored with `response_snippet = None`. `ToolFailureRule` evidence records carry no diagnostic detail. AC-03 fails silently only if tested with a fixture that also tests the `hook` value — without asserting `response_snippet.is_some()` the loss is invisible.

Historical context: Entry #699 documents a near-identical pipeline break ("hardcoded None in hook pipeline breaks entire feedback loop"). Entry #3474 (ADR-002 col-027) was written specifically to mitigate this.

**Test Scenarios**:
1. Unit test `extract_observation_fields()` with a payload `{"tool_name": "Bash", "error": "permission denied", "tool_input": {}}` — assert `obs.response_snippet == Some("permission denied")`.
2. Unit test with a payload where `"error"` is absent — assert `obs.response_snippet == None` and no panic.
3. Unit test with a payload where `"error"` is a 600-char string — assert `obs.response_snippet` is truncated to 500 chars at a valid UTF-8 boundary.
4. Negative: confirm calling `extract_response_fields()` on a failure payload returns `(None, None)` — demonstrates the guard value of the separate extractor.

**Coverage Requirement**: `extract_error_field()` must have its own tests independent of `extract_observation_fields()`. AC-03 test must assert both `obs.hook == "PostToolUseFailure"` and `obs.response_snippet.is_some()` in the same assertion block.

---

### R-02: Divergent Pre-Post differential fix across two sites

**Severity**: High
**Likelihood**: Med
**Impact**: `permission_friction_events` metric shows 0 for a session while `PermissionRetriesRule` fires a finding for the same session (or vice versa). Retrospective consumers see contradictory signals. Downstream metric trend analysis across features produces erroneous results. Entry #3472 identifies this as a documented pattern risk.

**Test Scenarios**:
1. Construct records: 4 Pre + 2 Post + 2 Failure for the same tool. Assert `compute_universal()` returns `permission_friction_events == 0` AND `PermissionRetriesRule::detect()` returns empty findings — both in the same test.
2. Construct records: 5 Pre + 2 Post + 1 Failure. Assert `permission_friction_events == 2` AND `PermissionRetriesRule` fires with `retries == 2` (at-threshold, no finding). Both assertions in same test function.
3. Construct records: 5 Pre + 0 Post + 0 Failure. Assert both sites agree: metric == 5, rule fires with `retries == 5`.

**Coverage Requirement**: At least one test must exercise both `compute_universal()` and `PermissionRetriesRule::detect()` on the same observation set in the same test function — not in separate files. ADR-004 requires the commit to include an integration test for the coupled fix.

---

### R-03: Wildcard fall-through in `extract_observation_fields()` stores tool = None

**Severity**: High
**Likelihood**: Low
**Impact**: `PostToolUseFailure` records stored with `tool = None`. `ToolFailureRule` iterates `record.tool.as_ref()` — None records are skipped entirely. Rule never fires regardless of actual failure count. The Pre-Post differential also misses them because tool-keyed counting requires a non-None tool. The entire feature becomes a no-op for per-tool detection.

Historical context: SR-07 from SCOPE-RISK-ASSESSMENT.md; ADR-001 col-027 explicitly calls out the wildcard arm risk.

**Test Scenarios**:
1. Unit test `extract_observation_fields("PostToolUseFailure", payload)` — assert `obs.tool.is_some()` and equals the tool name from the payload.
2. Unit test the full pipeline: `build_request()` → `extract_observation_fields()` — assert end-to-end `tool` propagation.
3. Confirm `obs.hook == "PostToolUseFailure"` (not `""` or `"PostToolUse"`) — verifying the explicit arm ran, not the wildcard.

**Coverage Requirement**: `obs.tool.is_some()` must be an explicit assertion in AC-03 test. Not implicit.

---

### R-04: `PermissionRetriesRule` continues firing for failure-only imbalance

**Severity**: High
**Likelihood**: Low
**Impact**: The entire motivation of col-027 (fixing the false friction signal) is not achieved. Retrospectives for sessions with high tool failure rates continue to emit `permission_retries` findings. AC-05 would fail.

**Test Scenarios**:
1. Construct 5 Pre + 0 Post + 5 Failure for tool "Bash". Run `PermissionRetriesRule::detect()`. Assert findings is empty.
2. Construct 3 Pre + 0 Post + 3 Failure. Assert findings is empty (balanced, at threshold).
3. Construct 5 Pre + 0 Post + 2 Failure. Assert one finding fires with `retries == 3` (genuine imbalance remains).
4. Run all existing `PermissionRetriesRule` tests unmodified — all must pass (AC-06 regression guard).

**Coverage Requirement**: Scenario 1 must use the `make_failure` helper from NFR-04. Scenario 4 must be a non-modification constraint enforced at PR review.

---

### R-05: `build_request()` wildcards the failure event — `tool_name` not extracted

**Severity**: Med
**Likelihood**: Low
**Impact**: `RecordEvent` produced with `tool_name = None` and `event_type` potentially wrong. Listener stores a record with `tool = None`, same downstream consequence as R-03. AC-11 would fail.

**Test Scenarios**:
1. Call `build_request("PostToolUseFailure", mock_input)` with `extra["tool_name"] = "Read"` and `extra["error"] = "file not found"`. Assert returned `HookRequest::RecordEvent` with `event_type == "PostToolUseFailure"` and tool_name propagated.
2. Call `build_request("PostToolUseFailure", empty_input)` — assert no panic and returns valid `RecordEvent` (not wildcard generic_record_event path).

**Coverage Requirement**: AC-11 test must inspect `event_type` on the returned `HookRequest`. A code inspection check should verify the match arm exists in the source.

---

### R-06: `ToolFailureRule` threshold boundary error (off-by-one)

**Severity**: Med
**Likelihood**: Med
**Impact**: Rule fires at exactly 3 failures (should only fire at > 3), generating findings for legitimate low-failure tools. Or rule does not fire at 4 failures. AC-08/AC-09 boundary conditions are inverted.

ADR-005 specifies: fires when `count > threshold` (strictly greater than 3), i.e., threshold = 3, fires at 4+.

**Test Scenarios**:
1. Construct exactly 3 `PostToolUseFailure` records for "Read". Assert findings is empty (boundary: at-threshold, no finding).
2. Construct exactly 4 `PostToolUseFailure` records for "Bash". Assert exactly 1 finding fires with `measured == 4.0`, `threshold == 3.0`.
3. Construct 3 for "Read" + 4 for "Bash" + 2 for "Write". Assert exactly 1 finding (only "Bash"), with correct claim text `"Tool 'Bash' failed 4 times"`.

**Coverage Requirement**: Both at-threshold (3, no finding) and above-threshold (4, finding) cases must be explicit tests. The threshold constant value `3` must be visible in the test assertion, not hardcoded as a magic number.

---

### R-07: `ToolFailureRule` missing `source_domain` guard

**Severity**: Med
**Likelihood**: Low
**Impact**: Records from MCP-server-internal sources, other agents, or test domains inflate failure counts. Findings fire for tools that are not Claude Code tool calls. Violates entry #2907 (ADR-005 col-023: mandatory source_domain guards).

**Test Scenarios**:
1. Construct 5 `PostToolUseFailure` records with `source_domain = "non-claude-code"`. Assert no findings.
2. Construct 4 `PostToolUseFailure` records with `source_domain = "claude-code"` + 5 with `source_domain = "other"`. Assert exactly 1 finding (from the claude-code records only), `measured == 4.0`.

**Coverage Requirement**: source_domain guard test is a mandatory companion to the threshold tests. Must appear in the `ToolFailureRule` test block.

---

### R-08: Hook exits non-zero on malformed payload

**Severity**: Med
**Likelihood**: Low
**Impact**: Claude Code treats a non-zero hook exit as a hook failure, potentially halting or disrupting the agent session. Violates FR-03.7. Any JSON parsing error, missing field, or unexpected type in the failure payload could trigger this.

Historical context: Entry #247 (ADR-006: Defensive Parsing of Claude Code Hook JSON) and entry #3335 document the graceful degradation contract for hook binaries.

**Test Scenarios**:
1. Call `build_request("PostToolUseFailure", input_with_empty_extra)` — assert no panic, returns valid `HookRequest`.
2. Call `build_request("PostToolUseFailure", input_where_error_is_null)` — assert no panic, `response_snippet` is None.
3. Call `build_request("PostToolUseFailure", input_where_tool_name_missing)` — assert no panic, `tool_name` is None or empty string.
4. Integration: `echo '{}' | unimatrix hook PostToolUseFailure` — assert exit code 0 (AC-12).
5. Integration: `echo 'not-json' | unimatrix hook PostToolUseFailure` — assert exit code 0.

**Coverage Requirement**: All three unit scenarios must be present. The integration exit-code test (AC-12) must run against the compiled binary.

---

### R-09: `extract_event_topic_signal()` generic stringify for failure events

**Severity**: Low
**Likelihood**: Med
**Impact**: `topic_signal` populated with the entire `input.extra` serialized as a JSON blob instead of the `tool_input` field content. Semantically wrong but not a correctness bug — the observation stores and the rule logic still works. Low user-visible impact.

**Test Scenarios**:
1. Unit test `extract_event_topic_signal("PostToolUseFailure", input)` where `extra["tool_input"] = {"path": "/foo"}` — assert result is derived from `tool_input`, not the full extra blob.

**Coverage Requirement**: One unit test is sufficient. The explicit arm existence is the primary mitigation.

---

### R-10: `response_size` set for failure records contaminates context-loaded metrics

**Severity**: Low
**Likelihood**: Low
**Impact**: Metrics such as `total_context_loaded_kb` are currently guarded by `event_type == POSTTOOLUSE` filters, so contamination requires two simultaneous failures: `response_size` set to non-None AND a filter removal. Low probability.

**Test Scenarios**:
1. Assert `obs.response_size == None` in the AC-03 unit test for `extract_observation_fields()`.

**Coverage Requirement**: Single assertion in existing AC-03 test. No separate test needed.

---

### R-11: `POSTTOOLUSEFAILURE` constant value misspelled

**Severity**: Med
**Likelihood**: Low
**Impact**: All detection rule comparisons use `hook_type::POSTTOOLUSEFAILURE` — if the constant value is `"PostToolUseFailure"` in the constant but the hook fires with a different casing or spelling, zero records match any rule. Silent complete failure of the entire feature.

**Test Scenarios**:
1. Unit test: assert `hook_type::POSTTOOLUSEFAILURE == "PostToolUseFailure"` — exact string equality (AC-02).
2. In the AC-03 test, assert `obs.hook == hook_type::POSTTOOLUSEFAILURE` using the constant (not a string literal) — ensures the constant is what rules actually compare against.

**Coverage Requirement**: AC-02 test must use exact string assertion. Use the constant in all test comparisons, not inline string literals.

---

### R-12: Negative `permission_friction_events` from signed subtraction

**Severity**: Low
**Likelihood**: Low
**Impact**: Metric value goes negative if failure count exceeds pre_count (e.g., in pathological test data). Downstream consumers may misinterpret or propagate the negative value.

**Test Scenarios**:
1. Assert `compute_universal()` returns `permission_friction_events >= 0.0` when `failure_count > pre_count` (e.g., 1 Pre + 0 Post + 5 Failure). Arithmetic must use `saturating_sub`.

**Coverage Requirement**: One boundary test in the `metrics.rs` test module.

---

### R-13: `ToolFailureRule` not registered in `default_rules()`

**Severity**: Med
**Likelihood**: Low
**Impact**: Rule is implemented and tested in isolation but never executed in production retrospectives. All AC-08/AC-09 unit tests pass but the feature delivers no retrospective signal. Requires a separate bugfix after deployment.

**Test Scenarios**:
1. Unit test `default_rules()` — assert the returned slice contains a rule with `name() == "tool_failure_hotspot"`. Assert count equals 22 (from 21 + 1 new rule per FR-07.6).

**Coverage Requirement**: `default_rules()` count assertion must be explicit. This is a registration gate test.

---

### R-14: Settings.json hook registration uses wrong command pattern or event key

**Severity**: High
**Likelihood**: Low
**Impact**: Claude Code never invokes the hook binary on tool failure. The entire feature is a no-op at runtime — every subsequent observation gap persists, PermissionRetriesRule continues to fire false positives. Difficult to detect without live testing.

**Test Scenarios**:
1. Inspect `.claude/settings.json` — assert `"PostToolUseFailure"` key exists (exact casing), with `matcher: "*"` and a command containing `unimatrix hook PostToolUseFailure`.
2. Assert the command path pattern is consistent with the `PreToolUse` and `PostToolUse` entries (same binary path format).

**Coverage Requirement**: AC-01 test must be a structural JSON inspection. Casing of the event key is critical — validate against the Claude Code documentation spelling.

---

## Integration Risks

### Hook Registration → Hook Dispatcher Handoff

The event key in `.claude/settings.json` and the match arm string in `build_request()` must be identical. A casing mismatch (e.g., `postToolUseFailure` vs. `PostToolUseFailure`) means the hook fires but routes to the wildcard arm in `build_request()`. No error is produced — the record is stored with wrong fields.

**Scenario**: End-to-end integration test: inject a mock `PostToolUseFailure` event at the UDS transport layer and verify the stored observation has `hook == "PostToolUseFailure"` with non-None `tool` and `response_snippet`.

### Hook Dispatcher → Storage Layer Payload Contract

`build_request()` puts the raw `input.extra` into the `RecordEvent` payload. `extract_observation_fields()` reads from that payload. If `build_request()` wraps or transforms `input.extra` before forwarding (e.g., re-keying fields), the listener's `payload["tool_name"]` and `payload["error"]` reads will fail silently.

**Scenario**: Trace the exact payload shape from `hook.rs` to `listener.rs`. Verify `payload["error"]` in listener.rs is the same field as `input.extra["error"]` set in hook.rs — no intermediate transformation.

### Detection Rules → Observation Table Contract

`ToolFailureRule` and `PermissionRetriesRule` both iterate `ObservationRecord` structs deserialized from the `observations` table. If the stored `hook` column value is anything other than `"PostToolUseFailure"` (e.g., normalized, lower-cased, or truncated), both rules silently skip all failure records.

**Scenario**: Write a test that inserts a record with `hook = "PostToolUseFailure"` directly into a test SQLite instance, reads it back via the observation query, and confirms `record.event_type == "PostToolUseFailure"` round-trips intact.

### Two-Site Atomicity (metrics.rs ↔ friction.rs)

The `terminal_counts` rename in `friction.rs` and the post-bucket widening in `metrics.rs` have no compile-time linkage. A reviewer could approve a partial commit. The only enforcement mechanism is the integration test that asserts both sites agree on the same observation set (see R-02 scenarios).

---

## Edge Cases

- **Empty observation set**: `ToolFailureRule::detect(&[])` and `PermissionRetriesRule::detect(&[])` must return empty findings without panic.
- **Single tool, exactly 3 failures**: No finding (boundary condition, strictly greater than 3 threshold).
- **Single tool, 1 Pre + 1 Failure**: `PermissionRetriesRule` retries = 0, no finding. `permission_friction_events` contribution = 0.
- **Multiple tools, each under threshold**: No `ToolFailureRule` finding. One tool at 3, another at 2 — both below threshold.
- **Multiple tools, multiple findings**: Tool A at 5 failures + Tool B at 4 failures → two separate `HotspotFinding` records (not one aggregate).
- **Failure with empty error string**: `payload["error"] = ""` — `extract_error_field()` should return `(None, None)` (empty string is non-informative), or `Some("")` — the spec says "non-empty" for AC-03; verify the implementation handles `""` consistently.
- **Error string exactly 500 chars**: No truncation applied. 501-char string truncated at char boundary.
- **`is_interrupt: true` in payload**: The field is present but must not affect any stored field or cause parse failure. `response_snippet` must still contain the `error` string.
- **`tool_name` present in payload but `error` field absent**: `obs.tool.is_some()` and `obs.response_snippet == None`. Rule counts this record for the tool (it still appeared in failure count even without error text).
- **`PostToolUseFailure` arrives when server not running**: Fire-and-forget path enqueues event. Hook exits 0. No data loss (replay on reconnect). Existing behavior, unchanged.

---

## Security Risks

### Untrusted Input via `payload["error"]`

The `error` field in `PostToolUseFailure` payloads originates from Claude Code and reflects the tool's error output. In the case of Bash and Read tools, the error message may include attacker-controlled content (e.g., a file path like `../../etc/passwd` included in a "file not found" error, or injected control characters in a command stderr output).

**What untrusted input does this feature accept?**: `payload["error"]` (plain string), `payload["tool_name"]` (string), `payload["tool_input"]` (JSON object, same as PostToolUse).

**What damage could malformed input cause?**: The error string is stored as `response_snippet` (truncated to 500 chars). It is later displayed as evidence detail in `HotspotFinding.evidence[].detail`. If downstream consumers render this without escaping, stored XSS or log injection is theoretically possible. The 500-char truncation limits blast radius. No SQL injection risk exists — the value is bound via SQLite prepared statement parameters.

**Blast radius**: Limited to `response_snippet` storage and retrospective evidence display. No code execution path. Entry #2909 (ADR-007 col-023) covers depth-limit sandboxing for untyped external payloads — the same ingest security bounds apply here.

**Mitigation scenarios**:
1. Verify `extract_error_field()` uses `as_str()` (returns `Option<&str>`) — rejects non-string `error` fields (e.g., arrays, objects) without panic.
2. Verify the 500-char truncation is applied before storage — no unbounded string stored from external input.
3. Confirm the payload is passed through the existing JSON depth limit guard (per ADR-007 col-023) before `extract_observation_fields()` is called.

### `tool_input` Depth in `topic_signal` Extraction

`extract_event_topic_signal()` reads `input.extra["tool_input"]`, which is a JSON object. A deeply nested `tool_input` could trigger stack overflow or excessive allocation in serde deserialization if no depth limit is applied.

**Mitigation**: Existing ADR-007 col-023 depth limit on ingest payloads covers this. No new risk beyond existing PostToolUse path (same extraction logic).

---

## Failure Modes

| Failure | Expected Behavior |
|---------|------------------|
| Server unavailable when `PostToolUseFailure` fires | Hook exits 0; event enqueued for replay on reconnect (existing path, unchanged) |
| `payload["error"]` absent or null | `response_snippet = None`; record stored with tool_name and correct hook type; no panic |
| `payload["tool_name"]` absent | `obs.tool = None`; record stored; `ToolFailureRule` skips None-tool records gracefully |
| Malformed JSON on hook stdin | `build_request()` defensive parse; returns a minimal valid `RecordEvent` or exits 0 |
| `extract_error_field()` receives non-string `error` field | Returns `(None, None)`; no panic; `response_snippet = None` |
| `ToolFailureRule` receives 0 records | Returns empty `Vec<HotspotFinding>` immediately |
| `PermissionRetriesRule` with only failure records | `terminal_count == pre_count`; retries = 0; no finding |
| `compute_universal()` with failure count > pre count | `saturating_sub` returns 0; metric never negative |
| `ToolFailureRule` not in `default_rules()` | Rule runs in unit tests but not in production; undetected until retrospective output compared |

---

## Scope Risk Traceability

| Scope Risk | Architecture Decision | Architecture Risk | Resolution |
|-----------|----------------------|------------------|------------|
| SR-01 | ADR-002: `extract_error_field()` sibling function | R-01 | Mitigated: separate function prevents wrong-extractor call at call site; R-01 tests verify `response_snippet` is populated |
| SR-02 | FR-03.5 / NFR-02: defensive Option access; `is_interrupt` absent = no error | R-08 (partial) | Mitigated: `is_interrupt` absence handled by defensive parsing; R-08 scenarios 2–3 cover missing fields |
| SR-03 | NFR-01 / C-03: fire-and-forget `RecordEvent` path — no synchronous DB write | — | Accepted: same path as all other observation events; no new latency risk |
| SR-04 | ARCHITECTURE.md §Detection Rule Audit + FR-08 | R-07, R-13 | Mitigated: full 21-rule audit documented in architecture; R-13 tests `default_rules()` registration |
| SR-05 | ADR-005: threshold 3 hardcoded constant, `TOOL_FAILURE_THRESHOLD` extraction recommended | R-06 | Partially accepted: threshold is a compile-time constant; R-06 tests the boundary; tuning deferred |
| SR-06 | Out of scope per SCOPE.md non-goals | — | Accepted as follow-on; no architecture risk created |
| SR-07 | ADR-001: explicit `"PostToolUseFailure"` arm before wildcard in both `build_request()` and `extract_observation_fields()` | R-03, R-05 | Mitigated: explicit arms required by FR-03.1 and FR-04.1; R-03 asserts `obs.tool.is_some()` |
| SR-08 | ADR-004: atomic two-site commit requirement; coupled AC-05/AC-07 | R-02 | Mitigated: ADR-004 mandates same-commit delivery; R-02 requires cross-site assertion in one test function |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | 7 scenarios minimum |
| High | 3 (R-03, R-04, R-14) | 8 scenarios minimum |
| Med | 6 (R-05, R-06, R-07, R-08, R-11, R-13) | 14 scenarios minimum |
| Low | 3 (R-09, R-10, R-12) | 3 scenarios minimum |

**Total**: 14 risks, 32+ test scenarios required across `listener.rs`, `friction.rs`, `metrics.rs`, `hook.rs`, and integration.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection observation hook" — found entry #699 (silent data orphaning in hook pipeline, near-identical to SR-01/R-01 scenario)
- Queried: `/uni-knowledge-search` for "risk pattern observation pipeline hook dispatch detection rule" — found entries #763 (server-side observation intercept pattern), #2907 (mandatory source_domain guards), #2928 (string-refactor test patterns)
- Queried: `/uni-knowledge-search` for "PermissionRetriesRule differential metrics friction detection false positive" — found entries #3446 (lesson-learned: misattribution), #3419 (permission_friction_events proxy), #3472 (atomic update pattern), #3476/#3477 (col-027 ADRs)
- Queried: `/uni-knowledge-search` for "SQLite observation event_type string match arm wildcard silent data loss" — found entries #2903 (string-based hook_type ADR), #384 (silent event loss accepted risk), #3312 (observation parsing pitfalls)
- Queried: `/uni-knowledge-search` for "hook binary exit code payload extraction defensive parsing" — found entries #247 (ADR-006 defensive parsing), #3335 (graceful degradation contract), #2909 (ingest security bounds ADR-007 col-023)
- Stored: nothing novel to store — R-02 (two-site atomicity pattern) is already captured in entry #3472; R-01 (wrong extractor silent loss) is already captured in entry #699 and #3474. No cross-feature pattern visible beyond what is already stored.
