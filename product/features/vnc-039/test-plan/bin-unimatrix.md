# Test Plan — C3 `bin/unimatrix.js` (`mcp-bridge` subcommand routing)

> Scope **A** · `[no-cloud]` · MODIFIED `bin/unimatrix.js` (add `mcp-bridge` subcommand, early-return to JS).
> Risk: contributes to the bridge-reachability correctness (ADR-002 / OQ-4). AC: **AC-13**.
> New test file: `test/bin-mcp-bridge.test.js`. Cumulative: reuse the child-process spawn pattern from `test/shim.test.js` / `test/resolve-binary.test.js`.

## Behavior under test (ARCHITECTURE §5, ADR-002)
`bin/unimatrix.js` early-returns `mcp-bridge` to the JS bridge (C2) **before** the Rust `execFileSync` fallthrough block — mirroring the existing `init` early-branch. The hazard inverted: a `mcp-bridge` that *reaches* the Rust exec block throws on exactly the remote-only/non-Linux clients the bridge exists to serve (no Rust binary shipped).

## Tests

### AC-13 — subcommand routed to JS, never `execFileSync` the Rust binary
- `test_binMcpBridge_routesToJsBridge_noMissingBinaryThrow` — invoke `unimatrix mcp-bridge <projectHash>` in an environment with **no Rust platform binary** (point binary resolution at an absent path, mirroring `resolve-binary.test.js`); assert it runs the JS bridge (reaches bridge code / does not throw a missing-binary error).
- `test_binMcpBridge_earlyReturnsBeforeRustExecBlock` — structural/behavioral: assert the `mcp-bridge` branch returns before the `execFileSync` Rust fallthrough (e.g. the Rust binary is never spawned — assert no exec of the platform binary path occurs for this subcommand).
- `test_binMcpBridge_passesProjectHashArg` — assert the `projectHash` argv is forwarded to the bridge (the bridge's store key).

### Parity with the existing `init` branch
- `test_binMcpBridge_mirrorsInitEarlyBranchPattern` — assert `mcp-bridge` is handled by the same early-JS-branch shape as `init` (the one pre-existing JS-handled subcommand), so a future non-`init` subcommand audit stays coherent.

### Non-regression
- `test_bin_otherSubcommands_stillExecRustBinary` — a non-`mcp-bridge`, non-`init` subcommand still routes to the Rust `execFileSync` block unchanged (the early-return is scoped to `mcp-bridge`, not a blanket bypass).
- `test_bin_init_branchUnchanged` — `init` routing is untouched.

## Notes
- `[no-cloud]`: this is pure routing — no network, no store read required (the bridge it routes to will fail loud on a missing store, but C3's test only proves the route reaches JS).
- The `.mcp.json` entry written by C4 targets the **resolved module path** directly (lean spawn), not the literal `unimatrix mcp-bridge` string (ARCHITECTURE §5); the subcommand exists for discoverability/hand-debugging. AC-13 tests the subcommand route; the `.mcp.json` `args` shape is asserted in the init-remote plan (AC-01/AC-09).
