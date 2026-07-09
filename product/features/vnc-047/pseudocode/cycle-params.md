# C6 — Tool param `CycleParams.tags`

**File:** `crates/unimatrix-server/src/mcp/tools.rs` `CycleParams` (:516-542)
**ADR:** ADR-002. **AC:** AC-06. **Risks:** R-06 (declares interface but is NOT the persist source).

## Purpose

Declare the additive `tags` param on `context_cycle` so the MCP tool schema advertises it (AC-06).
This field EXISTS TO DECLARE THE INTERFACE ONLY — the persisted value is read by the hook from
`tool_input["tags"]` (C4), never from `CycleParams` in the handler (SR-03/R-06). Omission preserves
exact prior wire behavior (`Option`-typed).

## Pseudocode

```
# In struct CycleParams (after `goal` :537, beside it), add:

    /// Optional opaque run-identity labels for the feature cycle (vnc-047).
    ///
    /// Only meaningful for type="start"; ignored for "phase-end" and "stop".
    /// Set-once whole-set: the first tag-bearing start freezes the entire set; later
    /// starts are a whole-set no-op. Values are opaque — stored verbatim, non-empty is
    /// the only check (no vocabulary/length/prefix validation). Old callers omitting this
    /// field receive None. Persistence rides the hook path (not this handler).
    pub tags: Option<Vec<String>>,
```

- Same derive stack as the struct (`#[derive(Debug, Deserialize, JsonSchema)]`) — no per-field attr
  needed; `Option` already makes it omittable.
- Do NOT wire this field into any persistence in the handler body. It is consumed only by the
  best-effort ack echo (C12), which echoes the caller's own input.

## Error handling

None — additive optional field. Deserialization of an old call (no `tags`) yields `None`.

## Key test scenarios (hints)

1. Handler-registry / schema test: `context_cycle` advertises `tags`; no NEW tool registered (AC-06).
2. Old call omitting `tags` deserializes to `None`; behavior unchanged (NFR-4).
3. (R-06) A test confirms the *stored* value comes from `tool_input["tags"]` via the hook, NOT from
   `CycleParams.tags` in the bare handler (the bare handler persists nothing).
