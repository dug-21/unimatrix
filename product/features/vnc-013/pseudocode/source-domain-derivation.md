# vnc-013 Pseudocode: source-domain-derivation
## Files: `listener.rs` (Site A) + `background.rs` (Site B) + `services/observation.rs` (Site C)

---

## Purpose

Replace the three `"claude-code"` hardcodes with dynamic derivation. Each site has
a different mechanism: Site A has `ImplantEvent.provider` available directly; Sites B
and C are DB read paths with only the event_type string from the DB.

After this change, the string literal `"claude-code"` must not appear as a hardcoded
`source_domain` assignment in any of these three files.

---

## Site A: `listener.rs` line ~1894 (write path)

### Location

Inside the `content_based_attribution_fallback()` function, within the row-mapping
closure that builds `ObservationRecord` from SQL results.

Current code (line 1891-1894):
```rust
ObservationRecord {
    ts: (ts_millis / 1000) as u64,
    event_type: hook_str,
    source_domain: "claude-code".to_string(),  // ← REPLACE
    session_id,
    tool,
    input: input_str.map(serde_json::Value::String),
    response_size: None,
    response_snippet: None,
}
```

### Replacement

This code path reads from the DB (SELECT session_id, ts_millis, hook, tool, input FROM
observations). There is no ImplantEvent here — we do not have provider information.
This is a DB read path, NOT the live write path.

Wait — this is `content_based_attribution_fallback()` in `listener.rs`, which is a DB
read path called during session close. This is DIFFERENT from the live write path.

The live write path (the actual Site A) is in `dispatch_request()` → `RecordEvent` arm
→ `extract_observation_fields()` + `insert_observation()`. But `source_domain` is not
a column in the DB — it is derived at read time (Sites B and C) or at write time only
for the `ObservationRecord` struct used by the observation pipeline.

Let me re-examine. The `ObservationRecord` struct is used by:
1. `listener.rs content_based_attribution_fallback()` — DB read path, builds records
   from SQL for session attribution. source_domain here affects only attribution logic.
2. `background.rs fetch_observation_batch()` — DB read path for extraction pipeline.
3. `services/observation.rs parse_observation_rows()` — DB read path for cycle review.

The "live write path" Site A from ARCHITECTURE.md refers to line 1894 which is in
`content_based_attribution_fallback()`. Despite being in `listener.rs`, this is a
DB read path. However, ARCHITECTURE.md says "Site A: listener.rs:1894 — live write
path. Has ImplantEvent.provider directly."

This appears to be a mis-labeling in the architecture doc. Let me check whether line
1894 in listener.rs (as read from the source at offset 1880) corresponds to the
content_based_attribution_fallback or to the dispatch_request write path.

Reading the source: at offset 1870-1900, the code is inside `content_based_attribution_fallback()`.
This is NOT the live dispatch write path — it is used for session attribution on close,
not for persisting new observations. The `dispatch_request()` write path calls
`insert_observation()` with an `ObservationRow` (not `ObservationRecord`) struct.

ARCHITECTURE.md's "Site A: listener.rs:1894 — live write path, Has ImplantEvent.provider
directly" is correct in spirit: this is the closest to the write path in listener.rs.
However, the `ImplantEvent.provider` is NOT available at this specific code location
(content_based_attribution_fallback reads from DB, not from ImplantEvent directly).

Resolution: Apply Approach A (registry-with-fallback) to Site A as well, since it is
a DB read path. The ARCHITECTURE.md description "Has ImplantEvent.provider directly" 
appears to refer to a DIFFERENT location — possibly where `extract_observation_fields()` 
is called in dispatch_request, where `event.provider` is available.

HOWEVER: The spec says `source_domain` is derived at write time from `ImplantEvent.provider`
for Site A. This means there is ANOTHER location where the source_domain is set — likely
the `ObservationRow` struct or `insert_observation()` call in `dispatch_request()`.

Let me re-read the architecture more carefully. ARCHITECTURE.md Layer 3 Site A:
"**Site A: `listener.rs:1894`** — live write path. Has `ImplantEvent.provider` directly:
```rust
source_domain: event.provider.clone().unwrap_or_else(|| "claude-code".to_string()),
```"

