# dsn-001 Test Plan — server-instructions

Component: `crates/unimatrix-server/src/server.rs`

Risks covered: R-07 (partial — server.rs end), AC-05, AC-14.

---

## Scope of Changes

- `const SERVER_INSTRUCTIONS: &str = "..."` is removed from `server.rs`.
- `UnimatrixServer::new()` receives `instructions: Option<String>` from config.
- When `None`: use the compiled default (the original const value is now the
  `Default` backing, likely moved to `config.rs` or inlined as a default in `ServerConfig`).
- When `Some(s)`: use `s` as `ServerInfo.instructions` in the MCP `initialize` handshake.
- Three doc comments referencing `context_retrospective` are updated (see SR-05 checklist).

---

## None Path Uses Compiled Default (AC-01, no-config backward compat)

### test_server_instructions_none_uses_compiled_default

```rust
fn test_server_instructions_none_uses_compiled_default() {
    // When config.server.instructions is None, UnimatrixServer must use the
    // compiled default string — the same value that was the const before dsn-001.
    let instructions = resolve_server_instructions(None);
    // The compiled default is domain-agnostic (will be updated as part of dsn-001).
    // Assert it is non-empty and does not contain SDLC-only vocabulary.
    assert!(!instructions.is_empty(),
        "compiled default instructions must not be empty");
    // It must be the same string as before dsn-001 — regression test.
    // If the compiled default string is accessible, compare directly:
    // assert_eq!(instructions, COMPILED_SERVER_INSTRUCTIONS_DEFAULT);
}
```

Note: the exact assertion depends on whether the compiled default is still exported
as a constant or embedded as a `Default` value in `ServerConfig`. Adjust to compare
against the known default string.

---

## Some Path Uses Config String (AC-05)

### test_server_instructions_some_uses_config_string

```rust
fn test_server_instructions_some_uses_config_string() {
    let custom = "You are a legal research assistant.".to_string();
    let instructions = resolve_server_instructions(Some(custom.clone()));
    assert_eq!(instructions, custom,
        "Some(config_string) must be used verbatim as server instructions");
}
```

The helper `resolve_server_instructions(Option<String>) -> String` represents the
logic in `UnimatrixServer::new()` that selects between the config value and the
compiled default. If this logic is inlined in the constructor, test via constructing
a `UnimatrixServer` with the appropriate parameter.

---

## Doc Comment Update (AC-14 — manual verification)

Three doc comments in `server.rs` that reference `context_retrospective` must be
updated. This is a manual code-review gate, not a test:

| Location | Old Reference | New Reference |
|----------|--------------|---------------|
| Line 65 | `context_retrospective handler (drains on call)` | `context_cycle_review handler` |
| Line 147 | `features that complete without calling context_retrospective` | `context_cycle_review` |
| Line 207 | `Shared with UDS listener; drained by context_retrospective handler` | `context_cycle_review handler` |

In Stage 3c, verify these with:
```bash
grep "context_retrospective" crates/unimatrix-server/src/server.rs
```
Must return zero results.

---

## Integration Test: MCP `initialize` Response (AC-05)

This test requires a server started with a config file that sets
`[server] instructions = "Test domain guidance."`.

If the harness config-injection fixture is available (see OVERVIEW.md):

```python
def test_server_instructions_in_initialize_response(config_server):
    """AC-05: [server] instructions appears in ServerInfo.instructions."""
    # config_server: started with [server] instructions = "Test domain guidance."
    init_result = config_server.initialize()
    server_info = init_result.get("serverInfo", {})
    instructions = server_info.get("instructions", "")
    assert "Test domain guidance." in instructions, \
        f"Expected instructions in initialize response, got: {instructions!r}"
```

Fixture: `config_server` (requires harness config-injection support).

If harness fixture not available: document as gap. Unit test above covers the
logic path; MCP-level verification requires integration fixture enhancement (GH Issue).

---

## No-Op for Existing Tests

The removal of `SERVER_INSTRUCTIONS` const and the addition of the `instructions`
parameter to `UnimatrixServer::new()` change the constructor signature. All existing
tests that construct `UnimatrixServer` must be updated to pass the new parameter.

Test expectation: after the migration, all existing server tests continue to pass
with `instructions: None` (the default). This is validated by
`cargo test --workspace 2>&1 | tail -30` showing zero failures.
