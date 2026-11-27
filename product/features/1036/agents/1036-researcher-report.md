# 1036-researcher report

- SCOPE.md: `product/features/1036/SCOPE.md` (written from #1036 revised scope, AC-1..AC-10 verbatim, D-001..D-006).
- Key findings:
  - None of `CLAUDE_CONFIG_DIR`, `OPENCODE_CONFIG_DIR` or `XDG_CONFIG_HOME` is referenced in `packages/unimatrix/lib` or `bin`.
  - `lib/codex-install.js` `codexHome()` is the resolver pattern to reuse.
  - `lib/atomic-write.js` uses a pid temp name, no `O_EXCL`, and renames onto the link path. It is the shared writer behind the Claude, Codex and OpenCode writes.
  - `install-store.js` and `trust.js` already have a random temp name and a Windows rename retry.
  - `lib/status.js` has no Claude MCP presence line.
  - G3 (`plugin[]`) has no removal code.
  - No production file is near the 1,000 code-line cap; the largest is `init.js` at 391.
- Open questions: in SCOPE.md (writer coverage, twin equal-path rule, gitignore edge cases, Claude lost-update, Codex live check).

Unimatrix entries consulted: #5896, #5897, #5763, #5803, #5837 (lookup listed #5894, #5900, #5901, #5924).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing: nan-023 ADR-001, nan-024 ADRs, lessons #5803 and #5837.
- Stored: nothing new. The findings are feature-specific or already captured in the ass-110 findings.

## Revision r2 (1036-researcher-r2)
- SCOPE.md updated for D-007..D-013: hardened writer covers every written file (D-010; AC-3/AC-10 extended); In-6/AC-8 retargeted to `git rev-parse --git-path info/exclude` (D-011; D-039 verdict and R1 updated, tracked-`.gitignore` edit removed); twin real-path rule (D-007); AC-8 edge cases to spec (D-008); file-level Codex/Claude checks + #1053 (D-012); lost-update accepted, sync read-modify-rename, init hint (D-013).
- Open questions: none.
- Unimatrix entries consulted: briefing only (#5803, #5837, #5763); no new entries needed.
- Knowledge Stewardship: Stored nothing — changes are feature-specific decisions already in DECISIONS.md.

## Revision r3
- SCOPE.md updated for D-014..D-024: In-3 twins limited to nan-024 legacy locations (D-019); In-4 coverage per D-020, writer core in trust.js (D-021), modes per D-023; AC-4 meaning (D-022) and opencode.json parse handling (D-024) added to Constraints; Design Decisions section added. AC wording unchanged.
- r4: D-025..D-029 applied (AC-1/2/3 amended, In-3/In-4, constraints, decision lines).
- r5: D-030 note added; header cites D-001..D-030.
