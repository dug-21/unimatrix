# FINDINGS: Multi-LLM MCP Client Compatibility — External Research Track

**Spike**: ass-049
**Date**: 2026-04-11
**Approach**: evaluation + investigation
**Confidence**: directional with empirical grounding (documented behavior from primary sources; no live test environment)
**Track**: FINDINGS-EXTERNAL.md (dual-track — internal track handled separately)

---

## Findings

### Q: MCP client capability survey (SCOPE §0)

For each target client — Codex (OpenAI CLI), Gemini CLI, Continue (VSCode), Cursor, Zed — determine: Does it support MCP HTTP transport (Streamable HTTP / SSE-over-HTTP)? What is the configuration mechanism? What MCP spec version does it target? Produce a capability matrix. Clients with no MCP HTTP support are out of scope for Wave 2.

**Answer**: All five clients have some form of HTTP MCP transport support, but quality varies significantly. Codex and Gemini CLI both support Streamable HTTP natively and are in scope for Wave 2. Zed only supports stdio natively; HTTP requires an `mcp-remote` bridge proxy and is **out of scope for Wave 2** — requiring that bridge violates the "works OOB without custom configuration per deployment" success criterion in SCOPE.md.

**Evidence**:

**Codex CLI (OpenAI)**
- Transports: STDIO and Streamable HTTP. SSE is not listed as a supported transport in official documentation. Confirmed by developers.openai.com/codex/mcp.
- Configuration: TOML at `~/.codex/config.toml` (global) or `.codex/config.toml` (project-scoped). HTTP section uses `[mcp_servers.<name>]` with fields: `url`, `bearer_token_env_var`, `http_headers` (static map), `env_http_headers` (env-var-derived map). Also manageable via `codex mcp add` CLI subcommand.
- MCP spec version: Not stated explicitly in documentation. GitHub issue #5619 (open March 2026) shows Codex initiating connections with `protocolVersion: "2025-06-18"` but exhibiting `2024-11-05` behavioral expectations — indicating the client targets `2025-06-18` but has an unresolved interoperability bug with servers responding per the older spec.
- Auth: `bearer_token_env_var` sends `Authorization: Bearer <value>`. OAuth 2.1 DCR via `codex mcp login`. Static headers via `http_headers` / `env_http_headers`.
- Primary sources: developers.openai.com/codex/mcp; github.com/openai/codex/issues/5619; github.com/openai/codex/issues/15818

