# Agent Report: vnc-026-adr-amendments

Applied the three human-approved ADR amendments flagged by the vnc-026 retro (mode: amendment, no re-litigation). Each amendment preserves the original decision text and appends a clearly-marked `### Amendment (2026-06-08 — vnc-026 retro, human-approved)` section, in both the ADR file and Unimatrix (via context_correct, chain-linked).

## Amendment 1 — ADR-006 (config resolution / worktree roots)

- File: `product/features/vnc-026/architecture/ADR-006-config-resolution-precedence.md`
- Unimatrix: #4756 → corrected to **#4793**
- Appended: (a) root resolution MUST port `project.rs::resolve_git_file` — gitdir-pointer chase for `.git` FILEs (relative resolved against containing dir), walk up to the `.git`-DIRECTORY ancestor, return its parent (main repo root), non-throwing fallback to the containing dir, all return paths realpath-canonicalized; shipped as `resolveGitFile` in `packages/unimatrix/lib/hook-client/config.js` (commit b2e215fd, agent-26/27 reports); `project.rs` is the verified oracle. (b) Enumerated BOTH `projectRoot` consumers — state-dir project hash (ADR-003) and the `{root}/.claude/settings.local.json` config anchor — any future divergence proposal must address every enumerated consumer (lessons #4785/#4791).

## Amendment 2 — ADR-008 (elision anchor wording)

- File: `product/features/vnc-026/architecture/ADR-008-elision-frame-end-anchored-offset.md`
- Unimatrix: #4758 → corrected to **#4794** (1 incoming edge redirected)
- Appended: elided frames anchor at `effectiveEnd` (= `file_len` backed off ≤3 bytes when the file ends mid-UTF-8 character; equals `file_len` for well-formed JSONL), not the literal `file_len` — the Gate 3a WARN A resolution, shipped in `packages/unimatrix/lib/hook-client/delta.js`. All four pinned server-state assertions (a–d) hold against `effectiveEnd`.

## Amendment 3 — vnc-024 ADR-003 (format_injection header as wire contract)

- File: `product/features/vnc-024/architecture/ADR-003-observe-content-negotiation.md` (file exists; amended)
- Unimatrix: #4714 → corrected to **#4795**
- Appended: the `--- Unimatrix Context ---\n` header `format_injection` prepends to every Entries body (`crates/unimatrix-server/src/uds/hook.rs:1040`, served on the HTTP text path) is a LOAD-BEARING WIRE CONTRACT — the TS client (`transform.js`, `INJECTION_HEADER`) dispatches envelope-vs-plain on exactly this prefix for SubagentStart. Server MUST NOT remove/alter the prefix; any change requires coordinated client + Layer-1-golden updates. Provenance: lesson #4783 (`caused_by_feature:vnc-024`).

Note: the spawn prompt cited `uds/observe.rs` for the header; the literal actually lives in `uds/hook.rs:1040` (`format_injection`) — the amendment cites the verified location.

Not committed — leader commits.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- corrected entries #4793/#4795 rank top for the amendment domain, lesson #4785 (enumerate all consumers) surfaced consistently; plus context_get over #4756/#4758/#4714 (originals) and #4793/#4794/#4795 (verified corrections landed with amendment text intact).
- Stored: entries #4793, #4794, #4795 via context_correct (chain-linked amendments of #4756, #4758, #4714 — amendment mode per human approval; /uni-store-adr new-entry flow not used because these are corrections, not new ADRs).
