# Agent Report — vnc-048-agent-7-readme

## Task
Add the canonical per-slug restore procedure to `README.md` per `pseudocode/readme.md` (FR-16 / AC-12 / OQ-3 / SR-07).

## Outcome: COMPLETE

### File modified
- `README.md` — added `#### Backup and restore a per-slug project` subsection inside the `### Container Deployment` section, immediately after the "Serving projects" paragraph (28 insertions). Placed alongside the register/client-bundle operator workflow, matching the README's `####` sub-section heading style and dense operator voice.

### Canonical command sequence as written (order load-bearing)
```
1. unimatrix project register <slug>              # creates {base}/<slug>/{unimatrix.db, vector}, writes [[projects]]
2. unimatrix stop                                 # daemon releases the per-slug stores; the live-PID gate clears
3. unimatrix --project-dir <dir> import --slug <slug> -i dump.jsonl
4. unimatrix start                                # daemon boots and loads the rebuilt index
```

Export side documented: `unimatrix --project-dir <dir> export --slug <slug> -o dump.jsonl` (stderr count summary; `exported 0 entries` = wrong/empty store).

### Required content coverage (per pseudocode + test-plan)
- Ordered `project register → stop → import --slug → start` sequence present as the supported, load-bearing procedure (AC-12 file-check).
- Why `stop` is mandatory: `import --slug` hard-errors while a live daemon holds the store (stale in-memory vector index would clobber the rebuilt one at shutdown); refusal not warning; no `--force` (R-12 / R-03).
- Restore target must be a freshly-registered (audit-empty) slug; re-import into a slug with audit history fails loud → register a fresh slug.
- Vector search works from `start` onward (rebuilt `{base}/<slug>/vector` is what the daemon loads).
- `--slug` targets a store dir under the base derived from `--project-dir`; in-container `exec` (HOME=/data) is the expected posture.
- `--slug` means "a store dir under the base," not "a registered project."
- Section heading kept stable so import's `--slug` help pointer (Component 4, AC-07) does not dangle.

## Discipline / git
- Owned ONLY `README.md`. Touched no `.rs` file.
- `git status --short` showed modified `.rs` files (`export.rs`, `import/mod.rs`, `main.rs`, `projects.rs`, `export_integration.rs`, `import_integration.rs`) — these are **parallel swarm agents' uncommitted work** (agents 4/5), NOT my churn. Deliberately did NOT revert them: doing so would destroy other agents' work (known swarm shared-worktree hazard). No `git reset` / `git checkout <branch>` / `cargo fmt` run.
- Committed path-scoped: `git commit -- README.md` only. Commit `669497ff`, branch `feature/vnc-048`.

## Issues / blockers
- None. Note for the leader: the discipline instruction "revert any change outside README.md" was applied as "do not touch/revert others' in-flight `.rs` work"; a blanket revert in this shared worktree would have wiped agents 4/5. Flagging rather than acting.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` (vnc-048) — surfaced the personal-cloud capability cluster (#5691 backup/restore, #5591 per-slug routing), vnc-048 ADRs #5695 (live-PID-only import refusal) and #5696 (import rebuilds HNSW into slug_dir/vector), and #5079 (`[[projects]]` config read once at boot). Applied #5695/#5696 to phrase the "why stop is mandatory" and "vector search from start" content accurately.
- Stored: nothing novel to store — this is a documentation-only change transcribing already-decided design (captured in vnc-048 ADRs #5695/#5696 and the SCOPE) into the README. No new implementation gotcha or runtime trap was discovered.
