# Test Plan Overview: vnc-012 — Server-Side Integer Coercion for MCP Parameters

## Overall Test Strategy

This feature adds three serde deserializer helpers and applies them to nine fields across
five parameter structs. The change is a narrow deserialization boundary fix — no handler
logic changes, no new crate dependencies, no storage schema changes. The test strategy
follows two parallel tracks:

**Track 1 — Unit tests (Rust, in-process)**
All Rust unit tests live in `#[cfg(test)]` blocks. Two locations:
- `serde_util.rs`: helper-level tests exercising each Visitor method directly
- `tools.rs`: struct-level tests exercising full `serde_json::from_str::<Struct>()` paths

**Track 2 — Integration tests**
Two vehicles:
- AC-13 (Rust): in-process integration test using `make_server()` + `call_tool` to exercise
  the rmcp `Parameters<T>` dispatch path — the exact code path where the live bug fires
- IT-01/IT-02 (Python, infra-001): end-to-end tests over stdio transport validating that
  the fix is visible to real MCP clients

Unit tests alone are insufficient because `serde_json::from_str` in the test block does not
exercise rmcp's `Parameters<T>` transparent delegation path. AC-13 bridges this gap. IT-01
and IT-02 bridge the transport layer.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Risk | Covered By |
|---------|----------|------|-----------|
| R-01 | Critical | Missing `#[serde(default)]` — absent fields error instead of `None` | AC-03-ABSENT-ID, AC-03-ABSENT-LIMIT, AC-04-ABSENT, AC-05-ABSENT, AC-06-ABSENT (5 tests in tools.rs) |
| R-02 | Critical | rmcp dispatch path not tested — unit tests miss `Parameters<T>` delegation | AC-13 (Rust integration), IT-01 + IT-02 (infra-001 smoke) |
| R-03 | High | JSON null → `Some(0)` or error instead of `None` | AC-03-NULL-ID, AC-03-NULL-LIMIT, AC-04-NULL, AC-05-NULL, AC-06-NULL (5 tests in tools.rs) |
| R-04 | High | `usize` truncation from `as usize` on 32-bit targets | AC-09 (negative string rejection), AC-06-ZERO, u64-overflow test (in serde_util.rs or tools.rs) |
| R-05 | High | `#[schemars(with)]` typo emits empty schema `{}` | AC-10 (schema snapshot in tools.rs) |
| R-06 | High | Float JSON Numbers not handled by `visit_f64` — panic or wrong error | AC-09-FLOAT-NUMBER (in serde_util.rs and tools.rs) |
| R-07 | Med | `deserialize_with` path string literal not compiler-validated | Covered implicitly by `cargo build --workspace`; no dedicated test |
| R-08 | Med | Non-numeric string silently coerces to 0 | AC-08 (required fields), AC-08-OPT (optional fields) |
| R-09 | Med | `make_server()` not accessible in test context | Implementation agent must verify; schema snapshot test (AC-10) is the gate |
| R-10 | Low | Existing `test_retrospective_params_evidence_limit` regression | AC-11 (`cargo test --workspace` baseline) |

---

## Cross-Component Test Dependencies

| Dependency | Direction | Implication |
|-----------|-----------|-------------|
| `serde_util.rs` helpers used by `tools.rs` annotations | serde_util → tools | tools.rs tests exercise serde_util indirectly; serde_util.rs tests exercise it directly |
| `mod serde_util;` declaration in `mod.rs` | mod.rs → serde_util | mod.rs test is compile-only (mod declaration test) |
| IT-01/IT-02 exercise the full server binary | infra_001 → all components | infra-001 tests require a passing build; run after `cargo build --release` |
| AC-13 requires `make_server()` from `server.rs` | tools.rs / server.rs | Implementation agent must confirm visibility of `make_server()` and `tool_router` |

---

## Integration Harness Plan

### Suites to Run

This feature modifies tool parameter deserialization directly in `mcp/tools.rs`. Per the
suite selection table:

| Feature touch | Suite |
|--------------|-------|
| Server tool logic (tools.rs) | `tools` |
| MCP protocol compliance | `protocol` |
| Minimum gate | `smoke` |

The `security` suite is also relevant because the deserialization boundary is an input
validation surface. Run `security` to verify no unintended coercion of security-relevant
fields (categories, statuses, agent IDs) was introduced.

**Suites to run in Stage 3c:**
1. `smoke` — mandatory minimum gate (`pytest -m smoke`)
2. `tools` — all 73 tests, validates all tool parameter paths
3. `protocol` — 13 tests, validates MCP handshake and JSON-RPC compliance
4. `security` — 17 tests, input validation boundaries

Suites **not** required: `lifecycle`, `volume`, `confidence`, `contradiction`, `edge_cases`,
`adaptation` — no storage schema changes, no confidence logic changes, no scanning changes.

### New Integration Tests Required

Two new tests in `product/test/infra-001/suites/test_tools.py`:

**IT-01: `test_get_with_string_id`** (marked `@pytest.mark.smoke`)
- Fixture: `server` (fresh DB, function scope)
- Arrange: call `context_store` to create an entry; extract the integer id from the response
- Act: call `context_get` with `{"id": str(entry_id), "agent_id": "human"}` — id as string
- Assert: response is success, content is non-empty
- Location: after existing `test_store_roundtrip` in the `context_get` section

**IT-02: `test_deprecate_with_string_id`** (marked `@pytest.mark.smoke`)
- Fixture: `server` (fresh DB, function scope)
- Arrange: call `context_store` to create an entry; extract the integer id
- Act: call `context_deprecate` with `{"id": str(entry_id), "agent_id": "human"}`
- Assert: response is success
- Location: in the `context_deprecate` section

Both tests must use `extract_entry_id()` from `harness.assertions` to obtain the integer id
from the store response, then convert to string via `str(entry_id)` for the subsequent call.

### AC-13 Rust In-Process Test

AC-13 lives in `crates/unimatrix-server/tests/mcp_coercion.rs` (preferred — integration
test file) or `crates/unimatrix-server/src/server.rs` test block. It must:

1. Call `make_server()` (must be `pub(crate)` or accessible in integration test scope)
2. Insert a test entry into the store to obtain a real `u64` id
3. Construct a `CallToolRequestParams` with `arguments` containing `{"id": "<id-as-string>", "agent_id": "human"}`
4. Call `ServerHandler::call_tool` (or invoke `tool_router` directly if `RequestContext<RoleServer>` is not constructible)
5. Assert the result is `Ok(_)` and content is non-empty

The implementation agent must resolve OQ-04: whether `RequestContext<RoleServer>` is
constructible from rmcp's public API. If not, a `pub(crate) fn call_tool_for_test` shim
on `UnimatrixServer` is the fallback (see RISK-TEST-STRATEGY.md R-02). The test name
must include `coercion` or `string_id`.

---

## Component Test Plan Files

| File | Component | Primary Risks |
|------|-----------|--------------|
| `serde_util.md` | `mcp/serde_util.rs` (new) | R-03, R-04, R-06, R-08 |
| `tools.md` | `mcp/tools.rs` (modified) | R-01, R-02, R-03, R-05, R-08, R-10 |
| `mod.md` | `mcp/mod.rs` (modified) | R-07 (compile only) |
| `infra_001.md` | infra-001 `test_tools.py` (modified) | R-02 (IT-01, IT-02) |
