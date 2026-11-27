# 1036-agent-18-tests-a — C17 tests part A

Files (not committed): packages/unimatrix/test/wire.test.js (+16 tests, appended blocks "1036 AC-3 project-file links via spawned init" and "1036 S-3 D-025 enforced modes via spawned init"). install-store.test.js and safe-write.test.js unchanged (existing coverage already matched plan; see gaps). lib/ untouched.

## Results
- wire + install-store + safe-write: 139 tests, 138 pass, 1 fail.
- New: AC-3 spawned 11 pass; S-3 spawned 4 pass.

## Lib bug exposed (failing test)
- `test_ac3_parent_dir_link_outside_root_refused` (test/wire.test.js). Setup: `<root>/.claude` is a symlink to an outside dir. Expected: exit 1 `outside project root`, outside dir untouched. Actual: exit 1 with correct refusal message, BUT init first creates `skills/` inside the outside dir (readdir of outside = ['skills']). A directory is created through an out-of-root parent link before containment refuses. Likely the skills/dir mkdir path in init.js lacks containWithin. Not weakened.

## Gaps (not written, time/effort)
- AC-3: swap-at-before-rename fault seam not reachable via spawned init (no fault pass-through in init/wire); covered only by safe-write.test.js existing case. Dry-run for outside/dangling/prefix cases not added (only the outside-link case covers dry-run).
- AC-3 user/global outside-home links: already covered by existing wire.test.js in-process AC-3 block; not re-added.
- AC-4 RT-07 Codex project/user removal through wire()/init and RT-01..10 rows: existing table test covers RT-01..04/06..10 in-process; Codex RT-07 not added.
- AC-10 temp-leaves-none through init(): existing R-12 rename-failure tests (wire.test.js) cover it in-process; no spawned variant (no fault seam).
- S-3: remote.json drift is not exercised (file is absent in a local install; its presence changes project kind). install.json, trust.json, root marker, bin/* covered; "keep mode" control covers settings.local.json only, not ~/.claude.json or project opencode.json or skills.
- trust.json symlink tests: spawned exit-1 exact-message test added (live + dangling).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- #5911 (temp-home/assertTmpHome rule), #5776 (wire orchestrator traps), #5777 (hook commandSource by transport), #5768 (ADR-006 C14 spine). No new info beyond the plan.
- Stored: nothing novel to store -- the parent-dir mkdir leak is a single lib bug for the fixer, not yet a generalized pattern.
