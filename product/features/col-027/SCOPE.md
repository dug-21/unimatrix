# obs: PostToolUseFailure Hook Support

## Problem Statement

When a Claude Code tool call fails, `PostToolUse` does NOT fire — `PostToolUseFailure` fires instead
(per Claude Code documentation). The current `.claude/settings.json` registers `PreToolUse` and
`PostToolUse` but not `PostToolUseFailure`. This means every failed tool call produces a
`PreToolUse` observation record with no corresponding terminal record.

Two concrete harms result:

1. **Missing failure data.** Tool errors, MCP call failures, Bash non-zero exits, and file-not-found
   `Read` calls produce no failure observation. The retrospective pipeline has zero visibility into
   tool error rates.

2. **PermissionRetriesRule misreads the signal.** The rule computes `pre_count - post_count` per
   tool as its "retries" metric. Because `PostToolUseFailure` is unregistered, every tool failure
   inflates this differential. The rule has been reporting tool failure counts as "permission
   retries" — and emitting allowlist recommendations based on that false signal — across every
   feature since `nan-002`. All retrospectives from that point contain a misattributed finding.

This is a correctness bug in both the hook registration layer and the detection layer.

## Goals

1. Register `PostToolUseFailure` in `.claude/settings.json` so Claude Code fires the hook on tool
   failures.
2. Implement a `PostToolUseFailure` handler in `build_request()` (hook.rs) that routes the event
   to a `RecordEvent` with `event_type = "PostToolUseFailure"`, capturing tool name, error content
   in `response_snippet`, and session id.
3. Add `extract_observation_fields` support for `PostToolUseFailure` event type in the UDS listener
   so the server stores the record correctly.
4. Add `hook_type::POSTTOOLUSEFAILURE` constant to `unimatrix-core/src/observation.rs` alongside
   the existing `PRETOOLUSE` / `POSTTOOLUSE` constants.
5. Update `PermissionRetriesRule` to exclude `PostToolUseFailure` records from the Pre-Post
   differential so the rule measures only genuine cancelled/permission-blocked tool calls.
6. Add a new `ToolFailureRule` detection rule (or extend an existing rule) that counts
   `PostToolUseFailure` records per tool and fires a finding when failure count exceeds a threshold.
7. Update `permission_friction_events` metric computation in `metrics.rs` to treat
   `PostToolUseFailure` records as neutral (not contributing to the Pre-Post differential used as
   the metric proxy).

## Non-Goals

The following are explicitly out of scope for col-027:

- **Retroactive correction of past retrospective findings.** Stored `HotspotFinding` records from
  prior features will not be recomputed. The fix applies to future retrospectives.
- **Error message classification.** Categorizing failure messages (timeout vs. permission-denied
  vs. not-found) is a follow-on analysis capability; col-027 stores the raw error snippet only.
- **Changing the PermissionRetriesRule semantics.** The rule name and finding category stay as-is;
  only the differential calculation is fixed to exclude failures. Renaming or recategorizing the
  rule is deferred (potential col-028 or follow-on).
- **Changing the `permission_friction_events` metric name.** It remains as-is; the fix corrects the
  count algorithm without changing the field name or its meaning in `UniversalMetrics`.
- **Hook output injection.** `PostToolUseFailure` is observation-only — the hook cannot inject
  context into a failing tool call. No stdout output path is needed.
- **Allowlist recommendation text changes.** The content of the existing `permission_retries`
  recommendation in `report.rs` is addressed separately in col-026 (AC-19); col-027 does not
  change recommendation templates.
- **Bash failure detection overlap.** The existing `is_bash_failure()` path in the PostToolUse
  rework handler identifies Bash failures via exit_code. That path is for rework detection and
  remains separate from `PostToolUseFailure` observation recording.

## Background Research

### Hook Registration Gap

