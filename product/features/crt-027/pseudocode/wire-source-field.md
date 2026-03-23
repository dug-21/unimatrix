# Wire Source Field — Pseudocode
# File: crates/unimatrix-engine/src/wire.rs

## Purpose

Adds an optional `source: Option<String>` field to `HookRequest::ContextSearch` with
`#[serde(default)]`. This is the only change to `wire.rs` in crt-027.

The field allows the server to distinguish the originating hook event type (SubagentStart
vs UserPromptSubmit) when recording observations. The default is backward-compatible: all
existing callers that omit `source` deserialize to `None`, which the server treats as
`"UserPromptSubmit"`. See ADR-001.

---

## Modified Type

### `HookRequest::ContextSearch` variant (in `HookRequest` enum)

**Before** (current):
```
ContextSearch {
    query: String,
    #[serde(default)]
    session_id: Option<String>,
    role: Option<String>,
    task: Option<String>,
    feature: Option<String>,
    k: Option<u32>,
    max_tokens: Option<u32>,
}
```

**After** (add exactly one field):
```
ContextSearch {
    query: String,
    #[serde(default)]
    session_id: Option<String>,
    role: Option<String>,
    task: Option<String>,
    feature: Option<String>,
    k: Option<u32>,
    max_tokens: Option<u32>,
    #[serde(default)]
    source: Option<String>,   // NEW — ADR-001 crt-027
                              // None → treated as "UserPromptSubmit" by dispatch_request
                              // Some("SubagentStart") → set by hook.rs SubagentStart arm
}
```

No other changes to `wire.rs`. `HookRequest::Briefing` is NOT touched (C-04).

---

## Invariants

1. `#[serde(default)]` on `source` ensures `None` when the JSON key is absent.
2. Serialization of `source: None` omits the key (standard serde behavior for `Option`).
3. Serialization of `source: Some("SubagentStart")` writes `"source": "SubagentStart"`.
4. Round-trip: `ContextSearch { source: None, ... }` → serialize → deserialize → `source == None`.
5. All existing struct literal constructions of `ContextSearch` in tests and production code
   must add `source: None` (or use `..Default::default()` spread if available) or they will
   fail to compile. This is intentional — compile error makes the gap visible (ADR-001).

---

## Error Handling

None. This is a pure data type change. No logic, no error paths.

---

## Key Test Scenarios

Implemented in `wire.rs` `#[cfg(test)]` block:

**T-W-01** `source_field_default_on_absent_key`:
- Input JSON: `{"type": "ContextSearch", "query": "test"}`
- Assert: deserialize succeeds, `source == None`

**T-W-02** `source_field_explicit_subagentstart`:
- Input JSON: `{"type": "ContextSearch", "query": "test", "source": "SubagentStart"}`
- Assert: `source == Some("SubagentStart")`

**T-W-03** `source_field_roundtrip_none`:
- Construct: `HookRequest::ContextSearch { query: "test".into(), session_id: None, role: None, task: None, feature: None, k: None, max_tokens: None, source: None }`
- Serialize to JSON
- Deserialize from JSON
- Assert: `source == None` (not an error)

**T-W-04** `existing_struct_literal_compiles_with_source_none`:
- This is a compile-time check: all existing `HookRequest::ContextSearch { ... }` struct
  literals in the codebase must add `source: None`. Verified by `cargo build --release`
  succeeding without `non_exhaustive` or `missing_field` errors.

Note: If `HookRequest` derives `#[non_exhaustive]` or if struct literal patterns are used
in tests outside `wire.rs`, those tests must be updated. Grep for `ContextSearch {` in
`hook.rs` tests and `listener.rs` tests — all instances need `source: None` added.
