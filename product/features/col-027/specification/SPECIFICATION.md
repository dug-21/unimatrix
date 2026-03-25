# SPECIFICATION: col-027 — PostToolUseFailure Hook Support

GH Issue: #382

---

## Objective

When a Claude Code tool call fails, `PostToolUse` does not fire — `PostToolUseFailure` fires instead. Because `PostToolUseFailure` is not registered in `.claude/settings.json`, every failed tool call produces a `PreToolUse` observation record with no corresponding terminal record. This inflates the Pre-Post differential used by `PermissionRetriesRule` and the `permission_friction_events` metric, causing false findings across all retrospectives since nan-002. This feature registers the hook, stores failure records with the correct event type, adds a new `ToolFailureRule` detection rule, and corrects the two sites that compute the Pre-Post differential.

---

## Functional Requirements

### FR-01: Hook Registration

**FR-01.1** — `PostToolUseFailure` must be added to `.claude/settings.json` as a registered hook event with `matcher: "*"` and the command `unimatrix hook PostToolUseFailure`, using the same command pattern as `PreToolUse` and `PostToolUse`.

**FR-01.2** — The hook registration must use the same absolute binary path pattern as the existing `PreToolUse` and `PostToolUse` entries.

### FR-02: hook_type Constant

**FR-02.1** — A `pub const POSTTOOLUSEFAILURE: &str = "PostToolUseFailure"` constant must be added to the `hook_type` module in `unimatrix-core/src/observation.rs`, alongside the existing `PRETOOLUSE`, `POSTTOOLUSE`, `SUBAGENTSTART`, and `SUBAGENTSTOPPED` constants.

**FR-02.2** — The doc comment on `ObservationRecord::response_snippet` must be updated to include `PostToolUseFailure` as a source of this field (currently documented as "PostToolUse only").

### FR-03: Hook Dispatcher (hook.rs build_request)

**FR-03.1** — `build_request()` in `hook.rs` must contain an explicit `"PostToolUseFailure"` match arm — it must not fall through to the `_` wildcard arm.

**FR-03.2** — The `PostToolUseFailure` arm must extract `tool_name` from `input.extra["tool_name"]` (same field as `PostToolUse`).

**FR-03.3** — The `PostToolUseFailure` arm must extract `topic_signal` via `extract_event_topic_signal()` from `input.extra["tool_input"]`, consistent with the `PostToolUse` path. It must not fall through to the generic stringify path.

**FR-03.4** — The `PostToolUseFailure` arm must route to `HookRequest::RecordEvent` with `event_type = "PostToolUseFailure"`. It must not route through rework logic; failure events are not rework candidates.

**FR-03.5** — The error content must be captured: the error string from `input.extra["error"]` (a plain `String` field, not an object) must be placed in the payload so the server can populate `response_snippet`. The `is_interrupt` boolean (`input.extra["is_interrupt"]`) must be included in the payload if present; absence must not cause any error.

**FR-03.6** — The hook binary must exit 0 for a `PostToolUseFailure` event, consistent with FR-03.7 (the existing invariant that the hook never fails). All parse errors must be handled defensively.

**FR-03.7** — `PostToolUseFailure` is observation-only. No stdout output path is needed; the hook produces no injection content for this event type.

### FR-04: Storage Layer (listener.rs extract_observation_fields)

**FR-04.1** — `extract_observation_fields()` in `listener.rs` must contain an explicit `"PostToolUseFailure"` match arm before the `SubagentStop | _` wildcard.

**FR-04.2** — The `PostToolUseFailure` arm must extract `tool_name` from `payload["tool_name"]`, producing a non-None `tool` field in the stored `ObservationRow`.

**FR-04.3** — The `PostToolUseFailure` arm must extract the error string from `payload["error"]` directly (not via `extract_response_fields()`, which reads `tool_response` — an object field absent on failure payloads). The extracted string must be stored as `response_snippet` (truncated to 500 chars at a valid UTF-8 char boundary, consistent with the existing snippet budget).

**FR-04.4** — `response_size` must be left as `None` for `PostToolUseFailure` records. Error strings are small; size is not meaningful.

**FR-04.5** — The stored `hook` column value for a `PostToolUseFailure` event must be `"PostToolUseFailure"`. It must NOT be normalized to `"PostToolUse"`. This is the inverse of the rework-candidate normalization (which converts `post_tool_use_rework_candidate` to `PostToolUse`).

