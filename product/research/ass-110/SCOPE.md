# ASS-110 — Harness wiring placement under per-machine install

Issue: #1040. Input to #1036.

## Question
Where should Unimatrix write its hook and MCP wiring for each harness, now that no install is portable? (Every machine runs its own init; a cloud attach needs explicit per-client approval.)

## Why it matters
nan-024 picked placements to keep committed config portable, then gave that up piecemeal (D-026, D-060: local-scope MCP in global `~/.claude.json`, hooks in `settings.local.json`). Shipping that placement locks users into a future migration. Placement should follow from: project-scoped, per-machine, explicit intent. Advances `personal-cloud` (safe client attach) and `platform` (clean install surface).

## Constraints (fixed)
- R1: no changes to the user's PATH, profile or repo layout.
- Project-scoped: never write to a harness's global state when a project-level surface exists. A harness with no project surface must be named explicitly, and writes there must be Unimatrix-owned, surgical and reversible.
- Explicit intent: a clone never auto-connects; a cloud attach is never implicit.
- Never silently dark (R3); silent outside a Unimatrix project (D-043/D-045).
- Low churn: an install produces no tracked-file diffs for other developers.

## Build on
- `product/research/ass-109/ass-109-findings.md`: surface inventory (Q3, Q6) and gap P2. Re-evaluate P2 without the portability premise.
- nan-024 decisions D-026, D-040, D-046, D-060 (Unimatrix ADRs plus `product/features/nan-024/`).
- #1035 F2: symlinked `~/.claude.json` replaced by a regular file.

## Questions
1. **Inventory**: every file nan-024 writes, per harness (Claude, Codex; OpenCode per #998) and OS family: project, local overlay, user or global.
2. **Harness capabilities**, verified against docs or behavior:
   - Claude: does `.mcp.json` or `settings.json` expand env vars (`${HOME}`, `${CLAUDE_PROJECT_DIR}`)?
   - Claude: what are the approval and trust semantics of project `.mcp.json` vs local scope?
   - Codex: does any project-scoped surface exist (hooks, MCP) and how does its approval model behave?
   - OpenCode: what project and local surfaces exist?
3. **Options per harness**: compare against each constraint and show what passes and fails.
4. **Local vs cloud mode**: does placement differ? How does explicit intent map onto each surface?
5. **Migration from nan-024 placement**: can init idempotently move entries without duplicating them, and remove Unimatrix-owned entries from abandoned locations? How is ownership detected?
6. **Write safety**: symlink-preserving, race-tolerant writes for any global or user file that is unavoidable (#1035 F2/F4). Recommend, don't design.

## Output
`ass-110-findings.md`:
- a placement table (harness × mode → surface), with the evidence for each cell;
- a verdict on each of D-026/D-040/D-046/D-060: keep, revise or supersede;
- migration rules for init;
- a proposed feature scope (in/out, acceptance criteria) ready to become the delivery issue (#1036).

## Out of scope
Fixing anything. Shim, trust check, launcher process model and latency budgets stay unchanged unless placement forces a change.
