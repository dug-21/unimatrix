## ADR-002: Bridge entrypoint is a `unimatrix mcp-bridge` subcommand routed to JS (OQ-4)

### Context

The bridge (ADR-001) needs an entrypoint reachable from `.mcp.json`. OQ-4 poses two shapes:
(a) a new `unimatrix mcp-bridge` subcommand routed in `bin/unimatrix.js` to JS (not the Rust binary), or
(b) a direct `node <bridge-path> …` command written into `.mcp.json` (mirroring how hooks invoke `node <client> <EVENT>`).

`bin/unimatrix.js` today special-cases `init` to a JS implementation with an early `return` (`unimatrix.js:10-34`) before the generic Rust `execFileSync` fallthrough (`unimatrix.js:36-54`). **Every other subcommand `execFileSync`s the Linux-only Rust binary.** This makes JS routing **required, not merely preferred**: a non-Linux or remote-only client that ships the pure-JS edge has **no Rust binary on disk**, so any subcommand that falls through to the exec block throws — on exactly the clients the bridge exists to serve (self-signed remote cloud, where Claude Code cannot do native TLS). A `mcp-bridge` left to fall through would be dead on arrival for its entire target population. The OQ-4 worry that a new subcommand "touches the Rust-binary exec path" is therefore inverted: the hazard is **not** routing it to JS, not the reverse.

### Decision

Add a `mcp-bridge` subcommand to `bin/unimatrix.js`, **routed to JS** (`lib/hook-client/mcp-bridge.js`) via an early branch — the same pattern as `init` — that `return`s before the Rust exec block. JS routing is **REQUIRED**: the bridge's target clients (non-Linux / remote-only, pure-JS edge) have no Rust binary, so falling through to `execFileSync` would throw on every such client. The early-return branch is the only shape that runs on the population the bridge serves.

`init` resolves the bridge module path via `require.resolve("./hook-client/mcp-bridge.js")` (the contract `initRemote` already uses for the hook-client path, `init.js:409`), guaranteeing a correct per-install absolute path. The `.mcp.json` entry written by `init` (ADR-004 §`.mcp.json` contract) targets that **resolved module path directly** — `{command:"node", args:[<bridge module path>, <projectHash>]}` — for a lean spawn that does not re-enter `bin/unimatrix.js`. The subcommand exists as the **human/debug surface** (`unimatrix mcp-bridge <projectHash>` runs the same module by hand), while `.mcp.json` uses the direct module path; both resolve to one module.

### Consequences

**Easier:** single CLI entrypoint convention preserved (the JS-handled subcommands `init` and now `mcp-bridge` branch early, identically); the bridge runs on its target non-Linux/remote-only clients at all (the required JS route, not the dead Rust-exec fallthrough); a discoverable, documentable `unimatrix mcp-bridge` command for hand-debugging the bridge; per-install path correctness via `require.resolve`; the `.mcp.json` spawn stays lean (direct module, no extra `bin/unimatrix.js` hop).

**Harder:** two invocation faces for one module (subcommand for humans, direct path in `.mcp.json`) — a documentation note, not a code cost. If a reviewer prefers `.mcp.json` to carry the literal `unimatrix mcp-bridge` string for legibility, that is a one-line flip at the cost of an extra process hop (flagged as Open Question 1 for the human).

Related: ADR-001 (this feature, the bridge module), ADR-004 (this feature, the `.mcp.json` write contract + `projectHash` arg), ADR-003 vnc-034 (init-routed-to-JS precedent in `bin/unimatrix.js`).
