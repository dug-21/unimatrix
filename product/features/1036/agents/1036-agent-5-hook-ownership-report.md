# 1036-agent-5-hook-ownership — C11

Files: packages/unimatrix/lib/hook-ownership.js, packages/unimatrix/test/hook-ownership.test.js
- Added `isLegacyOpencodePlugin(el)` + export, per pseudocode/hook-ownership.md.
- Test: all positives/negatives from test plan. `node --test test/hook-ownership.test.js`: 10 pass, 0 fail.
- `isUnimatrixOpencodePlugin` absent from lib/test/bin. No deps added. Not committed (per instruction).
- Size gate N/A (lib/hook-ownership.js is outside lib/hook-client/).

Unimatrix entries consulted: #5947, #5763, #5743

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- OpenCode ADRs (nan-024 ADR-006, vnc-049 ADR-006); nothing bearing on predicate logic
- Stored: nothing novel to store -- trivial pure predicate
