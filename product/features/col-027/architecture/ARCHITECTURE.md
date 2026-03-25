# Architecture: col-027 — PostToolUseFailure Hook Support

## System Overview

col-027 closes an observation gap that has existed since hook registration was first introduced. When a Claude Code tool call fails, `PostToolUse` does not fire — `PostToolUseFailure` fires instead. Because `PostToolUseFailure` was never registered in `.claude/settings.json`, every tool failure since `nan-002` produced a `PreToolUse` observation with no corresponding terminal record. This inflated the Pre-Post differential used by both `PermissionRetriesRule` (friction.rs) and `permission_friction_events` (metrics.rs), causing false "permission retries" findings across every retrospective.

This feature makes three targeted corrections:

1. **Hook registration** — add `PostToolUseFailure` to `.claude/settings.json` so the event fires.
2. **Observation pipeline** — add an explicit dispatch and storage path for the new event type.
3. **Detection layer** — fix the Pre-Post differential and introduce a `ToolFailureRule` to surface genuine failure signals.

The change is entirely additive at the schema level (no migration) and forward-only at the data level (prior retrospective findings are not recomputed).

---

## Component Breakdown

### Component 1: Hook Registration (`.claude/settings.json`)

Registers `PostToolUseFailure` with Claude Code so the hook binary is invoked on tool failures.

**Responsibility:** Declare the hook event. Passes event name and payload to the hook binary via stdin.

**Constraint:** Must follow the same command pattern as `PreToolUse` and `PostToolUse` — `unimatrix hook PostToolUseFailure` with `matcher: "*"`.

---

### Component 2: Hook Dispatcher (`unimatrix-server/src/uds/hook.rs`)

Contains `build_request()` and `extract_event_topic_signal()`. Converts raw stdin JSON into a typed `HookRequest` for transport.

**Responsibilities for col-027:**
- Add an explicit `"PostToolUseFailure"` arm in `build_request()` that extracts `tool_name` and `topic_signal` from `tool_input`, and routes to `RecordEvent` with `event_type = "PostToolUseFailure"`. Does NOT enter rework logic.
- Add a `"PostToolUseFailure"` arm in `extract_event_topic_signal()` that reads from `input.extra["tool_input"]`, identical to the `PostToolUse` path (per SCOPE resolved question 4).
- The hook binary continues to always exit 0 (FR-03.7). All field accesses use defensive `Option` chaining.

**Key constraint:** `PostToolUseFailure` carries `error` (plain string), not `tool_response` (object). The dispatcher must NOT call `extract_response_fields()` — that function reads `tool_response`. Instead, extract the error string directly from `input.extra["error"]` and pass it as `topic_signal` content or forward it via the event payload so `extract_observation_fields()` in listener.rs can pick it up. See ADR-001 for the exact approach.

---

### Component 3: Core Constants (`unimatrix-core/src/observation.rs`)

The `hook_type` module holds well-known event type string constants.

**Responsibility for col-027:** Add `pub const POSTTOOLUSEFAILURE: &str = "PostToolUseFailure";`. Update the `response_snippet` doc comment on `ObservationRecord` to include `PostToolUseFailure`.

No struct changes are required — `response_snippet` already exists and accepts `Option<String>`.

---

### Component 4: Observation Storage (`unimatrix-server/src/uds/listener.rs`)

Contains `extract_observation_fields()` and `extract_response_fields()`. Converts `ImplantEvent` to `ObservationRow` for SQLite insertion.

**Responsibilities for col-027:**
- Add an explicit `"PostToolUseFailure"` arm in the `extract_observation_fields()` match that extracts `tool_name` from `payload["tool_name"]` and populates `response_snippet` from the `"error"` field in the payload (not `"tool_response"`).
- Add `extract_error_field()` — a sibling to `extract_response_fields()` — that reads `payload["error"]` as a plain string and returns `(None, Some(snippet))`. `response_size` is intentionally `None` for failure events (error strings are small; measuring them provides no analytical value).
- The stored `hook` column value must remain `"PostToolUseFailure"` — **no normalization to `"PostToolUse"`**.

**SR-01 / SR-07 risk mitigation:** The separation between `extract_response_fields()` (reads `tool_response` object) and `extract_error_field()` (reads `error` string) is explicit and not runtime-detected. The `"PostToolUseFailure"` arm calls only `extract_error_field()`. There is no ambiguity about which extractor runs for which event type.

---

### Component 5: Pre-Post Differential Fix — Two-Site Atomic Update

Addresses SR-08: `metrics.rs` and `friction.rs` independently implement the same differential. Both must be updated in the same commit to prevent divergence.

#### 5a: `PermissionRetriesRule` (`unimatrix-observe/src/detection/friction.rs`)

The rule tracks `pre_counts` and `post_counts` per tool. Fix: rename `post_counts` to `terminal_counts` and count both `"PostToolUse"` and `"PostToolUseFailure"` records as terminal events. The `retries` computation (`pre.saturating_sub(terminal)`) then measures only genuinely cancelled tool calls (Pre with no terminal of any kind). Existing tests must pass unchanged.

#### 5b: `compute_universal()` (`unimatrix-observe/src/metrics.rs`)

The `permission_friction_events` computation sums `(pre - post)` per tool. Fix: count both `hook_type::POSTTOOLUSE` and `hook_type::POSTTOOLUSEFAILURE` in the denominator bucket. The variable currently named `post_counts` should be widened to include failure terminal records.

---

### Component 6: `ToolFailureRule` (`unimatrix-observe/src/detection/friction.rs`)

A new detection rule added alongside `PermissionRetriesRule`.

**Responsibility:** Count `PostToolUseFailure` records per tool. Fire a `HotspotFinding` when a single tool accumulates more than 3 failures within the observation set.

