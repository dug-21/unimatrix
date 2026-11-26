# nan-024-agent-8-latency report

Unimatrix entries consulted: #5898 (ADR-005), #5904, #5902, #5372, #1198 (briefing); ADR-004/005 semantics kept

## Report
Files modified (uncommitted): packages/unimatrix/bin/unimatrix.js (child_process lazy, only in passThrough), lib/hook-route.js (trust.js lazy, only when preflight.trustChecked absent; child_process lazy in spawnRust), lib/shim-entry.js (crypto lazy inside computeProjectHash), lib/hook-client/index.js (transport-uds lazy, only for mode=uds). Size gate: -35 B stripped/raw (115047 -> 115012); still over cap (pre-existing).

Profile (Linux, Node 18.20.8, noisy box: node -e 0 median ~48 ms):
- Post-bootstrap hook process ~42 ms: require tree ~25-30 ms (crypto 3.3, hook-route 5, hook-client/index tree 12 incl. build-request 3.7, build-request-tools 3.1, transport-http 3.5 of which http+https 2.4, project-state/config/remote-resolve ~3.5), then run phase ~15-18 ms (TLS connect, handshake, state/IO).
- --cpu-prof: no hotspot. Time is spread across V8 internalCompileFunction (~5.5 ms), builtin loader (~4.4), compileFunction (~4), GC (~3), module resolution stat/realpath (~2.5). Floor = file count + builtin loads, not any one module.
- crypto cannot be avoided in the cloud path (project hash in shim, state.js, config.js, tls); the shim lazy only helps non-project paths that return before hashing.
- bin/unimatrix.js and hook-route.js were already largely lazy (init/wire/merge-settings/codex/opencode/status load only in their verbs). Nothing further to hoist there.

Before/after (benchmark-wiring.js, same machine, 50 iters, node baseline from same run; run-to-run noise ~+-10 ms p95):
| case | before med/p95 | after med/p95 |
|---|---|---|
| node -e 0 baseline | 49.7 / 59.2 | 50.2 / 59.7 |
| cloud FNF (PostToolUse) | 113.4 / 132.1 | 105.3 / 117.1 |
| cloud sync (UserPromptSubmit) | 107.9 / 120.6 | 110.1 / 130.2 |
| not-a-project hook | 60.4 / 69.2 | 62.2 / 76.2 |
| mcp-stdio not-a-project | 59.8 / 69.8 | 62.0 / 73.9 |
Targeted A/B on the cloud PostToolUse path (interleaved-free, 40 samples): med 105.8 -> 100.5-104.0, i.e. ~2-5 ms. The not-a-project/mcp-stdio paths were not code-changed beyond the shim crypto lazy; their delta is noise.

Tests: node --test launcher-hook, fire-time-parity, launcher-status, shim-{cmd,entry,sh,superset}, shim, hook-client/*.test.js: 1173 tests, 0 fail (1 skipped pre-existing). Size gate: still FAIL (pre-existing over-cap).

## Remaining gap / options
Cloud path stays ~+50 ms over bare node (limit base+15 -> ~65-79). Structural floor: ~25-30 ms module load/compile + ~15 ms TLS/IO run phase; lazy requires recover only a few ms. Options (human decision):
1. Recalibrate path-2 budget (ADR-005) to base+~55 ms for cloud-project.
2. Single-file bundle of hook-client + shim/launcher path (fewer stat/read/realpath ops; est. 8-12 ms) -- build step + size-gate accounting decision.
3. V8 compile cache: module.enableCompileCache() exists only on Node >=22.1 (not on 18/20; CI matrix is 20/24); node:v8 cache for 24 only. Behaviour/disk-state change; needs decision. Not enabled.
4. PRENODE pre-check (not enabled, D-088) only helps the not-a-project path.

## Knowledge Stewardship
- Queried: context_briefing (#5898, #5904, #5902) -- ADR-005 budget, launcher guard, require-cycle trap
- Stored: nothing novel to store -- findings are measurement results recorded in this report (no hotspot; floor is distributed module compile/load)
