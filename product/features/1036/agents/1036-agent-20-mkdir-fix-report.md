# 1036-agent-20-mkdir-fix report

## Fix
- `packages/unimatrix/lib/init.js` `installSkills`: removed eager `mkdirSync` of `.claude/skills` and each `skills/<name>` (real path only; dry-run already skipped). Parent creation now happens solely in the writer (`trust.js hardenedWriteSync`), which runs after `checkContained` on the resolved real target (safe-write `updateBuffer`), and never in dry-run. Out-of-root `.claude` link now refused before anything is created (D-026, FR-22a).

## Sibling audit
- `trust.js hardenedWriteSync` mkdir: preceded by containment check; dry-run returns earlier. OK.
- `git-exclude.js` (`.git/info/`): exempt per D-026.
- `install-store.js`, `trust.js` store dirs: user-home store, not project files.
- codex-install / wire / merge-settings: no direct mkdir; all via writer. No new test needed.

## Tests (node --test)
init 27/27, wire 99/99 (incl. test_ac3_parent_dir_link_outside_root_refused), safe-write 22/22, install-store 21/21, codex-install 60/60.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- #5962 ADR-003 (one hardened writer) relevant; confirmed writer-owned parent creation.
- Stored: nothing novel to store -- the lesson (callers must not pre-mkdir; writer owns parent creation post-containment) is implied by ADR-003.

Unimatrix entries consulted: #5962
