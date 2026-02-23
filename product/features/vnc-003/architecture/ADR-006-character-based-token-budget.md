## ADR-006: Character-Based Token Budget Estimation

### Context

`context_briefing` accepts a `max_tokens` parameter to control output size. Accurate token counting requires a tokenizer (e.g., tiktoken for Claude, sentencepiece for other models). Adding a tokenizer dependency is not justified for a budget estimation feature.

### Decision

Use character-based estimation at ~4 characters per token. The `max_tokens` parameter (default 3000) is converted to a character budget: `budget_chars = max_tokens * 4`. Content is assembled in priority order (conventions, duties, relevant context) and truncated when the character budget is reached. Truncation removes least-relevant entries first (from the relevant context section, lowest similarity first).

The 4:1 ratio is a rough average for English text with code snippets. It underestimates for code-heavy content and overestimates for prose. This is acceptable: the budget is a soft guideline, not a hard limit.

### Consequences

**Easier:**
- No tokenizer dependency
- Simple implementation: running character count
- Budget enforcement is predictable and fast

**Harder:**
- Actual token count may vary by +/- 30% from estimate
- No model-specific accuracy (Claude tokenizes differently than GPT)
- If precise budget control becomes important, a tokenizer dependency would need to be added (deferred)