**Gemini CLI (Google)**
- Transports: All three — STDIO (`command`), SSE (`url` field), Streamable HTTP (`httpUrl` field). `httpUrl` takes priority when multiple options are present.
- Configuration: JSON `settings.json` via `mcpServers` object. Key fields: `command`, `url`, `httpUrl`, `headers` (map, supports Authorization), `timeout`, `trust`, `includeTools`, `excludeTools`. Env variable expansion supported. Also: `gemini mcp add --transport http` CLI command.
- MCP spec version: Documentation explicitly references `2025-06-18` in `CallToolResult` structure and server initialization behavior.
- Auth: `headers` map supports `Authorization: Bearer <token>`. OAuth 2.0 with DCR (issue #4172 closed, PR #3569 merged), Google Application Default Credentials, service account impersonation.
- Primary sources: geminicli.com/docs/tools/mcp-server/; github.com/google-gemini/gemini-cli/blob/main/docs/tools/mcp-server.md

**Continue (VSCode extension)**
- Transports: STDIO, SSE, and Streamable HTTP — all three. Config via YAML in `.continue/mcpServers/` using `type: sse` or `type: streamable-http`. Authorization Bearer header supported via `headers` map with env variable expansion.
- MCP spec version: Issue #8118 (continuedev/continue) requested `2025-06-18` support — resolution not confirmed from available external data.
- Primary source: docs.continue.dev/customize/deep-dives/mcp; github.com/continuedev/continue/issues/8118

**Cursor**
- Transports: STDIO, SSE, and Streamable HTTP — all three confirmed in current documentation for early 2026.
- Configuration: JSON config via `"mcpServers"` key in Cursor settings.
- MCP spec version: Not explicitly stated in available documentation.
- Primary source: cursor.com/docs; toolradar.com/blog/best-mcp-servers-cursor

**Zed**
- Transports: stdio only, natively. Streamable HTTP is **not implemented**. GitHub discussion #34719 (zed-industries/zed, open March 2026) confirms the gap with community demand for HTTP transport. The `"url"` configuration field exists and supports an OAuth prompt-based auth flow for remote servers, but the underlying transport is still stdio via the `mcp-remote` npm bridge.
- MCP spec version: Documentation references `2025-11-25`.
- Wave 2 status: **OUT OF SCOPE.** No native Streamable HTTP transport. `mcp-remote` bridge required violates "works OOB" success criterion.
- Primary sources: zed.dev/docs/ai/mcp; github.com/zed-industries/zed/discussions/34719

**Capability Matrix**:

| Client | Streamable HTTP | SSE | STDIO | Bearer Auth Mechanism | MCP Spec Version | Wave 2 |
|--------|:-:|:-:|:-:|---|---|:-:|
| Codex CLI | Yes (native) | No | Yes | `bearer_token_env_var` + `env_http_headers` | 2025-06-18 (inferred; bug #5619) | In scope |
| Gemini CLI | Yes (`httpUrl`) | Yes (`url`) | Yes | `headers` map + OAuth DCR + GCP ADC | 2025-06-18 (confirmed) | In scope |
| Continue | Yes | Yes | Yes | `headers` map + env var expansion | ~2025-06-18 (issue #8118 pending) | In scope |
| Cursor | Yes | Yes | Yes | Bearer header | Undocumented | In scope |
| Zed | No (bridge only) | No | Yes | Static Bearer + OAuth prompt | 2025-11-25 | Out of scope |

**Recommendation**: Codex CLI and Gemini CLI are the primary Wave 2 targets. Continue and Cursor are in scope at lower priority — their transport and auth are well-behaved. Zed is out of scope for Wave 2; revisit when discussion #34719 resolves.

---

### Q: Tool invocation and multi-step workflow behavior (SCOPE §1 — external side)

From published documentation, GitHub issues, and available behavioral evidence: Do Codex and Gemini support multi-step tool invocation sequences? Is there documented evidence of them maintaining multi-step patterns? Are there published notes on parameter format reliability (integer fields, JSON arrays) from these clients?

**Answer**: Both clients support multi-step sequential tool invocation via the standard agent loop. Gemini CLI's non-interactive headless mode lacks session continuity and is incompatible with multi-step patterns. Gemini has significantly more severe JSON Schema validation bugs than Codex, and several remain open as of March 2026.

**Evidence**:

**Multi-step invocation — Codex**

Codex implements the standard agent loop: the model requests a tool call, Codex executes it, appends the result to conversation history, and re-queries until no tool calls remain. Documented in "Unrolling the Codex agent loop" and confirmed in the Agents SDK cookbook at developers.openai.com/cookbook/. The cookbook explicitly demonstrates a briefing-then-work pattern: an orchestrating agent creates planning documents, passes them to specialized sub-agents, validates results. Tool calls are sequential within a session; parallel execution happens at the multi-agent orchestration layer, not within single sessions.

**Multi-step invocation — Gemini CLI**

Gemini CLI implements the same agent loop. Within an interactive session, `GeminiClient` orchestrates tool execution and each result is submitted back to the LLM; prior tool results inform subsequent decisions (confirmed by DeepWiki architecture analysis of google-gemini/gemini-cli). Tool calls share conversational context via `AgentLoopContext` with a per-session `sessionId`. However: Gemini CLI's non-interactive/headless mode is run-and-exit with no session state and no long-lived MCP connections. GitHub issue #15338 (open, March 2026) explicitly documents this gap — daemon/server mode with persistent MCP connections remains unimplemented.

**Parameter format reliability — Codex**

Codex 0.20.0 rejected `type: "integer"` in JSON Schema tool definitions (only `"number"` was accepted), as well as union types `type: ["string","null"]` and schemas missing an explicit root `type: "object"`. Error: `"unknown variant 'integer', expected one of 'boolean', 'string', 'number', 'array', 'object'"`. Fixed in v0.21.0 via PR #1975 (issue #2204, closed). Current Codex versions handle integer types correctly.

A separate protocol-version mismatch (issue #5619, open March 2026) causes Codex to send `protocolVersion: "2025-06-18"` but close the connection before receiving the initialize response via SSE, because it expects the older synchronous response behavior from `2024-11-05`. This is a transport-level issue that can prevent tool discovery from completing entirely.

**Parameter format reliability — Gemini CLI / Gemini API**

Gemini has significantly more JSON Schema compatibility issues than Codex:

1. **`$defs` references** (issue #13326, open): The Gemini API returns `400 INVALID_ARGUMENT` for any MCP tool schema using `$defs` for type reuse. Affects FastMCP v2.12+ servers, Snowflake MCP, BEADS MCP, Basic Memory MCP. Claude Code handles the same schemas without issue.

2. **Multi-type union arrays** (issue #2654, closed as duplicate of #1481 — underlying issue unresolved as of July 2025): `type: ["string", "null"]` causes `TypeError: fieldValue.toUpperCase is not a function`. The parser incorrectly assumes `type` is always a string.

3. **Reserved parameter names** (issue #13705, closed "not planned" March 2026): Parameters named `title` or `type` cause INVALID_ARGUMENT because Gemini's schema processor misinterprets them as JSON Schema keywords. No fix planned.

4. **Missing type fields** (issue #6632, closed via PR #6961): Schemas where tool parameters lack an explicit `type` caused 400 errors. A strict validation rule was relaxed, but the underlying strictness exceeds Claude Code or VS Code MCP clients.

5. **Parameter hallucination** (issue #16318): Gemini CLI v0.23.0 incorrectly injects parameters from other tools into a target tool call, causing INVALID_ARGUMENT. Affects any deployment where multiple tools coexist in the registry.

The Mastra compatibility layer analysis (empirical, not peer-reviewed) found Gemini tool call success rates of 73–90% at baseline before schema remediation, vs. Anthropic at 97–100%. This aligns with the GitHub issue volume.

**Recommendation**: Do not downgrade Unimatrix integer fields to `"number"` — current Codex handles `"integer"` correctly. For Gemini compatibility, audit all Unimatrix tool schemas before Wave 2 for: (1) any `$defs` references — inline them; (2) any union-type arrays like `["string","null"]` — replace with `oneOf`; (3) parameters named `title` or `type` — rename them. Document Gemini non-interactive mode as unsupported for multi-step Unimatrix workflows.

---

### Q: Session and agent attribution compatibility (SCOPE §2)

How does each MCP-capable client (Codex, Gemini) typically provide agent identity to tool calls? Via system prompt, dedicated parameter, or convention? Is there published documentation on session context maintenance? What are the implications for the `context_cycle` start/stop pattern?

**Answer**: Neither Codex nor Gemini injects an `agent_id` field into MCP tool call parameters natively. Agent identity must be instructed via AGENTS.md / GEMINI.md and carried by the model as a tool parameter value. Within an interactive session both clients maintain conversational context enabling multi-step sequences. The `context_cycle` start/stop pattern is viable for interactive sessions but is structurally broken for Gemini non-interactive mode and is vulnerable to mid-session OAuth token expiry in Gemini (issue #23296).

**Evidence**:

**Codex — agent identity**

AGENTS.md files provide per-repository behavioral instructions injected before each run. They do not create an explicit `agent_id` field in MCP tool call parameters. The Codex CLI does not add identity metadata to `call_tool` request payloads. From Unimatrix's perspective, each tool call contains only what the model included as parameters — if the model was not instructed to include `agent_id`, it will not appear. The Agents SDK cookbook identifies agents by `name` at the orchestration layer, not within MCP tool parameters. The `/resume` command reloads previous sessions by ID (CLI-level feature, not an MCP-level session token).

**Gemini CLI — agent identity**

GEMINI.md files serve the same role: context injection for behavioral instructions, not tool call parameter injection. No documented `agent_id` field or session token appears in MCP `call_tool` requests from Gemini CLI. The internal `sessionId` (UUID, stored in `~/.gemini/tmp/<project_hash>/chats/`) is CLI-internal and is not transmitted to MCP servers as a tool call parameter (confirmed by DeepWiki session management architecture analysis). All MCP tools are assigned FQNs in format `mcp_{serverName}_{toolName}` for local routing, but this is not transmitted to the MCP server.

**Session continuity — interactive sessions**

Within an interactive Gemini CLI session: tool calls share conversational context. The model sees previous tool call results when deciding subsequent calls, enabling the briefing → work → store → cycle pattern. Sessions persist to `~/.gemini/tmp/` and can be resumed via `--resume`.

Within an interactive Codex CLI session: same agent loop behavior with session transcripts accumulated and re-injectable via `/resume`.

**Session continuity — Gemini non-interactive mode**

Gemini CLI headless (non-interactive) mode: run-and-exit, no session state, no long-lived MCP connections (issue #15338, confirmed explicitly). Each invocation is a fresh session. A `context_cycle` `action: "start"` in one invocation cannot be paired with `action: "stop"` in a separate invocation. The pattern is structurally broken for this mode.

**OAuth token expiry mid-session — Gemini**

Issue #23296 (open): After OAuth access tokens expire within a running chat session, Gemini CLI's MCP transport fails silently. Root cause: transport is built with a bearer token injected statically at connection setup; it does not use a refresh-aware auth provider. All subsequent tool calls fail until the user manually runs `gemini mcp list` to force reconnection. This can leave a `context_cycle` in orphaned open state.

**Implications for `context_cycle` start/stop**

Neither client will include `agent_id` in tool calls without explicit instruction. For the pattern to work:
1. AGENTS.md (Codex) or GEMINI.md (Gemini) must explicitly instruct the model to pass a consistent `agent_id` value in every Unimatrix tool call.
2. For interactive sessions, both clients maintain conversation history so the model can maintain a consistent `agent_id` across calls within a session.
3. For Gemini non-interactive mode: the pattern is broken — cycle cannot span invocations.
4. If Gemini's OAuth token expires mid-session (issue #23296), the cycle stop call may fail, leaving orphaned open cycles.

The enterprise HTTP `sub` claim from OAuth JWT is architecturally more reliable, but requires OAuth 2.1 client credentials flow which neither client currently supports (see Q4).

**Recommendation**: Document AGENTS.md / GEMINI.md configuration as a required Wave 2 onboarding step for `context_cycle` attribution. Warn users that Gemini non-interactive mode is incompatible with `context_cycle` semantics. Add server-side monitoring for orphaned open cycles — they will occur when Gemini's OAuth token expires mid-session.

---

### Q: HTTP auth compatibility (SCOPE §5)

Does Codex's MCP HTTP client forward `Authorization: Bearer` correctly on all requests? Same for Gemini. Are there known header-forwarding bugs equivalent to anthropics/claude-code#28293? Do any of these clients support OAuth 2.1 client credentials flows?

**Answer**: Both Codex and Gemini forward static `Authorization: Bearer` headers correctly with no documented drop on tool call POSTs (no direct analog to claude-code#28293 for static tokens). However, both have open bugs affecting OAuth-based auth in practice. Neither client supports OAuth 2.1 client credentials flow natively — both require static bearer tokens or interactive OAuth DCR.

**Evidence**:

**Codex — static bearer token forwarding**

Configuration: `bearer_token_env_var = "MY_TOKEN_VAR"` or `env_http_headers = { "Authorization" = "MY_AUTH_VAR" }`. Both apply at transport layer across all requests. No documentation or reported bug suggests static headers are dropped on tool call POSTs vs. initial connection.

**Known Codex auth bugs**:
1. Issue #12859 (open, Feb 2026): `rmcp` HTTP client sends no `User-Agent` header by default. Cloudflare-protected MCP servers return 403 during OAuth discovery. Workaround: `http_headers = { "User-Agent" = "codex-mcp/1.0" }`. Does **not** affect static bearer token configurations — only OAuth flow initiation via Cloudflare-protected servers.
2. Issue #7318 (open, March 2026): When short-lived OAuth tokens expire, Codex does not auto-reload headers. Users must kill and restart the session, losing conversation context. Affects dynamic OAuth auth only; static bearer tokens do not expire.
3. Issue #5619 (open, March 2026): Protocol version mismatch `2025-06-18` vs `2024-11-05` causes connection drops on Streamable HTTP initialization. Not an auth bug but prevents MCP connection establishment.

**Codex — OAuth 2.1 client credentials**

Codex implements OAuth via `codex mcp login` using Authorization Code flow with Dynamic Client Registration. Client credentials flow (machine-to-machine) is not documented. Issue #15818 (open, March 2026): Codex fails entirely when the server does not support DCR — errors `"Auth required, when send initialize request"` and `"Dynamic client registration not supported"`. No alternative credential flow is offered.

**Gemini CLI — static bearer token forwarding**

Configuration: `"headers": { "Authorization": "Bearer YOUR_TOKEN" }` in `mcpServers` settings.json. Forwarded on all HTTP requests. Env variable expansion supported. No documented forwarding bug analogous to claude-code#28293 for static headers.

**Known Gemini auth bugs**:

Issue #23296 (open): OAuth token refresh fails in active chat sessions. Root cause: MCP transport is built with a static bearer token injected at connection setup; it does not use a refresh-aware auth provider. After access token expiry, all MCP tool calls fail silently. Workaround: `gemini mcp list` forces reconnection. This is the Gemini equivalent of the Codex issue #7318 class of bug — and is the closest analog to the claude-code#28293 class of header-staleness bugs — not a header drop on the initial request, but mid-session token staleness.

**Gemini CLI — OAuth 2.1 client credentials**

Issue #4172 (closed, PR #3569 merged): OAuth 2.1 DCR implemented. Auth provider types: `dynamic_discovery`, `google_credentials`, `service_account_impersonation`. Client credentials flow (machine-to-machine grant) not mentioned in available documentation — implementation targets user-facing OAuth flows with browser redirect.

**Summary**: Neither Codex nor Gemini CLI supports OAuth 2.1 client credentials grant. Both support static bearer tokens and interactive OAuth DCR. For Unimatrix's enterprise tier, users must obtain a bearer token via an out-of-band client credentials exchange and configure it as a static env-var token. The OAuth 2.1 client credentials exchange cannot be delegated to these MCP clients.

**Recommendation**: For Wave 2 developer cloud tier (static bearer token): no changes needed — both clients forward `Authorization: Bearer` correctly. Instruct Codex users to add `User-Agent` header in config to avoid Cloudflare 403 on OAuth discovery (issue #12859). For enterprise OAuth 2.1 client credentials tier: document that neither client supports this flow natively; users must obtain a bearer token out-of-band and pass it as `bearer_token_env_var` (Codex) or `headers.Authorization` (Gemini). The MCP clients are token consumers, not OAuth 2.1 grant executors.

---

### Q: Context window and tool response size behavior (SCOPE §3 — external side)

What are the effective context window sizes for Codex and Gemini for tool response content? Are there documented limits on tool response payload sizes? What is a realistic tool-call MRR baseline expectation for these clients vs Claude's 0.2558 baseline?

**Answer**: Codex's binding constraint for MCP tool responses is approximately 10 KiB / 256 lines — not the 200K raw model context window. Gemini CLI's configurable threshold is 40,000 characters per tool output (default), but its applicability to MCP server tools (vs. shell tools) is uncertain. No published MRR baseline exists for Codex or Gemini against Unimatrix-style MCP knowledge servers — the 0.2558 is Claude-specific and non-transferable.

**Evidence**:

**Codex — context window and tool response limits**

Raw model limits (o3 / o4-mini): 200,000-token context window, 100,000 max output tokens (platform.openai.com/docs/models/o3). Usable Codex CLI session context: approximately 258,000 tokens (272,000 minus 5% compaction threshold per issues #9429, #9857).

Practical per-tool-response limits (binding constraints):
- Hardcoded truncation: 10 KiB or 256 lines per tool output, using head+tail preservation. Both issues #5913 and #6426 are closed as completed, meaning this truncation is implemented and active.
- Issue #7906 (Dec 2025): A `TruncationPolicy::Bytes(10_000)` hard limit on MCP responses was reported for the GPT-5.2 harness.
- OpenAI collaborators confirmed in issue #5913 comments that model training incorporates the 256-line limit.

**Practical implication for Unimatrix**: A `context_briefing` response with 5 entries × ~200 tokens each (~4,000 characters) fits within 10 KiB. A `context_search` response with 20+ full-content entries could hit the truncation threshold, resulting in truncated JSON delivered to the model — which may cause parsing failures or silently incomplete results.

**Gemini CLI — context window and tool response limits**

Raw model limits (Gemini 2.5 Pro): 1,000,000-token context window, 64,000 max output tokens (ai.google.dev/gemini-api/docs/models).

Practical per-tool-response limits via `settings.json`:
- `truncateToolOutputThreshold`: maximum characters per tool output, default **40,000 characters**. Setting to 0 disables truncation (geminicli.com/docs/reference/configuration/).
- LLM-based summarization triggered above 20,000 tokens; the model receives a summary rather than full content.
- Per-turn: `contextManagement.messageLimits.retainedMaxTokens` defaults to 12,000 tokens; history compression at 150,000 tokens accumulated.

**Uncertainty on MCP applicability**: PR #12173 (closed as stale, Jan 2026) attempted to extend tool output truncation specifically to MCP server tools. Its closure due to a scheduler rewrite — not feature rejection — means it is uncertain whether `truncateToolOutputThreshold` applies to MCP tools by default or only to shell tools. Issue #18318 documents a 1.2M-token single tool response causing context saturation, suggesting no hard per-tool byte limit analogous to Codex's 10 KiB is enforced in Gemini CLI currently.

**Token counting divergence**

Unimatrix uses character-count approximation for content limits. Google SentencePiece and OpenAI BPE differ from Anthropic's tokenizer, but for English prose all three approximate roughly 1 token per 4 characters. For JSON-heavy knowledge entries or code content, variance increases. However, Codex's 10 KiB byte limit is the binding constraint — it will be hit before tokenizer differences cause problems for typical Unimatrix response sizes.

**MRR baseline expectations**

No published study provides MRR for MCP knowledge server interactions with Codex or Gemini. Available directional evidence:

- Mastra blog (empirical, not peer-reviewed, 30 property types tested): Gemini tool call success rate 73–90% at baseline before schema remediation; Anthropic 97–100%. This measures JSON Schema validation failures, not semantic retrieval quality.
- LM Council benchmarks (April 2026): Claude leads on agentic tool-use tasks requiring sustained tool reliability; Gemini 2.5 Pro and Codex trail.
- The 0.2558 MRR is specific to Unimatrix's 2,096-scenario Claude-calibrated eval harness using Claude tool-calling format and system prompt assumptions. It is not transferable to other clients without redesigning eval scenarios.

A pre-compatibility-fix Gemini MRR for Unimatrix would be severely depressed: the `$defs` bug (issue #13326) alone would block tool discovery for any Unimatrix tool schema using `$defs`, making affected tools invisible entirely. Until schema compatibility bugs are resolved, MRR measurement is not meaningful.

**Recommendation**: Set Unimatrix `context_briefing` default response budget to no more than 8,000 characters to fit within Codex's 10 KiB / 256-line limit with margin. For `context_search`, implement a per-provider `max_entries` configuration option with Codex set to a conservative default. For Gemini, document explicit `truncateToolOutputThreshold` configuration in deployment docs to ensure MCP tool truncation is covered. Do not attempt to establish a provider-specific MRR baseline until Gemini schema compatibility bugs are resolved — any pre-fix measurement conflates schema errors with retrieval quality.

---

## Unanswered Questions

1. **Codex Streamable HTTP protocol version resolution**: Issue #5619 (open March 2026) shows Codex sends `2025-06-18` but behaves per `2024-11-05` for SSE responses. Whether Unimatrix's `rmcp`-based server targets `2024-11-05` or `2025-06-18` in its response semantics is unknown from external research alone — requires reading the Unimatrix codebase (internal track). If Unimatrix uses `2024-11-05` protocol semantics in responses, this is a live breaking bug for Codex Streamable HTTP connections.

2. **Gemini CLI `truncateToolOutputThreshold` applicability to MCP tools**: PR #12173 was closed as stale without merging. Whether the setting applies to MCP server tool outputs (not just shell) in the current Gemini CLI release is unconfirmed. Requires empirical testing against a live Gemini CLI session.

3. **Continue MCP spec version `2025-06-18` resolution**: Issue #8118 requested this support; resolution not confirmed from available external data. Check issue for current state before treating Continue as fully Wave 2 ready.

4. **Gemini CLI parameter hallucination scope**: Issue #16318 (v0.23.0 injects parameters from other tools into target tool calls). Whether Unimatrix tools have parameter name overlap that triggers this requires tool schema analysis (internal track). If `context_search` and `context_get` both have a `query` parameter, the hallucination bug may fire.

5. **Enterprise OAuth 2.1 client credentials via static token workaround**: That the enterprise tier can work by having users obtain a static bearer token out-of-band via client credentials exchange and pass it as an env var is architecturally plausible but untested against live deployments of either client.

---

## Out-of-Scope Discoveries

1. **Gemini CLI MCP tool count limit of 100** (issue #21823, feature request to raise to 500). Unimatrix has 12 tools — within the limit. Not a Wave 2 concern, but document for future tool-count growth planning.

2. **Codex "as MCP server" pattern** (github.com/kky42/codex-as-mcp): Codex can be exposed as an MCP server itself for orchestration by other agents. Different architecture from Unimatrix-as-server-used-by-Codex. Potentially relevant for Wave 3 multi-agent composition — warrants a separate spike if that architecture is considered.

3. **Continue issue #9151**: Continue fails to start MCP servers in WSL workspaces on Windows. Not relevant for Unimatrix's Linux/cloud deployment targets but relevant for developer onboarding documentation on Windows.

4. **Gemini CLI ACP mode** (Agent Communication Protocol): A distinct inter-agent communication protocol in Gemini CLI. May interact with MCP tool attribution patterns. Not a Wave 2 blocker — monitor for Wave 3.

---

## Recommendations Summary

- **Q0 (Capability matrix)**: Codex CLI and Gemini CLI are in scope for Wave 2 — native Streamable HTTP + static bearer auth. Continue and Cursor are in scope at lower priority. **Zed is out of scope** — no native HTTP transport; `mcp-remote` bridge violates "works OOB" criterion.
- **Q1/ext (Multi-step invocation and parameter format)**: Both clients support multi-step agent loops. Gemini non-interactive mode lacks session continuity — document as unsupported for multi-step workflows. Gemini schema bugs require three concrete Unimatrix schema fixes before Wave 2: inline all `$defs` references, replace union-type arrays with `oneOf`, rename any parameters named `title` or `type`. Codex integer-type bug was fixed in v0.21.0 — no action needed on current Codex versions.
- **Q2 (Session/attribution compatibility)**: Neither client injects `agent_id` natively — requires explicit AGENTS.md / GEMINI.md instruction as a required onboarding step. `context_cycle` start/stop works within interactive sessions; incompatible with Gemini headless mode. Gemini OAuth token expiry (issue #23296) can silently orphan open cycles mid-session — add server-side monitoring.
- **Q5 (HTTP auth compatibility)**: Static `Authorization: Bearer` forwarding works correctly in both clients for non-expiring tokens — no analog to claude-code#28293. Codex users need `User-Agent` header workaround for Cloudflare-protected deployments (issue #12859). Neither client supports OAuth 2.1 client credentials flow; enterprise tier must use static bearer tokens obtained out-of-band.
- **Q3/ext (Context window and tool response size)**: Codex's binding constraint is 10 KiB / 256 lines per tool response — not the 200K model context window. Set `context_briefing` budget to ≤ 8,000 characters for Codex safety. Gemini's 40,000-character default gives more headroom but MCP tool applicability is uncertain. No published MRR baseline exists for these clients against Unimatrix — defer measurement until Gemini schema compatibility bugs are resolved.
