## ADR-003: Priority-Based Token Budget Allocation

### Context

The compaction payload has a 2000-token budget (~8000 bytes at 4 bytes/token). Different categories of knowledge have different preservation priorities. ASS-014 research (data-model.md Section 9.2) proposed a priority-ordered allocation:

1. Active decisions (ADRs) — constrain what the agent can do
2. Session context metadata — "where am I?" information
3. High-confidence injections — entries the agent has already seen
4. Cross-cutting conventions — patterns and rules for current work

The question is whether to use fixed allocation (each category gets a set byte budget) or dynamic allocation (fill categories in priority order until total budget is exhausted).

### Decision

Use dynamic priority-based allocation with soft caps per category. Categories are filled in priority order. Each category has a soft cap (maximum bytes it can consume) to prevent any single category from dominating the payload. Remaining budget after a category is under-filled rolls over to the next category.

Constants:
- `MAX_COMPACTION_BYTES = 8000` (~2000 tokens total)
- `DECISION_BUDGET_BYTES = 1600` (~400 tokens, soft cap for decisions)
- `CONTEXT_BUDGET_BYTES = 800` (~200 tokens, soft cap for session context)
- `INJECTION_BUDGET_BYTES = 2400` (~600 tokens, soft cap for re-injected entries)
- `CONVENTION_BUDGET_BYTES = 1600` (~400 tokens, soft cap for conventions)

Remaining budget (~1600 bytes / ~400 tokens) serves as buffer for formatting overhead and rollover.

Fill order:
1. Session context (role, feature, compaction count) — always first, minimal bytes
2. Active decisions — entries with `category: "decision"` from injection history, or from fallback query
3. High-confidence injections — remaining entries from injection history sorted by confidence
4. Conventions — entries with `category: "convention"` from injection history, or from fallback query

### Consequences

**Easier:**
- Budget rollover ensures efficient use of the total budget — if few decisions exist, more room for injections
- Named constants are easy to tune after empirical observation
- Priority order matches agent needs post-compaction (decisions are most critical for correct behavior)
- The soft cap prevents edge cases (e.g., a session with 50 decisions consuming the entire budget)

**Harder:**
- Initial allocation values are theoretical — may need adjustment after real compaction events
- Dynamic allocation is slightly more complex to implement than fixed slices
- If the knowledge base is sparse (few decisions, few conventions), the payload may be mostly re-injected entries — acceptable but not optimal