The `ObservationRow` struct (used by `insert_observation()` and `extract_observation_fields()`)
likely has a `source_domain` field that is populated at line 1894. The offset we read
was the `content_based_attribution_fallback` function which uses `ObservationRecord`
(a different struct used by the observation pipeline). Line 1894 of listener.rs is
approximately at the same location as the ObservationRecord construction we saw.

For implementation: there are TWO locations where source_domain is set in listener.rs:
a) `content_based_attribution_fallback()` builds `ObservationRecord` with `source_domain`
b) `extract_observation_fields()` or `insert_observation()` builds `ObservationRow`

The implementer must locate BOTH and apply the correct derivation to each:
- Where `ImplantEvent` is available: use `event.provider.clone().unwrap_or_else(|| "claude-code".to_string())`
- Where only event_type from DB is available: use Approach A registry-with-fallback

### Site A Pseudocode (where ImplantEvent is available in dispatch path)

Find: any location in `dispatch_request()` where `source_domain` is set from a
hardcoded `"claude-code"`. Replace with:

```
// Source domain derived from ImplantEvent.provider (ADR-002, FR-06.1).
// This is the only site that correctly labels live Gemini events as "gemini-cli".
// Fallback to "claude-code" for events processed before vnc-013 normalization
// or when --provider was not supplied.
let source_domain = event.provider.clone()
    .unwrap_or_else(|| "claude-code".to_string());
```

### Site A Pseudocode (content_based_attribution_fallback — DB read path)

Apply Approach A:

```
// content_based_attribution_fallback() reads from DB; ImplantEvent not available.
// Apply registry-with-fallback (Approach A) same as Sites B and C.
// import DEFAULT_HOOK_SOURCE_DOMAIN from services::observation.
let resolved = registry.resolve_source_domain(&hook_str);
let source_domain = if resolved != "unknown" {
    resolved
} else {
    DEFAULT_HOOK_SOURCE_DOMAIN.to_string()
};
```

Note: `content_based_attribution_fallback()` does not currently have `registry`
available as a parameter. If the registry is not accessible from this function's
call context, the fallback `"claude-code"` can be used directly (Approach A with
the understanding that the registry call is a no-op shortcut here). The implementer
must check whether registry is accessible and add it as a parameter if needed, or
use the constant directly as a simplified Approach A.

### debug_assert guard in extract_observation_fields()

Add BEFORE the `match hook.as_str()` block in `extract_observation_fields()`:

```
// Normalization contract enforcement (AC-16, ADR-005, FR-08.1).
// "post_tool_use_rework_candidate" is an internal routing label that must be
// converted to "PostToolUse" in build_request() before reaching the DB write path.
// If this assert fires in debug builds, normalization failed or was bypassed.
// Scoped to rework candidate only — PostToolUseFailure is intentionally preserved (ADR-003 col-027).
// Compiled out in release builds (debug_assert vs assert).
debug_assert!(
    hook != "post_tool_use_rework_candidate",
    "rework candidate string escaped normalization boundary and reached extract_observation_fields: {hook}"
);
```

Placement: after `let hook = event.event_type.clone();` line, before the `match hook.as_str()` block.

Note: this guard fires for ANY path — not just Gemini. It enforces the invariant
that `post_tool_use_rework_candidate` is always converted to `PostToolUse` by the
match arm at line 2753 (the existing normalization in extract_observation_fields).
The guard is a belt-and-suspenders check that the event never reaches the DB hook
column with the internal label. The match arm at line 2753 is the real enforcement.

---

## Site B: `background.rs` line ~1330 (`fetch_observation_batch()`)

### Current code
```rust
// All hook-path records get source_domain = "claude-code" (FR-03.3).
let source_domain = "claude-code".to_string();
```

### Replacement (Approach A)