**FR-04.6** — The `input` field (tool_input) must be extracted from `payload["tool_input"]` and stored as the JSON-serialized string, same as `PreToolUse`.

### FR-05: PermissionRetriesRule Fix (friction.rs)

**FR-05.1** — `PermissionRetriesRule::detect()` must count `PostToolUseFailure` records in the terminal event bucket alongside `PostToolUse` records. The differential used to compute retries must be `pre_count - terminal_count`, where `terminal_count = post_count + failure_count`.

**FR-05.2** — The rule must not fire for a session where the sole source of Pre-Post imbalance is `PostToolUseFailure` records (i.e., where `pre_count == post_count + failure_count`).

**FR-05.3** — The rule must still fire correctly when genuine Pre-with-no-terminal imbalance exceeds the threshold of 2. All existing tests for this rule must pass without modification to their fixture data.

**FR-05.4** — The rule name (`"permission_retries"`), category (`Friction`), severity (`Warning`), claim format, and threshold (2) must remain unchanged.

### FR-06: permission_friction_events Metric Fix (metrics.rs)

**FR-06.1** — In `compute_universal()`, `PostToolUseFailure` records must be counted in the terminal event denominator alongside `PostToolUse` records when computing `permission_friction_events`.

**FR-06.2** — The computation must produce `sum of max(pre - (post + failure), 0) per tool`. A tool where `pre == post + failure` contributes 0 to the metric.

**FR-06.3** — The `permission_friction_events` field name, its position in `UniversalMetrics`, and all other metric computations in `compute_universal()` must remain unchanged.

**FR-06.4** — The fixes in FR-05 (friction.rs) and FR-06 (metrics.rs) are coupled. Both must be updated in the same delivery wave. A partial fix (one without the other) is not acceptable; the metric and the detection rule must not diverge.

### FR-07: New ToolFailureRule Detection Rule (friction.rs)

**FR-07.1** — A new `ToolFailureRule` struct implementing `DetectionRule` must be added to `friction.rs`.

**FR-07.2** — `ToolFailureRule::detect()` must count `PostToolUseFailure` records per tool name across the provided observation records, filtering to `source_domain == "claude-code"`.

**FR-07.3** — When a single tool accumulates more than 3 `PostToolUseFailure` records, the rule must fire one `HotspotFinding` for that tool.

**FR-07.4** — The finding must use: `category = HotspotCategory::Friction`, `severity = Severity::Warning`, `rule_name = "tool_failure_hotspot"`, `claim = "Tool 'X' failed N times"` (where X is the tool name and N is the failure count), `measured = N as f64`, `threshold = 3.0`.

**FR-07.5** — `ToolFailureRule` must produce no finding when all tools have 3 or fewer `PostToolUseFailure` records.

**FR-07.6** — `ToolFailureRule` must be added to `default_rules()` in `detection/mod.rs` in the Friction group, making the total rule count 22. The doc comment on `default_rules()` and the module-level comment must be updated to reflect the new count.

**FR-07.7** — `ToolFailureRule` must collect evidence records (one per failure event, consistent with the `PermissionRetriesRule` evidence pattern) so retrospective consumers can trace individual failure events.

### FR-08: Detection Rule Audit

**FR-08.1** — All 21 current detection rules must be audited for sensitivity to `PostToolUseFailure` records. For each rule, the implementer must produce an explicit disposition: either "no action needed" (rule does not use `PostToolUse`/`PreToolUse` string matching) or "fix required" (rule uses Pre-Post differential or would match `PostToolUseFailure` unintentionally). The audit disposition must be documented in the implementation brief or PR description.

**FR-08.2** — Rules that filter on specific `event_type == "PostToolUse"` + tool combinations (e.g., `PostToolUse` + `Write` for file-write detection) are not affected because `PostToolUseFailure` has a distinct `event_type` string. These must be marked "no action needed" in the audit.

**FR-08.3** — Only `PermissionRetriesRule` (friction.rs) and `compute_universal()` (metrics.rs) are known to use the Pre-Post differential. If the audit uncovers additional rules with the same pattern, they must be fixed within this feature.

---

## Non-Functional Requirements

**NFR-01: Hook Latency** — The `PostToolUseFailure` dispatch path must complete within the existing 40ms transport budget (`HOOK_TIMEOUT`). The event routes to `HookRequest::RecordEvent` (fire-and-forget channel send), which is the same path as `PreToolUse` and `PostToolUse`. No synchronous DB writes may be added to this path.

