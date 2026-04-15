# FINDINGS: Multi-LLM MCP Client Compatibility

**Spike**: ass-049
**Date**: 2026-04-11
**Approach**: evaluation + empirical (dual-track synthesis)
**Confidence**: directional with empirical grounding — external findings are documented from primary sources and GitHub issues; internal findings are confirmed by code read; no live Codex/Gemini test sessions were executed

---

## Findings

### §0 — MCP Client Capability Survey

**Answer**: All five surveyed clients have some form of HTTP MCP transport support, but quality varies. Codex CLI and Gemini CLI both support Streamable HTTP natively and are the primary Wave 2 targets. Continue and Cursor are in scope at lower priority. Zed requires an `mcp-remote` npm bridge for HTTP transport and is **out of scope for Wave 2** — the bridge violates the "works OOB without custom configuration per deployment" success criterion stated in SCOPE.md.

**Evidence** (external track, primary sources):

| Client | Streamable HTTP | SSE | STDIO | Bearer Auth | MCP Spec Version | Wave 2 |
|---|:-:|:-:|:-:|---|---|:-:|
| Codex CLI | Yes (native) | No | Yes | `bearer_token_env_var` + `env_http_headers` | 2025-06-18 (inferred; bug #5619) | In scope |
| Gemini CLI | Yes (`httpUrl`) | Yes (`url`) | Yes | `headers` map + OAuth DCR + GCP ADC | 2025-06-18 (confirmed) | In scope |
| Continue | Yes | Yes | Yes | `headers` map + env var expansion | ~2025-06-18 (issue #8118 pending) | In scope |
| Cursor | Yes | Yes | Yes | Bearer header | Undocumented | In scope |
| Zed | No (bridge only) | No | Yes | Static Bearer + OAuth prompt | 2025-11-25 | Out of scope |

Codex configuration: TOML at `~/.codex/config.toml` or `.codex/config.toml`, using `[mcp_servers.<name>]` with `url`, `bearer_token_env_var`, `http_headers`, `env_http_headers`. Also configurable via `codex mcp add`. Gemini configuration: JSON `settings.json` via `mcpServers` object with `httpUrl`, `headers` (supports env variable expansion), `timeout`, `trust`, `includeTools`, `excludeTools`. Both clients confirmed by official documentation.

**Recommendation**: Target Codex CLI and Gemini CLI as primary Wave 2 compatibility deliverables. Continue and Cursor are low-effort additions once the primary two work. Defer Zed until discussion zed-industries/zed#34719 resolves.

---

### §1 — Tool Description and Behavioral Compatibility

This section merges internal-track code analysis of Unimatrix's current tool descriptions with external-track findings on Gemini's JSON Schema bugs and Codex's parameter handling. These two bodies of evidence combine into a unified picture of what changes are required before Wave 2.

**Answer**: Three tool description sections carry significant non-Claude risk (identified by internal track from code read). Gemini has five distinct JSON Schema validation bugs that are independent of description vocabulary — they concern schema structure, not language (identified by external track from GitHub issues). Together they define two separate, non-overlapping work streams: description rewrites and schema structural fixes. Both are required before Wave 2.

**Evidence — tool description vocabulary risk (internal track, read from `mcp/tools.rs`):**

The three highest-risk description sections:

1. **`context_briefing` — "NLI entailment scoring" in the `task` parameter doc comment** (`BriefingParams.task`, line 232). This machine-learning research term has no behavioral implication for agents trained on typical developer interaction data. The instruction reads: "the ranking uses NLI entailment scoring which works best with coherent sentences." A Codex or Gemini agent may produce terse keyword-style task strings that degrade ranking quality. Claude agents' RLHF training includes researcher/developer discourse where "NLI" appears; other providers' training coverage is less certain.

2. **`context_cycle` — "feature cycle" and "hook path" framing.** The description says: "Attribution is best-effort via the hook path; confirm via context_cycle_review." "Hook path" refers to Unimatrix's UDS side channel that fires only when Claude Code executes tool calls. No other MCP client fires Claude Code hooks. A non-Claude agent reading this description has no referent for the term, and the behavioral implication — that attribution may work passively without `agent_id` — is false for all non-Claude clients. This is the intersection of a description problem and a behavioral contract gap (see §2 below).

3. **`context_briefing` — "Use at the start of any task to orient yourself before designing or implementing."** This behavioral directive assumes Claude Code's concept of a distinct session-initialization moment. Codex CLI and Gemini CLI agents operate more request/response — they do not maintain "session start" as a distinct behavioral moment unless the orchestrating prompt explicitly scaffolds it. The directive will be silently ignored by agents that lack this concept.

**Evidence — Gemini JSON Schema structural bugs (external track, from GitHub issues):**

Five confirmed bugs in Gemini's schema processor, ordered by Wave 2 impact:

1. **`$defs` references** (gemini-cli issue #13326, open): Gemini API returns `400 INVALID_ARGUMENT` for any MCP tool schema using `$defs`. Affects FastMCP v2.12+ servers and others. Must audit whether Unimatrix's tool schemas (generated via `schemars`) emit `$defs` and inline any found.

2. **Multi-type union arrays** (issue #2654, underlying issue #1481 unresolved as of July 2025): `type: ["string", "null"]` causes `TypeError: fieldValue.toUpperCase is not a function` — Gemini's schema parser incorrectly assumes `type` is always a string scalar.

3. **Reserved parameter names** (issue #13705, closed "not planned"): Parameters named `title` or `type` cause `INVALID_ARGUMENT` — Gemini's schema processor misinterprets them as JSON Schema keywords. No fix planned upstream; Unimatrix must avoid these names.

4. **Missing explicit `type` fields** (issue #6632, fixed via PR #6961): Relaxed but not eliminated. Unimatrix schemas should have explicit `type` on all parameter objects.

5. **Parameter hallucination from other tools** (issue #16318, Gemini CLI v0.23.0): Gemini incorrectly injects parameters from other tools into a target tool call when multiple tools coexist. Whether Unimatrix tools have parameter name overlap that triggers this is partially addressed by the internal track (see Unanswered Questions §4 below).

**Evidence — Codex parameter handling (external track, from GitHub issues):**

Codex 0.20.0 rejected `type: "integer"` in JSON Schema (only `"number"` accepted). Fixed in v0.21.0 via PR #1975 (issue #2204, closed). Current Codex versions handle integer types correctly. The internal track confirms Unimatrix emits integer types via `#[schemars(with = "i64")]` on `GetParams.id`, `CorrectParams.original_id`, etc. — these are safe for current Codex. The `deserialize_i64_or_string` deserializer tolerates string coercion from clients that emit `"3267"` instead of `3267`. Float coercion (`3267.0`) is rejected with an error.

The `tags` field (`Option<Vec<String>>`) is emitted as an array type. If any client sends a comma-separated string (e.g., `"adr,design"` instead of `["adr","design"]`), serde rejects it with a parse error. No documented Codex or Gemini bug causing this specific coercion was found, but it remains a latent risk.

**Combined picture**: The description vocabulary changes (three sections) and the Gemini schema structural fixes (audit for `$defs`, union types, reserved names) are parallel work streams. The description changes affect all non-Claude clients' invocation quality. The schema structural fixes are specifically required for Gemini tool discovery to succeed at all — Gemini's `$defs` bug would make affected tools invisible entirely, making any MRR measurement meaningless before these fixes are in place.

**Recommendation**: (1) Rewrite `context_briefing` `task` parameter doc to replace "NLI entailment scoring" with a behavioral instruction: "Be specific and sentence-form; terse keyword strings return lower-quality results." (2) Rewrite `context_cycle` description to add explicit non-Claude guidance: "If hooks are unavailable (non-Claude Code clients), set `agent_id` explicitly — it is the only attribution signal available." (3) Add a `tags` example showing array form: `["adr", "design"]`. (4) Audit all 12 tool schemas for `$defs` references and inline them. (5) Replace any `type: ["string","null"]` union arrays with `oneOf`. (6) Confirm no parameters use reserved names `title` or `type`. Items 4–6 must be completed before any Gemini MRR measurement is attempted.

---

### §2 — Session and Agent Attribution Compatibility

This section merges external findings on how Codex and Gemini surface agent identity with the internal finding that `context_cycle`'s "hook path" is Claude Code-only.

**Answer**: Neither Codex nor Gemini injects `agent_id` into MCP tool call parameters natively. Both require explicit AGENTS.md or GEMINI.md instruction. The `context_cycle` start/stop pattern works within interactive sessions for both clients but is structurally broken for Gemini non-interactive (headless) mode. The `context_cycle` description's reference to "hook path" is factually wrong for all non-Claude-Code clients — the hook path fires only via Claude Code's UDS mechanism, and no other client invokes it.

**Evidence (external track + internal track cross-reference):**

Codex: AGENTS.md files inject per-repository behavioral instructions but do not add an `agent_id` field to MCP `call_tool` request payloads. The Agents SDK identifies agents by `name` at the orchestration layer, not within MCP tool parameters. Gemini: GEMINI.md serves the same role. The internal `sessionId` (UUID, stored in `~/.gemini/tmp/`) is CLI-internal and is not transmitted to MCP servers as a tool call parameter. Both clients will produce tool calls without `agent_id` unless the model was explicitly instructed to include it.

The internal track confirmed from `mcp/tools.rs` that `context_cycle`'s description says "Attribution is best-effort via the hook path." The internal track also confirmed the hook path is a UDS side channel exclusive to Claude Code (Out-of-Scope Discovery #2 in the internal findings). These two facts combine: the description advertises a passive attribution mechanism that is unavailable to all non-Claude clients and provides no indication that explicit `agent_id` is the only available path.

Session continuity for interactive sessions: Within an interactive Codex CLI session, the agent loop accumulates tool results in conversation history; `context_cycle` start can be followed by stop within a single session with the model maintaining consistent `agent_id` across calls — provided it was instructed to do so. Within an interactive Gemini CLI session, the same pattern holds (`AgentLoopContext` with per-session `sessionId`).

Gemini non-interactive / headless mode: Run-and-exit with no session state and no persistent MCP connections (issue gemini-cli#15338, open). A `context_cycle action:"start"` in one invocation cannot be paired with `action:"stop"` in a separate invocation. The pattern is structurally broken for this mode.

Gemini OAuth token expiry mid-session: Issue gemini-cli#23296 (open) — after OAuth access tokens expire within a running session, Gemini CLI's MCP transport fails silently. The token is injected statically at connection setup; no refresh-aware provider is used. Subsequent tool calls fail silently, which can leave a `context_cycle` in an orphaned open state. This is the closest analog to the `claude-code#28293` class of header-staleness bug — not a drop on initial request, but mid-session staleness causing silent failure.

Concurrent attribution (Codex + Claude running in parallel): The `agent_id` parameter is the authoritative attribution signal for all non-hook clients. The enterprise OAuth JWT `sub` claim would provide a stronger identity signal but requires OAuth 2.1 client credentials flow, which neither client supports natively (see §5). For Wave 2, `agent_id` via explicit instruction is the only viable attribution mechanism.

**Recommendation**: Add AGENTS.md and GEMINI.md configuration as a required Wave 2 onboarding step, with explicit `agent_id` instruction. Document Gemini non-interactive mode as unsupported for `context_cycle` semantics. Add server-side monitoring for orphaned open cycles — they will occur when Gemini's OAuth token expires mid-session (issue #23296). Correct `context_cycle` description to remove misleading "hook path" framing for non-Claude-Code users.

---

### §3 — Context Injection Size Behavior

This section surfaces the most important cross-track interaction in this spike. The internal track found that `max_tokens` is a dead parameter on the MCP path. The external track found that Codex hard-truncates tool responses at 10 KiB / 256 lines. Together these define a dangerous combination: `max_tokens` being unenforced means a Codex user cannot rely on it to prevent Codex's truncation threshold from being hit.

**Answer**: Unimatrix's `max_tokens` parameter for `context_briefing` is accepted, validated, and silently ignored on the MCP path — it has no effect on response size. Codex hard-truncates tool responses at 10 KiB / 256 lines. The combination means that a Codex user who sets `max_tokens` to reduce briefing size gets no effect — the truncation will occur at Codex's client side, potentially mid-JSON, without any server-side signal.

**Evidence (internal track, from `infra/validation.rs`, `mcp/tools.rs`, `services/index_briefing.rs`):**

- `max_tokens` range: 500–10,000 (default 3,000), validated in `validate_briefing_params`.
- `IndexBriefingService::index()` does not reference `max_tokens` anywhere. The service always returns up to `effective_k` = 20 entries.
- Field doc comment in `mcp/tools.rs` line 236 explicitly states: "Reserved for future output truncation. Accepted and validated (500–10000, default 3000) but not currently enforced on results."
- The character-count approximation (`len / 4`) is applied only on the UDS path (`uds/listener.rs`), not the MCP path.
- A 20-entry briefing response at `SNIPPET_CHARS = 150` (from `mcp/response/briefing.rs` line 16) is approximately 20 × 250 chars = ~5,000 characters.

**Evidence (external track, from GitHub issues):**

- Codex: 10 KiB or 256 lines per tool output, head+tail preservation (issues codex#5913 and codex#6426 both closed as completed — this truncation is active). Also `TruncationPolicy::Bytes(10_000)` confirmed in issue codex#7906. OpenAI collaborators confirmed the 256-line limit is incorporated into model training.
- Gemini: `truncateToolOutputThreshold` defaults to 40,000 characters (configurable in `settings.json`). LLM-based summarization triggered above 20,000 tokens. MCP applicability of this threshold is uncertain — PR gemini-cli#12173 (closed stale) attempted to extend it to MCP server tools; whether it applies by default is unconfirmed.

**The dangerous interaction**: At the current briefing size (~5,000 characters for 20 entries), Codex's 10 KiB limit provides ~5 KiB of headroom. This is adequate for `context_briefing` at the current k=20 cap. However, `context_search` returning many full-content entries can exceed 10 KiB, resulting in Codex silently truncating mid-JSON without the model or the server receiving any indication. The `max_tokens` parameter, being unenforced, gives the user a false sense that this is controllable from the Unimatrix side — it is not.

**Recommendation**: Enforce `max_tokens` on the MCP path before Wave 2 launch — truncate the entry list by cumulative snippet character count / 4 until the budget is respected. Separately, set the `context_briefing` default character budget to ≤ 8,000 characters to provide margin below Codex's 10 KiB hard limit. For `context_search`, document the Codex truncation risk and add a `max_entries` configuration option defaulting conservatively. A `max_tokens` parameter that has no effect is a contract violation that will mislead any client operator trying to manage context window usage.

---

### §4 — Eval Harness Provider-Agnostic Gap Analysis

This section merges the internal track's definitive code read of the harness architecture with the external track's finding that no published MRR baseline exists for Codex or Gemini, and the cross-track interaction that Gemini's schema bugs would make any pre-fix MRR measurement meaningless.

**Answer**: The eval harness's format and replay logic are already structurally provider-neutral — there are zero Claude-specific assumptions in scenario format, replay logic, or metric computation. The corpus is 100% Claude-session-derived query patterns. Together these define the eval gap precisely: the harness structure is fine; a provider-neutral corpus is the missing component; and no cross-provider MRR measurement is meaningful until Gemini schema compatibility bugs are resolved.

**Evidence (internal track, from `eval/scenarios/types.rs`, `eval/runner/replay.rs`, `eval/runner/metrics.rs`):**

- `ScenarioRecord` fields: `id`, `query` (raw text), `context.agent_id` (populated from `session_id` as proxy — no dedicated agent_id column exists in `query_log`), `context.retrieval_mode`, `context.phase`, `baseline.entry_ids`, `source` (`"mcp"` or `"uds"`), `expected` (always `null` for log-sourced scenarios).
- `run_single_profile()` builds `ServiceSearchParams` directly from the scenario and calls `SearchService.search()` in-process. No MCP handler invoked, no LLM inference used, no tool call constructed.
- Metrics (MRR, P@K, Kendall tau, category coverage, Shannon entropy) are purely algorithmic over the result list.
- 100% of existing 2,096 scenarios would execute without modification for Codex or Gemini — the harness replays queries through `SearchService` in-process, no client involved.
- The corpus bias: all query text uses Claude Code agent patterns — sentence-form, technical vocabulary, biased toward `context_search` / `context_briefing` → `context_get` flow. Zero scenarios for keyword-form queries, filter-form queries, or lifecycle-phase queries from a non-Claude agent.

**Evidence (external track, from public benchmarks and GitHub issues):**

- No published study provides MRR for MCP knowledge server interactions with Codex or Gemini. The Mastra empirical analysis (30 property types, not peer-reviewed) measures JSON Schema validation failure rates (73–90% Gemini vs. 97–100% Anthropic), not semantic retrieval quality.
- Gemini's `$defs` bug (issue gemini-cli#13326) would cause tools using `$defs` in their schema to be invisible to Gemini entirely — tool discovery fails, so MRR would be artificially at zero for those tools regardless of retrieval quality. Any pre-fix MRR measurement conflates schema errors with retrieval quality and is meaningless as a quality signal.

**Cross-track synthesis**: The internal track establishes that the harness structure needs no changes and that adding 20–40 hand-authored scenarios with `expected` labels and `source: "hand"` tag is backward-compatible with the current `ScenarioSource` enum. The external track establishes that Gemini schema bugs must be resolved before any Gemini measurement is attempted. Together: the sequencing is (1) fix Gemini schema issues, (2) add hand-authored provider-neutral scenarios, (3) run baseline measurements. Do not attempt to establish a provider-specific MRR baseline until both prerequisites are met.

**Recommendation**: Do not add Claude-specific scenario types to the harness. Add 20–40 hand-authored scenarios with populated `expected` fields covering four query types: (a) sentence-form natural language, (b) keyword-form, (c) compound filter (by tag/category), (d) lifecycle-phase queries. Mark them `source: "hand"` — backward-compatible. No harness code changes required. Do not attempt Gemini MRR measurement until Gemini schema compatibility bugs (§1 items 1–3) are resolved. The 0.2558 baseline is Claude-specific and non-transferable.

---

### §5 — HTTP Auth Compatibility

**Answer**: Both Codex and Gemini forward static `Authorization: Bearer` headers correctly across all requests with no documented drop on tool call POSTs — no direct analog to `claude-code#28293` for static tokens. Both have open bugs affecting OAuth-based auth in active sessions (mid-session token expiry), but these do not affect static bearer token configurations. Neither client supports OAuth 2.1 client credentials flow natively — both require static bearer tokens or interactive OAuth DCR. For Wave 2 enterprise tier, token exchange must be performed out-of-band.

**Evidence (external track, from GitHub issues and official documentation):**

**Codex**:
- Static bearer: `bearer_token_env_var = "MY_TOKEN_VAR"` or `env_http_headers = { "Authorization" = "MY_AUTH_VAR" }`. Applied at transport layer across all requests. No documented static header drop on tool call POSTs.
- Issue codex#12859 (open, Feb 2026): `rmcp` HTTP client sends no `User-Agent` header. Cloudflare-protected MCP servers return 403 during OAuth discovery. Does not affect static bearer token configurations.
- Issue codex#7318 (open, March 2026): Short-lived OAuth tokens not auto-reloaded on expiry. Static bearer tokens are not affected.
- Issue codex#5619 (open, March 2026): Protocol version mismatch `2025-06-18` vs `2024-11-05` causes connection drops on Streamable HTTP initialization. Not an auth bug — but prevents MCP connection establishment regardless of auth configuration. This is the most critical live bug for Codex Wave 2 compatibility (see Unanswered Questions).
- OAuth 2.1 client credentials: Not documented. `codex mcp login` uses Authorization Code flow with Dynamic Client Registration. Issue codex#15818: Codex fails entirely when the server does not support DCR — no alternative credential flow is offered.

**Gemini CLI**:
- Static bearer: `"headers": { "Authorization": "Bearer YOUR_TOKEN" }` in `mcpServers` settings.json. Forwarded on all HTTP requests. Env variable expansion supported. No documented forwarding bug analogous to `claude-code#28293`.
- Issue gemini-cli#23296 (open): OAuth token refresh fails in active sessions. MCP transport builds with a static bearer token at connection setup; no refresh-aware auth provider. After access token expiry, all MCP tool calls fail silently. Closest analog to the `claude-code#28293` bug class — mid-session staleness rather than initial drop.
- OAuth 2.1 client credentials: DCR implemented (PR gemini-cli#3569 merged). Auth provider types: `dynamic_discovery`, `google_credentials`, `service_account_impersonation`. Client credentials grant (machine-to-machine) not documented — targets user-facing OAuth flows with browser redirect.

**Recommendation**: For Wave 2 developer cloud tier (static bearer token): no server-side changes required. Both clients forward static `Authorization: Bearer` correctly. Instruct Codex users to add `User-Agent: codex-mcp/1.0` in `http_headers` config to avoid Cloudflare 403 on OAuth discovery (issue codex#12859). For Wave 2 enterprise OAuth 2.1 client credentials tier: document that neither Codex nor Gemini CLI supports the client credentials grant natively. Users must obtain a bearer token out-of-band via a client credentials exchange and configure it as a static env-var token. The MCP clients are token consumers, not OAuth 2.1 grant executors.

---

## Unanswered Questions

1. **Codex Streamable HTTP protocol version mismatch (codex issue #5619)**: Codex sends `protocolVersion: "2025-06-18"` but exhibits `2024-11-05` behavioral expectations, closing the connection before receiving the SSE initialize response. The internal track did not cover which rmcp protocol version Unimatrix targets in its response semantics — `rmcp` was referenced in the internal code read only in the context of codex#12859 (User-Agent), not in the context of `protocolVersion` handshake behavior. If Unimatrix's rmcp targets `2024-11-05` response semantics, this is a live breaking bug for all Codex Streamable HTTP connections. **Must be investigated before Wave 2 Codex testing begins** — check rmcp 0.16.0 changelog for `protocolVersion` declaration.

2. **Gemini CLI `truncateToolOutputThreshold` applicability to MCP server tools**: PR gemini-cli#12173 (closed stale) attempted to extend tool output truncation to MCP server tools. Its closure without merge means the setting's applicability to MCP tools (vs. shell tools) in current Gemini CLI is unconfirmed. Requires empirical testing against a live Gemini CLI session.

3. **Continue MCP spec version `2025-06-18` resolution**: Issue continuedev/continue#8118 requested 2025-06-18 support; resolution not confirmed from available external data. Confirm issue state before treating Continue as fully Wave 2 ready.

4. **Gemini parameter hallucination scope for Unimatrix tools (partial)**: Issue gemini-cli#16318 injects parameters from other tools into target tool calls. The internal track's tool description analysis confirms that `context_search` and `context_get` both have a `query` parameter — this overlap is precisely the pattern that triggers the hallucination bug. The full scope across all 12 Unimatrix tools requires a complete parameter-name audit. This is a prerequisite for Gemini compatibility, not just a monitoring concern.

5. **Enterprise OAuth 2.1 static bearer token workaround, empirical validation**: That the enterprise tier can work by having users obtain a bearer token out-of-band via client credentials exchange and pass it as a static env-var token is architecturally plausible but untested against live deployments of either client.

---

## Out-of-Scope Discoveries

1. **`max_tokens` is a dead parameter on the MCP path** (internal track). Validated, passed to `IndexBriefingParams`, and ignored. Independent of multi-LLM compatibility, this is a public API contract violation. Warrants a separate delivery task to implement enforcement.

2. **`context_cycle`'s hook-path attribution is Claude Code-only and the MCP path description is inaccurate** (internal track). The description implies passive attribution may occur; this is false for non-Claude-Code clients. The behavioral contract gap — that non-hook clients have no indication `agent_id` is their only attribution path — is separate from the description rewrite task and may require explicit server-side documentation of behavior divergence.

3. **Scenario `agent_id` is populated from `session_id` as proxy** (internal track). The `extract.rs` comment notes this explicitly. Accuracy limitation in the current baseline but not a blocker.

4. **Gemini CLI MCP tool count limit of 100** (external track, issue gemini-cli#21823, feature request to raise to 500). Unimatrix has 12 tools — within the limit. Not a Wave 2 concern but note for future tool-count growth.

5. **Codex "as MCP server" pattern** (external track, github.com/kky42/codex-as-mcp). Codex can be exposed as an MCP server for orchestration by other agents. Potentially relevant for Wave 3 multi-agent composition — warrants a separate spike if that architecture is considered.

6. **Continue issue #9151** (external track): Continue fails to start MCP servers in WSL workspaces on Windows. Not relevant for Unimatrix's Linux/cloud deployment targets but relevant for Windows developer onboarding documentation.

7. **Gemini CLI ACP (Agent Communication Protocol)** (external track): A distinct inter-agent communication protocol in Gemini CLI. May interact with MCP tool attribution patterns. Not a Wave 2 blocker — monitor for Wave 3.

---

## Recommendations Summary

### (A) Schema fixes required before Wave 2

- **Gemini `$defs` audit**: Audit all 12 Unimatrix tool schemas for `$defs` references generated by `schemars` and inline them. Gemini API returns `400 INVALID_ARGUMENT` for any schema using `$defs` — affected tools are invisible to Gemini entirely (issue gemini-cli#13326).
- **Gemini union-type arrays**: Replace any `type: ["string","null"]` union arrays with `oneOf`. Required for Gemini CLI (issue gemini-cli#2654).
- **Gemini reserved parameter names**: Confirm no Unimatrix tool parameters are named `title` or `type`. Gemini treats these as JSON Schema keywords (issue gemini-cli#13705, won't-fix upstream).
- **Gemini parameter name overlap audit**: `context_search` and `context_get` both have a `query` parameter — exactly the overlap pattern that triggers Gemini's parameter hallucination bug (issue gemini-cli#16318). Conduct a full parameter-name audit across all 12 tools.

### (B) Tool description rewrites

- **`context_briefing` `task` parameter doc**: Remove "NLI entailment scoring." Replace with: "Be specific and sentence-form; terse keyword strings return lower-quality results."
- **`context_cycle` description**: Remove or qualify "hook path" framing. Add explicit guidance: "If hooks are unavailable (non-Claude Code clients), set `agent_id` explicitly — it is the only attribution signal."
- **`context_briefing` session-initialization directive**: Replace "Use at the start of any task to orient yourself before designing or implementing" with wording not predicated on a distinct session-initialization moment, or qualify: "In agentic sessions, call this at session start..."
- **`tags` parameter**: Add an explicit array-form example: `["adr", "design"]`.

### (C) Server-side changes

- **Implement `max_tokens` enforcement on the MCP path**: Truncate the briefing entry list by cumulative snippet character count / 4 until the budget is respected. The parameter is currently a dead API contract — a `max_tokens` value that has no effect misleads clients trying to manage Codex's 10 KiB hard limit.
- **Set `context_briefing` default character budget to ≤ 8,000 characters**: Provides margin below Codex's active 10 KiB / 256-line truncation threshold.
- **Add orphaned-cycle monitoring**: Gemini's OAuth token expiry (issue gemini-cli#23296) and non-interactive mode (issue gemini-cli#15338) will produce orphaned open `context_cycle` entries. Server-side detection and alerting for cycles open beyond a configurable timeout is needed.
- **Investigate rmcp 0.16.0 `protocolVersion` semantics**: If rmcp targets `2024-11-05` in response behavior, Codex's issue #5619 is a live breaking bug for all Codex Streamable HTTP connections. Must be confirmed before any Codex Wave 2 testing.

### (D) Documentation and onboarding

- **AGENTS.md and GEMINI.md templates as required onboarding step**: Both must explicitly instruct the model to pass a consistent `agent_id` in every Unimatrix tool call. Without this, no attribution works for either client.
- **Gemini non-interactive mode warning**: Document explicitly that Gemini headless/non-interactive mode is incompatible with `context_cycle` semantics (no persistent session state; issue gemini-cli#15338).
- **Codex `User-Agent` header workaround**: Instruct Codex users to add `http_headers = { "User-Agent" = "codex-mcp/1.0" }` to avoid Cloudflare 403 on OAuth discovery (issue codex#12859).
- **Enterprise OAuth 2.1 documentation**: Document that neither Codex nor Gemini CLI supports OAuth 2.1 client credentials grant natively. Enterprise tier users must obtain a bearer token out-of-band and configure it as a static env-var token.
- **Zed deferred**: Document Zed as out of scope for Wave 2 with a note to revisit when discussion zed-industries/zed#34719 resolves.

### (E) Deferred and monitoring

- **Gemini MRR baseline**: Do not attempt to establish a Gemini MRR baseline until schema compatibility fixes (A) are complete. Pre-fix measurement conflates schema errors with retrieval quality and produces meaningless numbers.
- **Provider-neutral eval corpus**: After schema fixes are in, add 20–40 hand-authored scenarios to the eval harness with `expected` labels and `source: "hand"` covering four query types (sentence-form, keyword-form, compound filter, lifecycle-phase). No harness code changes required — `expected: Option<Vec<u64>>` field already exists.
- **Continue Wave 2 readiness**: Verify continuedev/continue#8118 (2025-06-18 spec support) is resolved before declaring Continue in scope.
- **Gemini `truncateToolOutputThreshold` for MCP tools**: Empirical test required to confirm whether this setting applies to MCP server tool outputs (PR gemini-cli#12173 was closed stale without merge).
- **Gemini CLI ACP mode**: Monitor for Wave 3 — may interact with MCP attribution patterns but is not a Wave 2 blocker.
- **Codex-as-MCP-server pattern**: Warrants a separate spike if Wave 3 multi-agent composition architecture is considered.
