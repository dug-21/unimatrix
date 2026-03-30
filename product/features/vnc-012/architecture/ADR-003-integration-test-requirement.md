## ADR-003: Mandatory Integration Test for MCP Dispatch Path (SR-03)

### Context

Unit tests in `tools.rs` exercise `serde_json::from_str` directly against the
parameter structs. They confirm that the serde deserialization layer correctly
coerces string-encoded integers. However, they do not exercise the full path:
JSON-RPC request over stdin/stdout -> rmcp dispatch -> `serde_json::from_value`
on `Parameters<T>` -> struct deserialization -> handler execution -> response.

SR-03 from the risk assessment classifies the missing integration test as High
severity: a bug in the rmcp dispatch layer (e.g., pre-processing of tool
arguments before delegating to serde) would not be caught by unit tests alone.
The scope's Open Question 4 is explicitly unresolved: no existing integration
test exercises the string-encoded integer path over the transport.

The infra-001 Python integration harness is the correct vehicle. It spawns a
real `unimatrix-server` subprocess, performs the MCP handshake over stdio, and
calls tools via JSON-RPC. The `UnimatrixClient.call_tool(name, arguments)` low-
level method allows sending arbitrary argument shapes — including `"id": "3770"`
as a JSON string — without the typed Python helper methods constraining the
input to integers.

A Rust-based integration test using `TestHarness` is not viable for this purpose:
`TestHarness` calls service methods directly, bypassing the rmcp JSON-RPC
dispatch layer entirely.

### Decision

Require one integration test in the infra-001 tools suite (`test_tools.py`) as
a non-negotiable acceptance gate for this feature:

**IT-01**: Call `context_get` via `call_tool("context_get", {"id": "3770"})` on
a real stored entry (store first to obtain a valid id, then call get with that
id as a string). Assert the response is a success (not an MCP error). This
confirms the rmcp -> serde -> handler path coerces the string id correctly.

**IT-02**: Call `context_deprecate` via `call_tool` with `"id": "<string_id>"`.
Assert success. Covers a second required-integer field from a write tool to
confirm coercion works on mutation paths.

The tests must use integer ids obtained from prior `context_store` calls (not
hardcoded), per SR-06 (avoid self-inflicted failures from string ids in test
fixtures themselves).

Mark both tests with the `smoke` marker so they run in the fast smoke subset
and act as a regression guard in CI.

Additionally, require a Rust unit test in `tools.rs` or `serde_util.rs` that
directly serializes the tool list schema and asserts `type: integer` for all
nine affected fields. This covers SR-01 (schema snapshot) and SR-02 (serde
Visitor correctness for null/absent/overflow paths) with in-process verification
that does not require the Docker harness.

### Consequences

Easier:
- SR-03 is fully resolved: the rmcp dispatch path is covered.
- The `smoke` marker means CI catches regressions on every merge.
- Infra-001 harness already has `call_tool` for raw argument passing — no new
  harness infrastructure required.

Harder:
- The integration test requires a running server and the ONNX model (Docker
  environment). It cannot be run locally without Docker unless the model is
  present. This is an existing constraint of all infra-001 tests, not new to
  this feature.
- Two test files are touched (infra-001 `test_tools.py` and `tools.rs`/
  `serde_util.rs`) — the test author must coordinate both.