```rust
// Approach A: derive source_domain from DomainPackRegistry (ADR-004, FR-06.2).
// The builtin claude-code pack lists: PreToolUse, PostToolUse, PostToolUseFailure, SubagentStart.
// Events not in the pack (Stop, SessionStart, cycle_start, cycle_stop, UserPromptSubmit,
// PreCompact, SubagentStop) return "unknown" → fallback to DEFAULT_HOOK_SOURCE_DOMAIN.
// This preserves the existing "claude-code" behavior for all non-listed events.
let source_domain = {
    let resolved = registry.resolve_source_domain(&event_type);
    if resolved != "unknown" {
        resolved
    } else {
        DEFAULT_HOOK_SOURCE_DOMAIN.to_string()
    }
};
```

`registry` is of type `&DomainPackRegistry`. Verify it is available in
`fetch_observation_batch()`. If not, add it as a parameter at the call site.
`DEFAULT_HOOK_SOURCE_DOMAIN` is imported from `crate::services::observation`.

The comment `// All hook-path records get source_domain = "claude-code" (FR-03.3)` is
removed. The `FR-03.3` reference is to the old specification — the new contract is
FR-06.2 (SPECIFICATION.md).

---

## Site C: `services/observation.rs` line ~585 (`parse_observation_rows()`)

### Current code
```rust
// All hook-path records get source_domain = "claude-code" (FR-03.3).
// Domain is inferred from the ingress path, not from event_type.
let source_domain: String = "claude-code".to_string();
```

Note: the function signature currently has `_registry: &DomainPackRegistry`
(underscore prefix suppresses unused-variable warning). The registry is already a
parameter but unused.

### Changes

1. Remove underscore from parameter: `_registry` → `registry`
2. Define `DEFAULT_HOOK_SOURCE_DOMAIN` constant in this file (above the function)
3. Replace hardcode with Approach A pattern

```rust
// Constant: fallback source_domain for DB read paths (Approach A, ADR-004, FR-06.3).
pub(crate) const DEFAULT_HOOK_SOURCE_DOMAIN: &str = "claude-code";
```

```rust
// Approach A: derive source_domain from DomainPackRegistry.
// See DEFAULT_HOOK_SOURCE_DOMAIN for the fallback contract.
let source_domain: String = {
    let resolved = registry.resolve_source_domain(&event_type);
    if resolved != "unknown" {
        resolved
    } else {
        DEFAULT_HOOK_SOURCE_DOMAIN.to_string()
    }
};
```

The two old comment lines are removed. The new comment references the correct FR/ADR.

### Test comment update (FR-07.2)

Find `test_parse_rows_unknown_event_type_passthrough`. Update the comment inside:

Old comment (approximate): "// All hook-path records get source_domain = 'claude-code' (FR-03.3)"

New comment:
```rust
// Approach A fallback contract (FR-06.4, ADR-004):
// registry.resolve_source_domain("UnknownEventType") returns "unknown" (not in builtin pack).
// DEFAULT_HOOK_SOURCE_DOMAIN ("claude-code") is used as fallback.
// This test is the primary regression canary for Approach A fallback correctness (R-04).
// If this test fails, Approach A fallback was removed or inverted.
//
// Additional assertion: DEFAULT_HOOK_SOURCE_DOMAIN value is visible in test record.
assert_eq!(DEFAULT_HOOK_SOURCE_DOMAIN, "claude-code");
```

The existing assertion `assert_eq!(source_domain, "claude-code")` remains valid and
must not be changed — the Approach A fallback restores exactly `"claude-code"` for
unknown event types.

---

## Initialization Sequence

No initialization needed. `DEFAULT_HOOK_SOURCE_DOMAIN` is a `const &str` — zero-cost,
available at compile time.

`DomainPackRegistry` is already initialized at server startup. No changes to its
initialization or its `builtin_claude_code_pack()` event_types list.

---

## Error Handling

`resolve_source_domain()` returns `&str` — it cannot fail. The fallback pattern is
a simple string comparison (`!= "unknown"`). No error propagation needed.

---

## Key Test Scenarios

### Site A — AC-06, R-02

