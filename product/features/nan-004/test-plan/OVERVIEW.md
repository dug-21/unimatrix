# nan-004: Versioning & Packaging — Test Strategy

## Test Layers

| Layer | Scope | Tooling |
|-------|-------|---------|
| Rust unit tests | Binary rename, new subcommands (Version, ModelDownload), CLI parsing | `cargo test --workspace` |
| JavaScript unit tests | merge-settings, resolve-binary, init logic, JS shim routing | Node.js `assert` / `node --test` (built-in test runner) |
| Integration (infra-001) | MCP server still works after binary rename; smoke gate | `pytest` via infra-001 harness |
| Shell integration | End-to-end init flow, postinstall behavior, exit code passthrough | Bash scripts in `packages/unimatrix/test/` |
| CI validation | Release pipeline structure (YAML lint, step ordering) | Manual review + dry-run tag |

## Risk-to-Test Mapping

| Risk ID | Priority | Component(s) | Test Layer | Coverage Target |
|---------|----------|-------------|------------|-----------------|
| R-01 | Critical | C5 | JS unit | 7 merge scenarios: empty, permissions-only, existing hooks, pre-rename hooks, absolute path update, extra keys, round-trip idempotency |
| R-02 | Critical | C3, C4 | JS unit + shell integration | Absolute path verification, path-break simulation, re-init repair |
| R-03 | High | C10 | CI pipeline | `ldd` check step, clean-container smoke (CI-only, not local tests) |
| R-04 | High | C4, C5 | JS unit | Double-init produces exactly 7 hook entries, 1 MCP entry |
| R-05 | Med | C2 | JS unit + shell | Argv routing for init/hook/export/no-args/--version, exit code passthrough |
| R-06 | Med | C9, C11 | Shell + CI | Version match validation between Cargo.toml and package.json |
| R-07 | High | C10 | CI review | Rust 1.89 pin, patches/anndists assertion step |
| R-08 | Med | C6 | JS unit + shell | Postinstall always exits 0: network fail, model cached, binary missing |
| R-09 | High | C4 | JS unit | .mcp.json merge preserves existing servers, updates unimatrix entry |
| R-10 | Low | C4 | JS unit | Skill copy count, non-unimatrix skill preservation |
| R-11 | Med | C4 | Shell integration | Init creates DB, then binary opens same DB (path agreement) |
| R-12 | Low | C7 | Rust unit + integration | Binary compiles as `unimatrix`, existing tests still pass |
| R-13 | Med | C3 | JS unit | UNIMATRIX_BINARY env fallback, error message on resolution failure |
| R-14 | Med | C5 | JS unit | Malformed JSON diagnostic, empty file treated as {}, non-object hooks key |
| R-15 | High | C10 | CI review | Platform package published before root package, failure halts root publish |

## Cross-Component Test Dependencies

- C2 (JS Shim) depends on C3 (Binary Resolution): shim tests need a mock or real binary path
- C4 (Init) depends on C3 + C5 (Settings Merge): init integration tests exercise the full pipeline
- C6 (Postinstall) depends on C3 + C8 (Model Download): postinstall calls the binary
- C10 (Release Pipeline) depends on C9 (Version Sync): CI validates version match

## Integration Harness Plan (infra-001)

### Binary Rename Impact

The integration harness resolves the binary via `UNIMATRIX_BINARY` env var or searches for `unimatrix-server` in `target/release/`. After the rename (C7), the harness's `get_binary_path()` in `harness/conftest.py` must be updated to search for `unimatrix` instead of `unimatrix-server`. This is a one-line change, not a new test.

### Suites to Run

Per the suite selection table, nan-004 touches server tool logic (binary rename) and introduces new CLI subcommands:

| Suite | Reason | Mandatory |
|-------|--------|-----------|
| `smoke` (-m smoke) | Minimum gate for any change | Yes |
| `protocol` | Binary rename could break MCP handshake/discovery | Yes |
| `tools` | Validates all 9 tools still work after rename | Yes |
| `lifecycle` | DB path/schema unchanged but binary entry point changed | Yes |

Suites NOT needed: `confidence`, `contradiction`, `security`, `volume` -- nan-004 does not modify engine logic, scoring, or security boundaries.

### New Integration Tests Needed

**None for infra-001.** The nan-004 feature adds JavaScript distribution and a CLI rename. The MCP protocol behavior is unchanged. The new subcommands (`version`, `model-download`) are non-MCP and tested via shell integration, not the MCP harness. The only harness change is updating the binary name in `get_binary_path()`.

### Feature-Specific Integration Tests (outside infra-001)

The JavaScript components need their own test infrastructure since infra-001 only tests MCP protocol behavior:

1. **`packages/unimatrix/test/`** -- Node.js test directory for JS unit tests
2. **Shell integration tests** -- Bash scripts validating end-to-end init flow

These are detailed in per-component test plans below.

## JavaScript Test Infrastructure

All JS tests use Node.js built-in test runner (`node --test`) with `assert` module. No external test framework dependencies.

Test files:
- `packages/unimatrix/test/merge-settings.test.js` -- C5 unit tests
- `packages/unimatrix/test/resolve-binary.test.js` -- C3 unit tests
- `packages/unimatrix/test/shim.test.js` -- C2 routing tests
- `packages/unimatrix/test/init.test.js` -- C4 unit tests (mocked filesystem)
- `packages/unimatrix/test/postinstall.test.js` -- C6 unit tests

Fixtures: `packages/unimatrix/test/fixtures/` containing sample `.claude/settings.json` and `.mcp.json` files for merge testing.
