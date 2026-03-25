# col-027 Pseudocode Overview: PostToolUseFailure Hook Support

## Problem Summary

`PostToolUseFailure` has never been registered. When a Claude Code tool call fails, the hook fires
but no binary is invoked, producing a `PreToolUse` observation with no terminal record. This inflates
the Pre-Post differential in `PermissionRetriesRule` (friction.rs) and `permission_friction_events`
(metrics.rs), generating false friction findings in every retrospective since nan-002.

---

## Components Involved

| Component | File | Wave | Action |
|-----------|------|------|--------|
| core-constants | `unimatrix-core/src/observation.rs` | 1 | Add `POSTTOOLUSEFAILURE` constant + update doc comment |
| hook-registration | `.claude/settings.json` | 1 | Add `PostToolUseFailure` hook entry |
| hook-dispatcher | `unimatrix-server/src/uds/hook.rs` | 2 | Add explicit match arms in `build_request()` and `extract_event_topic_signal()` |
| observation-storage | `unimatrix-server/src/uds/listener.rs` | 2 | Add `PostToolUseFailure` arm in `extract_observation_fields()` + new `extract_error_field()` |
| friction-metrics | `unimatrix-observe/src/detection/friction.rs` + `detection/mod.rs` + `metrics.rs` | 2 | Fix `PermissionRetriesRule`, add `ToolFailureRule`, fix `compute_universal()` — all atomic |

---

## Data Flow

```
Claude Code (tool fails)
  --> fires PostToolUseFailure with payload:
        { tool_name: String, tool_input: Object, error: String, is_interrupt?: bool }
  --> stdin pipe to `unimatrix hook PostToolUseFailure` binary

hook.rs build_request("PostToolUseFailure", input):
  --> extracts tool_name from input.extra["tool_name"]
  --> calls extract_event_topic_signal("PostToolUseFailure", input)
        which reads input.extra["tool_input"]
  --> returns HookRequest::RecordEvent {
        event: ImplantEvent {
          event_type: "PostToolUseFailure",   -- verbatim, no normalization
          session_id: ...,
          timestamp: now_secs(),
          payload: input.extra.clone(),        -- carries tool_name, error, tool_input
          topic_signal: from tool_input,
        }
      }
  --> fire-and-forget UDS transport (40ms budget, unchanged path)

listener.rs extract_observation_fields("PostToolUseFailure" arm):
  --> tool    <- payload["tool_name"].as_str()
  --> input   <- payload["tool_input"] serialized
  --> (_, snippet) <- extract_error_field(payload)   -- reads payload["error"] as plain string
  --> response_size = None (intentional, error strings are small)
  --> hook = "PostToolUseFailure"  (NOT normalized)
  --> stored in observations table

observations table (no schema change, hook TEXT column accepts new value)

context_retrospective triggers detection:
  --> friction.rs PermissionRetriesRule::detect():
        PostToolUseFailure counted in terminal_counts (alongside PostToolUse)
        pre.saturating_sub(terminal) -- retries = 0 for fully-terminal sessions
  --> friction.rs ToolFailureRule::detect():
        Counts PostToolUseFailure per tool (source_domain == "claude-code")
        Fires HotspotFinding when count > TOOL_FAILURE_THRESHOLD (3)
  --> metrics.rs compute_universal():
        PostToolUseFailure counted in terminal bucket for permission_friction_events
        sum of pre.saturating_sub(post + failure) per tool
```

---

## Shared Types (unchanged structs, new constant)

### New Constant (core-constants component)

```
hook_type::POSTTOOLUSEFAILURE: &str = "PostToolUseFailure"
```

Defined in `unimatrix-core/src/observation.rs` `hook_type` module. Used by:
- friction.rs `PermissionRetriesRule` — to widen terminal bucket
- friction.rs `ToolFailureRule` — to filter records by event_type
- metrics.rs `compute_universal()` — to widen terminal bucket

### New Function (observation-storage component)

```
fn extract_error_field(payload: &serde_json::Value) -> (Option<i64>, Option<String>)
```

