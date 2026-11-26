# nan-024-agent-5 report: hook-client test fix + footprint

Unimatrix entries consulted: #5123, #5125, #4878 (briefing; none changed the approach)

## 1. config.test.js test_homedir_resolution_degrades_to_null_state_dir
Root cause: OUR change. `config.resolve()` wrapped effectiveEnv in try/catch, but `resolveRemote()` (remote-resolve.js) now calls `effectiveEnv(..., os.homedir())` unguarded; throwing homedir -> exception -> resolve() catch -> ok:false. Not pre-existing (main had no homedir call there). Not a D-052/D-091 mandated behaviour change; fail-open requires degrade to raw env.
Fix (code only, test untouched): lib/hook-client/remote-resolve.js - effectiveEnv call wrapped in try/catch, falls back to raw env.
Result: node --test test/hook-client/ -> pass 1000, fail 0.

## 2. 380 KB footprint gate (cap untouched)
Current: 414,113 bytes (lib=345,693, skills=68,420) vs 380,000; main ~370,112. Over by ~34 KB. Figure moves because other agents' in-flight files change (opencode-install.js currently 12,161 B smaller than main).
Added/changed in lib vs main (bytes now, delta):
- install-store.js 10,437 (+10,437)
- trust.js 8,583 (+8,583)
- shim-entry.js 7,189 (+7,189)
- resolve-runtime.js 6,350 (+6,350)
- claude-json.js 5,659 (+5,659)
- hook-route.js 4,791 (+4,791, in flight)
- project-state.js 4,404 (+4,404)
- hook-ownership.js 4,071 (+4,071)
- merge-settings.js 18,118 (+1,619)
- mcp-entry.js 1,234; shim/unimatrix.sh 1,222; hook-client/last-hook.js 885; shim/unimatrix.cmd 369
- hook-client/* small edits: config +155, remote-resolve +246(+ this fix), mcp-bridge +258, index +150
- resolve-binary.js 537 (-1,460)
- opencode-install.js 8,957 (-12,161, in flight)
New-file total approx 55 KB; net with removals ~+44 KB at HEAD before in-flight shrink.

Slimming options (proposed, not applied):
1. Strip/condense comment blocks in new files (long D-xxx rationale headers): est. 8-12 KB; footprint counts raw bytes, comments included.
2. Merge overlapping modules (trust.js + hook-ownership.js + project-state.js share path/hash/json read helpers; resolve-runtime.js vs resolve-binary.js remnants): est. 4-6 KB.
3. Dedupe atomic-write / safe-read-json helpers across install-store, claude-json, merge-settings, hook-client/state.js into one shared module: est. 3-5 KB.
4. Drop dead code from legacy opencode/codex install paths once wave 3 replaces them (opencode-install already -12 KB): est. 5-15 KB.
5. Trim skills/ (68 KB; unchanged by us): any SKILL.md duplication - est. unknown, needs separate review.
Even with 1-4 (~20-38 KB), Wave 3/4 (launcher, codex-install, wire-init) additions will likely need either aggressive trimming or a human cap decision on GH #994 precedent.

## 3. check-hook-client-size.js (caps untouched)
stripped=115,047 / cap 113,639 (OVER by 1,408); raw=203,093 / cap 201,399 (OVER by 1,694). Largest: index.js 19,259 raw; tls-guard.js 871; last-hook.js 842 stripped. Hook-client growth from Waves 1-2 (+~1.4 KB over main: config, remote-resolve, mcp-bridge, index, last-hook) pushed it over; needs trimming in hook-client or a human cap call.

## Knowledge Stewardship
- Queried: context_briefing - nothing directly applicable (#5123/#5125 are config repoint history).
- Stored: nothing yet - candidate lesson: "any new os.homedir()/effectiveEnv call on the hook resolve path must be try/wrapped; test_homedir_resolution_degrades guards it".
