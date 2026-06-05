# FINDINGS: Unimatrix as Session Host — Observation Fidelity, Product Surface, and Vision Implications

**Spike**: ass-066
**Date**: 2026-05-30
**Approach**: Investigation + evaluation (code+ecosystem, exhaustive)
**Confidence**: Directional
**Mode**: Synthesis of FINDINGS-INTERNAL.md and FINDINGS-EXTERNAL.md

---

## Findings

### Q1: What programmatic session interfaces exist for target clients?

**Answer**: Claude Code offers the most complete programmatic interface by a significant margin and is the only client viable for full-fidelity session hosting today. Codex CLI is architecturally capable but constrained by internal APIs and limited hooks. Gemini CLI is feasible for single-shot work but lacks headless session resumption and an official SDK.

**Evidence**:

**Claude Code** provides four interfaces of increasing capability:

1. **CLI print mode** (`claude -p`): Non-interactive, supports stdin piping (10MB cap), three output formats (text, json, stream-json), session resumption (`--resume`, `--continue`, `--fork-session`), full system prompt control (replace, append, from file), MCP server configuration (`--mcp-config`), budget caps (`--max-budget-usd`), turn limits (`--max-turns`), permission modes, and bare mode for fast scripted invocations.

2. **Bidirectional streaming** (`--input-format stream-json` + `--output-format stream-json`): Persistent multi-turn sessions over stdin/stdout in a single process. The closest thing to a "session socket" at the CLI level. Currently experimental with minimal documentation (open issue anthropics/claude-code#24594) and a known bug causing duplicate JSONL entries (#5034).

3. **Agent SDK** (Python and TypeScript): The primary recommended interface for session hosting. The `query()` function returns an async iterable of typed messages. Provides programmatic hook callbacks (13 events in Python, 21 in TypeScript), native subagent orchestration via `agents` parameter, session management with resume/fork, MCP server configuration, and system prompt control. The TypeScript SDK is more complete (has `SessionStart`/`SessionEnd`, `PostToolBatch`, `MessageDisplay`, `WorktreeCreate/Remove`, and several other callbacks the Python SDK lacks).

4. **Stream-json event observation**: NDJSON events including `system.init` (session_id, tools, model), `system.compact_boundary`, `assistant` messages with `tool_use` content blocks, `user` messages with `tool_result` blocks, and `result` with session costs. Subagent messages identified by `parent_tool_use_id`. With `--verbose --include-partial-messages --include-hook-events`, provides maximum observation fidelity.

**Authentication**: Six paths — `ANTHROPIC_API_KEY`, Claude subscription OAuth, `claude setup-token` for CI, Amazon Bedrock, Google Vertex AI, Microsoft Azure Foundry. As of June 15, 2026, `claude -p` and Agent SDK usage on subscription plans draws from a separate Agent SDK credit allocation.

**Codex CLI** provides:

1. **`codex exec`**: Non-interactive with `--json` NDJSON event streaming. Rich event types (thread/turn/item lifecycle, approval requests, MCP server events). Session resumption via `resume --last` or `resume <SESSION_ID>`.

2. **App-server JSON-RPC 2.0**: Bidirectional protocol over stdio/WebSocket/Unix socket. Full thread lifecycle control (start, resume, fork, compact), per-turn settings overrides, approval system, MCP tool calls, and process spawning. Architecturally the richest wire protocol of any client, but internal — designed for first-party clients, not external hosting.

3. **TypeScript SDK** (`@openai/codex-sdk`): `startThread()` and `run()` API with `runStreamed()` for event observation.

4. **Hooks**: Only 5 events (SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, Stop). Shell commands only — no programmatic callbacks. PreToolUse can only deny, not approve or modify input.

5. **MCP server mode** (`codex mcp-server`): Codex as a tool invoked by other agents. Exposes `codex` and `codex-reply` tools.

**Gemini CLI** provides:

1. **Headless mode** (`gemini -p`): Non-interactive with text/json/stream-json output. **No session resumption in headless mode** — the most significant limitation for session hosting.

2. **Hooks**: 11 events — the most comprehensive set, including unique capabilities like `BeforeModel` (prompt modification, model swapping, response mocking) and `BeforeToolSelection` (tool filtering). Shell commands only, no programmatic callbacks.

3. **No official SDK**: `@google/gemini-cli-core` exists but undocumented for external use. Community package `@ketd/gemini-cli-sdk` wraps CLI as subprocess.

4. **A2A server** (`@google/gemini-cli-a2a-server`): Experimental Agent-to-Agent protocol for remote control.

**Comparative ranking for session hosting feasibility**:
1. **Claude Code**: Deeply feasible. SDK provides everything needed.
2. **Codex CLI**: Architecturally possible with reduced control (limited hooks, no programmatic callbacks, app-server is internal).
3. **Gemini CLI**: Feasible for single-shot only. No headless session resumption makes multi-turn hosting impractical without maintaining a long-lived process.

**Recommendation**: Use the Claude Agent SDK (Python or TypeScript) as the primary session hosting interface. For non-Claude clients, fall back to CLI wrapping (`codex exec --json`, `gemini -p --output-format stream-json`) with reduced observation fidelity. Full model-agnosticism at the session hosting level is not achievable today without significant abstraction work.

---

### Q2: What observation fidelity does session hosting provide vs. hooks?

**Answer**: Session hosting via the Claude Agent SDK provides **equivalent or superior observation fidelity** for every signal the UDS hook path currently delivers. No signals are lost. Six new signals become available that hooks cannot provide.

**Evidence**:

**Signal-by-signal comparison**:

| Signal | UDS Hooks (current) | Session Host (SDK) | Fidelity |
|---|---|---|---|
| **Session lifecycle** | SessionStart/Stop as subprocess events, identity from `session_id` + process lineage (`/proc/{pid}/cmdline`) | Host creates session — identity assigned authoritatively at creation time | **Host superior** — no race conditions, no ppid fallback |
| **Tool call observation** | PreToolUse/PostToolUse as subprocesses with `tool_name`, `tool_input`, `tool_response` | SDK hooks as in-process callbacks with same payload fields, plus `stream-json` as secondary channel | **Host equivalent/superior** — eliminates 3-5ms subprocess spawn overhead |
| **Prompt content** | UserPromptSubmit fires after submission, stdout injection is reactive | Host controls prompt submission — sees prompt before calling `query()` | **Host superior** — proactive injection unlocked |
| **Context compaction** | PreCompact reads 12KB transcript tail via `extract_transcript_block()`, 50ms latency budget | In-process callback with direct memory access to full conversation history | **Host strictly superior** — full transcript vs 12KB window, no file I/O |
| **Subagent spawn/stop** | SubagentStart/Stop hooks, limited injection via `hookSpecificOutput` | SDK callbacks with injection capability at subagent spawn, can inject CLAUDE.md or custom system prompts | **Host equivalent to superior** |
| **Session identity** | Complex multi-step: `SO_PEERCRED` + `/proc/cmdline` + majority-vote feature attribution | Authoritative assignment at creation — feature, cycle, phase, role all known | **Host strictly superior** — eliminates entire attribution pipeline |
| **Non-Unimatrix MCP tool calls** | PreToolUse/PostToolUse fire for all tool calls | SDK hooks fire identically for all tool calls | **Host equivalent** |
| **Transcript content** | File I/O via `transcript_path`, 12KB tail window, seek + BufReader | Full conversation in memory as session message history | **Host strictly superior** |

**Signals hooks cannot provide that session hosting can**:
1. **Pre-submission context injection** — modify system context before prompt reaches the model
2. **Full conversation history** — complete message array, not a 12KB tail window
3. **Authoritative session identity** — assigned at creation, not discovered via process lineage voting
4. **Inter-turn injection** — modify system prompts between turns
5. **Token usage tracking** — `stream-json` includes cost/token metrics per turn
6. **Model selection control** — host chooses which model to use

**MCP tool call stream coverage analysis**: The MCP tool call stream (context_search, context_store, etc.) covers approximately **40-50%** of the learning pipeline inputs by data volume — primarily knowledge retrieval/mutation tracking and confidence signals. The remaining **50-60%** requires the hook observation stream — tool call observations (feeding 21 detection rules in the retrospective pipeline), rework detection, session lifecycle, and transcript access. Without hooks, the entire retrospective pipeline loses its primary data source.

**PPR impact**: Session hosting would **improve PPR data quality** because phase, session, and feature attribution are authoritative from the start. The entire `enrich_topic_signal()` fallback chain and `check_eager_attribution()` majority vote become unnecessary.

**Tension between tracks**: The external track identified that bidirectional streaming (`--input-format stream-json`) could serve as an alternative to the SDK — a "session socket" approach maintaining a long-lived `claude -p` subprocess. The internal track focused exclusively on the SDK path. Both are viable; the SDK path is recommended because it provides typed message objects, programmatic hook callbacks, and native subagent control that the CLI path would require reverse-engineering.

**Recommendation**: Session hosting achieves full observation parity and provides strictly superior fidelity for the most critical signals. The current MCP tool call stream covers ~40-50% of the learning pipeline; session hosting delivers the full 100% from a single mechanism. For remote deployments where UDS is unavailable, session hosting closes a fundamental gap that HTTP-transported hooks cannot — remote hooks can never achieve process-lineage-quality session identity.

---

### Q3: What is the minimum viable session host?

**Answer**: A new `unimatrix run` subcommand using the Claude Agent SDK (Python), registering SDK hooks that map to the existing `dispatch_request()` wire protocol. **2-3 weeks** for observation parity; **5-8 weeks** for the full knowledge-aware runtime vision. Session hosting does NOT require Unimatrix to hold API keys — it uses the user's existing Claude Code authentication.

**Evidence**:

**Four architecture options evaluated**:

| Option | Observation Parity? | Complexity | Verdict |
|---|---|---|---|
| **A: Wrapper around `claude -p`** | Partial — `stream-json` lacks PreCompact, SubagentStart hooks | Low | Insufficient for full parity |
| **B: SDK (Python)** | Full — 13 hook event types as in-process callbacks | Medium | **Recommended** |
| **C: SDK (TypeScript)** | Full — 21 events including SessionStart/SessionEnd | Medium | Viable alternative, slightly better hook coverage |
| **D: Direct Anthropic API** | Full+ — complete control | Very High | Overengineered for this purpose |

**Recommended architecture**: Option B — Python SDK-based `unimatrix run`.

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

**Key design decision**: The existing `dispatch_request()` function in `listener.rs` is already transport-agnostic — both UDS and HTTP paths call it. A session host constructs `HookRequest` values in SDK hook callbacks and dispatches through either transport. The hook.rs client-side logic is partially reused (`build_request()`, `format_injection()`) and partially replaced (stdin/stdout I/O, `extract_transcript_block()`, process lineage auth).

**Dependencies introduced**: Python runtime, `claude-agent-sdk` package, Claude subscription or API credits. Critical: the SDK uses the user's existing authentication — Unimatrix does NOT need to store or manage API keys.

**SDK gap**: Python SDK lacks `SessionStart`/`SessionEnd` as in-process callbacks (TypeScript only). Non-blocking — the host synthesizes at `query()` boundaries.

**Effort breakdown**:

| Component | Effort |
|---|---|
| CLI subcommand (argument parsing, SDK client, transport) | 3-5 days |
| Hook callback implementations (13 events) | 5-7 days |
| Transport adapter (UDS + HTTP) | 2-3 days |
| UserPromptSubmit + PreCompact injection | 4-6 days |
| Testing and integration | 3-5 days |
| **Total observation parity** | **2-3 weeks** |
| Inter-turn injection + dynamic CLAUDE.md | 5-8 days |
| **Total knowledge-aware runtime** | **5-8 weeks** |

**Minimum viable vs full vision**:

| Capability | Min Viable | Full Vision |
|---|---|---|
| Authoritative session identity | Yes | Yes |
| All 13 hook events forwarded | Yes | Yes |
| Proactive injection via UserPromptSubmit | Yes (reactive) | Yes (proactive — modify before submission) |
| Full conversation access at PreCompact | Yes | Yes |
| Feature/phase/role at session start | Yes | Yes |
| Inter-turn knowledge injection | No | Yes |
| Dynamic tool provisioning | No | Yes |
| Adaptive model selection | No | Yes |

**ASS-014 WASM thin client relationship**: Session hosting does NOT supersede ASS-014. They serve different purposes — WASM client is for remote observation of interactive sessions (human at the keyboard), session host is for programmatic sessions (Unimatrix runs Claude). Complementary, not competing.

**Recommendation**: Build `unimatrix run` as a Python CLI using the Claude Agent SDK. Start with observation parity (2-3 weeks). The knowledge-aware runtime capabilities are additive — they build on observation parity, not a separate architecture.

---

### Q4: What are the product surface implications?

**Answer**: Session hosting creates a fundamentally new product surface that is additive to the existing MCP server, introduces no new credential management requirements, works locally and remotely, does not compete with Claude Code's UX, creates initial model coupling that can be mitigated architecturally, and opens product capabilities that do not exist today in any comparable tool.

**Evidence**:

#### Does Unimatrix now need API credentials?

**No.** The Claude Agent SDK uses the user's existing Claude Code authentication — `ANTHROPIC_API_KEY`, `claude setup-token` for CI, or cloud provider credentials (Bedrock, Vertex, Azure Foundry). Unimatrix creates the SDK client with hooks; it does not authenticate with Anthropic.

The deployment model change is subtle but real:

- **Today**: Unimatrix server (single binary, zero config) + hook binary (configured per-client). Unimatrix has no relationship with any LLM provider.
- **With session hosting**: Unimatrix server + `unimatrix run` (Python process). The `unimatrix run` process needs the user's LLM credentials in its environment. Unimatrix still does not store credentials, but it now needs them at invocation time.

This preserves architectural principle 8 ("No secrets in any database") completely.

**Caveat for remote/hosted scenarios**: If deployed as a shared service, per-user credential injection without storage needs architectural design (session-scoped credential injection pattern).

#### Does the local story change?

**Session hosting works identically locally and remotely.** The interesting local use case is NOT replacing interactive Claude Code — it is automating programmatic tasks. CI/CD pipelines, scheduled maintenance, batch operations. A developer continues using Claude Code interactively; `unimatrix run` handles programmatic work with full observation fidelity.

#### Does this compete with Claude Code's UX?

**No, and the distinction is structural.** Claude Code is an interactive development environment. `unimatrix run` is a programmatic session launcher — no terminal UI, no permission prompts, no human in the loop during execution. It is closer to `make` or `cargo test` than to an IDE.

Anthropic has explicitly designed the Agent SDK for this use case. The June 2026 billing change (separate Agent SDK credit allocation) confirms they see interactive and programmatic as distinct product surfaces.

Where tension could emerge: if `unimatrix run` adds an interactive mode, it overlaps with Claude Code's `claude agents` and `claude remote-control`. The recommendation is to keep `unimatrix run` purely programmatic.

#### Does this create model coupling?

**Initial implementation creates Claude coupling. Architectural mitigation is feasible but not free.**

| Approach | Fidelity | Effort |
|---|---|---|
| Claude Agent SDK | Full (13-21 hook events, typed messages, native subagents) | Medium |
| `codex exec --json` wrapper | Reduced (5 hook events, deny-only PreToolUse) | Medium |
| `gemini -p --output-format stream-json` wrapper | Minimal (no session resumption, no SDK) | Medium |
| Direct provider API integration | Full control but massive scope | Very High |

**Practical recommendation**: Start Claude-only. The MCP server surface already works with all clients (knowledge engine remains model-agnostic); session hosting is additive, not a replacement for hooks.

#### What happens to the existing MCP server surface?

**It coexists.** MCP server handles agent-initiated knowledge operations. Session hosting handles Unimatrix-initiated session management with observation as byproduct. Both feed the same pipeline: MCP calls provide knowledge-layer signals (~40-50%), hook/SDK events provide behavior-layer signals (~50-60%).

#### How does this affect the domain-agnostic goal?

**Session hosting is domain-neutral.** The `unimatrix run` subcommand launches agent sessions, observes them, and feeds the observation pipeline — nothing domain-specific.

#### NEW product capabilities session hosting creates

**1. Knowledge-Conditioned Sessions**: Unimatrix injects knowledge continuously between turns — not just at first-turn. Stored patterns are immediately available, eliminating session-boundary latency.

**2. Proactive Compaction Defense**: Full conversation in memory at PreCompact, not a 12KB tail window. Every architectural decision, every pattern application preserved through compaction.

**3. Session-Level Cost Attribution**: Token/cost metrics correlated with feature cycle, phase, and outcome. Cost-per-feature, cost-per-phase datasets. "Which knowledge injections reduce cost by preventing rework?"

**4. Reproducible Session Patterns**: Entire configuration explicit (prompt, system prompt, model, tools, phase, feature, MCP servers). Failed sessions re-runnable. Successful sessions templatable.

**5. Cross-Session Knowledge Continuity**: Unimatrix injects outcome and key decisions from previous session into next session's system prompt. Design -> implementation -> review as three sequential `unimatrix run` invocations with automatic continuity.

**6. Observation-Driven Session Steering**: If observation pipeline detects rework pattern, inject relevant lesson. If session exceeds cost thresholds, reduce turn limit. NOT orchestration — knowledge-aware session management. The ass-063 litmus test applies: "what should I do differently?" (knowledge) is within bounds; "do this instead" (control) crosses the line.

**7. Auditable Agent Sessions**: Every event passes through observation pipeline and audit log. Complete, attributable, tamper-evident record. For regulated domains, this audit trail is a differentiating capability — no other knowledge engine provides it because no other knowledge engine hosts the sessions.

**Recommendation**: Session hosting creates a new product surface that is additive to the MCP server. Requires no credential management by Unimatrix. Works locally and remotely. Does not compete with Claude Code's interactive UX. Initial Claude coupling acceptable because knowledge engine remains model-agnostic. The seven new capabilities represent product differentiation that no comparable tool provides today.

---

### Q5: What does this mean for the vision?

**Answer**: Session hosting is a vision evolution, not a vision replacement. The core — knowledge engine, intelligence pipeline, graph, confidence system — does not change. The question is not "should we do this" (the capability is clearly valuable) but "how do we frame what Unimatrix becomes."

Three candidate vision framings follow. Each represents a genuinely different product identity with different strategic implications.

---

#### Framing A: "Knowledge-Aware Runtime" — Additive Layer

**What changes**:
- Vision statement adds: "Unimatrix can also host agent sessions, creating a knowledge-aware runtime where observation is not instrumented but inherent, and knowledge delivery is not reactive but continuous."
- Strategic goals gain a fifth: **Session-native observation** — programmatic sessions run through Unimatrix for complete observation fidelity without external instrumentation.
- The "not an orchestration engine" boundary is refined: "Unimatrix does not decide what work to do. It can decide how knowledge is delivered during work it hosts."

**What stays the same**: Knowledge engine is the core product identity. MCP server surface unchanged. Hook-based observation continues for interactive sessions. Domain-agnostic goal unchanged. All architectural principles unchanged.

**Relationship**: Runtime is a **delivery mechanism for the knowledge engine**. Analogous to how a search engine added a browser — the browser is a delivery mechanism for search, not a separate product.

**Strategic implications**:
- Product category: remains "knowledge engine for agentic workflows"
- Competitive moat: observation fidelity becomes a differentiator
- Risk profile: LOW — strictly additive, can be shipped incrementally, can be removed
- Complexity cost: Python dependency alongside Rust core

**Opportunities**: CI/CD integration, scheduled maintenance, batch operations, session-level analytics.

**Risks**: Scope creep pressure, Python dependency complicates single-binary story, attention dilution from knowledge engine improvements.

**Framing A is the conservative choice.** Captures benefits without rethinking identity. Easiest to walk back.

---

#### Framing B: "Intelligence Platform" — Transformative Evolution

**What changes**:
- Vision statement becomes: "Unimatrix is an intelligence platform for agentic workflows. It makes knowledge curation a first-class workflow activity, makes observation a natural consequence of session hosting, and makes every session smarter than the last because the platform learns from all of them."
- The knowledge engine is one subsystem of a platform. Session hosting is another. The intelligence pipeline is the unifying layer.
- "Not an orchestration engine" evolves to: "Unimatrix is not a task scheduler. It does not decide what work to do. It provides the intelligent substrate on which agentic work executes."

**Strategic goals restructure**:

| Current Goal | Evolution |
|---|---|
| Self-learning intelligence | Unchanged — now benefits from richer hosted-session signals |
| Proactive knowledge delivery | Expanded — from "inject at hook events" to "continuous session-aware delivery" |
| Developer-friendly deployment | Refined — "one container, one command" now includes session hosting |
| Domain-agnostic platform | Unchanged |
| **(NEW)** Session-native intelligence | Sessions through Unimatrix gain knowledge conditioning, observation, cross-session continuity |

**What stays the same**: Knowledge graph, confidence system, detection rules, PPR, embedding pipeline. MCP server surface. Hash-chain integrity, audit log. Single-user deployment model initially.

**Relationship**: Knowledge engine and session runtime are **peers within a platform**. Neither primary. Connected by the intelligence pipeline feedback loop: observations from sessions improve knowledge delivery, knowledge delivery improves session outcomes.

**Strategic implications**:
- Product category shifts: "developer knowledge tool" -> "agentic intelligence platform"
- Competitive landscape changes: stops competing only with knowledge tools, begins competing with agentic infrastructure (LangSmith, Braintrust, Arize) from unique position — no agentic infrastructure tool has a knowledge engine with confidence scoring and self-learning delivery
- Competitive moat deepens: knowledge engine makes sessions smarter; sessions make knowledge engine smarter. This flywheel exists in no competitor
- Risk profile: MEDIUM — product identity change that commits to session hosting as first-class

**Opportunities beyond Framing A**:
- **Intelligence flywheel narrative**: "Your 100th delivery is dramatically better than your 1st because Unimatrix learned from all 99"
- **Session benchmarking**: compare outcomes across models, configurations, phases. "Opus costs 3x more than Sonnet for delivery but produces 40% fewer rework cycles"
- **Knowledge ROI measurement**: measure whether stored patterns actually improve outcomes. Curation becomes measurable
- **Template marketplace (future)**: session configurations that produce good outcomes become shareable
- **Multi-project intelligence (future)**: patterns from one project surfaced in another

**Risks**: Identity confusion ("knowledge tool or agentic platform?"), increased surface area, platform gravity making anti-orchestration boundary harder to hold, dependency multiplication.

**Framing B is the ambitious choice.** Larger market, deeper moat, commits to session hosting as permanent first-class surface.

---

#### Framing C: "The Agent's Memory" — Reframing the Whole Product

**What changes**:
- Vision reframes around the agent's experience: "Unimatrix is the agent's memory — persistent, trustworthy, and continuously improving. It remembers what the agent learned, understands where the agent is in its work, and surfaces what the agent needs before it asks. When Unimatrix hosts the session, memory is seamless; when it observes from outside, memory requires instrumentation."
- Knowledge engine becomes "memory storage." Intelligence pipeline becomes "memory recall." Session hosting becomes "native memory" (seamless). Hook observation becomes "instrumented memory" (works but with gaps).
- "Not an orchestration engine" becomes: "Unimatrix is not an agent. It is what agents remember."

**What stays the same**: Everything technical unchanged — same graph, confidence system, observation pipeline, MCP surface. What changes is the metaphor, the product narrative, the user mental model.

**Relationship**: Session hosting is the **preferred deployment** of the memory system. When Unimatrix hosts the session, memory is complete and effortless — like a brain that is always on. When observing via hooks, memory is mediated and lossy — like taking notes vs. experiencing firsthand.

**Strategic implications**:
- Product category shifts to: "agent memory infrastructure"
- Most defensible framing: memory is the hardest unsolved problem in agentic workflows. Every orchestration framework has a memory problem. None have solved it. Unimatrix could be the solution they integrate.
- Integration as growth channel: any framework needing agent memory plugs in Unimatrix. PostgreSQL for agents.

**Opportunities beyond Framings A and B**:
- **Integration as primary growth**: becomes a library/service that orchestration frameworks integrate
- **Memory-as-a-Service**: hosted Unimatrix any agentic workflow connects to. SaaS model that does not exist
- **The memory API**: standardize the agent-memory interface. Define it, become the standard.

**Risks**: Too abstract ("agent memory" is evocative but vague), undersells the intelligence (PPR, phase-conditioned ranking, detection rules are far more than "memory"), market timing (may be premature), loss of boundary clarity.

**Framing C is the visionary choice.** Largest addressable market if agentic ecosystem grows as anticipated. Most speculative, hardest to execute.

---

#### Comparative Analysis of Framings

| Dimension | A: Knowledge-Aware Runtime | B: Intelligence Platform | C: The Agent's Memory |
|---|---|---|---|
| **Core identity** | Knowledge engine + session hosting | Intelligence platform (knowledge + sessions + learning) | Memory infrastructure for agents |
| **Session hosting role** | Feature of the knowledge engine | Peer subsystem alongside knowledge | Preferred deployment of memory system |
| **Product category** | Developer knowledge tool | Agentic intelligence platform | Agent memory infrastructure |
| **Target user** | Developer who uses Claude Code | Team building agentic systems | Anyone building or using agent workflows |
| **Competitive position** | Unique in niche | Novel platform category | Infrastructure layer |
| **Identity risk** | Low | Medium | High |
| **Market risk** | Low | Medium | High (timing) |
| **Scope creep risk** | Medium | High | Medium ("we are memory, not the agent") |
| **Moat depth** | Moderate (observation fidelity) | Deep (intelligence flywheel) | Deepest (infrastructure position) |
| **Walk-back cost** | Low | Medium | High |
| **Implementation delta from today** | Small — add `unimatrix run` | Medium — reframe product, add session analytics | Large — reframe product, build integrations |

#### What Is Explicitly NOT Changing (All Framings)

Regardless of framing:
- Knowledge graph, confidence system, hash-chain integrity, audit log unchanged
- MCP server surface continues serving all clients
- Hook-based observation continues for interactive sessions
- All architectural principles maintained
- Unimatrix does not decide what work to do (anti-orchestration boundary)
- Unimatrix does not manage agent lifecycles beyond single sessions
- Intelligence pipeline internals (PPR, detection rules, phase-conditioned ranking) unchanged — better data in, same logic

#### The ass-063 Tension

ASS-063 established that the "workflow-literate knowledge engine" framing is the stable equilibrium and warned that crossing into execution control slides toward orchestration.

Session hosting creates tension: is hosting a session "execution control"?

**No, if the boundary is maintained.** The ass-063 litmus test: "If a feature requires Unimatrix to *initiate an action* (spawn an agent, call an API, enforce a gate), it has crossed the line." Session hosting does initiate an action — it spawns a Claude session. But at the user's explicit request (`unimatrix run --prompt "..."`), not autonomously.

The distinction: `unimatrix run` is a tool, not an autonomous agent. It is `cargo build`, not Jenkins. The user invokes it; it runs; it reports results. The moment it starts autonomously deciding what to run next, or spawning follow-up sessions without user initiation, it crosses the ass-063 boundary.

**Recommendation**: Present all three framings for decision. Framing A is conservative with lowest risk. Framing B is ambitious with the best strategic narrative. Framing C is visionary with the deepest moat but the most uncertainty. The recommendation is **Framing B** (Intelligence Platform) as the target vision, implemented via **Framing A's approach** (additive, incremental, reversible). Build `unimatrix run` as an additive feature. If session hosting validates — if sessions observably improve from knowledge conditioning, if the learning pipeline benefits from hosted-session data quality — then evolve the vision toward Framing B. If not, Framing A stands on its own.

---

### Q6: What is the impact on vnc-024 and the remote observation roadmap?

**Answer**: vnc-024 remains valuable as a fallback and transitional architecture but is no longer the strategic solution for remote observation. The sequencing: ship vnc-024 at reduced scope (wire contract documentation + content negotiation, 1-2 weeks), then build `unimatrix run` as the primary remote observation mechanism.

**Evidence**:

**Key reframing**: The natural architecture is not "hooks for local, session hosting for remote" — it is **"hooks for interactive, session hosting for programmatic."** The distinction is about who creates the session, not where it runs.

- **Interactive sessions** (human at keyboard): hooks observe. Works locally (UDS) and remotely (HTTP via vnc-024).
- **Programmatic sessions** (automated tasks, CI/CD, scheduled work): session host creates and observes. Works locally and remotely.

**vnc-024 scope reduction**:

| vnc-024 Goal | Still Needed? |
|---|---|
| Curl-based hook config validation | Yes, lower priority (debugging/fallback) |
| Wire contract documentation | Yes (session host HTTP transport uses same `/observe` endpoint) |
| Copy-paste hook examples | Reduced priority |
| Installation mechanism | Reduced priority (`unimatrix run` subsumes "how do I connect") |
| Timeout validation | Still useful |

**vnc-024 critical finding absorbed**: Raw curl returns JSON envelopes but local hook binary performs response transformation. Session hosting eliminates this — host formats its own injection content.

**Revised remote observation roadmap**:

| Step | What | Effort | Status |
|---|---|---|---|
| vnc-022 | `/observe` endpoint | Shipped | Done |
| vnc-024 (reduced) | Wire contract docs + content negotiation | 1-2 weeks | Proceed |
| **NEW** | `unimatrix run` — observation parity | 2-3 weeks | Build after vnc-024 |
| **NEW** | `unimatrix run` — knowledge-aware runtime | 3-5 weeks additional | Incremental |
| ASS-014 Phase 3 | WASM thin client | Deferred | Interactive remote only |

**Can vnc-024 proceed in parallel?** Yes. Wire contract docs and content negotiation are additive.

**Recommendation**: Proceed with vnc-024 at reduced scope. Begin `unimatrix run` immediately after. WASM thin client deferred until session hosting validates.

---

## Unanswered Questions

1. **Python SDK SessionStart/SessionEnd gap**: Python SDK lacks these as in-process callbacks (TypeScript only). Non-blocking — host synthesizes at `query()` boundaries — but needs validation on whether gap closes in future releases.

2. **Agent SDK billing model under June 2026 changes**: Cost implications of `unimatrix run` sessions need analysis — additive cost or replacement of interactive usage?

3. **SDK hook payload completeness**: Exact payload structure differences between SDK hooks and CLI hooks need PoC validation. Some fields (e.g., `mcp_context` for Gemini normalization) may be absent.

4. **`--input-format stream-json` protocol specification**: Undocumented bidirectional streaming protocol. Relevant if CLI path preferred over SDK.

5. **Claude Agent SDK compaction callbacks**: Whether host can intercept and customize compaction behavior unclear. Open feature request (anthropics/claude-agent-sdk-python#772).

6. **Gemini CLI session resumption roadmap**: Whether Google plans `--resume` for headless mode is unknown.

7. **Codex app-server stability contract**: JSON-RPC protocol documented but internal. Public API stabilization unknown.

8. **Multi-user credential handling**: Per-user credential injection for shared-service deployments needs architectural design.

---

## Out-of-Scope Discoveries

1. **`claude -p --output-format stream-json` as lightweight observation**: NDJSON with session_id, tool I/O, timing. Simpler-than-SDK path for read-only monitoring (no PreCompact, SubagentStart). Relevant if minimal-dependency path desired.

2. **Session forking for exploration**: SDK supports resume and fork. Unimatrix could fork sessions to explore alternatives. Deep future, likely crosses orchestration boundary.

3. **Token economics signal**: SDK and stream-json expose per-turn token usage. New signal for retrospective pipeline (context bloat detection, injection efficiency).

4. **Multi-model session hosting**: Host controls model selection — could route phases to different models. Technically feasible, explicitly outside orchestration boundary.

5. **Claude Code Workflow tool** (TypeScript SDK v0.3.149+): Orchestrates many subagents from a script. Relevant to multi-agent `unimatrix run`.

6. **Codex Cloud** (`codex cloud exec`): OpenAI's managed execution environment. Comparison point.

7. **Claude Code Agent View** (`claude agents`): Built-in session monitoring/dispatching UI. Relevant to understanding replication vs. complementing.

8. **Claude Code Remote Control** (`claude remote-control`): Server controllable from claude.ai. Shows Anthropic's vision for hosted agents.

9. **Gemini CLI A2A protocol**: Unimatrix as remote subagent to Gemini sessions. Different architectural direction.

---

## Recommendations Summary

- **Q1 (Programmatic Interfaces)**: Use the Claude Agent SDK as the primary session hosting interface. Only client with SDK maturity, programmatic hook callbacks, session management, and observation fidelity needed. Codex/Gemini hostable at reduced fidelity via CLI wrapping. Full model-agnosticism not achievable today.

- **Q2 (Observation Fidelity)**: Session hosting achieves full parity and strictly superior fidelity for session identity, phase attribution, transcript access, and proactive injection. Six new signals. MCP stream covers ~40-50% of the learning pipeline; session hosting delivers 100% from a single mechanism.

- **Q3 (Minimum Viable Session Host)**: Build `unimatrix run` as a Python CLI using Claude Agent SDK. 2-3 weeks for observation parity, 5-8 weeks for knowledge-aware runtime. No credential management by Unimatrix. Complementary to (not replacing) ASS-014 WASM client.

- **Q4 (Product Surface)**: Additive to MCP server, no credential management, works locally and remotely, does not compete with Claude Code UX, initial Claude coupling mitigated by SessionHost abstraction. Seven new capabilities: knowledge-conditioned sessions, proactive compaction defense, cost attribution, reproducible patterns, cross-session continuity, observation-driven steering, auditable sessions.

- **Q5 (Vision Evolution)**: Three framings — (A) Knowledge-Aware Runtime (conservative), (B) Intelligence Platform (ambitious), (C) The Agent's Memory (visionary). Recommend **Framing B as target vision, implemented via Framing A's approach**: build incrementally, validate, then evolve. All framings preserve anti-orchestration boundary. ass-063 tension resolved: `unimatrix run` is a user-invoked tool, not an autonomous agent.

- **Q6 (vnc-024 Impact)**: Reduce vnc-024 to wire contract docs + content negotiation (1-2 weeks). Proceed in parallel. `unimatrix run` becomes primary remote observation mechanism. WASM client deferred. Natural split: hooks-for-interactive, sessions-for-programmatic.
