## ADR-002: Content Scanning Architecture

### Context

`context_store` writes must be scanned for prompt injection patterns and PII before storage. Unimatrix is a cumulative knowledge engine -- a single poisoned entry propagates across feature cycles. The MCP security analysis (`product/research/mcp-security/`) identified ~50 pattern categories including instruction override, role impersonation, system prompt extraction, delimiter injection, and encoding-based evasion, plus PII patterns (email, phone, SSN, API keys).

Three architectural questions:
1. **When to compile patterns**: per-request (wasteful), at module load (lazy_static/OnceLock), or at server startup (explicit init)
2. **Hard-reject vs flag-and-store**: reject the request outright, or store with a flag for review
3. **Pattern organization**: flat list, categorized groups, or pluggable pipeline

### Decision

Use `std::sync::OnceLock` for a global `ContentScanner` singleton that compiles all regex patterns exactly once on first access. Hard-reject on any match -- return `ServerError::ContentScanRejected` with the category and matched pattern description. No flagging or review workflow.

Pattern organization: Two categories (injection patterns, PII patterns), each subdivided by `PatternCategory` enum for actionable error messages. Patterns are defined as static data arrays in `scanning.rs`. The scanner checks injection patterns first (higher priority), then PII patterns, and short-circuits on the first match.

Title and content are scanned separately. Title gets injection patterns only (PII in a title is unusual and would be caught in content). Content gets all patterns.

### Consequences

**Easier:**
- Zero per-request regex compilation cost -- patterns are compiled once and reused forever
- `OnceLock` is std-only (no external dependency for caching)
- Hard-reject is simpler to implement and reason about than a flagging system
- Categorized error messages help agents understand what was rejected and why
- No database changes needed (no "flagged" status or review queue)

**Harder:**
- Hard-reject may frustrate agents storing legitimate content that incidentally matches a pattern (e.g., documentation about prompt injection). The initial pattern set must be tuned to minimize false positives.
- Adding new patterns requires a code change and recompile (not runtime-configurable). Acceptable for v0.1; runtime pattern loading could be added in a future feature.
- `OnceLock` initialization happens on first `context_store` call, not at server startup. The first store call pays a one-time ~1ms initialization cost.
