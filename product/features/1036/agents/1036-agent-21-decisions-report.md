# 1036-agent-21-decisions report

## D-034
`test/remote-client.test.js`: SIZE_LIMIT 440000 -> 500000; test renamed `test_remote_install_under_500kb`; comments note "amends D-093, human-approved (#1036, D-034)". No other 440kb references in packages/unimatrix.

## D-036
`lib/init.js` initRemote already runs `credstore.write` (safe-write with enforceMode/newMode 0o600) at Step 3, before `transport.pingForInit` at Step 6. No reorder needed; ping failure still throws after the store is written (existing "Configuration files were written" contract). Added `test_initBundle_storeModeRepairedBeforePing` in `test/init-remote.test.js`: pre-seeds a 0644 store, records call order, asserts `["store","ping"]`, mode 0600 at ping time, on a failing ping.

## D-037
resolve-binary.test.js: 7 pass / 2 fail on branch AND on main (a5f93a92, symlinked identical node_modules, scratchpad worktree, removed after). Failing: test_missing_package_throws_with_platform_info, test_linux_x64_resolves_correct_package ("Missing expected exception"). Pre-existing -> waived; filed #1055 (tech-debt).

## Tests
init-remote + remote-client: 84/84 pass. Size gate: see return. No commit (Delivery Leader commits). Tmp homes only.

## Knowledge Stewardship
- Unimatrix entries consulted: #5830, #5094 (briefing; none changed the approach)
- Queried: mcp__unimatrix__context_briefing -- no entries specific to init ping ordering
- Stored: nothing novel to store -- ordering already correct; resolve-binary env dependence captured in #1055