**NFR-02: Defensive Parsing** — All payload field extractions in both `build_request()` (hook.rs) and `extract_observation_fields()` (listener.rs) must use `Option`-chained access. Absence of `tool_name`, `error`, `tool_input`, or `is_interrupt` must not panic or cause a non-zero exit. Missing `error` field must produce `None` for `response_snippet` rather than an error.

**NFR-03: No Schema Migration** — The `observations` table stores `hook TEXT` without an enum constraint. Adding `"PostToolUseFailure"` as a stored `hook` value requires no schema migration, no new table, and no column change. This is a zero-migration feature.

**NFR-04: Test Additivity** — New tests must be added using the `make_pre` / `make_post` test helper pattern established in `friction.rs`. A `make_failure` helper (producing an `ObservationRecord` with `event_type = "PostToolUseFailure"`) must be added alongside the existing helpers. Existing tests must not be modified to accommodate the fix.

**NFR-05: Test Count Baseline** — Current baseline: 2169 unit + 16 migration + 185 infra integration tests. New tests are additive; the total must increase. No existing tests may be deleted.

**NFR-06: String Constant Discipline** — Per col-023 ADR-001, new event types are added as `pub const` string constants in `hook_type`, not as enum variants. No HookType enum must be created or extended.

---

## Acceptance Criteria

All 12 criteria from SCOPE.md are carried forward verbatim. AC-05 and AC-06 are coupled with AC-07 per SR-08 risk.

**AC-01** — `PostToolUseFailure` is registered in `.claude/settings.json` with the same command pattern as `PreToolUse` and `PostToolUse`.
Verification: Inspect `.claude/settings.json`; assert the key `"PostToolUseFailure"` is present with `matcher: "*"` and a command string matching the `unimatrix hook PostToolUseFailure` pattern.

**AC-02** — `hook_type::POSTTOOLUSEFAILURE` constant exists in `unimatrix-core/src/observation.rs` with value `"PostToolUseFailure"`.
Verification: Compile `unimatrix-core`; assert `hook_type::POSTTOOLUSEFAILURE == "PostToolUseFailure"` in a unit test.

**AC-03** — A failed tool call that fires `PostToolUseFailure` produces a stored observation record with `event_type = "PostToolUseFailure"`, the correct `tool` name, and a non-empty `response_snippet` containing the error content.
Verification: Unit test in `listener.rs` — construct an `ImplantEvent` with `event_type = "PostToolUseFailure"` and `payload["error"] = "some error message"`, call `extract_observation_fields()`, assert `obs.hook == "PostToolUseFailure"`, `obs.tool.is_some()`, `obs.response_snippet == Some("some error message")`.

**AC-04** — The stored `hook` column value for a `PostToolUseFailure` event is `"PostToolUseFailure"` (not normalized to `"PostToolUse"`).
Verification: Same unit test as AC-03; assert `obs.hook == "PostToolUseFailure"` explicitly (not `"PostToolUse"`).

**AC-05** — `PermissionRetriesRule` no longer fires for sessions where tool failures (Pre with corresponding `PostToolUseFailure`) are the sole source of Pre-Post imbalance.
Verification: Unit test — construct records with 5 `PreToolUse` + 0 `PostToolUse` + 5 `PostToolUseFailure` for the same tool; run `PermissionRetriesRule::detect()`; assert findings is empty. [Coupled with AC-07: both must pass before fix is considered complete.]

**AC-06** — `PermissionRetriesRule` still fires correctly when genuine Pre-with-no-terminal imbalance exceeds threshold (existing tests must pass with the fix applied).
Verification: All existing `PermissionRetriesRule` unit tests pass without modification. Additionally: construct records with 5 `PreToolUse` + 2 `PostToolUse` + 0 `PostToolUseFailure`; assert the rule fires with `retries = 3`. [Coupled with AC-07.]

**AC-07** — `permission_friction_events` metric is computed by subtracting both `PostToolUse` and `PostToolUseFailure` terminal records from `pre_count` per tool, so failures do not inflate the metric.
Verification: Unit test in `metrics.rs` — construct records with 4 `PreToolUse` + 2 `PostToolUse` + 2 `PostToolUseFailure` for the same tool; call `compute_universal()`; assert `permission_friction_events == 0`. [Coupled with AC-05/AC-06.]

**AC-08** — A new `ToolFailureRule` detection rule fires a `HotspotFinding` when a single tool accumulates more than 3 `PostToolUseFailure` records within a feature cycle's observation set.
Verification: Unit test — construct 4 `PostToolUseFailure` records for tool `"Bash"`; run `ToolFailureRule::detect()`; assert exactly 1 finding, `rule_name == "tool_failure_hotspot"`, `measured == 4.0`, `threshold == 3.0`.