Defined in `unimatrix-server/src/uds/listener.rs`. Sibling to `extract_response_fields()`.
- Always returns `None` for the first tuple element (response_size)
- Returns `Some(snippet)` where snippet = `payload["error"].as_str()` truncated to 500 chars
  via `truncate_at_utf8_boundary(s, 500)`
- Returns `(None, None)` if `payload["error"]` is absent, null, or non-string

### New Struct (friction-metrics component)

```
pub struct ToolFailureRule;
```

Defined in `unimatrix-observe/src/detection/friction.rs`. Implements `DetectionRule`.

### Internal Rename (friction-metrics component)

`PermissionRetriesRule::detect()`: `post_counts: HashMap<String, u64>` renamed to
`terminal_counts: HashMap<String, u64>`. Semantics widen to include both PostToolUse and
PostToolUseFailure records. External rule name, category, severity, and claim text unchanged.

---

## Integration Surface (from ARCHITECTURE.md)

| Point | Type/Signature | Owner |
|-------|---------------|-------|
| `hook_type::POSTTOOLUSEFAILURE` | `pub const &str = "PostToolUseFailure"` | core-constants |
| `extract_error_field(payload: &Value) -> (Option<i64>, Option<String>)` | new fn in listener.rs | observation-storage |
| `ToolFailureRule` — `impl DetectionRule` | `name() -> "tool_failure_hotspot"`, `detect(&[ObservationRecord]) -> Vec<HotspotFinding>` | friction-metrics |
| `RecordEvent` for PostToolUseFailure | `ImplantEvent { event_type: "PostToolUseFailure", payload: input.extra, topic_signal }` | hook-dispatcher |
| Stored `hook` column value | `"PostToolUseFailure"` verbatim, not normalized | observation-storage |

---

## Wave Dependency Graph

```
Wave 1 (no dependencies):
  core-constants    -- observation.rs: new POSTTOOLUSEFAILURE constant
  hook-registration -- settings.json: new PostToolUseFailure entry
  (these two are independent of each other)

Wave 2 (depends on Wave 1):
  hook-dispatcher      -- uses hook_type::POSTTOOLUSEFAILURE from core-constants
  observation-storage  -- uses hook_type::POSTTOOLUSEFAILURE; adds extract_error_field()
  friction-metrics     -- uses hook_type::POSTTOOLUSEFAILURE from core-constants
                       -- friction.rs + mod.rs + metrics.rs MUST ship as one atomic commit
                       -- (ADR-004: two-site differential fix)
```

### Atomicity Constraint (friction-metrics)

`friction.rs` (PermissionRetriesRule fix + ToolFailureRule) and `metrics.rs`
(`compute_universal()` fix) and `mod.rs` (`default_rules()` registration) must be delivered
in the same commit. A partial fix causes `permission_friction_events` and `PermissionRetriesRule`
to diverge — reporting contradictory signals from the same observation data.

---

## Key Constraints Summary

1. `extract_error_field()` is a NEW sibling to `extract_response_fields()` — never reuse/modify the
   latter for failure payloads (ADR-002). Wrong function = silent `(None, None)`.
2. `build_request()` must have an explicit `"PostToolUseFailure"` arm — not wildcard fallthrough
   (ADR-001, FR-03.1). Wildcard stores records with `tool = None`.
3. `hook = "PostToolUseFailure"` stored verbatim — no normalization to `"PostToolUse"` (ADR-003).
4. `ToolFailureRule` threshold is strictly greater than 3 (`count > 3`, fires at 4+) (ADR-005).
5. `terminal_counts` rename in `PermissionRetriesRule` is internal only — rule_name, category,
   severity, claim text unchanged (ADR-004, FR-05.4).
6. `response_size = None` for all PostToolUseFailure records (ADR-002, FR-04.4).
7. `make_failure` test helper follows `make_pre`/`make_post` pattern (NFR-04).
8. No schema migration required (C-01).
9. Fire-and-forget `RecordEvent` path — 40ms HOOK_TIMEOUT must not be exceeded (NFR-01).
10. Hook always exits 0 — all field accesses use defensive Option chaining (FR-03.6, C-04).
