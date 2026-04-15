# ASS-049: Multi-LLM MCP Client Compatibility

**Date**: 2026-04-10
**Tier**: 2 (independent — no Wave 2 spike dependencies)
**Feeds**: W2-6 (multi-LLM compatibility), W2-0 (tool description strategy)
**Related**: ASS-041 (confirmed MCP HTTP transport design), ASS-048 (OAuth 2.1 auth model)

---

## Question

Does Unimatrix work correctly with MCP-compatible LLM clients beyond Claude Code — specifically Codex (OpenAI) and Gemini (Google)? If not, what needs to change?

This is the single most important Wave 2 feature from a strategic positioning standpoint. The primary defensible moat for Unimatrix is vendor independence — the argument that enterprise teams should not couple their institutional memory to a single LLM provider. That argument is hollow if Unimatrix only works well with Claude.

The question is NOT "can we make the protocol work" — the MCP standard is provider-agnostic and protocol-level compatibility is expected. The question is "does Unimatrix behave consistently and usefully when agents from different LLM providers use it" — and what work is required to close the gaps.

---

## Why It Matters

Unimatrix's competitive differentiation depends on being the memory layer that survives LLM provider transitions. If an enterprise team switches from Claude to Codex, or runs both in parallel, their institutional knowledge stays intact and fully accessible. This claim requires that Unimatrix actually works well with both — not in theory, but in practice.

LLM lock-in fear is the primary enterprise buyer concern in 2026. An honest "works with Codex, Gemini, and Claude" claim is worth more than any compliance certification for the first wave of enterprise sales.

---

## What to Explore

### 0. MCP Client Capability Survey

Map the current MCP HTTP transport support status for each target client:

**Codex (OpenAI CLI)**:
- Does `codex` support MCP HTTP transport (Streamable HTTP / SSE-over-HTTP)?
- Does it support the `Authorization: Bearer` header on all requests (initial + tool call POSTs)?
- What is the configuration mechanism (CLI flag, config file, environment variable)?
- What MCP spec version does the current release target?
- Is there a known equivalent of the Claude Code anthropics/claude-code#28293 header-forwarding bug?

**Gemini (Google Gemini CLI / AI Studio MCP)**:
- Does the Gemini CLI or Gemini API support MCP HTTP transport?
- Same header and config questions as Codex.
- MCP spec version targeted.

**Other clients worth evaluating** (lower priority):
- Continue (VSCode extension, open-source): MCP HTTP support status?
- Cursor: MCP support status and transport type?
- Zed: MCP support and transport type?

The output of §0 is a capability matrix: which clients support which transport and auth patterns. Clients with no MCP HTTP support are out of scope for Wave 2.

---

### 1. Tool Description and Behavioral Compatibility

Unimatrix's tool descriptions, parameter schemas, and behavioral contracts were designed and tuned for Claude. The concern is that other LLMs may interpret descriptions differently, fail to invoke tools where Claude would, or invoke them incorrectly.

Specifically:

**Tool invocation fidelity**: Do Codex and Gemini reliably invoke the correct tool given the same natural language prompt that would trigger it in Claude? Evaluate at least: `context_search`, `context_store`, `context_briefing`, `context_get`, `context_cycle`. Test with the same prompts used in the existing eval harness.

**Parameter format adherence**: Do these clients reliably produce well-formed JSON tool call parameters (integer `id` fields, JSON array `tags`, string content without escaped inner quotes)? What failure modes appear?

**Description vocabulary**: Tool descriptions use vocabulary tuned to Claude's training (e.g., "semantic search", "context briefing", "knowledge cycle"). Evaluate whether Gemini/Codex interpret this vocabulary consistently. Are there terms in current descriptions that appear in Claude's training but not in other providers' training? Identify the three highest-risk description sections.

**Multi-step workflows**: Unimatrix is designed for agents that invoke multiple tools in sequence (e.g., `context_briefing` at session start, `context_store` after a decision, `context_cycle` at session end). Do Codex and Gemini maintain this multi-step behavioral pattern, or do they invoke tools in isolation? Evaluate the briefing → work → store → cycle workflow specifically.

---

### 2. Session and Agent Attribution Compatibility

Unimatrix attributes knowledge to agents via `agent_id`. In the Claude model, agent identity is typically set at agent spawn via tool parameters. The concern is that other LLMs may surface agent identity differently or inconsistently.

**Agent ID surfacing**: How does each client (Codex, Gemini) typically provide agent identity to tool calls? Is it via system prompt injection, a dedicated parameter, or convention? Do they reliably set the same `agent_id` across all calls in a session?

**Session continuity**: Does each client maintain a consistent session context such that the `context_cycle` start/stop pattern works correctly (cycle open at session start, cycle close at session end)? Or does each tool call arrive effectively stateless?

**Concurrent session attribution**: The scenario: Codex agent and Claude agent running concurrently, both writing to the same Unimatrix repo. Does attribution work correctly? Is the `agent_id` parameter sufficient, or does the HTTP `sub` claim in the enterprise model provide the authoritative identity?

---

### 3. Context Injection Size Behavior

