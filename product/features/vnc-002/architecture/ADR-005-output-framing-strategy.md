## ADR-005: Output Framing with KNOWLEDGE DATA Markers

### Context

Knowledge entries stored in Unimatrix may contain text that resembles agent instructions (e.g., an entry documenting a convention that says "Always use conventional commits"). When such entries are returned to an agent, the agent might interpret the entry content as a new instruction rather than as retrieved knowledge.

The MCP security analysis identified this as an "indirect prompt injection" vector -- stored content influencing agent behavior. Output framing wraps returned content with markers that signal "this is data, not instructions."

Options:
1. **No framing**: Simplest but leaves the injection vector open
2. **XML-style tags**: `<knowledge-data>...</knowledge-data>` -- familiar but could conflict with actual XML in content
3. **Bracket markers**: `[KNOWLEDGE DATA]...[/KNOWLEDGE DATA]` -- distinctive, unlikely to appear in real content
4. **JSON wrapper**: Content inside a JSON string field -- escaping handles special chars but less readable

### Decision

Use bracket markers `[KNOWLEDGE DATA]` and `[/KNOWLEDGE DATA]` wrapping the content field in **markdown format** responses. The markers are placed on their own lines immediately before and after the entry content.

Markers wrap only the `content` field, not metadata (title, topic, category, timestamps). Metadata appears outside the framing markers in the markdown header and footer.

Summary format does NOT use framing markers — it contains no full content (just ID, title, category, tags, similarity). JSON format does NOT use framing markers — structured data is inherently unambiguous.

### Consequences

**Easier:**
- Agents trained on MCP conventions recognize framing markers as data boundaries
- Bracket markers are visually distinctive and unlikely to appear in legitimate content
- Simple string concatenation -- no escaping, no parsing overhead
- Metadata remains outside the frame, clearly separated from content

**Harder:**
- If an entry's content contains the literal string `[/KNOWLEDGE DATA]`, the framing could be broken. This is extremely unlikely in practice, and content scanning (ADR-002) could add this as a pattern if needed.
- Agents must be trained/instructed to respect framing markers -- this is reinforced by the server's `instructions` field (set in vnc-001)
- Framing adds ~40 bytes per entry to response size (negligible)
