# Agent Report: 901-investigator (bugfix-901)

Diagnosis posted: https://github.com/dug-21/unimatrix/issues/901#issuecomment-4884236020

## Verdict
Root cause confirmed (confidence: high). Two-part:
1. `TranscriptScope` / `Window` (`crates/unimatrix-observe/src/types.rs:685`, `:707`) are serde-only — no `JsonSchema` derive; `unimatrix-observe/Cargo.toml` has no schemars dep.
2. `RetrospectiveParams.transcript` (`crates/unimatrix-server/src/mcp/tools.rs:476-478`) uses `#[schemars(with = "Option<serde_json::Value>")]` — the tool schema advertises a free-form object, clients send a string, rmcp `Parameters` extraction rejects with `-32602` before the handler runs.

## Proposed Fix (minimal)
- Add `schemars = "1"` to unimatrix-observe (workspace already locks 1.2.1, same as rmcp 1.7.0).
- Derive `JsonSchema` on `TranscriptScope` + `Window` (serde `rename = "match"` is honored).
- Remove the `schemars(with)` attr + stale "Schema note" doc on `RetrospectiveParams.transcript`.
- Rejected: params-layer mirror struct (drift class this bug embodies; crt-057 already mutated the shape mid-delivery).

## Missing Tests
1. Schema-shape assertion in the vnc-012 AC-10 region (`server.rs:3578`): `context_cycle_review.input_schema.properties.transcript` must advertise phase/anchor/match/window (window → millis/blocks).
2. Live duplex `tools/call` test with a real `transcript` object (extend `run_initialize_handshake`, `server.rs:3660`; rmcp `client` feature already enabled). No `call_tool` client test exists workspace-wide.

## Risks
- schemars nested-struct `$ref`/`$defs` emission — assert actual rmcp 1.7 output shape in the test.
- `Option<TranscriptScope>` emits the 2020-12 nullable union under rmcp ≥ 1.7 (GH#684) — assertions must accept it.
- Zero runtime behavior change (serde path untouched).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — pattern #3813 (real type in `schemars(with)`), ADR #4734 (vnc-012), ADR #5434 (crt-057 ADR-002); context_get #5444 (interface consistent but not consumer-callable — same class).
- Stored: nothing novel to store — convention covered by #3813, gate class by #5444; defect specifics live on GH #901 per project policy. Possible #3813 extension ("no `serde_json::Value` escape hatch on MCP params") is a post-fix retro call.
