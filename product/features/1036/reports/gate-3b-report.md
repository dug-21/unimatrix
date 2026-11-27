# Gate 3b Report: 1036

> Gate: 3b (Code Review)
> Date: 2026-11-27
> Result: PASS (with WARNs)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | C1-C16 implemented; CX collapse and `claude.dir` default noted below (WARN) |
| Architecture / ADR compliance | PASS | §6a names enforced by `contract-conformance.test.js` (6/6); git-exclude leg is `claude-code`/`git-exclude` |
| Interfaces | PASS | `updateFileSync`/`writeFileSync` return `{action,file,target,before}`; `SafeWriteError{file,code}` |
| Test/plan alignment & risk coverage | PASS | All affected suites green (see below) |
| Code quality | PASS/WARN | No TODO/FIXME/stubs; no async; new modules <=147 code lines; init.js 632 lines (was 638, pre-existing, PL-10 WARN only) |
| Security | PASS | containment, `wx` temp, enforceMode callers, no new deps |
| NFR-03 / zero-deps / trust.js self-contained / hook-client size | PASS | `check-zero-deps` OK; `check-hook-client-size` OK (114996/115012 stripped; 202841/203058 raw); `trust-selfcontained` 3/3 |
| Stewardship | WARN | All impl reports have Stewardship + Queried + Stored/nothing-novel; 9 reports lack an explicit "Unimatrix entries consulted" line (agents 6,7,9,10,14,15,16,18,19) though Queried entries are present |

## Test runs (node --test per file, all rc=0, 0 fail)
safe-write 22, config-dirs 10, wire-plan 20, wire 99, git-exclude 19, status 17, claude-json 10, codex-install 60 (1 skipped), opencode-install 58, merge-settings 110, hook-ownership 10, install-store 21, init 27, init-integration 26, trust 34, trust-selfcontained 3, contract-conformance 6, home-guard 5, launcher-status 14, codex-toml-surgical 31, hook-client/credstore 36.

## Explicit checks

(a) **outside-project code**: PASS. `trust.js:177` throws `new SafeWriteError("outside-project", file, "<file> resolves to <target>, outside project root ...")`, naming link and target. `safe-write.test.js:173` asserts `code` via `isCode("outside-project")` AND message contains link and target; codex-install, init and merge-settings tests also assert `e.code`.

(b) **claude.dir default**: WARN. Code sets default `claude.dir = <home>/.claude`; pseudocode `config-dirs.md:42` says `home`. `claude.dir` is not consumed outside the resolver (only `claudeJson`/`userSettings`/`source`/`value` are), and `config-dirs.test.js:14` pins `/home/u/.claude`. Behaviorally harmless; the code value is the more accurate one. Recommend aligning pseudocode text; no rework.

(c) **status --json leak**: PASS. `run()` strips keys starting `_` (so `_dirs`, `_root`); `configDirs` is built by `configDirsOf` with exact §6a sets. `status.test.js:50` (R-10) asserts key sets and no `"_dirs"`/`"value"` in spawned stdout.

(d) **Codex surfaces collapsed to CX**: ACCEPTABLE (WARN-level note). `pseudocode/wire-plan.md:36-37` itself specifies a single CX step with W3/W4/R7-R9 kept in the existing internal order inside `maybeWireCodex` (gating "none" for R7-R9 per ARCH §3.3). Dry-run equals real (same `run(dryRun)`). Per-surface coverage exists in codex-install.test.js (hooks/mcp/project removals, outside-project, malformed). The test-plan (`test-plan/wire-plan.md:6`) lists W3/W4/R7-R9 ids while `wire-plan.test.js` asserts `CX`; plan/test text mismatch is cosmetic.

(e) **Real HOME isolation**: PASS. Every spawned fixture (wire.test.js 929/1076/1152, init-integration 497/540/664, status 200) builds env via `gitEnv`/`scrubbedEnv` with `HOME` from `assertTmpHome`, which refuses non-tmp and real home; `scrubbedEnv` strips the four override vars and asserts any re-added override is under tmp. `wire-fixture.wireOpts` calls `assertTmpHome`. I snapshotted mtimes of `~/.codex/{hooks,config}`, `~/.unimatrix/{install.json,bin}` around 11 re-runs of the heaviest suites: no change. Observation: those real-home paths carry mtime 02:00:30 (before this gate began, during wave-4 agent work), i.e. a pre-gate real-home write exists; not reproducible from the current tests. Recommend the SM confirm that was a manual/dogfood run, not a fixture.

## Other findings
- enforceMode callers: install-store (root marker 0600, shim/bin 0700, install.json 0600), credstore (0600), trust.json via core 0600. All pass enforceMode.
- `Atomics.wait` sync sleep in `trust.js:95` for win32 retry is synchronous (NFR-03 holds).
- Remaining `fs.renameSync` in install-store is the package-dir swap (D-020 exempt); hook-client state/queue/last-hook writers are hook runtime (D-020 exempt, asserted by conformance test).
- Known open items (remote-client footprint 473KB vs 440KB; remote.json spawned mode-drift) not failed per instruction.

## Rework Required
None.

Unimatrix entries consulted: #5763, #5958, #5768, #2463, #5822, #5774, #5803; ADR-INDEX 5937, 5960, 5962, 5959, 5955, 5963, 5961 (summarized via index, not individually re-read).
