# nan-024-agent-5-launcher Report

Unimatrix entries consulted: #5775, #5901 (ADR-004), #5900 (ADR-002)

## Report
Files modified: packages/unimatrix/bin/unimatrix.js (run/passThrough/require.main guard, null-binary handling in resolveWireTransport; headlessEnvSet edit preserved), test/cli-routing.test.js, test/shim.test.js, test/bin-mcp-bridge.test.js (wrappers call .run(); isolated HOME; `hook` test now asserts Node routing).
Files created: lib/hook-route.js, lib/status.js, test/helpers/launcher-fixture.js, test/launcher-hook.test.js, test/launcher-status.test.js (includes cloud-env cases; no separate status-cloud-env file).
Tests: launcher-hook 25, launcher-status 14, cli-routing/shim/bin-mcp-bridge 37 pass, 0 fail. compat-matrix "legacy enabled" fails at baseline (not mine).
Issues:
- Deviation: hook-route decides installed-ness BEFORE trust check (trustSet always requires install.json; trust-first makes "not installed" exit 1). shim-entry.js (Wave 2) runs trust first and has the same inconsistency when install.json is absent on a project machine.
- Size gate FAILS (stripped 115047/113639, raw 203093/201399) from lib/hook-client changes by other agents; my files are outside lib/hook-client.
- init.js:663 still calls resolveBinary() unguarded (wire-init, Wave 4).
- Pass-through uses the resolve-binary wrapper (allowSelf) plus a redirect trust check; D-086 win32 branch kept in hook-route spawnRust.
- Windows-only signal rows not exercised on this OS (POSIX host); win32 platform simulated only for non-spawn paths.

## Knowledge Stewardship
- Queried: context_briefing, ADR-004/002 via context_get -- found prior pattern #5775 (child-process helper).
- Stored: #5904 "Launcher: decide installed-ness before trust check; node -e test wrappers must call run()"
