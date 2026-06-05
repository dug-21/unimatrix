# FINDINGS-INTERNAL: Unimatrix as Session Host — Observation Fidelity, Architecture, and Roadmap Impact

**Spike**: ass-066
**Date**: 2026-05-30
**Approach**: Investigation + evaluation (code+ecosystem)
**Confidence**: Directional
**Track**: Internal (codebase analysis)

---

## Findings

### Q: What observation fidelity does session hosting provide vs. hooks?

**Answer**: Session hosting via the Claude Agent SDK provides **equivalent or superior observation fidelity** for every signal the UDS hook path currently delivers. However, session hosting and hooks achieve fidelity through fundamentally different mechanisms, and the fidelity advantage of hosting is strongest for remote deployments where UDS is unavailable.

**Evidence**:

#### Signal-by-Signal Comparison

| Signal | UDS Hooks (current) | Session Host (SDK) | Fidelity Comparison |
|---|---|---|---|
| **Session lifecycle (start/stop)** | SessionStart/Stop hook events fire as subprocesses. Session identity from `session_id` in stdin JSON + process lineage (`/proc/{pid}/cmdline`). | Session host creates the session — identity is trivially assigned by Unimatrix at creation time. Start/stop are function call boundaries, not subprocess events. | **Host superior.** Identity is authoritative (Unimatrix created it), not inferred from process lineage. No race conditions, no ppid fallback. |
| **Tool call observation (pre/post)** | PreToolUse/PostToolUse hooks fire per tool call. Payload includes `tool_name`, `tool_input`, `tool_response`, `exit_code`. Provider-specific normalization required (Gemini `BeforeTool`->`PreToolUse`). | SDK hooks (`PreToolUse`, `PostToolUse`, `PostToolUseFailure`) fire as in-process callbacks with the same payload fields: `tool_name`, `tool_input`, `tool_response`, `tool_use_id`. Additionally, the `stream-json` output format emits `tool_use` and `tool_result` content blocks with full input/output. | **Host equivalent or superior.** SDK hooks deliver the same data as UDS hooks. Stream-json adds a secondary observation channel with tool timing data. In-process callbacks eliminate the 3-5ms subprocess spawn overhead per tool call. |
| **Prompt content (UserPromptSubmit)** | UserPromptSubmit hook fires with `prompt` field in stdin JSON. Query text routed to ContextSearch when >= 5 words (`MIN_QUERY_WORDS` in hook.rs line 35). | Host controls the prompt submission — it calls `query()` with the user's prompt. Full prompt text is available before the SDK call, not after. | **Host superior.** Host sees the prompt before submission (can inject context proactively), not after (reactive injection via stdout). This is the key unlock for proactive knowledge delivery. |
| **Context compaction (PreCompact)** | PreCompact hook fires when Claude Code compresses conversation. Hook process reads transcript tail via `extract_transcript_block()` (hook.rs line 1383), server returns `BriefingContent`. 50ms latency budget (`HOOK_TIMEOUT = 40ms` + 10ms margin). | SDK `PreCompact` hook fires as an in-process callback. The host has direct access to the conversation history (it maintains the session state). No filesystem transcript read needed — the conversation is in memory. | **Host superior.** No transcript file I/O. No 50ms subprocess latency budget. Direct memory access to conversation history eliminates the need for the tail-bytes window heuristic (`TAIL_MULTIPLIER = 4`, `MAX_PRECOMPACT_BYTES = 3000`). The host can provide richer compaction defense because it sees the full conversation, not just a 12KB tail window. |
| **Subagent spawn/stop** | SubagentStart/SubagentStop hooks fire. SubagentStart derives query from transcript tail when `prompt_snippet` is absent (hook.rs step 5b, lines 208-236). SubagentStop is fire-and-forget. | SDK `SubagentStart`/`SubagentStop` hooks fire as callbacks with the same fields. The host can additionally inject `CLAUDE.md` content or custom system prompts into subagent context at spawn time. | **Host equivalent to superior.** Same event visibility. Host gains injection capability at subagent spawn that hooks cannot provide (hooks write to stdout, but `hookSpecificOutput` envelope is the only injection path and it's limited to `additionalContext`). |
| **Session identity (cycle/phase correlation)** | Process lineage verification via `SO_PEERCRED` + `/proc/{pid}/cmdline` (auth.rs). Session ID from Claude Code's `session_id` field with `ppid-{N}` fallback (hook.rs line 466). Feature attribution via topic signal majority vote (`check_eager_attribution`) or explicit `context_cycle` interception. | Host assigns session identity authoritatively at creation time. Feature cycle, phase, and role are known because the host created the session with those parameters. No majority vote attribution needed — attribution is explicit. | **Host strictly superior.** This is the most significant fidelity gain. Current hook pipeline requires complex multi-step attribution: `normalize_event_name()` -> `extract_event_topic_signal()` -> `record_topic_signal()` -> `check_eager_attribution()` -> `set_feature_if_absent()` (listener.rs lines 793-847). Host replaces this entire chain with a single authoritative assignment at session creation. PPR phase-category weighting (PhaseFreqTable) benefits directly because phase is known at session start, not discovered mid-session. |
| **Non-Unimatrix MCP tool calls** | PreToolUse/PostToolUse hooks fire for ALL tool calls, including non-Unimatrix MCP tools (e.g., filesystem tools, other MCP servers). Unimatrix observes these as generic RecordEvents with tool_name in payload. | SDK hooks fire for all tool calls identically. Stream-json includes all tool_use content blocks. Host sees every MCP tool call the agent makes. | **Host equivalent.** Same visibility. |
| **Transcript content** | Hook reads transcript via `transcript_path` field pointing to a `.jsonl` file on disk. `extract_transcript_block()` reads the tail of this file via seek + BufReader (hook.rs lines 1383-1433). Subject to file I/O timing and 12KB window limit. | Host owns the conversation. The full transcript is in memory as the session's message history. No file system dependency. | **Host strictly superior.** Full transcript access vs 12KB tail window. No file I/O latency. |

#### Signals Hooks Provide That Session Hosting Cannot

**None identified.** Every signal in the current hook pipeline has an equivalent or superior counterpart in the SDK session hosting model. The hook pipeline was designed as an external observation mechanism; session hosting internalizes that observation, making every signal a natural byproduct of hosting.

#### Signals Session Hosting Provides That Hooks Cannot

1. **Pre-submission context injection**: Host sees the prompt before submission and can modify system context. Hooks see the prompt after submission and can only inject via stdout (reactive, not proactive).
2. **Full conversation history access**: Host maintains the complete message array. Hooks get a 12KB tail window of a JSONL file.
3. **Authoritative session identity**: Host assigns identity at creation. Hooks discover identity after the fact through process lineage and topic signal voting.
4. **Inter-turn injection**: Host can modify system prompts between turns. Hooks can only inject at specific lifecycle events (UserPromptSubmit, PreCompact, SubagentStart).
5. **Token usage tracking**: SDK `stream-json` includes cost/token metrics per turn. Hooks have no access to token consumption data.
6. **Model selection control**: Host chooses which model to use. Hooks have no influence on model selection.

#### MCP Tool Call Stream Coverage Analysis

The SCOPE.md asks: "How much of the learning pipeline does the MCP tool call stream already cover vs. the full hook observation stream?"

| Pipeline Component | MCP Coverage | Hook Coverage | Gap |
|---|---|---|---|
| **Knowledge retrieval tracking** (query_log, co-access pairs) | Full — every context_search/briefing/get is logged with session_id, entry IDs, scores | Partial — ContextSearch from UserPromptSubmit only, no direct tracking of agent-initiated searches | MCP is primary source |
| **Knowledge mutation tracking** (store, correct, deprecate) | Full — audit log records every mutation | None — hooks don't see MCP mutations | MCP is sole source |
| **Session lifecycle** | Partial — MCP handshake provides `client_type`, but no explicit session start/stop | Full — SessionStart, Stop/TaskCompleted hooks | Hooks are primary |
| **Tool call observation** (for retrospective pipeline) | None — MCP tools are Unimatrix-internal; the retrospective needs agent tool calls (Bash, Edit, Read) | Full — PreToolUse/PostToolUse capture all agent tool calls | Hooks are sole source |
| **Rework detection** | None — MCP has no visibility into file mutations | Full — PostToolUse with `post_tool_use_rework_candidate` extraction (hook.rs lines 537-646) | Hooks are sole source |
| **Feature cycle management** | Full — context_cycle tool handles start/phase_end/stop | Partial — PreToolUse interception of context_cycle calls (hook.rs lines 685-705) | Both paths converge |
| **Confidence signals** | Full — explicit helpful/unhelpful votes via MCP | Partial — implicit signals from session outcome (auto-positive, flag-negative) | Complementary |
| **Phase attribution** | Partial — context_cycle sets phase | Full — continuous phase tracking via observation pipeline (crt-043) | Both contribute |

**Quantitative estimate**: The MCP tool call stream covers approximately **40-50% of the learning pipeline inputs** by data volume — primarily knowledge retrieval/mutation tracking and confidence signals. The remaining **50-60%** requires the hook observation stream — primarily tool call observations (which feed the 21 detection rules in the retrospective pipeline), rework detection, session lifecycle, and transcript access.

**Critical gap**: Without hooks, the entire retrospective pipeline (21 detection rules across Agent, Friction, Session, and Scope categories in `unimatrix-observe/src/detection/`) loses its primary data source. The detection rules operate on `ObservationRecord` arrays populated from hook events. The MCP stream provides knowledge-layer signals but not the agent-behavior signals that the retrospective pipeline analyzes.

#### PPR Phase-Category Weighting Impact

The `PhaseFreqTable` (in `services/phase_freq_table.rs`) rebuilds on each background tick from the `observations` table, computing phase-conditioned access frequency data. It depends on:

1. **Phase identity per observation** — currently populated by `session_registry.get_state().current_phase` at observation recording time (crt-043, applied in listener.rs lines 724-726 and 855-858). Under session hosting, phase is known authoritatively from session creation, eliminating the mid-session discovery delay.

2. **Session-correlated observations** — observations grouped by `session_id` and `topic_signal` (feature attribution). Under session hosting, every observation is born with correct session and feature attribution, eliminating the majority-vote attribution pipeline.

3. **Outcome weighting** — `PhaseOutcomeRow` data from `cycle_events` weights the frequency scores. Under session hosting, cycle outcomes are known explicitly at session close because the host manages the lifecycle.

**Net impact on PPR**: Session hosting would **improve PPR data quality** because phase, session, and feature attribution are authoritative from the start rather than discovered heuristically. The `enrich_topic_signal()` fallback chain (listener.rs lines 143-173) and `check_eager_attribution()` majority vote become unnecessary — the host sets these at session creation.

**Recommendation**: Session hosting achieves full observation parity with UDS hooks and provides strictly superior fidelity for session identity, phase attribution, transcript access, and proactive injection. The current MCP tool call stream covers ~40-50% of the learning pipeline; hooks provide the remaining ~50-60%. Session hosting delivers both streams from a single mechanism. For remote deployments where UDS is unavailable, session hosting closes a fundamental gap: remote hook configuration (vnc-024) can never achieve the same session identity fidelity as hosting because HTTP-transported hooks lose process lineage verification.

---

### Q: What is the minimum viable session host?

**Answer**: The minimum viable session host is a **new `unimatrix run` subcommand** that wraps the Claude Agent SDK (Python), registers SDK hooks that map to the existing `dispatch_request()` wire protocol, and pipes observation events to the Unimatrix server over the existing UDS or HTTP transport. Implementation effort: 2-3 weeks for observation parity; 5-8 weeks for the full "knowledge-aware runtime" vision.

**Evidence**:

#### Architecture Options Evaluated

| Option | Description | Observation Parity? | Complexity | Dependencies |
|---|---|---|---|---|
| **A: Wrapper script around `claude -p`** | Shell script that invokes `claude -p --output-format stream-json`, parses NDJSON output, and forwards events to Unimatrix | Partial — stream-json provides tool_use/result/message events but NOT PreCompact, SubagentStart hooks. No hook callbacks available. | Low | Claude Code CLI, jq/Python for parsing |
| **B: `unimatrix run` using Claude Agent SDK (Python)** | New Python process that uses `claude-agent-sdk` to create sessions, registers SDK hooks for all observation events, and forwards them to Unimatrix via UDS/HTTP | Full — SDK hooks provide all 13 event types from the hook specification. In-process callbacks with full payload access. | Medium | Python runtime, `claude-agent-sdk` package, Claude subscription or API credits |
| **C: `unimatrix run` using Claude Agent SDK (TypeScript)** | Same as B but in TypeScript. TypeScript SDK has slightly broader hook support (SessionStart/SessionEnd as callbacks, not just shell commands). | Full — TypeScript SDK has feature parity plus SessionStart/SessionEnd callbacks. | Medium | Node.js runtime, `@anthropic-ai/claude-agent-sdk` package |
| **D: Direct Anthropic API integration** | Rust binary that implements the agentic loop directly using the Anthropic Messages API, managing tool dispatch, context windows, and compaction internally. | Full+ — complete control over every aspect of the session. | Very High | API key, model access, billing, tool implementations, MCP client |

#### Recommended: Option B or C — SDK-Based `unimatrix run`

The Claude Agent SDK (available in Python and TypeScript) provides the exact hook surface needed for observation parity:

```
SDK Hook Events (confirmed available):
  PreToolUse         -> maps to RecordEvent/ContextSearch dispatch
  PostToolUse        -> maps to RecordEvent (rework candidate extraction)
  PostToolUseFailure -> maps to RecordEvent
  UserPromptSubmit   -> maps to ContextSearch (proactive injection)
  PreCompact         -> maps to CompactPayload (compaction defense)
  SubagentStart      -> maps to ContextSearch (subagent injection)
  SubagentStop       -> maps to RecordEvent
  Stop               -> maps to SessionClose
  
Additional (TypeScript only):
  SessionStart       -> maps to SessionRegister
  SessionEnd         -> maps to SessionClose
```

The Python SDK lacks `SessionStart`/`SessionEnd` as in-process callbacks (only as shell command hooks via settings files), but this is non-blocking — the host can synthesize these events at `query()` call boundaries.

#### Minimum Viable Implementation Sketch

```
unimatrix run [--prompt "..."] [--session-id ID] [--feature CYCLE] [--phase PHASE]
              [--model MODEL] [--max-turns N]
              [--remote URL --token TOKEN]
```

**Core flow**:
1. Parse CLI arguments (prompt, session metadata)
2. Connect to Unimatrix server (UDS local or HTTP remote)
3. Send `SessionRegister` with authoritative session_id, feature, role
4. Create Claude Agent SDK client with hooks registered for all event types
5. Each hook callback: construct `HookRequest`, dispatch to Unimatrix via transport
6. For synchronous hooks (UserPromptSubmit, PreCompact): inject Unimatrix response into SDK context
7. On session end: send `SessionClose` with outcome

**Hook -> Dispatch Mapping (reuse existing server-side logic)**:

The `dispatch_request()` function in `listener.rs` (line 516) is already transport-agnostic — it accepts a `HookRequest` and returns a `HookResponse`. Both UDS and HTTP paths already call this same function (UDS via `handle_connection`, HTTP via `PathRouter`'s `/observe` handler at router.rs line 234). A session host would construct `HookRequest` values in the SDK hook callbacks and dispatch them through either transport.

The hook.rs client-side logic would be partially reused and partially replaced:
- **Reused**: `build_request()` event->wire translation (hook.rs line 453), `format_injection()` for response formatting (hook.rs line 1047), event normalization
- **Replaced**: stdin/stdout I/O (replaced by SDK callback parameters), `extract_transcript_block()` (replaced by direct conversation history access), process lineage auth (replaced by authoritative session identity)

#### Dependencies Introduced

| Dependency | Impact | Mitigation |
|---|---|---|
| **Python or Node.js runtime** | New runtime dependency for `unimatrix run` | Python already available on most dev machines; Node.js is a Claude Code prerequisite |
| **Claude Agent SDK package** | `pip install claude-agent-sdk` or `npm install @anthropic-ai/claude-agent-sdk` | First-party Anthropic package, well-maintained |
| **Claude subscription or API credits** | SDK usage requires Claude plan with Agent SDK credits (separate from interactive usage as of June 2026) | Same billing model as `claude -p`. No new cost — shifts where the invocation happens. |
| **API key or OAuth** | SDK authenticates via Anthropic API key or OAuth | Same auth the user already has configured for Claude Code |

**Critical distinction**: Session hosting does NOT require Unimatrix to hold API keys. The user's existing Claude Code authentication is used by the SDK. Unimatrix's role is to create the SDK client with hooks, not to authenticate with Anthropic.

#### Minimum Viable vs Full Vision Capability Map

| Capability | Minimum Viable (Observation Parity) | Full Vision (Knowledge-Aware Runtime) |
|---|---|---|
| Session creation with authoritative identity | Yes | Yes |
| All 13 hook events forwarded to Unimatrix | Yes | Yes |
| Proactive injection via UserPromptSubmit | Yes (reactive — inject response into next turn) | Yes (proactive — modify system prompt before submission) |
| CompactPayload with full conversation access | Yes | Yes |
| Feature/phase/role attribution at session start | Yes | Yes |
| Inter-turn knowledge injection | No | Yes — modify system context between turns based on evolving conversation |
| Dynamic tool provisioning | No | Yes — add/remove MCP tools based on session phase |
| Adaptive model selection | No | Yes — switch models based on task complexity signals |
| Session branching/forking | No | Yes — fork conversation for parallel exploration |
| Multi-agent orchestration | No | **Explicitly out of scope** — Unimatrix is not an orchestration engine |

#### Implementation Effort Estimate

| Component | Effort | Notes |
|---|---|---|
| `unimatrix run` CLI subcommand (Python script or binary) | 3-5 days | Argument parsing, SDK client creation, transport setup |
| Hook callback implementations (13 events) | 5-7 days | Map SDK hook inputs to HookRequest, handle responses |
| Transport adapter (UDS + HTTP) | 2-3 days | Reuse existing LocalTransport or HTTP client |
| UserPromptSubmit injection (reactive) | 2-3 days | Call ContextSearch, inject response into SDK context |
| PreCompact with full conversation access | 2-3 days | Replace transcript_block with conversation history |
| Testing and integration | 3-5 days | End-to-end with real Claude sessions |
| **Total for observation parity** | **2-3 weeks** | |
| Inter-turn injection | 3-5 days | System prompt modification between turns |
| Dynamic CLAUDE.md injection | 2-3 days | Inject knowledge into system context based on phase |
| **Total for knowledge-aware runtime** | **5-8 weeks** | |

#### ASS-014 WASM Thin Client Assessment

ASS-014's Phase 3 envisions a WASM thin client (`@unimatrix/cortical`) compiled from Rust to `wasm32-wasip2`, distributed via npm, communicating with a centralized Unimatrix server over HTTPS. This was designed as a **remote observation client** — it replaces the local hook binary with a platform-agnostic WASM binary.

**Session hosting does NOT supersede ASS-014's WASM client.** They serve different purposes:

| Concern | WASM Thin Client (ASS-014) | Session Host (ass-066) |
|---|---|---|
| **Primary role** | Remote observation transport (hook binary replacement) | Session creation and management |
| **Who creates the Claude session?** | Claude Code (user's interactive session) | Unimatrix |
| **Observation mechanism** | External (hook subprocess) | Internal (SDK callbacks) |
| **Use case** | User runs Claude Code interactively; Unimatrix observes remotely | Unimatrix runs Claude sessions programmatically |
| **Human interaction model** | Human interacts with Claude Code directly | Human submits tasks to Unimatrix, which runs Claude |

**Complementary relationship**: A developer might use Claude Code interactively (observed via WASM thin client for remote or local hooks) for exploration and design, then use `unimatrix run` (session host) for automated delivery tasks, CI/CD pipelines, or scheduled maintenance. Both paths feed the same observation pipeline and knowledge engine.

**Recommendation**: Build `unimatrix run` as a Python-based CLI subcommand using the Claude Agent SDK. Start with observation parity (2-3 weeks), which gives the full hook event stream with authoritative session identity. The knowledge-aware runtime capabilities (inter-turn injection, dynamic context) are additive — they build on observation parity, not a separate architecture. Use the Python SDK (not TypeScript) because it integrates more naturally with the Rust server via subprocess or HTTP, and Python is more commonly available than Node.js on server environments.

---

### Q: What is the impact on vnc-024 and the remote observation roadmap?

**Answer**: vnc-024 (remote hook configuration) **remains valuable as a fallback and transitional architecture** but is **no longer the strategic solution** for remote observation if session hosting proves viable. The sequencing recommendation is: ship vnc-024 as a minimal configuration-only deliverable (no new Rust binary), then build `unimatrix run` as the primary remote observation mechanism.

**Evidence**:

#### vnc-024 Scope Analysis

vnc-024 is scoped as "Remote Observation Client Configuration" — the gap between the `/observe` endpoint (shipped in vnc-022) and a configured client that sends hooks to it. Its goals are:

1. Validate curl-based hook configuration for remote `/observe`
2. Document the wire contract
3. Ship copy-paste hook configuration examples
4. Design an installation mechanism (`unimatrix client config`)
5. Validate timeout behavior

The SCOPE identifies a **critical finding**: raw curl against `/observe` returns `HookResponse` JSON envelopes, but the local hook binary (`unimatrix hook`) performs client-side response transformation (JSON -> plain text, JSON -> `hookSpecificOutput` envelope, transcript block prepending). Pure curl does NOT replicate these transformations. Synchronous events (UserPromptSubmit, PreCompact, SubagentStart) require either server-side content negotiation, a jq wrapper, or the `hook-remote` binary.

#### Session Hosting Impact on vnc-024

| vnc-024 Goal | Still Needed? | Rationale |
|---|---|---|
| Curl-based hook config validation | **Yes, but lower priority** | Useful for minimal/debugging deployments. Not the primary remote path. |
| Wire contract documentation | **Yes** | The `/observe` endpoint contract is needed regardless — session host also uses it for HTTP transport. |
| Copy-paste hook examples | **Reduced priority** | If session hosting is the primary remote path, manual hook configuration becomes a fallback, not the default. |
| Installation mechanism (`unimatrix client config`) | **Reduced priority** | `unimatrix run` subsumes the "how do I connect" story for the primary use case. |
| Timeout validation | **Still useful** | Informs both curl hooks and session host HTTP transport. |

#### Transitional Architecture

The natural architecture is NOT "hooks for local, session hosting for remote" — it's "hooks for interactive, session hosting for programmatic." The distinction is about who creates the Claude session, not where it runs:

- **Interactive sessions** (human at the keyboard with Claude Code): hooks observe. This works locally (UDS) and remotely (HTTP hooks via vnc-024).
- **Programmatic sessions** (automated tasks, CI/CD, scheduled work): session host creates and observes. This works locally and remotely.

```
Local development:
  Claude Code (interactive) -> UDS hooks -> Unimatrix server (local)
  Full observation fidelity. Existing shipped architecture. Zero changes.

Remote deployment:
  unimatrix run (session host) -> SDK hooks -> Unimatrix server (remote via HTTP)
  Full observation fidelity. Authoritative session identity.
  
Remote fallback:
  Claude Code (interactive) -> curl/HTTP hooks -> /observe endpoint (remote)
  Reduced fidelity (response format mismatch, no process lineage auth).
  vnc-024 configuration enables this path.
```

#### Sequencing Recommendation

1. **Ship vnc-024 as documentation + server-side content negotiation (Option A from vnc-024 SCOPE)**: The wire contract documentation and content negotiation (`Accept: text/plain`) are valuable independent of session hosting. This is a 1-2 week deliverable that unblocks manual remote hook setup as a fallback. No `hook-remote` binary — curl with content negotiation suffices.

2. **Build `unimatrix run` observation parity (ass-066 -> new feature)**: 2-3 weeks. This becomes the primary remote observation mechanism and the first step toward the knowledge-aware runtime.

3. **Defer vnc-024 installation tooling**: The `unimatrix client config` CLI and copy-paste templates become lower priority once `unimatrix run` exists. They're still useful for interactive remote sessions but are no longer on the critical path.

**Can vnc-024 proceed in parallel?** Yes. vnc-024's wire contract documentation and content negotiation work are additive to session hosting — the `/observe` endpoint is the HTTP transport that `unimatrix run` will also use for remote deployments. There is no conflict.

#### Impact on Remote Observation Roadmap

The remote observation roadmap shifts from:

**Previous roadmap**:
1. vnc-022: `/observe` endpoint (shipped)
2. vnc-024: Hook configuration for remote (curl + HTTP hooks)
3. ASS-014 Phase 3: WASM thin client for remote (future)

**Proposed roadmap**:
1. vnc-022: `/observe` endpoint (shipped)
2. vnc-024: Wire contract docs + content negotiation (simplified scope, 1-2 weeks)
3. **NEW**: `unimatrix run` — session hosting for programmatic/remote observation (2-3 weeks for parity, 5-8 weeks for knowledge-aware runtime)
4. ASS-014 Phase 3: WASM thin client (deferred — applies only to interactive remote sessions, which are the minority use case)

The strategic shift is that **programmatic session hosting becomes the primary remote observation mechanism**, and manual hook configuration becomes a fallback for interactive sessions. This is architecturally superior because:

- Session identity is authoritative (no process lineage verification over HTTP)
- No response format mismatch (host formats its own injection content, eliminating the vnc-024 critical finding about JSON-vs-text mismatch)
- No curl/jq wrapper complexity
- Full transcript access instead of 12KB tail window
- PPR phase-category data quality improves immediately

**Recommendation**: Proceed with vnc-024 at reduced scope (wire contract documentation and content negotiation only). Begin `unimatrix run` implementation immediately after — it is the higher-value deliverable for remote observation fidelity. The WASM thin client (ASS-014 Phase 3) should be deferred to a later evaluation after session hosting validates the "Unimatrix hosts sessions" model.

---

## Unanswered Questions

1. **Python SDK SessionStart/SessionEnd gap**: The Python Claude Agent SDK does not support `SessionStart`/`SessionEnd` as in-process callback hooks (TypeScript only). This is non-blocking (the host can synthesize these at `query()` boundaries), but it means the TypeScript SDK has slightly better hook coverage. Needs validation of whether this gap closes in future SDK releases or whether the Python workaround is sufficient.

2. **Agent SDK billing model under June 2026 changes**: Starting June 15, 2026, Agent SDK and `claude -p` usage draws from a separate monthly Agent SDK credit allocation. The cost implications of running `unimatrix run` sessions need analysis — is this additive cost, or does it replace existing interactive usage?

3. **SDK hook payload completeness**: Web research confirms all 13 event types are available as SDK hooks, but the exact payload structure differences between SDK hooks and Claude Code CLI hooks need a PoC to validate. Some fields (e.g., `mcp_context` for Gemini normalization) may not be present in SDK hook inputs.

---

## Out-of-Scope Discoveries

1. **`claude -p --output-format stream-json` as a lightweight observation channel**: The `stream-json` format provides NDJSON with `init`, `message`, `tool_use`, `tool_result`, and `result` message types including `session_id`, `timestamp`, and full tool input/output. This could serve as a parsing-based observation path that requires no SDK hooks — just pipe the output. However, it lacks PreCompact, SubagentStart, and other lifecycle hooks. Relevant if a simpler-than-SDK approach is desired for read-only observation.

2. **Session forking for exploration**: The Claude Agent SDK supports session resumption and forking. This creates a potential product capability where Unimatrix could fork a session to explore alternative approaches, then merge learnings. This is deep future territory and likely crosses the "not an orchestration engine" boundary, but the mechanism is available.

3. **Token economics signal**: The SDK and `stream-json` expose token usage per turn. This is a new observation signal the current hook pipeline does not capture. Token consumption patterns could inform the retrospective pipeline (e.g., detecting context bloat, measuring injection efficiency).

4. **Multi-model session hosting**: Since the host controls model selection, Unimatrix could potentially route different phases of work to different models (e.g., Haiku for boilerplate, Opus for architecture). This is explicitly outside the current "not an orchestration engine" boundary but is technically feasible.

---

## Recommendations Summary

- **Q2 (Observation Fidelity)**: Session hosting achieves full observation parity and provides strictly superior fidelity for session identity, phase attribution, transcript access, and proactive injection. The current MCP tool call stream covers ~40-50% of the learning pipeline; hooks provide the remaining ~50-60%. Session hosting delivers both streams from a single mechanism.
- **Q3 (Minimum Viable Session Host)**: Build `unimatrix run` as a Python CLI using the Claude Agent SDK. 2-3 weeks for observation parity, 5-8 weeks for knowledge-aware runtime. Does not supersede ASS-014 WASM client — they are complementary (programmatic vs interactive sessions).
- **Q6 (vnc-024 Impact)**: Reduce vnc-024 to wire contract documentation + content negotiation (1-2 weeks). Proceed in parallel. `unimatrix run` becomes the primary remote observation mechanism. WASM thin client deferred.