**AC-09** — `ToolFailureRule` produces no finding when all tools have 3 or fewer failure records.
Verification: Unit test — construct exactly 3 `PostToolUseFailure` records for tool `"Read"`; run `ToolFailureRule::detect()`; assert findings is empty.

**AC-10** — Unit tests exist for all changed/new detection rule logic, following the `make_pre` / `make_post` test helper pattern established in `friction.rs`.
Verification: Inspect `friction.rs` tests; assert a `make_failure(ts, tool)` helper exists; assert tests for AC-05, AC-06, AC-08, and AC-09 use this helper.

**AC-11** — `build_request()` in `hook.rs` handles `"PostToolUseFailure"` with an explicit match arm (not falling through to the `_` wildcard).
Verification: Code inspection — the `build_request()` match statement must contain a `"PostToolUseFailure" =>` arm. Unit test: call `build_request("PostToolUseFailure", &mock_input)` with `extra["tool_name"] = "Bash"` and `extra["error"] = "permission denied"`; assert the returned `HookRequest` is `RecordEvent` with `event_type == "PostToolUseFailure"`.

**AC-12** — The hook binary exits 0 for a `PostToolUseFailure` event (consistent with FR-03.7: hook never fails).
Verification: Unit test for the `build_request` path with missing or malformed payload fields; assert no panic and the resulting request is a valid `RecordEvent`. Integration: run `echo '{}' | unimatrix hook PostToolUseFailure`; assert exit code 0.

---

## Domain Models

### Key Terms

**PostToolUseFailure event** — A Claude Code hook event that fires when a tool call fails (as opposed to `PostToolUse`, which fires on success). The two events are mutually exclusive for any single tool invocation. Fields: `tool_name` (string), `tool_input` (object), `error` (plain string), `is_interrupt` (optional bool).

**terminal event** — An observation record that "closes" a corresponding `PreToolUse` record for the same tool. Both `PostToolUse` and `PostToolUseFailure` are terminal events. A `PreToolUse` with no terminal event is a genuinely cancelled or permission-blocked call.

**Pre-Post differential** — The count `pre_count - terminal_count` per tool. Measures genuinely unresolved tool calls. Used by `PermissionRetriesRule` and `permission_friction_events`. Prior to col-027, `PostToolUseFailure` records were not counted as terminal events, inflating this value.

**permission_friction_events** — A `UniversalMetrics` field counting the total Pre-Post differential across all tools. A proxy for tool cancellation events. It is not a count of permission prompts shown to the user.

**PermissionRetriesRule** — A detection rule in `friction.rs` that fires a `Friction/Warning` finding when the Pre-Post differential for a single tool exceeds 2. Prior to col-027, it incorrectly treated tool failures (which have `PostToolUseFailure` but no `PostToolUse`) as cancelled calls.

**ToolFailureRule** — A new detection rule added by col-027. It counts `PostToolUseFailure` records per tool and fires a `Friction/Warning` finding when a single tool exceeds 3 failures in the observation set.

**response_snippet** — A field on `ObservationRecord` (max 500 chars) that stores the first portion of a tool's response. For `PostToolUseFailure` events, it stores the error string from `payload["error"]`, not from `payload["tool_response"]` (which does not exist on failure payloads).

**extract_response_fields()** — An internal listener.rs helper that reads `payload["tool_response"]` (an object) to compute `response_size` and `response_snippet`. This function must NOT be used for `PostToolUseFailure` extraction; it reads the wrong field. The `PostToolUseFailure` arm must extract `payload["error"]` directly.

**hook_type module** — String constants in `unimatrix-core/src/observation.rs` for well-known Claude Code event type strings. Added by col-023 ADR-001 to replace the former `HookType` enum. `POSTTOOLUSEFAILURE` is added by col-027.

### Entity Relationships

```
PreToolUse record (1) ---[terminal event]--> (0..1) PostToolUse record
                                         OR (0..1) PostToolUseFailure record
                    If no terminal record exists: genuinely cancelled call
                    If PostToolUseFailure exists: tool call failed

ToolFailureRule:
  PostToolUseFailure records (N per tool) --> HotspotFinding if N > 3

PermissionRetriesRule (fixed):
  pre_count - (post_count + failure_count) > 2 --> HotspotFinding
```

---

## User Workflows

