# vnc-050-agent-14-js-hang Report

## Report
- Root cause: `npm test` = bare `node --test` (Node v18.20.8) treats every .js under test/ as a test, incl. test/fixtures/mcp/stdio-mcp-fixture.js (stdio MCP server; waits on stdin forever). Confirmed: `node --test test/fixtures/mcp/stdio-mcp-fixture.js` -> rc=124. CI unaffected (uses run-hook-client.js non-recursive selection sets).
- Reproduces on main: YES (worktree, same rc=124; worktree removed).
- Orphans killed: 1055752/1055753/1055931 (Nov 24 node --test running the fixture). Unimatrix servers untouched.
- Fix (uncommitted): packages/unimatrix/package.json `test` -> hook-client set, parity-layer2, package, opencode-plugin, audit.
- Coverage diff: bare set 77 files = 63 *.test.js + 14 non-suite (fixture, helpers, runners, c14-verifier, check scripts, benchmark-spawn.js). New script runs 62/63 suites; only test/hook-client/benchmark-spawn.test.js dropped (CI-documented exclusion: machine-variant perf gate). headless-smoke, compat-matrix, init*, ca-trust-tls, observe-latch all included. Audit PASS 63/63.
- Full run rc=0: hook-client 929 tests/928 pass/0 fail/1 skip; package 405/404/0/1; opencode-plugin 37/37/0/0; parity-layer2 24/24/0/0. Total 1395 tests, 1393 pass, 0 fail, 2 skip.
- Failures triage: none.

## Knowledge Stewardship
- Queried: none this agent (task was diagnostic).
- Stored: nothing novel to store -- #995/lesson #5781 already cover CI selection sets; this is the local `npm test` counterpart.
