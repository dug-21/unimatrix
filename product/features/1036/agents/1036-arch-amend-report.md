# 1036-arch-amend report (uni-architect)

Gate 3a contract sync. Wording only; no design changes.

## Changes
- ARCHITECTURE.md §3.3 X1: harness `claude-code`, role `ensure (surface git-exclude)`.
- ARCHITECTURE.md §6: `updateFileSync` returns `before: string|null`; `ConfigDirs` per-harness `value` (raw env value, null for default; not in `status --json`); `ensureSettingsLocalIgnored` leg harness `claude-code`, surface `git-exclude`.
- ARCHITECTURE.md §6a: new row "Git-exclude leg (X1)", replaces `git`/`exclude`. `configDirs` JSON field set unchanged.
- IMPLEMENTATION-BRIEF.md: Data Structures (`value`), Function Signatures (`before`, leg identity), 3a open-items note marked resolved.
- ADR-006 #5943 corrected to #5963 (it stated harness `git`, surface `exclude`). ADR-INDEX row and all #5943 references in ARCHITECTURE/BRIEF updated.
- ADR-002 #5960 and ADR-003 #5962 do not state the old values; unchanged.

Unimatrix entries consulted: #5943, #5960, #5962, #5959

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned 1036 ADR-006 (#5943) and ADR-004; #3614/#3611 (multi-doc signature correction) applied.
- Stored: entry #5963 "ADR-006 (1036)" via context_correct of #5943; nothing else novel to store -- wording sync only.
