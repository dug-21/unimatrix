# C6 — Tool param (`CycleParams.tags`)

> File: `crates/unimatrix-server/src/mcp/tools.rs` `CycleParams` (~:515-542).
> Additive `tags: Option<Vec<String>>` — declares the interface (AC-06); does NOT itself persist.
> Risks: R-06 (interface), R-09 (backward compat). ACs: AC-06 (interface + authz), FR-1.

## Reuse
Capability test pattern: `infra/registry.rs` `test_require_capability_ok` (:288) /
`test_require_capability_denied` (:301). `context_cycle` is a Write tool — `require_cap(&ctx.agent_id,
Capability::Write)` (tools.rs:992). Capability-classification anchor for the tool set:
`test_tool_u04_capability_used_write_tools` (tools.rs:11814). Test-support cap bypass:
`test_support.rs:305`.

## Unit test expectations
- `test_cycle_params_tags_optional_deserializes` — `CycleParams` JSON with `tags` absent →
  `tags == None` (omission preserves prior wire behavior, FR-1/NFR-4); with `tags:["a","b"]` →
  `Some(vec!["a","b"])`.
- `test_cycle_params_backward_compatible` — a pre-vnc-047 `CycleParams` JSON (no `tags` key)
  deserializes without error (additive `Option`).

## Authorization (AC-06)
- `test_context_cycle_requires_write_cap` — a caller WITHOUT `Capability::Write` is denied
  (`CapabilityDenied`); a caller WITH Write succeeds. Single Write gate, no `Capability::Tag`.
- `test_agent_id_does_not_authorize_or_scope` — a Write-capable caller can tag any `feature_cycle`
  regardless of `agent_id`; `agent_id` is audit-only and does NOT gate or scope the write (parity
  vnc-045 SD-9). NB: no per-tag audit event is emitted (unlike vnc-045's entry path) — assert none.

## No new tool (AC-06)
- `test_no_new_mcp_tool_added` — assert the Write-tool capability-classification set
  (`test_tool_u04_capability_used_write_tools` anchor) is unchanged (still `context_store,
  context_correct, context_deprecate, context_cycle`); no `context_cycle_tag` / new tool appears.
  (There is no full tool-count harness; this classification anchor is the nearest — see OVERVIEW §7.)

## Constraint
`CycleParams.tags` exists ONLY to declare the interface. A test must confirm the persisted value is
read from `tool_input["tags"]` by the hook (C4), NOT from `CycleParams` in the handler (R-06,
AC-EXTRA-1 — proven in listener-persistence.md).
