# Agent Report: vnc-012-agent-1-pseudocode

## Summary

Produced per-component pseudocode for all four vnc-012 components:

- `pseudocode/OVERVIEW.md` — component interaction, data flow, OQ-04 resolution
- `pseudocode/serde_util.md` — three Visitor implementations, all edge cases
- `pseudocode/tools.md` — 9 field annotations, AC-13, AC-10, ~40 named test functions
- `pseudocode/mod.md` — single-line change, build-time failure mode documented
- `pseudocode/infra_001.md` — IT-01 and IT-02 Python test structure

## OQ-04 Resolution (AC-13 Test Vehicle)

`RequestContext<RoleServer>` is NOT constructible outside rmcp. `Peer::new` in rmcp
0.16.0 is `pub(crate)`. Verified by reading
`/usr/local/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rmcp-0.16.0/src/service.rs`.

AC-13 uses `serde_json::from_value::<GetParams>(Value::Object(args))` directly in the
`tools.rs` `#[cfg(test)]` block. This is the exact line executed by `Parameters<T>:
FromContextPart` in rmcp's `handler/server/tool.rs` line ~173. The test name includes
"coercion" to satisfy the findability requirement.

AC-10 (schema snapshot) requires `tool_router` access. `tool_router` is a private field
on `UnimatrixServer`. The pseudocode for `tools.md` documents that AC-10 must live in
`server.rs` tests where `make_server()` is accessible, and requires adding a
`#[cfg(test)] pub(crate) fn tool_router_for_test()` accessor to `UnimatrixServer`.

## Findings from Reading Codebase

1. `make_server()` is `pub(crate)` inside `server.rs`'s `#[cfg(test)]` mod, not visible
   from `tools.rs` tests. AC-10 placement must be `server.rs`.

2. `ToolRouter::list_all()` is a public method returning `Vec<Tool>`. Each `Tool` has a
   public `input_schema` field. The schema snapshot test is straightforward once
   `tool_router` is accessible.

3. The existing `test_wrong_type_doesnt_panic` test in `tools.rs` will transition from
   an implicit regression test (currently "string doesn't panic") to a documented
   correct-rejection test after annotation. The existing assertion `is_err()` remains
   correct for a non-numeric string `"not-a-number"`, so the test still passes.

4. `tools.rs` is 5020 lines — already exceeds the 500-line guideline. The pseudocode
   documents this as a pre-existing condition; splitting is out of scope for vnc-012.

5. The tool name for `RetrospectiveParams` must be verified against the
   `#[tool(name = "...")]` annotation in `tools.rs` before writing the AC-10 test.
   The implementation agent should search for `feature_cycle` or `evidence_limit` in
   `tools.rs` to find the correct tool name string.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 15 entries; #3784 (custom serde
  deserializer + schemars pattern), #3787 (ADR-001), #3789 (ADR-003), #3790 (ADR-004),
  and #3788 (ADR-002) were directly relevant and confirmed the design approach. #885
  (serde types need explicit test coverage) informed the emphasis on null/absent tests.
- Queried: `mcp__unimatrix__context_search` (category: pattern) — #3784 confirmed the
  `#[serde(deserialize_with)] + #[schemars(with)]` pairing pattern.
- Deviations from established patterns: none. Pattern #3784 is exactly the pattern
  applied here. All ADRs (#3787–#3790) are followed.

## Open Questions

None. All OQs from SPECIFICATION.md are resolved:
- OQ-01 through OQ-03: resolved in source documents before this pseudocode was written.
- OQ-04: resolved above (use `serde_json::from_value` directly; AC-10 in `server.rs`).
- OQ-05: resolved as FR-13 (visit_f64/visit_f32 return invalid_type error).
