## ADR-004: Format-Selectable Responses

### Context

All four tools must return responses that agents can consume effectively. The original design called for dual-format responses (markdown + JSON in every `CallToolResult`), but this doubles context window consumption for the receiving agent — every entry's content appears twice. For search results returning 5 entries, the wasted context is significant.

Agents typically scan search/lookup results to identify relevant entries, then fetch full content for the 1-2 entries they care about. Sending full content for all results by default is wasteful.

Options considered:
1. **Dual blocks (markdown + JSON)**: Both formats in every response — 2x context cost
2. **Markdown only**: Simple but no structured option for programmatic consumers
3. **JSON only**: Structured but loses output framing benefits for agent safety
4. **Format parameter with summary default**: Agent chooses format; default optimizes for minimal context

### Decision

Each tool accepts an optional `format` parameter with three values:

- **`summary`** (default): One compact line per entry — ID, title, category, tags, similarity score (if search). Minimal context window footprint. Agents call `context_get` for full content on entries they need.
- **`markdown`**: Full entry content with metadata header and `[KNOWLEDGE DATA]` output framing. For when the agent needs complete entries inline without a second round-trip.
- **`json`**: Structured JSON object (single-result tools) or array (multi-result tools). For programmatic consumers or agents that prefer parsing structured data.

`context_get` returns full content in all three formats (single-entry fetch — summary would be pointless).

`context_store` responses use the requested format to describe the created or duplicate entry.

Only one content block is returned per response — never two.

### Consequences

**Easier:**
- Default summary format minimizes context window consumption (~50 tokens/entry vs ~500)
- Agents that need full content use the existing `context_get` tool — natural two-step pattern (scan → fetch)
- Format choice is per-request — agents can use summary for exploratory searches and markdown for targeted lookups
- Single content block per response — no ambiguity about which block to consume
- Output framing only appears in markdown format where it's meaningful

**Harder:**
- Summary format requires a second tool call (`context_get`) to read full entry content — adds latency for agents that need all results in full
- Three code paths in `response.rs` instead of one
- `format` parameter must be added to all four tool parameter structs and validated