### Workflow 1: Failed Tool Call Observation

1. Claude Code executes a tool call.
2. The tool call fails (non-zero exit, permission error, file not found, etc.).
3. Claude Code fires `PostToolUseFailure` hook with payload `{ tool_name, tool_input, error, is_interrupt? }`.
4. `unimatrix hook PostToolUseFailure` is invoked with the payload on stdin.
5. `build_request()` in `hook.rs` extracts `tool_name`, `error`, and `tool_input`; returns `HookRequest::RecordEvent` with `event_type = "PostToolUseFailure"`.
6. The request is dispatched fire-and-forget to the UDS server within 40ms.
7. `extract_observation_fields()` in `listener.rs` stores the record with `hook = "PostToolUseFailure"`, `tool = tool_name`, `response_snippet = error[:500]`.
8. The hook exits 0.

### Workflow 2: Retrospective with Corrected Metrics

1. A retrospective is triggered for a completed feature cycle.
2. `compute_universal()` sums `(pre - (post + failure))` per tool; `PostToolUseFailure` records no longer inflate `permission_friction_events`.
3. `PermissionRetriesRule::detect()` computes `pre - terminal` per tool; tools with failures paired to their `PreToolUse` records report 0 retries for those calls.
4. If any tool accumulated more than 3 `PostToolUseFailure` records, `ToolFailureRule::detect()` fires a `HotspotFinding` for that tool.
5. The retrospective report reflects actual permission cancellations separately from tool errors.

### Workflow 3: Hook Binary with No Server Running

1. `PostToolUseFailure` fires but the Unimatrix server is not running.
2. `build_request()` returns a `RecordEvent`.
3. `transport.connect()` returns `TransportError::Unavailable`.
4. The event is enqueued to the local event queue for replay on next connection.
5. The hook exits 0 (existing graceful degradation path, unchanged).

---

## Constraints

**C-01: No schema migration.** The `observations` table stores `hook TEXT`. Adding `"PostToolUseFailure"` as a value requires no migration. Verified: no `IN (...)` constraint or enum check exists on the `hook` column.

**C-02: hook_type is string-based.** Per col-023 ADR-001, no `HookType` enum exists or may be created. New event types are `pub const` strings only.

**C-03: Fire-and-forget transport.** `PostToolUseFailure` must route to `RecordEvent` (not `ContextSearch` or any synchronous request variant). The 40ms `HOOK_TIMEOUT` must not be exceeded.

**C-04: Hook must not fail.** The hook binary always exits 0 (FR-03.7). All parse errors — missing fields, malformed JSON, absent `error` string — are handled defensively with `Option`/`unwrap_or` patterns.

**C-05: No stdout output for failure hooks.** `PostToolUseFailure` is observation-only. Claude Code does not act on hook stdout for this event type; no stdout write path is needed.

**C-06: extract_response_fields() must not be reused for error extraction.** That function reads `payload["tool_response"]` (an object). `PostToolUseFailure` payloads carry `payload["error"]` (a plain string). Reuse would produce `None` silently, violating AC-03. Direct field extraction is required.

**C-07: Blast radius audit required.** All 21 existing detection rules must be audited before delivery is marked complete. Rules that use only specific `event_type + tool` filtering (not the Pre-Post differential) are safe without modification, but must be explicitly confirmed.

**C-08: No retroactive correction.** Stored findings from prior features will not be recomputed. The fix is forward-only: future retrospectives on new observation data will be correct; existing stored `HotspotFinding` records remain as-is.

**C-09: No recommendation template changes.** The claim text and recommendation content in `report.rs` for `permission_retries` findings is addressed in col-026 (AC-19). col-027 does not touch recommendation templates.

---

## Dependencies

**Crates (internal):**
- `unimatrix-core` (`src/observation.rs`) — adds `hook_type::POSTTOOLUSEFAILURE`
- `unimatrix-server` (`src/uds/hook.rs`) — adds `"PostToolUseFailure"` match arm in `build_request()`
- `unimatrix-server` (`src/uds/listener.rs`) — adds `"PostToolUseFailure"` match arm in `extract_observation_fields()`
- `unimatrix-observe` (`src/detection/friction.rs`) — fixes `PermissionRetriesRule`, adds `ToolFailureRule`
- `unimatrix-observe` (`src/detection/mod.rs`) — registers `ToolFailureRule` in `default_rules()`
- `unimatrix-observe` (`src/metrics.rs`) — fixes `compute_universal()` for `permission_friction_events`

