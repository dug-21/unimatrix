# Agent Report: 901-agent-1-fix (bugfix-901)

## Fix
Root cause was the `transcript` retrieval axis of `context_cycle_review` advertising a
free-form any-value schema (`#[schemars(with = "Option<serde_json::Value>")]`) while the
deserializer demanded an exact `TranscriptScope` struct — a schema/deserializer contract
mismatch. A schema-driven client sent the value as a string; rmcp strict extraction
rejected it with -32602 before the handler ran.

Fixed by deriving `JsonSchema` on the source types so `tools/list` advertises the real
object shape. Serde deserialization path untouched (zero runtime behavior change).

## Files modified
- `Cargo.toml` — promoted `schemars = "1"` to `[workspace.dependencies]` (prevents observe/server/rmcp generator version drift; matches the serde/serde_json/bincode convention).
- `crates/unimatrix-server/Cargo.toml` — `schemars = { workspace = true }`.
- `crates/unimatrix-observe/Cargo.toml` — added `schemars = { workspace = true }`.
- `crates/unimatrix-observe/src/types.rs` — `import schemars::JsonSchema`; derived `JsonSchema` on `TranscriptScope` and `Window`; trimmed internals-leaking doc comments (they surface as client-facing schema `description`s).
- `crates/unimatrix-server/src/mcp/tools.rs` — removed the `#[schemars(with = "Option<serde_json::Value>")]` escape hatch and the stale Wave-1 "Schema note" doc on `RetrospectiveParams.transcript`.
- `crates/unimatrix-server/src/server.rs` — two new tests (below).
- `Cargo.lock` — schemars added to unimatrix-observe.

## New tests
- `test_schema_transcript_advertises_object_shape` (server.rs) — schema-snapshot (extends vnc-012 AC-10 region). Asserts `context_cycle_review`'s `transcript` emits the nullable union `anyOf:[{$ref #/$defs/TranscriptScope},{null}]`, resolves `$defs/TranscriptScope` to an object with sub-fields `phase`/`anchor`/`match`/`window` (wire key literally `match`), `window` `$ref`s `$defs/Window` (`millis`/`blocks`), and none required. Teeth verified: fails when the escape hatch is restored.
- `test_cycle_review_transcript_object_passes_mcp_boundary` (server.rs) — live duplex `tools/call`. Drives `context_cycle_review` with a real `transcript` object including a nested `window` through the MCP boundary; asserts extraction does NOT return -32602 (ERROR_NO_OBSERVATION_DATA/-32010 for the ghost feature is expected and proves the object deserialized past strict extraction). First `call_tool` client test in the workspace (seed only — no generalized harness, per scope hold).

## Tests
unimatrix-observe + unimatrix-server: all pass (server lib 4364 passed; observe 578 passed; 0 failed). Workspace build clean. clippy clean on both crates.

## Notes / boundaries honored
- Did NOT modify pattern #3813 or add a serde_json::Value-escape-hatch convention (deferred to retro).
- Did NOT build a generalized MCP-boundary schema-contract harness (separate follow-up).
- Boundary test caveat: sending a valid object deserializes regardless of advertised schema, so this test alone does not reproduce the client-side string coercion; the schema-snapshot test is the class-specific regression catch. The boundary test seeds the missing test CLASS (self-contained `$defs`/`$ref` resolution + real object round-trip through duplex).

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` (bugfix-901) — surfaced crt-033/crt-025 ADRs on `RetrospectiveReport` shape and unimatrix-server schema patterns; none altered the approved fix. Investigator/reviewer briefings already covered pattern #3813, ADR #4734 (typed `schemars(with)`), ADR #3795 (derive on observe source types, no DTO shim), lesson #5444 (consumer-callable gate class).
- Stored: nothing novel — this is a bug fix, not a reusable pattern. The typing convention is pattern #3813, the gate class is lesson #5444, and per project policy defect specifics live on GH #901. Whether #3813 should be extended ("no `serde_json::Value` escape hatch on MCP params") is a crt-057/901 retro decision, not this fix's to store.
