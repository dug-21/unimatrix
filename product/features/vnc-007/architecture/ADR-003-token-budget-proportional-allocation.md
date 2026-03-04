## ADR-003: Token Budget with Proportional Section Allocation

### Context

The MCP and UDS briefing paths use different budget models:
- MCP: token-based (default 3000 tokens), linear fill (conventions > duties > context)
- UDS CompactPayload: byte-based (default 8000 bytes), per-section byte constants (DECISION_BUDGET_BYTES=3200, INJECTION_BUDGET_BYTES=2400, CONVENTION_BUDGET_BYTES=1600, CONTEXT_BUDGET_BYTES=400)

BriefingService needs a single budget model. The MCP interface already exposes `max_tokens`, making tokens the natural unit. The UDS path converts bytes to tokens before calling BriefingService.

The question is how to allocate budget across sections when injection history is present (the CompactPayload path). The current byte budgets represent fixed proportions: 40% decisions, 30% injections, 20% conventions, 5% context header (with ~5% overhead).

### Decision

BriefingService uses a token budget (`max_tokens: usize`). Budget allocation depends on which entry sources are active:

**When injection_history is present** (CompactPayload path):
Fixed proportional allocation preserving current ratios:
- Context header: 5% of max_tokens
- Decisions: 40% of max_tokens
- Injections: 30% of max_tokens
- Conventions: 20% of max_tokens
- Remaining: 5% buffer

Each section fills up to its allocation. Unused budget from earlier sections does NOT cascade to later sections (preserving current CompactPayload behavior where each section has an independent cap).

**When injection_history is absent** (MCP/Briefing path):
Linear fill in priority order: conventions first, then relevant_context. Each entry is counted in tokens (estimated as `(title.len() + content.len() + 50) / 4`). When remaining budget is exhausted, stop adding entries. This preserves the current MCP briefing behavior.

The UDS transport converts its byte budget: `max_tokens = token_limit.unwrap_or(MAX_COMPACTION_BYTES) / 4`. For the default MAX_COMPACTION_BYTES=8000, this yields max_tokens=2000.

### Consequences

- **Easier**: Single budget unit (tokens) throughout the service. No char/byte confusion.
- **Easier**: MCP path passes max_tokens directly from its interface with zero conversion.
- **Harder**: The byte-to-token conversion (`/ 4`) is an approximation. For non-ASCII content, actual token counts may differ. This is acceptable because the current byte budgets are also approximate.
- **Risk**: The proportional allocation for injection sections may produce slightly different entry counts than the current fixed byte budgets due to the conversion. SR-03 from the risk assessment tracks this. Snapshot tests should compare old vs new output.
