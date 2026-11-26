# nan-024-agent-15-macos-ci report

Unimatrix entries consulted: #5788, #5784, #5830, #5778 (briefing); knowledge #5790 (package-suite excludes Windows) referenced from ci.yml.

## Root causes (macOS package-suite, all test artifacts; no product defect)
macOS/Windows are cloud-only (D-032): `installedDecision` (lib/shim-entry.js, lib/project-state.js) treats a non-Linux project as installed only with a credential (remote.json). The suites were written with Linux-local fixtures (store dir + install.json, no credential) and had never run on macOS (lesson #5788).

- parity 54/55 (fire-time-parity): NOT a macOS path/realpath/mode defect. On darwin the fixture project has no remote.json, so both shim-entry and the launcher return "not attached" (exit 0) BEFORE the trust check; the expected exit 1 (trust check failed) is never reached. Fixed by giving the shared fixtures a credential on non-Linux (the trust check then runs identically). Reproduced locally by spoofing process.platform=darwin via NODE_OPTIONS preload; same 0 !== 1.
- Same cause: launcher-hook (hook installed/trust failure/client throws/no entry), shim-entry (in project, argv, group-writable install.json, tampered/absent trust.js, packageRoot deleted, launcher rejection/throw), launcher-status (project kind, transport, reason text).
- Linux-only by design (skip with D-032 reason via test/helpers/linux-local.js): bin-mcp-bridge other-subcommands, init-integration (6), init.test mcp json, resolve-binary (5), postinstall (3), shim.test pass-through (7), launcher-hook pass-through/mcp-stdio local/Linux-hint (12), c14 claude/codex/opencode local retrieval + local hook-fires.
- Failures beyond the reported list (found in the full log): shim.test, postinstall, resolve-binary, launcher-hook/status, shim-entry, init.test, per above.

## Coverage table (cloud equivalents on macOS)
| Linux-local test (skipped on mac/win) | Equivalent kept/added |
|---|---|
| launcher-hook installed/trust/throws/no-entry | now run on all POSIX via credential fixture (cloud transport) |
| shim-entry installed/trust cases, parity 54/55 | now run on all POSIX via credential fixture |
| launcher-status local | KIND-aware assertions (cloud on non-Linux) |
| c14 claude hook-fires (UDS) | NEW test_verify_claude_hook_fires_cloud (shim -> stub HTTPS) |
| c14 codex hook-fires local | existing codex cloud hook-fires + 7-event table |
| c14 retrieval-returns local (claude/codex/opencode) | existing *_cloud tests (stub bridge) |
| c14 opencode sentinel-preserved | runs with cloud entry on non-Linux |
| init-integration/init.test/resolve-binary/postinstall/shim pass-through/mcp-stdio local | Linux-only by D-032; cloud init covered by init-remote.test.js, wiring-e2e (ubuntu+macos+windows) |
Windows: package-suite never ran on Windows (Unix shapes, #5781); Windows coverage is hook-client matrix + wiring-e2e + latency-gate, unchanged. No Linux-only assertion lost a counterpart there.

## Verification
- Local Linux: test:package 734 pass/0 fail/1 skip; darwin-spoof: 692 pass/0 fail/43 skip; opencode-plugin 37/0; orphan audit, test-count guard (1337, no decrease), size gate (unchanged, no lib/ edits) pass.
- parity-layer2 (test_no_subagentstop_full_lifecycle, fixed 150ms sleep after FNF delta) failed once on the push run, passed on `gh run rerun --failed`: runner flake, unrelated to this change.

## Knowledge Stewardship
- Queried: context_briefing -- #5788/#5784 macOS-first-run lessons applied.
- Stored: nothing -- parity 54/55 were test artifacts, not a macOS defect.
