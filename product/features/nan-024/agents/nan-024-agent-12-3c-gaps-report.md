# nan-024-agent-12-3c-gaps report

## Report
Files (all under packages/unimatrix/test unless noted):
- pkg-binary-copy.test.js (2 tests): init from a fake npx-cache package with a platform binary, cache deleted, literal written hook command via sh -> exit 0, silent, event bytes seen on a stub UDS server at the project socket; resolveRuntime binarySource "install".
- mcp-stdio-spawn.test.js (4): spawn the init-written MCP entry against a stub binary: first stdout byte, empty-stdout case, exit codes 1/7/42 mirrored, SIGTERM mirrored, stderr banner-free.
- init-preflight.test.js (3): real CLI process: init with unrecordable node -> exit 1, no install.json/hook/home change; wire with missing recorded node and empty PATH -> exit 1, tree unchanged; wire with PATH node -> exit 0 + warning naming node.
- codex-detect.test.js (7): detection rows 1-7 via init CLI (row 4 via `wire --harness codex-cli`; init has no --harness flag), asserting hooks.json + config.toml [mcp_servers.unimatrix] and destination.
- helpers/fake-package.js: shared fake npx-cache builder.
- .github/workflows/q9-windows-mcp-proof.yml: primary-form step is continue-on-error with id `primary`; follow-up step fails the job only if outcome is success (unexpected pass) or not failure. Fallback/hook/real-init steps unchanged.

Notes: init writes project skills before the node-recordable check, so "tree unchanged" for init-preflight is asserted on the home dir plus absence of hook/MCP files (skills in project are written). test-counts.json baseline left unchanged: it is generated from origin/main and does not list any nan-024-added suite (64 of 90); check-test-count PASS (1330). Orphan audit 90/90. wiring-e2e set 29 -> 45 tests, all pass. package set 728/727/0/1 unchanged. Size gate 115012/203058 (unchanged, lib/hook-client untouched). package.json untouched.

## Knowledge Stewardship
- Queried: context_briefing -- #4202/#2656 (test named in plan never implemented; Gate 3c passed silently), #5778 serial package suite.
- Stored: nothing novel to store -- assembled-test pattern is test-local.

Unimatrix entries consulted: #5778, #5781, #4202, #2656, #5784
