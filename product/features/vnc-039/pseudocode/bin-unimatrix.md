# C3 — `bin/unimatrix.js` (MODIFIED, Scope A)

**Purpose.** Add a `mcp-bridge` subcommand that early-returns to the JS bridge (C2) BEFORE the
generic Rust `execFileSync` block. JS routing is REQUIRED (not stylistic): non-Linux / remote-only
pure-JS clients ship no Rust binary, so a `mcp-bridge` that fell through to `execFileSync` would
throw on exactly the population the bridge serves (ADR-002).

This mirrors the existing `init` early-return branch (`bin/unimatrix.js:10-34`). Source: ADR-002.

## Current shape (verified, `bin/unimatrix.js`)

```
main():
    args = process.argv.slice(2)
    if args[0] === "init": <route to lib/init.js init()>; return        // existing early branch
    // <else> resolve Rust binary + execFileSync(binaryPath, args)       // lines 36-67
```

## Change — add a `mcp-bridge` early branch (parallel to `init`)

Insert AFTER the `init` branch and BEFORE the "resolve binary and exec" block.

```
// Route "mcp-bridge" to the JS bridge (ADR-002). REQUIRED: remote-only/non-Linux clients
// have no Rust binary; falling through to execFileSync would throw for the bridge's whole
// target population. The bridge reads its credential from the out-of-tree store keyed by
// the projectHash argument; no token ever on the command line (AC-09).
if (args[0] === "mcp-bridge") {
    const projectHash = args[1];
    if (!projectHash || typeof projectHash !== "string") {
        process.stderr.write("usage: unimatrix mcp-bridge <projectHash>\n");
        process.exitCode = 2;
        return;
    }
    // Delegate to C2. mcp-bridge.js owns store read, the pinned connection, and the
    // stdio loop. It calls process.exit() on its own fail-loud / EOF paths.
    require("../lib/hook-client/mcp-bridge.js").main(["node", "mcp-bridge", projectHash]);
    return;
}
```

Notes:
- `require("../lib/hook-client/mcp-bridge.js")` is loaded LAZILY inside the branch (only when the
  subcommand is invoked) so the existing fast paths (`init`, Rust exec) pay no load cost and the
  bridge's stdlib-only module is not evaluated unless needed.
- The exported entry MUST accept an argv-shaped array so it works both via this subcommand and via a
  direct `node <bridge> <projectHash>` spawn (the `.mcp.json` shape, ADR-002). Recommended: C2
  exports `main(argv)` that reads `argv[2]` as `projectHash` — call it here with
  `["node","mcp-bridge",projectHash]` so `argv[2] === projectHash`, identical to a direct spawn
  where `process.argv = ["node", "<bridge path>", "<projectHash>"]`. (See OVERVIEW "Bridge argv
  contract"; C2 `mcp-bridge.md` Entry.)
- The subcommand is the HUMAN/DEBUG surface (`unimatrix mcp-bridge <projectHash>`). `.mcp.json`
  written by C4 targets the resolved module path DIRECTLY (`node <bridge path> <projectHash>`) for a
  lean spawn that does not re-enter `bin/unimatrix.js` (ADR-002 §Decision). Both resolve to the same
  module.
- Do NOT modify the Rust `execFileSync` block — it stays the fallthrough for all other subcommands.

## Data flow

IN: `process.argv` `["node","unimatrix.js","mcp-bridge","<projectHash>"]`.
OUT: delegates to C2 `main`; C2 owns the process lifecycle (stdio loop, exit codes).

## Error handling

- Missing `projectHash` → usage to stderr, `exitCode = 2`, return (no exec fallthrough).
- All bridge-internal errors (store read, pin mismatch, transport) are owned by C2 and exit non-zero
  there (see mcp-bridge.md error table). C3 does not catch them.

## Key test scenarios (hints; full plan in test-plan/bin-unimatrix.md)

- `unimatrix mcp-bridge <hash>` routes to C2 and NEVER calls `resolveBinary`/`execFileSync` (assert
  the Rust path is not reached — the load-bearing ADR-002 correctness point: no binary required).
- `unimatrix mcp-bridge` with no hash → usage message + exit code 2, no exec.
- A non-`mcp-bridge`, non-`init` subcommand still routes to the Rust `execFileSync` block unchanged
  (regression: the new branch does not capture other subcommands).
- `init` branch unchanged (regression).