`context_briefing` returns knowledge entries up to a configured token budget. Claude Code's context window and token counting behavior is what the current default limits were calibrated against. Other LLMs may have different practical context window sizes or count tokens differently.

**Injection size limits**: What are the effective context window sizes for Codex and Gemini for tool response content? Are the current `context_briefing` return limits appropriate, or do they need per-provider tuning?

**Large result handling**: When `context_search` returns many entries, how do Codex and Gemini handle the response payload? Do they process all entries, truncate silently, or fail?

**Token counting divergence**: Unimatrix uses character-count-based approximation for content limits. Does this heuristic remain safe across provider tokenizers, or could it cause Gemini or Codex to receive truncated tool responses that appear complete?

---

### 4. Eval Harness Provider-Agnostic Gap Analysis

The current eval harness (`2,096 scenarios, MRR=0.2558 baseline`) was built and calibrated against Claude. This is an evaluation coverage gap, not just a compatibility gap — we cannot measure whether Unimatrix works well with Codex or Gemini because we have no eval scenarios that test from those clients.

**Eval methodology**: What would a provider-agnostic eval harness look like? The test oracle (expected tool call sequence and result quality) must not be Claude-specific. What is the minimum set of eval scenarios that tests knowledge storage, retrieval quality, briefing relevance, and cycle attribution in a provider-neutral way?

**Scenario coverage gaps**: Identify which of the 2,096 existing scenarios, if any, are Claude-specific in their assumptions (Claude tool calling format, Claude system prompt structure, Claude multi-step reasoning patterns). What fraction would be valid scenarios for Codex or Gemini without modification?

**Provider behavioral baseline**: What is a realistic baseline for tool-call MRR from a Codex or Gemini agent interacting with Unimatrix? Is the current MRR=0.2558 a realistic target, or does the lack of training signal for Unimatrix-specific tools mean baseline is meaningfully lower?

---

### 5. HTTP Auth Compatibility

ASS-041 confirmed `Authorization: Bearer` is supported by Claude Code's HTTP MCP transport. This needs the same confirmation for Codex and Gemini.

**Codex MCP auth**: Does Codex's MCP HTTP client forward `Authorization: Bearer` correctly on all requests (both initial SSE connection and tool call POSTs)? Same header-forwarding bug pattern as anthropics/claude-code#28293 possible?

**Gemini MCP auth**: Same question.

**OAuth 2.1 flow compatibility**: The enterprise tier requires OAuth 2.1 client credentials. Do any of the target MCP clients support OAuth 2.1 token acquisition flows, or do they require static bearer tokens? For the developer cloud tier (static token), this is simpler — confirm static bearer token support is sufficient.

---

## Output

1. **Client capability matrix** — which clients support MCP HTTP transport + auth today, which are in-progress, which are out of scope for Wave 2
2. **Tool description risk assessment** — which descriptions are highest-risk for non-Claude clients, with specific language change recommendations
3. **Session/attribution compatibility report** — whether agent_id attribution works correctly across providers; what changes are needed (if any) at the tool parameter or enterprise auth layers
4. **Eval coverage gap report** — what fraction of existing eval scenarios are provider-neutral; minimum scenario set for provider-agnostic coverage
5. **HTTP auth compatibility findings** — per-client status for `Authorization: Bearer` forwarding; known bugs equivalent to claude-code#28293
6. **Wave 2 delivery scope recommendation** — which compatibility gaps are engineering tasks (modify tool descriptions, adjust size limits) and which require spec/protocol work; effort estimates for each

---

## Constraints

- Do not encode assumptions about specific clients' internal architecture. Findings must be based on observable behavior and published documentation, not inference.
- The MCP protocol is the compatibility surface — do not propose changes to Unimatrix's MCP tool API schema to accommodate a specific client. Describe tool description changes only.
- "Works with Codex and Gemini OOB" is the success criterion. Not "theoretically compatible per spec" — actually works without custom configuration per deployment.
- The enterprise auth model (OAuth 2.1) is confirmed. The question is client-side support, not server-side design.

---

## Breadth

`product + industry`

Primary sources: Codex CLI documentation, Gemini CLI / Gemini API MCP documentation, MCP specification (spec.modelcontextprotocol.io), published client compatibility notes, GitHub issues for each client's MCP implementation.

This spike accesses the Unimatrix codebase to read current tool descriptions and eval harness structure. It does NOT modify any code.

---

## Approach

`evaluation + empirical`

Where possible: test actual client behavior against a live Unimatrix instance. Where clients are unavailable or not yet supporting HTTP MCP: rely on published documentation and GitHub issue tracking. Mark empirical findings as tested vs. documented.

---

## Confidence Required

`directional with empirical grounding` — findings must distinguish "confirmed by test" from "documented but untested" from "inferred." Do not state compatibility as confirmed without a test artifact.

---

## Inputs

- ASS-041 FINDINGS.md: `Authorization: Bearer` Claude Code confirmation + rmcp transport design
- `crates/unimatrix-server/src/tools/` — current tool descriptions and parameter schemas
- Eval harness structure — scenario format and Claude-specific assumptions
- MCP spec: https://spec.modelcontextprotocol.io/
