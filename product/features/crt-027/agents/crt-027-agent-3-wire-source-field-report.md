# Agent Report: crt-027-agent-3-wire-source-field

**Component**: `wire.rs` source field extension
**Feature**: crt-027 WA-4 Proactive Knowledge Delivery
**Date**: 2026-03-23

## Summary

Implemented `#[serde(default, skip_serializing_if = "Option::is_none")] source: Option<String>` on `HookRequest::ContextSearch` per ADR-001 crt-027. Updated all struct-literal constructions across the codebase to include `source: None`. Added 6 unit tests per the component test plan.

## Files Modified

- `/workspaces/unimatrix/crates/unimatrix-engine/src/wire.rs` — field added, 2 existing test struct literals updated, 6 new tests added
- `/workspaces/unimatrix/crates/unimatrix-server/src/uds/listener.rs` — 1 production pattern match updated (`source: _`), 10 test struct literals updated (`source: None`)
- `/workspaces/unimatrix/crates/unimatrix-server/src/uds/hook.rs` — 1 production struct literal updated (UserPromptSubmit arm), 1 test struct literal updated

## Tests

**unimatrix-engine**: 297 passed, 0 failed (net +6 new tests)

New tests added:
- `wire_context_search_source_absent_deserializes_to_none` — PASS
- `wire_context_search_source_present_deserializes_to_value` — PASS
- `wire_context_search_source_none_serializes_without_field` — PASS
- `context_search_source_none_round_trip` — PASS
- `context_search_source_subagentstart_round_trip` — PASS
- `hook_request_briefing_variant_still_present` — PASS

## Deviations from Pseudocode

One deviation from the pseudocode invariant 2: "Serialization of `source: None` omits the key (standard serde behavior for `Option`)" — this is incorrect. Standard serde behavior for `Option` serializes `None` as `null`. Omission requires `skip_serializing_if = "Option::is_none"`. Added this attribute to make the test plan's assertion hold. This is the correct behavior per ADR-001 ("source: None omits the key") and the test plan. The pseudocode's characterization of "standard serde behavior" was imprecise.

## Issues

`unimatrix-server` has a pre-existing compile error (`unresolved imports Briefing, format_briefing` in `mcp/tools.rs`) caused by another swarm agent having partially modified `mcp/response/briefing.rs` before `mcp/tools.rs` was updated. This is not caused by my changes and is scoped to other agents in this swarm.

## Commit

`2f1461a` — `impl(wire): add source field to HookRequest::ContextSearch for SubagentStart tagging (#349)`

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-engine` (serde default optional field wire types patterns, crt-027 architectural decisions) — found entry #646 (backward-compatible serde(default) config extension) confirming the pattern, and entries #3242–#3246 confirming all crt-027 ADRs are stored.
- Stored: entry #3255 "`serde(default)` alone does not omit None on serialization — pair with `skip_serializing_if` for wire optional fields" via `/uni-store-pattern`. Key gotcha: pseudocode said "standard serde behavior for Option omits the key" which is wrong — `None` serializes as `null` without `skip_serializing_if`. Also documented that explicit field pattern matches (without `..`) become compile errors on field addition, requiring `source: _` in production match arms.