**Specification:**
- Rule name: `"tool_failure_hotspot"`
- Category: `HotspotCategory::Friction`
- Severity: `Severity::Warning`
- Threshold: 3 (constant, not configurable in col-027)
- Claim format: `"Tool '{tool}' failed {n} times"`
- Source domain filter: `"claude-code"` only (consistent with all other friction rules)
- One finding per tool exceeding threshold (not one aggregate finding)

---

## Component Interactions

```
.claude/settings.json
    |
    | (Claude Code fires hook on tool failure)
    v
hook binary (hook.rs) — build_request("PostToolUseFailure", ...)
    |  extract tool_name from extra["tool_name"]
    |  extract topic_signal from extra["tool_input"]
    |  build RecordEvent { event_type: "PostToolUseFailure", payload: input.extra }
    |
    | (fire-and-forget transport, 40ms budget)
    v
listener.rs — extract_observation_fields()
    |  "PostToolUseFailure" arm:
    |    tool    <- payload["tool_name"]
    |    input   <- payload["tool_input"]
    |    snippet <- extract_error_field(payload)  // payload["error"] as string
    |    hook    = "PostToolUseFailure"  (no normalization)
    v
observations table (SQLite, no schema change)
    |
    v
context_retrospective
    |
    |-- friction.rs PermissionRetriesRule: counts PostToolUseFailure as terminal
    |-- friction.rs ToolFailureRule: counts PostToolUseFailure per tool
    |-- metrics.rs compute_universal: counts PostToolUseFailure in denominator
    v
HotspotFinding / MetricVector
```

---

## Technology Decisions

- **String constants over enum** — `hook_type::POSTTOOLUSEFAILURE` follows col-023 ADR-001. No enum change needed. (See ADR-001 col-027.)
- **Separate error extractor** — `extract_error_field()` instead of reusing/patching `extract_response_fields()`. This makes the `error`-vs-`tool_response` distinction explicit at the call site, not runtime-detected. (See ADR-002 col-027.)
- **No normalization** — `PostToolUseFailure` events retain their hook type verbatim in the `hook` column. Detection rules filter by string equality; normalization would hide the signal. (See ADR-003 col-027.)
- **Atomic two-site differential fix** — `metrics.rs` and `friction.rs` must be updated in the same commit. (See ADR-004 col-027.)
- **`terminal_counts` rename** — Renaming the internal variable in `PermissionRetriesRule` from `post_counts` to `terminal_counts` makes the intent explicit without changing the rule's external name or finding category (those are deferred to col-028 per SCOPE). (See ADR-004 col-027.)

---

## Integration Points

| Integration Point | Description |
|-------------------|-------------|
| `.claude/settings.json` | Adds `PostToolUseFailure` hook registration |
| `hook.rs` `build_request()` | New match arm for `"PostToolUseFailure"` |
| `hook.rs` `extract_event_topic_signal()` | New match arm for `"PostToolUseFailure"` |
| `observation.rs` `hook_type` module | New `POSTTOOLUSEFAILURE` constant |
| `listener.rs` `extract_observation_fields()` | New match arm for `"PostToolUseFailure"` |
| `listener.rs` (new) `extract_error_field()` | Reads `payload["error"]` as plain string |
| `friction.rs` `PermissionRetriesRule` | Widen terminal bucket to include `PostToolUseFailure` |
| `friction.rs` (new) `ToolFailureRule` | Count failure records per tool, threshold 3 |
| `metrics.rs` `compute_universal()` | Widen post-bucket in `permission_friction_events` |

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `hook_type::POSTTOOLUSEFAILURE` | `pub const &str = "PostToolUseFailure"` | `unimatrix-core/src/observation.rs` |
| `extract_error_field(payload: &serde_json::Value) -> (Option<i64>, Option<String>)` | Returns `(None, Some(snippet))` where snippet = `payload["error"].as_str()` truncated to 500 chars; returns `(None, None)` if field absent | `listener.rs` (new) |
| `ToolFailureRule` | `impl DetectionRule` — `name() -> "tool_failure_hotspot"`, `category() -> Friction`, `detect(&[ObservationRecord]) -> Vec<HotspotFinding>` | `friction.rs` (new) |
| `ObservationRecord.event_type` for `PostToolUseFailure` records | `"PostToolUseFailure"` (verbatim, not normalized) | stored in `observations.hook` TEXT column |
| `RecordEvent` for `PostToolUseFailure` | `ImplantEvent { event_type: "PostToolUseFailure", payload: input.extra, topic_signal: from tool_input, .. }` | `hook.rs` `build_request()` |

---

## Detection Rule Audit

All 21 detection rules were audited for `event_type` string comparisons. Rules that use the Pre-Post differential (the only category affected by this change):

| Rule / Location | Uses Differential? | Action Required |
|-----------------|-------------------|-----------------|
| `PermissionRetriesRule` — `friction.rs` | Yes | Fix: widen terminal bucket |
| `compute_universal()` — `metrics.rs` | Yes (`permission_friction_events`) | Fix: widen post-bucket |
| All other rules | No | No action — they filter on specific `event_type` strings; `PostToolUseFailure` has a distinct value and will not match `"PostToolUse"` comparisons |

Rules that filter `"PostToolUse"` for non-differential purposes (e.g., search miss rate, context loaded, edit bloat) are NOT affected because `PostToolUseFailure` is a distinct string and those paths will naturally ignore it, which is correct (failures carry no response payload).

---

## Open Questions

None. All resolved questions from SCOPE.md have been incorporated. SR-06 (data quality caveat for pre-col-027 retrospectives) is explicitly out of scope per SCOPE.md non-goals.
