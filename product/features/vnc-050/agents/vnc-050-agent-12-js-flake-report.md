# vnc-050-agent-12-js-flake — Report

## Root cause
`test/init.test.js` (`withShippedSkills` + the path-traversal test) and `test/init-integration.test.js` injected fixture skill dirs into the REAL package tree `packages/unimatrix/skills/` and removed them in `finally`. `node --test` runs files in parallel processes; `init-remote.test.js` / `remote-client.test.js` call `initRemote` → `installSkills`, which `readdirSync`s that same tree. A sibling listing a dir (e.g. `nan023-x`) that was then deleted before `readdirSync(src)` → `ENOENT scandir .../skills/nan023-*`. The failing test varied with timing. The `a..b` traversal fixture could also make sibling files throw "Path traversal detected". HOME/tmpdir/cwd were not the cause: `os.homedir` overrides only apply inside one process.

## Pre-existing on main: YES
Ran the 4 files in parallel in a `main` worktree, 5 runs. 4 of 5 failed with the same `ENOENT .../skills/nan023-*` error. It came in with nan-023 (#990), not vnc-050. The worktree has been removed.

## Fix (test files only)
- New `packages/unimatrix/test/helpers/shipped-skills.js`: `withShippedSkills(manifest, fn)` overlays fixture skills by patching `fs` inside the calling process only. The real tree is never written. One shared helper replaces two duplicated copies of the mutation code.
- `test/init.test.js`: the local helper is replaced by the shared one, and the traversal test now uses the overlay.
- `test/init-integration.test.js`: its 4 copySkills/installSkills tests now use the overlay.
- No changes to lib/, package.json or the lockfile.

## Results
- 4 named files plus init-integration in parallel: 8 of 8 runs at 147 pass / 0 fail. Before the fix, 3 of 4 runs failed.
- Full suite `node --test --test-concurrency=4`: no OOM. Memory stayed flat at about 17 GB used of 62 GB, the same as the host baseline. The run hit my 580 s timeout before finishing: 73 top-level suites ok, 0 failures. A complete full-suite run was not observed; it needs more than 10 minutes at concurrency 4. The earlier OOM did not recur at reduced concurrency. Whether the default (16-way) concurrency OOMs on main was not tested.

## Knowledge Stewardship
- Queried: none (no context_briefing call; root cause found directly from failure traces).
- Stored: nothing yet. Candidate lesson: "never mutate the package source tree from node --test files; files run in parallel processes and share the filesystem; use an in-process fs overlay". Recommend storing via /uni-store-lesson (topic: hook-client tests / init).