**Configuration:**
- `.claude/settings.json` — adds `"PostToolUseFailure"` hook registration

**No external crates** are required. No schema migration tooling is invoked.

**Existing patterns depended on:**
- `make_pre` / `make_post` test helpers in `friction.rs` — extended with `make_failure`
- Fire-and-forget `RecordEvent` dispatch path in `hook.rs` — reused without modification
- `extract_event_topic_signal()` in `hook.rs` — reused for `tool_input` extraction
- `truncate_at_utf8_boundary()` in `listener.rs` — reused for error string truncation

**Unimatrix knowledge entries consulted:**
- Entry #2903: col-023 ADR-001 — string-based hook_type constants
- Entry #2843: Observation pipeline blast-radius pattern
- Entry #3446: PermissionRetriesRule misattribution lesson
- Entry #3419: permission_friction_events as tool-cancellation proxy
- Entry #3471: Pattern for adding a new Claude Code hook event type
- Entry #3472: Atomic update requirement for Pre-Post differential sites

---

## NOT in Scope

The following are explicitly excluded from col-027. Scope additions require a separate feature or scope change request.

- **Retroactive correction of past findings.** Stored `HotspotFinding` records from features prior to col-027 will not be recomputed. Historical data remains as stored.
- **Error message classification.** Categorizing failure error strings (timeout vs. permission-denied vs. not-found vs. file-missing) is a follow-on analysis capability. col-027 stores the raw error snippet only.
- **Renaming PermissionRetriesRule.** The rule name `"permission_retries"` and finding category stay as-is. Renaming or recategorizing is deferred (potential col-028 follow-on).
- **Renaming permission_friction_events.** The metric field name is unchanged. Only the computation algorithm is corrected.
- **Hook output injection.** `PostToolUseFailure` is observation-only. No context injection into failing tool calls is implemented.
- **Allowlist recommendation text changes.** Addressed in col-026 (AC-19). col-027 does not change recommendation templates in `report.rs`.
- **Bash failure detection overlap.** The existing `is_bash_failure()` path in the `PostToolUse` rework handler identifies Bash failures via `exit_code`. That path is for rework detection and remains separate from `PostToolUseFailure` observation recording.
- **data_quality caveat in retrospective output.** Adding a `data_quality_note` warning for features observed before `PostToolUseFailure` registration is a follow-on (SR-06 in the risk assessment is accepted as low-priority for this feature).
- **ToolFailureRule threshold configuration.** The threshold of 3 failures is a hardcoded constant for col-027. Per-tool or per-phase threshold configuration is a follow-on if threshold tuning emerges (SR-05 accepted as-is).
- **is_interrupt field surfacing.** The `is_interrupt` boolean from the `PostToolUseFailure` payload is captured in the event payload for storage but is not used by any detection rule or metric in col-027. Distinguishing user-interrupted calls from genuine tool errors is a follow-on.

---

## Open Questions

None. All questions from SCOPE.md are resolved:

1. Payload structure confirmed: `tool_name`, `tool_input`, `error` (plain string), `is_interrupt` (optional bool). No `tool_response` field.
2. ToolFailureRule threshold confirmed: 3 failures per tool.
3. Retroactive reporting impact: accepted, forward-only fix.
4. topic_signal extraction for `PostToolUseFailure`: explicit arm in `extract_event_topic_signal()` using `input.extra["tool_input"]`, same as `PostToolUse`.

The following SR items are accepted risks with no spec action required:

- SR-02 (`is_interrupt` may be absent): Handled by FR-03.5 (defensive Option access). No spec change needed.
- SR-03 (40ms budget): Handled by NFR-01 and C-03. Fire-and-forget path is identical to existing events.
- SR-05 (threshold hardcoded): Accepted as follow-on. Noted in NOT in scope.
- SR-06 (data_quality caveat): Accepted as follow-on. Noted in NOT in scope.

---

## Knowledge Stewardship

- Queried: /uni-query-patterns for PostToolUse observation hook extraction detection rules -- found entries #763 (observation intercept pattern), #2903 (ADR-001 col-023 string-based hook_type), #2843 (blast-radius pattern), #3471 (pattern for adding new hook event type), #3472 (atomic update requirement for Pre-Post differential)
- Queried: /uni-query-patterns for PermissionRetriesRule friction detection pre post differential -- found entries #3446 (lesson-learned: misattribution), #3419 (permission_friction_events as cancellation proxy), #1279 (lesson-learned: false positives from capability check failures), #3472 (coupled update requirement)