```
// test_write_path_source_domain_gemini (AC-06):
// Inject ImplantEvent { provider: Some("gemini-cli"), ... } through dispatch_request().
// Read back the ObservationRecord/Row from DB.
// Assert source_domain == "gemini-cli".

// test_write_path_source_domain_claude_code (AC-06):
// Inject ImplantEvent { provider: Some("claude-code"), ... }.
// Assert source_domain == "claude-code".

// test_provider_none_falls_back_to_claude_code (R-02/SC-3):
// Inject ImplantEvent { provider: None, ... } (pre-vnc-013 wire frame).
// Assert source_domain == "claude-code" (unwrap_or_else fallback).
```

### Site A debug_assert — AC-16, R-07

```
// test_rework_candidate_guard_fires_in_debug (AC-16, #[cfg(debug_assertions)]):
// Create ImplantEvent { event_type: "post_tool_use_rework_candidate", ... }.
// Call extract_observation_fields() directly.
// Use #[should_panic] OR std::panic::catch_unwind to assert the debug_assert fires.
// Note: this test ONLY runs in debug builds (cfg(debug_assertions)). In release
// builds, the guard is compiled out and the existing match arm normalizes it.

// test_post_tool_use_failure_arm_unchanged (AC-16, R-07/SC-3):
// Create ImplantEvent { event_type: "PostToolUseFailure", ... }.
// Call extract_observation_fields().
// Assert obs.hook == "PostToolUseFailure" (not "PostToolUse").
// Verifies guard scope is correct: PostToolUseFailure is untouched.
```

### Sites B and C — AC-07(b), AC-07(c), R-04

```
// test_approach_a_listed_event_resolves_directly (AC-07b/c):
// Call parse_observation_rows() / fetch_observation_batch() with event_type = "PreToolUse".
// resolve_source_domain("PreToolUse") returns "claude-code" (in builtin pack).
// Assert source_domain == "claude-code". (No fallback needed — direct resolution.)

// test_approach_a_fallback_for_stop_event (R-04/SC-2):
// event_type = "Stop". resolve_source_domain("Stop") returns "unknown".
// Assert source_domain == DEFAULT_HOOK_SOURCE_DOMAIN == "claude-code".

// test_approach_a_fallback_for_cycle_events (R-04/SC-3):
// event_type = "cycle_start". resolve_source_domain("cycle_start") returns "unknown".
// Assert source_domain == "claude-code".
// Repeat for "cycle_stop".

// test_parse_rows_unknown_event_type_passthrough (existing — comment updated, R-06):
// event_type = "UnknownEventType". Assert source_domain == "claude-code".
// Assert DEFAULT_HOOK_SOURCE_DOMAIN == "claude-code".

// test_parse_rows_hook_path_always_claude_code (existing — unchanged, FR-07.1):
// event_type = "PreToolUse". Assert source_domain == "claude-code".
// Valid under Approach A because "PreToolUse" IS in the builtin pack.

// test_registry_prefix_removed (AC-07c):
// Call parse_observation_rows(rows, registry) — assert the registry IS used
// (not just the constant). Verifiable by using a test registry that returns
// a non-"unknown" value for a test event type and asserting that value appears.
```

### Cross-crate wire boundary — integration

```
// test_implant_event_provider_none_across_uds_boundary (R-02):
// Serialize ImplantEvent { provider: None }.
// Assert JSON does not contain "provider" key (skip_serializing_if).
// Deserialize the JSON into ImplantEvent.
// Assert provider == None (serde(default) handles missing key).
// Site A: unwrap_or_else("claude-code") → source_domain == "claude-code".
// This documents the known fallback for pre-vnc-013 wire frames.
```

---

## Known Limitation (Accepted)

After normalization, Gemini `BeforeTool` records are stored as canonical `"PreToolUse"`
in the DB. Sites B and C will return `source_domain = "claude-code"` for these records
when read back, because `resolve_source_domain("PreToolUse")` returns `"claude-code"` —
the registry cannot distinguish the origin.

Only Site A (the write path, where `ImplantEvent.provider` is available) correctly
labels live Gemini events as `"gemini-cli"`. This limitation is accepted (OQ-4,
IMPLEMENTATION-BRIEF Known Limitation section) and documented in the test comment
for `test_approach_a_listed_event_resolves_directly`.
