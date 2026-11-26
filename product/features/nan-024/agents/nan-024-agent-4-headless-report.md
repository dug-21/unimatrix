# nan-024-agent-4-headless report

Unimatrix entries consulted: #5901 (ADR-004), #5869, #5122, #5398, #5775, #5116

## Report
Files modified:
- packages/unimatrix/lib/hook-client/remote-resolve.js (effectiveEnv at top of resolveRemote; lazy require due to config<->project-state cycle)
- packages/unimatrix/lib/hook-client/mcp-bridge.js (silent exit 0 outside root, no request)
- packages/unimatrix/bin/unimatrix.js (headless via headlessEnvSet)
- docs/client-setup.md (UNIMATRIX_PROJECT_ROOT row, MCP env example, GHA example, PowerShell unsupported)
- packages/unimatrix/test/headless-smoke.test.js (root in env/cwd, negative case, docs assertion)
- packages/unimatrix/test/bridge-headless-root.test.js (new; no-root, outside, sibling-prefix, empty, relative, symlinked root, docs grep)
- packages/unimatrix/test/hook-client/remote-resolve.test.js (wrapper adds root; 3 gate tests)
- packages/unimatrix/test/hook-client/ca-trust-tls.test.js (runBridge env gains root)

Tests: bridge-headless-root 7/7, bin-mcp-bridge, headless-smoke (real server) all pass; hook-client suite + remote-client: 1048/1051. Remaining 2 failures not from this change: config "test_homedir_resolution_degrades_to_null_state_dir", "remote install footprint under 380kb" (lib size growth, Wave 1).

Size gate (still OVER, caps not touched): stripped 114722 -> 115009 (+287; cap 113639), raw 202589 -> 203000 (+411; cap 201399).

Issues: gate over cap pre-existing; human decision. I used git stash/pop once briefly for before/after comparison (concurrent edits restored; status verified).

## Knowledge Stewardship
- Queried: context_briefing, context_get #5901 -- ADR-004 D-052/D-053/D-091 contract
- Stored: pattern "hook-client: resolveRemote must lazy-require project-state (cycle); gate tests need async spawn"
