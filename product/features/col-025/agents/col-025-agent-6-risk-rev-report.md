# Agent Report: col-025-agent-6-risk-rev

**Mode**: architecture-risk (revision)
**Feature**: col-025 — Feature Goal Signal

## Task

Full replacement of RISK-TEST-STRATEGY.md to reflect settled design decisions from the architecture revision pass (col-025-rev), specifically:

1. ADR-003 revised: SubagentStart routes to `IndexBriefingService` (not `ContextSearch`) when goal is present; goal wins over `prompt_snippet`.
2. ADR-005 revised: one constant `MAX_GOAL_BYTES = 4096`; MCP hard-rejects; UDS truncates. The earlier two-constant discrepancy is resolved and removed from risk register.
3. ADR-006 new: `CONTEXT_GET_INSTRUCTION` constant prepended to all `format_index_table` output — new risk surface for existing tests.
4. Retry overwrite sequence (truncated UDS write → corrected MCP retry → second UDS write) must be verified.

## Changes from First Risk Pass

| Change | Prior Register | Revised Register |
|--------|---------------|-----------------|
| R-04 semantics corrected | "precedence inverted: goal overrides prompt_snippet" (framed as a bug) | "goal-present branch falls through to transcript path instead of IndexBriefingService" (correct framing per ADR-003) |
| R-11 repurposed | Was off-by-one at 2048 (two-constant design, now resolved) | Now covers `CONTEXT_GET_INSTRUCTION` header breaking existing `format_index_table` tests |
| R-12 new | Not present in first pass | SubagentStart IndexBriefingService new integration surface |
| R-13 new | Not present in first pass | UDS truncate-then-overwrite retry sequence correctness |
| R-04 test scenarios revised | Scenario 1 had goal NOT winning; test framed as inversion guard | Scenario 1 now confirms goal wins per ADR-003; all 5 branches documented |

## Artifacts Written

- `/workspaces/unimatrix/product/features/col-025/RISK-TEST-STRATEGY.md` — full replacement

## Risk Summary

- **High-priority risks**: 6 (R-02, R-04, R-05, R-06, R-11, R-12)
- **Medium-priority risks**: 5 (R-01, R-03, R-07, R-08, R-13)
- **Low-priority risks**: 3 (R-09, R-10, R-14)
- **Critical risks**: 0

## Non-Negotiable Test Scenarios (9)

1. `migration_v15_to_v16.rs` with idempotency scenario
2. SubagentStart goal-present → `IndexBriefingService` called; transcript path NOT taken
3. SubagentStart goal-present + non-empty `prompt_snippet` → goal still wins
4. SubagentStart goal-absent → existing `ContextSearch`/transcript path unchanged
5. UTF-8 char-boundary truncation at `MAX_GOAL_BYTES` boundary
6. Full column-value assertion on `insert_cycle_event` round-trip
7. DB error on resume → `None` + warn log + registration succeeds
8. `format_index_table` output starts with `CONTEXT_GET_INSTRUCTION` exactly once
9. UDS truncate-then-overwrite retry: second write overwrites first

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found #2758 (gate-3c non-negotiable test names), #1203, #1204, #2800
- Queried: `/uni-knowledge-search` for "SQLite migration schema version cascade" — found #2933 (informs R-02)
- Queried: `/uni-knowledge-search` for "risk pattern SessionState session resume" — found #3180 (informs R-06), #3301 (informs R-03)
- Queried: `/uni-knowledge-search` for "SubagentStart IndexBriefingService hook injection" — found #3398, #3230, #3297
- Stored: nothing novel to store — all relevant patterns already in Unimatrix; R-11 (format_index_table header breakage) is col-025-specific, not yet a cross-feature pattern
