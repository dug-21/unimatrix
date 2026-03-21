# Pseudocode: observation-record

**Wave**: 1 (Foundation — everything depends on this)
**Crate**: `unimatrix-core`
**File**: `crates/unimatrix-core/src/observation.rs`

## Purpose

Replace `ObservationRecord.hook: HookType` (a closed 4-variant enum) with two string
fields: `event_type: String` and `source_domain: String`. Retain `HookType` as a
`pub mod hook_type` constants module for documentation. This is the only change to
`unimatrix-core` in this feature.

All other crates consume this type. Wave 1 breakage is expected and intentional — the
downstream crates compile again only after their own wave updates.

## Modified Types

### ObservationRecord (struct replacement)

Current:
```
struct ObservationRecord:
    ts: u64
    hook: HookType          -- DELETE THIS FIELD
    session_id: String
    tool: Option<String>
    input: Option<serde_json::Value>
    response_size: Option<u64>
    response_snippet: Option<String>
```

After Wave 1:
```
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ObservationRecord:
    ts: u64
    event_type: String       -- NEW: replaces hook: HookType
    source_domain: String    -- NEW: domain origin, set server-side
    session_id: String
    tool: Option<String>     -- unchanged
    input: Option<serde_json::Value>    -- unchanged
    response_size: Option<u64>          -- unchanged
    response_snippet: Option<String>    -- unchanged
```

No serde rename attributes are needed. The DB column is still named `hook` (TEXT),
but the Rust struct field is `event_type`. The mapping is done in
`parse_observation_rows` in `unimatrix-server`, not here.

`ParsedSession` and `ObservationStats` require no structural changes beyond what is
implied by the field rename above (FR-01.4).

### HookType deprecation

The `HookType` enum is removed as a type. In its place, introduce a constants module:

```
/// Well-known event type strings for the "claude-code" domain pack.
/// These are string constants for documentation only.
/// Use `event_type: String` and `source_domain: String` in all hot paths.
pub mod hook_type {
    pub const PRETOOLUSE: &str = "PreToolUse";
    pub const POSTTOOLUSE: &str = "PostToolUse";
    pub const SUBAGENTSTART: &str = "SubagentStart";
    pub const SUBAGENTSTOPPED: &str = "SubagentStop";
}
```

The `HookType` enum type is deleted entirely. No re-export, no alias, no deprecated
attribute. Downstream callers that imported `HookType` will fail to compile — that is
expected and handled in Waves 2-4.

## Field Mapping Reference (for implementors in later waves)

When converting old code that matched on `HookType` variants, use these equivalences:

```
Old: r.hook == HookType::PreToolUse
New: r.event_type == "PreToolUse" && r.source_domain == "claude-code"

Old: r.hook == HookType::PostToolUse
New: r.event_type == "PostToolUse" && r.source_domain == "claude-code"

Old: r.hook == HookType::SubagentStart
New: r.event_type == "SubagentStart" && r.source_domain == "claude-code"

Old: r.hook == HookType::SubagentStop
New: r.event_type == "SubagentStop" && r.source_domain == "claude-code"
```

Note: the `source_domain` guard is mandatory, not optional. Rules without it will
receive mixed-domain records after Wave 4 changes to `parse_observation_rows`.

## Initialization / Construction

No constructor function needed. The struct is constructed with named fields at all
callsites. Callers in later waves are responsible for supplying both `event_type` and
`source_domain`.

Test fixture helper pattern (for Wave 2-4 test updates):
```
fn make_pre(ts: u64, tool: &str) -> ObservationRecord:
    ObservationRecord {
        ts,
        event_type: "PreToolUse".to_string(),
        source_domain: "claude-code".to_string(),
        session_id: "sess-1".to_string(),
        tool: Some(tool.to_string()),
        input: None,
        response_size: None,
        response_snippet: None,
    }

fn make_post(ts: u64, tool: &str) -> ObservationRecord:
    ObservationRecord {
        ts,
        event_type: "PostToolUse".to_string(),
        source_domain: "claude-code".to_string(),
        session_id: "sess-1".to_string(),
        tool: Some(tool.to_string()),
        input: None,
        response_size: None,
        response_snippet: None,
    }

fn make_subagent_start(ts: u64, session: &str, agent_type: &str) -> ObservationRecord:
    ObservationRecord {
        ts,
        event_type: "SubagentStart".to_string(),
        source_domain: "claude-code".to_string(),
        session_id: session.to_string(),
        tool: Some(agent_type.to_string()),
        input: None,
        response_size: None,
        response_snippet: None,
    }

fn make_subagent_stop(ts: u64, session: &str) -> ObservationRecord:
    ObservationRecord {
        ts,
        event_type: "SubagentStop".to_string(),
        source_domain: "claude-code".to_string(),
        session_id: session.to_string(),
        tool: None,
        input: None,
        response_size: None,
        response_snippet: None,
    }
```

These fixture helpers replace the current `make_pre`/`make_post` etc. in the existing
test modules in `detection/mod.rs`, `detection/agent.rs`, `detection/friction.rs`, etc.
They must all supply a non-empty `source_domain` to avoid false-green tests (R-03).

## Error Handling

No errors in this module. The struct is a pure data type with no fallible operations.
Serde derives handle serialization; missing fields produce a compile error at callsites.

## Key Test Scenarios

1. **Struct fields present**: `cargo check -p unimatrix-core` passes with new fields.
   No compilation errors within `unimatrix-core` itself.

2. **HookType enum absent**: `grep -r "HookType::" crates/unimatrix-core/` returns
   zero matches after Wave 1.

3. **hook_type constants accessible**: `use unimatrix_core::observation::hook_type;`
   compiles and `hook_type::PRETOOLUSE == "PreToolUse"` holds.

4. **Serde round-trip**: `ObservationRecord` serializes to and deserializes from JSON
   with both `event_type` and `source_domain` fields present.

5. **R-13 guard**: No code in `unimatrix-core` uses `hook_type` constants in a match
   expression or type position — they are `&str` constants, not enum variants.
