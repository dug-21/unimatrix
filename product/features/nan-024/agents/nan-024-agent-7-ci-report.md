# nan-024-agent-7-ci report (CI half of Part C)

Unimatrix entries consulted: #5790, #5128, #4779, #5896, #5898, #5901, #5894, #5897 (briefing/search; ADR-004, ADR-005 read in full)

## Report
Commits: ea5baaaa, a83b85ad, 88db216a on feature/nan-024 (pushed). Only workflow/script/test-infra files committed.

Files: .github/workflows/nan-024-e2e.yml (new), .github/workflows/ci.yml (+test-count step), .github/workflows/q9-windows-mcp-proof.yml, .github/q9-proof/real.js (new), .github/q9-proof/driver.js, .github/scripts/install-ort-linux.sh,
packages/unimatrix/{package.json (scripts only), test/run-hook-client.js (wiring-e2e set, exclusive claim, audit), test/check-test-count.js (+.test.js), test/fixtures/test-counts.json (baseline from origin/main), test/helpers/e2e-harness.js, test/wiring-e2e.test.js, test/wiring-e2e-local.test.js, test/hook-client/benchmark-wiring.js}.

Last full grid (run 45741013954 for linux-local/layer2; 45740213732 for the rest):
- wiring-e2e ubuntu/macOS x Node 20/24: PASS (4 cells). Windows x20/24: FAIL (trust ACL defect below).
- wiring-e2e linux local (built server, UDS, stored observation read from SQLite, zero Rust exec marker, --provider opencode attribution): PASS.
- parity-layer2 (gating, no continue-on-error): PASS. test-count: PASS.
- latency-gate: ubuntu/macOS BREACH, Windows blocked by init failure.
- Q9 run 45740213753: driver.js forms unchanged; real.js blocked on Windows by the same trust defect.

Not done: ci-config.test.js (not in my task), Linux local path-1 latency, Windows E2E/latency/real-init evidence (blocked).

## Issues / escalations
1. Windows init fails: `trust check failed ... cannot read ACL`. icacls /save emits DACL only (captured in Q9 run, UTF-16LE, `D:(A;;FA;;;BA)...`, no `O:`); lib/trust.js readSddl requires `O:`. Needs an owner source (e.g. PowerShell Get-Acl GetOwner SID) or dropping the owner check. Blocks every Windows E2E/latency/Q9-real result. Not fixed (outside CI scope).
2. Latency, cloud-project path: structurally +45-50 ms over bare node (ubuntu 74 ms vs limit 54; mac 88-126 vs 79). In-process RTT is ~4 ms. ADR-005 path 2 budget (base+15+RTT) is not attainable; needs recalibration decision.
3. Latency, not-a-project: ubuntu PASS (34.9 vs 50). macOS BREACH (85 vs 71.5, baseline p95 56 with median 44; noise-dominated). Pre-check (PRENODE_ENABLED) is the ADR remedy; not enabled by me. Windows unmeasured. D-088: no Windows pre-check enabled.
4. Size gate FAILS on this branch (stripped 115047/113639, raw 203093/201399) from waves 1-4 lib/hook-client growth; hook-client-audit will fail. Cap change is a human decision.
5. package set failures (pre-existing from waves, not mine): vnc-050 R-21 compat matrix; remote install footprint test (380 KB).
6. github.com/microsoft/onnxruntime release assets 404: release.yml's ORT install is likely broken too; my jobs use the PyPI wheel.
7. New jobs are non-required per #5790; ci.yml header comment about Layer 2 is now stale.

## Knowledge Stewardship
- Queried: context_search/briefing/get #5790, #5128, #4779, ADR-004/005 -- CI topology, file-list runner, platform guards applied
- Stored: entry #5906 "CI traps for wiring E2E: icacls /save has no owner, ORT release 404, ls-tree is cwd-relative" via context_store
