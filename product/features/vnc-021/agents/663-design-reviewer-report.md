# Agent Report: 663-design-reviewer

## Task
Design review of proposed bugfix for GH Issue #663 — chunked transfer encoding bypasses body size limit in `McpAdapter::handle`.

## Assessment: APPROVED WITH NOTES

## Findings

### F-01 (non-blocking): Retain Content-Length fast-path rejection
The investigator proposes replacing the Content-Length check. It should be composed with `Limited`, not replaced. The header check is a cheap fast-path; `Limited` is the enforcement backstop.

### F-02 (non-blocking): LengthLimitError not catchable at McpAdapter level
The investigator's claim about catching `LengthLimitError` is incorrect — rmcp's `expect_json` catches the error internally and returns a 500 response. The adapter sees `Ok(response)` with status 500, never the raw error. Recommended approach: pre-consume the body with `Limited::new(body, max).collect().await` in `McpAdapter::handle`, then pass `Request<Full<Bytes>>` to rmcp. This gives proper 413 responses.

## Revised Approach
1. Keep Content-Length fast-path (existing lines 292-301)
2. Decompose request, wrap body with `Limited`, collect
3. On `LengthLimitError` return 413; on other errors return 500
4. Reconstruct `Request<Full<Bytes>>` and pass to rmcp
5. Add test: over-limit body without Content-Length header

## Comment Posted
https://github.com/dug-21/unimatrix/issues/663#issuecomment-4574709469

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- found #4683 (lesson-learned: this exact bug), #4667 (ADR-003 thin adapter), #4665 (ADR-001 constant-time token), #3408 (byte-guard pattern), #83 (ADR-007 enforcement points). All applied.
- Stored: nothing novel to store -- lesson #4683 already captures the takeaway. Review findings are bug-specific, not generalizable beyond existing knowledge.
