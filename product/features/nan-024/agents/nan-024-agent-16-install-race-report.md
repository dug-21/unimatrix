# nan-024-agent-16-install-race report

## Report
- Root cause: `copyPackage` in `packages/unimatrix/lib/install-store.js` did `rmrf(dest)` then `rename(tmp, dest)` after a non-atomic `isCompleteCopy` check. Concurrent inits (or a hook during upgrade) could delete another run's valid dest, so trust reported `missing`.
- Fix: unique tmp (pid+random); never delete a valid dest (complete copy wins, tmp discarded); rename only when dest absent/invalid; rename failure re-checks validity (ENOTEMPTY/EEXIST/EPERM treated as success); invalid dest moved aside by rename, not rm'd. install.json swap, prune, modes/ACL, trust.js refresh unchanged.
- Test: added 12-process stress test with a polling reader (fails on old code, passes 40/40 loops on new).
- Results: test:package 735 pass / 0 fail / 1 skip; audit PASS (90/90 suites); size gate OK (hook-client untouched); package.json/lockfile unchanged.
- Files: packages/unimatrix/lib/install-store.js, packages/unimatrix/test/install-store.test.js

Unimatrix entries consulted: #771, #5825, #735, #770, #4578 (briefing; none directly applicable)

## Knowledge Stewardship
- Queried: context_briefing -- no relevant install-store entries
- Stored: lesson-learned "rmrf(dest)+rename(tmp,dest) directory install is a race; never delete a valid dest"
