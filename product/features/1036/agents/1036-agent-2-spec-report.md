# 1036-agent-2-spec report

- Output: `product/features/1036/specification/SPECIFICATION.md`
- AC-1..AC-10 carried with verification methods. AC-8 is reworded per D-011, and AC-3/AC-10 are extended per D-010.
- Defined per the spawn prompt:
  - D-007 same-file rule (FR-12);
  - Removal Target Table RT-01..RT-10 (FR-10);
  - hardened-writer cases S1–S12 (FR-20..FR-33);
  - AC-8 git decision table FR-40..FR-46: no-git, not-repo/bare, tracked, overridden, worktree, submodule, prefixed entry text, cwd = R with GIT_* scrubbed;
  - status lines FR-34..FR-37;
  - init resolved-file, same-env and D-013 notice lines FR-50..FR-53;
  - dry-run parity FR-60..FR-63;
  - env edge values FR-06.
- Open questions: OQ-1..OQ-7 (in the spec).

Unimatrix entries consulted: #5803, #5763, #5911, #5774, #5837, #5768 (briefing).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing: #5803 (tracked-settings lesson), #5837 (own every surface), #5911 (tmp-home test guard → NFR-05), #5774 (precondition surfacing on success legs), #5763/#5768 (nan-023 wire manifest / verification spine).
- Stored: nothing (read-only tier).