`/workspaces/unimatrix/.claude/settings.json` registers 8 hook events: `SessionStart`, `Stop`,
`UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `SubagentStart`, `SubagentStop`, `PreCompact`.
`PostToolUseFailure` is absent.

Per project research artifacts (`ass-008`, `ass-011`), `PostToolUseFailure` is a valid Claude Code
hook that fires when a tool call fails. `PostToolUse` does not fire in that case. The hook payload
shares structure with `PostToolUse` but the `tool_response` field contains the error message rather
than successful output.

### Hook Dispatcher (hook.rs)

`build_request()` in `crates/unimatrix-server/src/uds/hook.rs` matches the event name string. The
`PostToolUse` arm handles rework-eligible tool routing and falls through to `generic_record_event`
for non-rework tools. The `_` wildcard arm at the bottom also calls `generic_record_event`.

`PostToolUseFailure` would currently fall through to the `_` wildcard, which calls
`generic_record_event` with the raw `input.extra` payload. However, because the hook is not
registered in settings.json, this code path is never reached.

When `PostToolUseFailure` is registered and added as an explicit match arm, it can extract
`tool_name` and `tool_response` from `input.extra` (same field names as `PostToolUse`) and route to
`RecordEvent` with `event_type = "PostToolUseFailure"`.

### ObservationRecord Schema

`ObservationRecord` in `crates/unimatrix-core/src/observation.rs` has:
- `event_type: String` — stores the hook name verbatim (col-023 ADR-001 replaced a HookType enum
  with string fields; no schema change is needed to add a new event type string)
- `response_snippet: Option<String>` — docstring says "PostToolUse only"; this needs updating to
  include `PostToolUseFailure`. For failure events, populated from `input.extra["error"]` (a
  plain string, not an object — `extract_response_fields()` must handle this distinction).
- `response_size: Option<u64>` — not meaningful for failures (error strings are small); leave None.

`hook_type` module has string constants for `PRETOOLUSE`, `POSTTOOLUSE`, `SUBAGENTSTART`,
`SUBAGENTSTOPPED`. A `POSTTOOLUSEFAILURE` constant must be added.

### Storage Layer (listener.rs)

`extract_observation_fields()` in `listener.rs` has a match on `hook` string. The
`"PostToolUse" | "post_tool_use_rework_candidate"` arm extracts `tool_name`, `tool_input`,
and calls `extract_response_fields()`. The `"SubagentStop" | _` arm returns `(None, None, None, None)`.

`PostToolUseFailure` needs an explicit arm that extracts `tool_name` and calls
`extract_response_fields()` to capture the error content in `response_snippet`. The failure event
should NOT be normalized to `"PostToolUse"` — it must retain `"PostToolUseFailure"` as the stored
`hook` value so detection rules can distinguish it.

### PermissionRetriesRule (friction.rs)

The rule computes `pre_count - post_count` per tool. It only counts records where
`event_type == "PreToolUse"` or `event_type == "PostToolUse"`. It does not account for
`PostToolUseFailure`, so tool failures that fire `PostToolUseFailure` (instead of `PostToolUse`)
inflate the Pre-Post gap and trigger false findings.

Fix: The rule should treat `PostToolUseFailure` records as "resolved" — i.e., count them toward
the "terminal event" bucket alongside `PostToolUse`. Alternatively, the rule can be reframed as
measuring only genuinely cancelled (Pre with no terminal at all) events, subtracting both
`PostToolUse` and `PostToolUseFailure` from `pre_count`.

Unimatrix #3446 (lesson-learned) documents this misattribution. Unimatrix #3330 confirms the
signal has been misread across all features since nan-002. Unimatrix #3419 notes that
`permission_friction_events` is already understood to be a tool-cancellation proxy.

### permission_friction_events Metric (metrics.rs)

The `compute_universal()` function sums `(pre - post)` per tool for `permission_friction_events`.
The same root cause applies: `PostToolUseFailure` records must be counted alongside `PostToolUse`
in the denominator to produce an accurate cancellation-only count.

### Detection Rules Survey

Across all detection rule files (`friction.rs`, `agent.rs`, `scope.rs`, `session.rs`), event_type
filtering hardcodes `"PreToolUse"` and `"PostToolUse"` string literals. Rules that use the
Pre-Post differential are: `PermissionRetriesRule` (friction.rs) and implicitly the metric
computation in `metrics.rs`. Other rules filter on specific tool + event_type combinations but do
not use the differential, so they are not affected.

### No Schema Migration Required

Observations are stored with the raw `hook` column value. Adding a new `event_type` string value
does not require a schema migration — the `observations` table stores `hook TEXT` without a
constraint to an enum. The existing ingest path (line 581: `let event_type: String = hook_str`) is
already fully generic.

## Proposed Approach

### 1. settings.json — Add Registration

Add a `PostToolUseFailure` entry to `.claude/settings.json` with `matcher: "*"` and the same
`unimatrix hook PostToolUseFailure` command pattern used by `PreToolUse` and `PostToolUse`.

### 2. hook_type Constants — Add Constant

Add `pub const POSTTOOLUSEFAILURE: &str = "PostToolUseFailure";` to the `hook_type` module in
`unimatrix-core/src/observation.rs`. Update the doc comment on `response_snippet` in
`ObservationRecord` to include `PostToolUseFailure`.

### 3. hook.rs build_request() — Add Explicit Arm

Add a `"PostToolUseFailure"` match arm that extracts `tool_name` from `input.extra["tool_name"]`
and `topic_signal` via `extract_event_topic_signal()` (from `tool_input`, same path as
`PostToolUse`), skipping rework logic entirely — failures are not rework candidates. Returns a
`RecordEvent` with `event_type = "PostToolUseFailure"`. The error content is carried in
`input.extra["error"]` and captured via `extract_response_fields()` → `response_snippet`.

### 4. listener.rs extract_observation_fields() — Add Arm

Add `"PostToolUseFailure"` to the match in `extract_observation_fields()`, extracting `tool_name`
and calling `extract_response_fields()` to capture the error in `response_snippet`. The stored
`hook` value must remain `"PostToolUseFailure"` (no normalization to `"PostToolUse"`).

### 5. friction.rs PermissionRetriesRule — Fix Differential

Count both `PostToolUse` AND `PostToolUseFailure` records in the `post_counts` bucket (or rename
it `terminal_counts`). The rule claim text can stay as-is for now; the measured value will be
accurate.

### 6. metrics.rs permission_friction_events — Fix Computation

In `compute_universal()`, count `PostToolUseFailure` records alongside `PostToolUse` in the
denominator of the Pre-Post differential to eliminate false inflation.

### 7. New ToolFailureRule Detection Rule

Add a `ToolFailureRule` in `friction.rs` that counts `PostToolUseFailure` records per tool and
fires a `HotspotFinding` when a tool's failure count exceeds a threshold (proposed: 3 failures for
a single tool). Claim format: `"Tool 'X' failed N times"`. Category: `Friction`. Severity: `Warning`.

## Acceptance Criteria

- AC-01: `PostToolUseFailure` is registered in `.claude/settings.json` with the same command
  pattern as `PreToolUse` and `PostToolUse`.
- AC-02: `hook_type::POSTTOOLUSEFAILURE` constant exists in `unimatrix-core/src/observation.rs`
  with value `"PostToolUseFailure"`.
- AC-03: A failed tool call that fires `PostToolUseFailure` produces a stored observation record
  with `event_type = "PostToolUseFailure"`, the correct `tool` name, and a non-empty
  `response_snippet` containing the error content.
- AC-04: The stored `hook` column value for a `PostToolUseFailure` event is `"PostToolUseFailure"`
  (not normalized to `"PostToolUse"`).
- AC-05: `PermissionRetriesRule` no longer fires for sessions where tool failures (Pre with
  corresponding `PostToolUseFailure`) are the sole source of Pre-Post imbalance.
- AC-06: `PermissionRetriesRule` still fires correctly when genuine Pre-with-no-terminal imbalance
  exceeds threshold (existing tests must pass with the fix applied).
- AC-07: `permission_friction_events` metric is computed by subtracting both `PostToolUse` and
  `PostToolUseFailure` terminal records from `pre_count` per tool, so failures do not inflate the
  metric.
- AC-08: A new `ToolFailureRule` detection rule fires a `HotspotFinding` when a single tool
  accumulates more than 3 `PostToolUseFailure` records within a feature cycle's observation set.
- AC-09: `ToolFailureRule` produces no finding when all tools have 3 or fewer failure records.
- AC-10: Unit tests exist for all changed/new detection rule logic, following the `make_pre` /
  `make_post` test helper pattern established in `friction.rs`.
- AC-11: `build_request()` in hook.rs handles `"PostToolUseFailure"` with an explicit match arm
  (not falling through to the `_` wildcard).
- AC-12: The hook binary exits 0 for a `PostToolUseFailure` event (consistent with FR-03.7: hook
  never fails).

## Constraints

- **No schema migration.** The `observations` table stores `hook TEXT` without an enum constraint.
  A new `event_type` value requires no migration.
- **hook_type is string-based.** col-023 ADR-001 replaced the `HookType` enum with string
  constants. New event types are added as `pub const` strings, not enum variants.
- **Fire-and-forget transport.** `PostToolUseFailure` must route to `RecordEvent` (fire-and-forget)
  not a synchronous request, consistent with all other observation events. The 40ms hook timeout
  must not be exceeded.
- **Hook must not fail.** FR-03.7 (documented in hook.rs): the hook binary always exits 0. All
  parse errors must be handled defensively.
- **No stdout output for failure hooks.** `PostToolUseFailure` is observation-only; Claude Code
  does not act on hook stdout for this event type.
- **Blast radius: all 21 detection rules.** Any rule that uses event_type string comparison against
  `"PostToolUse"` or `"PreToolUse"` must be audited. Rules that use the Pre-Post differential must
  be updated. Rules that filter on specific tool + event_type combinations (e.g., `PostToolUse` +
  `Write` for file write detection) are not affected because `PostToolUseFailure` has a distinct
  event_type string.
- **Test count baseline.** Current test count: 2169 unit + 16 migration + 185 infra integration.
  New tests must be additive.

## Resolved Questions

1. **PostToolUseFailure payload structure** (resolved via Claude Code docs): The failure payload
   uses `tool_name` (same as `PostToolUse`), `tool_input` (same), and **`error`** (a string
   description of what went wrong) instead of `tool_response`. There is no `tool_response` field
   on failure events. An optional `is_interrupt` boolean indicates user-interruption vs. tool error.
   Implementation: extract `tool_name` from `input.extra["tool_name"]`, populate `response_snippet`
   from `input.extra["error"]`, extract `topic_signal` from `input.extra["tool_input"]`.

2. **ToolFailureRule threshold** (resolved): Threshold of 3 failures per tool.

3. **Retroactive reporting impact** (resolved): No concern. Future retrospectives on old data may
   show lower `permission_retries` counts; this is expected and correct behaviour.

4. **topic_signal extraction for PostToolUseFailure** (resolved): Add an explicit arm in
   `extract_event_topic_signal()` to extract from `input.extra["tool_input"]`, consistent with
   the `PostToolUse` path. Do not fall through to generic stringify.

## Tracking

GH Issue: #382
