# LLM SDK Integration Patterns: Embedding AI Inside Infrastructure Systems

**Research Document — ASS-008**
**Date:** 2026-02-24
**Scope:** How to safely embed an LLM (via SDK) inside a trusted infrastructure system while maintaining trustworthiness

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [The Claude Agent SDK Architecture](#2-the-claude-agent-sdk-architecture)
3. [Claude Code as an Embeddable Component](#3-claude-code-as-an-embeddable-component)
4. [Existing Systems That Embed LLMs as Components](#4-existing-systems-that-embed-llms-as-components)
5. [The Deterministic Shell, Generative Core Pattern](#5-the-deterministic-shell-generative-core-pattern)
6. [Tool Execution Sandboxing](#6-tool-execution-sandboxing)
7. [API-Based vs Local LLM Integration](#7-api-based-vs-local-llm-integration)
8. [The Unimatrix as GitHub App Pattern](#8-the-unimatrix-as-github-app-pattern)
9. [Container-as-Trust-Boundary](#9-container-as-trust-boundary)
10. [Identity Delegation Chains](#10-identity-delegation-chains)
11. [Real-World Architectures](#11-real-world-architectures)
12. [Feasibility Assessment for Unimatrix](#12-feasibility-assessment-for-unimatrix)
13. [Recommended Integration Pattern](#13-recommended-integration-pattern)
14. [Bibliography](#14-bibliography)

---

## 1. Executive Summary

This document analyzes integration patterns for embedding LLM capabilities inside Unimatrix, a Rust-based knowledge engine for multi-agent development orchestration. The central challenge is preserving system trustworthiness while leveraging generative AI for reasoning, planning, and language understanding.

**Key findings:**

- The **Claude Agent SDK** (Python/TypeScript) provides comprehensive hooks, permission controls, subagent isolation, and tool restriction mechanisms that can serve as the orchestration layer
- The **Deterministic Shell / Generative Core** pattern (formalized by Google DeepMind's CaMeL and independently described as "Deterministic Core, Agentic Shell") is the dominant safety architecture for embedding LLMs in trusted systems
- **WASM-based sandboxing** (Microsoft's Wassette) is emerging as the standard for AI tool execution isolation, with Rust-native support via Wasmtime
- **API-based LLM integration** is safer than in-process local models for Unimatrix's use case because it maintains a clear trust boundary between the deterministic system and the generative component
- The **GitHub App model** provides a well-understood trust chain that maps cleanly to Unimatrix's needs: GitHub verifies Unimatrix, Unimatrix mediates agent access
- Rust's **type system** (newtypes, phantom types, capability tokens) enables compile-time enforcement of security boundaries that no other mainstream language can match

**Recommended architecture:** Unimatrix as a **Deterministic Shell** that calls the Claude API for reasoning, with all tool execution mediated through Rust-enforced capability boundaries. Unimatrix never trusts LLM output for security decisions. The LLM proposes; Unimatrix disposes.

---

## 2. The Claude Agent SDK Architecture

### 2.1 Overview

The Claude Agent SDK (formerly "Claude Code SDK") provides the same tools, agent loop, and context management that power Claude Code, available as programmable libraries in Python and TypeScript. The SDK was announced May 2025 alongside Claude Opus 4 and Sonnet 4.

```
+------------------------------------------------------------------+
|                    Claude Agent SDK                               |
|                                                                   |
|  +-------------------+  +-------------------+  +---------------+  |
|  |   Agent Loop      |  |   Tool System     |  |  Hooks System |  |
|  |                   |  |                   |  |               |  |
|  | - Prompt routing  |  | - Built-in tools  |  | - PreToolUse  |  |
|  | - Context mgmt    |  |   (Read, Write,   |  | - PostToolUse |  |
|  | - Session state   |  |    Bash, Grep...) |  | - Stop        |  |
|  | - Auto caching    |  | - Custom tools    |  | - Subagent*   |  |
|  |                   |  |   (in-process     |  | - Session*    |  |
|  |                   |  |    MCP servers)   |  | - Permission  |  |
|  |                   |  | - External MCP    |  | - Notification|  |
|  +-------------------+  +-------------------+  +---------------+  |
|                                                                   |
|  +-------------------+  +-------------------+  +---------------+  |
|  |   Subagents       |  |   Permissions     |  |  MCP Layer    |  |
|  |                   |  |                   |  |               |  |
|  | - Isolated context|  | - Deny rules      |  | - stdio       |  |
|  | - Restricted tools|  | - Allow rules     |  | - in-process  |  |
|  | - Parallel exec   |  | - Ask rules       |  | - SSE         |  |
|  | - No nesting      |  | - canUseTool cb   |  |               |  |
|  +-------------------+  +-------------------+  +---------------+  |
+------------------------------------------------------------------+
```

### 2.2 Hook System — Control Points

The hooks system is the primary mechanism for deterministic control over LLM behavior. Hooks are Python/TypeScript functions invoked at specific points in the agent loop.

**Processing order for tool execution:**
```
PreToolUse Hook --> Deny Rules --> Allow Rules --> Ask Rules
     |                                               |
     v                                               v
Permission Mode Check --> canUseTool Callback --> Tool Executes
                                                      |
                                                      v
                                              PostToolUse Hook
```

**Available hook events:**

| Hook Event | Trigger | Control Level |
|---|---|---|
| `PreToolUse` | Before tool execution | Can deny, allow, modify input |
| `PostToolUse` | After tool execution | Can log, audit, transform output |
| `PostToolUseFailure` | Tool execution failed | Error handling (TS only) |
| `UserPromptSubmit` | Prompt submitted | Can inject context |
| `Stop` | Agent execution stops | Session state save |
| `SubagentStart` | Subagent spawns | Track parallel tasks (TS only) |
| `SubagentStop` | Subagent completes | Aggregate results |
| `PreCompact` | Context compaction | Archive transcripts |
| `PermissionRequest` | Permission dialog | Custom auth logic (TS only) |
| `SessionStart` | Session begins | Initialize telemetry (TS only) |
| `SessionEnd` | Session ends | Cleanup (TS only) |
| `Notification` | Status update | External notifications (TS only) |

**PreToolUse decision output:**
```python
return {
    "hookSpecificOutput": {
        "hookEventName": "PreToolUse",
        "permissionDecision": "deny",          # "allow" | "deny" | "ask"
        "permissionDecisionReason": "Blocked: dangerous command",
        "updatedInput": { ... }                # Optional: modify tool input
    },
    "systemMessage": "...",                    # Inject context for LLM
    "continue": True                           # Whether agent continues
}
```

**Key security property:** If ANY hook returns `deny`, the operation is blocked. Multiple hooks returning `allow` cannot override a single `deny`. This provides a monotonic security guarantee.

### 2.3 Subagent Architecture

Subagents are isolated agent instances spawned via the `Task` tool.

**Isolation properties:**
- **Separate context windows** — subagent state does not pollute the main conversation
- **Restricted tool sets** — e.g., read-only agent gets only `[Read, Grep, Glob]`
- **No recursive nesting** — subagents cannot spawn their own subagents
- **Parallel execution** — multiple subagents can run concurrently
- **Model selection** — each subagent can use a different model (sonnet, opus, haiku)

```
+---------------------+
|   Main Agent        |
|   (all tools)       |
|                     |
|   spawns via Task:  |
|                     |
|   +---------+  +---------+  +---------+
|   | code-   |  | test-   |  | sec-    |
|   | reviewer|  | runner  |  | scanner |
|   | R,Gr,Gl |  | Bash,R  |  | R,Grep  |
|   | sonnet  |  | sonnet  |  | opus    |
|   +---------+  +---------+  +---------+
|       |             |            |
|   (results)    (results)   (results)
+---------------------+
```

### 2.4 Custom Tools and In-Process MCP Servers

Custom tools are implemented as **in-process MCP servers** running directly within the application process:
- No subprocess management overhead
- No IPC latency for tool calls
- Single-process deployment
- Tools defined with `@tool` decorator (Python) or `tool()` helper (TypeScript)

This means Unimatrix could expose its knowledge base operations (context_search, context_store, etc.) as custom tools that an embedded agent can invoke directly, with the tools running in Unimatrix's own process space under Unimatrix's control.

### 2.5 Rust SDK Availability

Several community Rust SDKs exist:
- **anthropic-agent-sdk** (crates.io): mirrors TypeScript SDK patterns, async/await, strong typing
- **claude-agent-sdk** (crates.io): comprehensive Rust bindings with zero-cost abstractions
- **Claudius** (crates.io): direct API access + agent framework
- **cc-sdk** (crates.io): has `ClaudeAgentOptions` type

No official Anthropic Rust SDK exists. The community crates wrap the Claude CLI as a subprocess or call the API directly. For production use, direct API integration (via `reqwest` + `serde`) may be more reliable than depending on CLI subprocess management.

### 2.6 Security Assessment

**Strengths:**
- Hook system provides deterministic interception points
- Deny-takes-precedence permission model
- Tool restriction per subagent
- In-process MCP avoids subprocess escape vectors

**Limitations:**
- Python/TypeScript only (official SDK); Rust SDKs are community-maintained
- Subagent subprocess inherits environment (known CLAUDECODE=1 bug)
- Permission system relies on configuration correctness
- No formal verification of hook chain completeness

---

## 3. Claude Code as an Embeddable Component

### 3.1 Programmatic Access Modes

Claude Code offers three integration surfaces:

**CLI Mode (headless):**
```bash
claude -p "Your prompt" --output-format json
```
- Non-interactive, suitable for scripts and CI/CD
- All CLI options work: `--allowedTools`, `--model`, `--systemPrompt`
- Structured JSON output via `--output-format`

**Python SDK:**
```python
from claude_agent_sdk import query, ClaudeAgentOptions
async for message in query(
    prompt="...",
    options=ClaudeAgentOptions(
        allowed_tools=["Read", "Grep"],
        hooks={"PreToolUse": [...]},
        agents={...}
    )
):
    process(message)
```

**TypeScript SDK:**
```typescript
import { query } from "@anthropic-ai/claude-agent-sdk";
for await (const message of query({
    prompt: "...",
    options: { allowedTools: [...], hooks: {...} }
})) {
    handle(message);
}
```

### 3.2 Subagent Spawning and Identity

Claude Code spawns subagents via the `Task` tool, with each subprocess being a fresh `claude` CLI process. Key observations:

- Subagents use their own isolated context windows
- Only send relevant information back to the orchestrator
- Cannot spawn their own subagents (prevents infinite nesting)
- Each subagent's transcript persists independently
- Filesystem-based agents loaded from `.claude/agents/` directories
- Programmatic agents from the `agents` parameter take precedence

**Identity model:** There is no formal agent identity system. Subagents are distinguished by:
- `agent_id`: unique identifier for the subagent instance
- `agent_type`: type/role of the subagent
- `parent_tool_use_id`: correlation back to parent agent

### 3.3 Can Unimatrix Wrap Claude Code?

**Yes, but with caveats:**

```
+----------------------------------------------------------+
|  Unimatrix Process (Rust)                                |
|                                                          |
|  +------------------+     +---------------------------+  |
|  | Knowledge Engine |     | Agent SDK Wrapper         |  |
|  | (redb, hnsw,     |<--->| (Python subprocess or     |  |
|  |  embeddings)     |     |  direct API calls)        |  |
|  +------------------+     +---------------------------+  |
|          |                         |                     |
|          v                         v                     |
|  +------------------+     +---------------------------+  |
|  | MCP Server       |     | Hook System               |  |
|  | (existing vnc-*) |     | (PreToolUse/PostToolUse)  |  |
|  +------------------+     +---------------------------+  |
+----------------------------------------------------------+
```

**Option A: Wrap CLI as subprocess**
- Unimatrix spawns `claude -p "..." --output-format json`
- Parse JSON output, extract tool calls, mediate execution
- Pro: Uses official tooling; Con: Subprocess management, environment inheritance issues

**Option B: Direct API integration**
- Unimatrix calls Anthropic Messages API directly via `reqwest`
- Implements its own tool-calling loop
- Pro: Full control, Rust-native; Con: Must reimplement agent loop logic

**Option C: Sidecar Python process**
- Long-running Python process with Agent SDK, communicating via IPC
- Unimatrix sends work items, receives structured responses
- Pro: Full SDK features; Con: Multi-language complexity

**Recommendation for Unimatrix: Option B (Direct API).** The agent loop is conceptually simple (send message, receive tool_use blocks, execute tools, send results back) and implementing it in Rust gives maximum control. The hook system can be replicated as Rust trait implementations.

---

## 4. Existing Systems That Embed LLMs as Components

### 4.1 GitHub Copilot

**Architecture:**
```
+------------------+     +-------------------+     +------------+
| IDE Extension    |---->| GitHub Copilot    |---->| LLM        |
| (context gather) |     | Proxy Service     |     | (GPT-4.1,  |
|                  |<----| (prompt engineer) |<----| Claude,    |
|                  |     |                   |     | Gemini)    |
+------------------+     +-------------------+     +------------+
```

- IDE collects context (current file, neighboring files, repository URLs)
- Proxy service constructs prompts and routes to appropriate LLM
- Multi-model architecture: developers choose from GPT-4o, o1, o3-mini, Claude 3.5 Sonnet, Gemini 2.0 Flash
- Agent mode runs agents in **isolated environments** respecting repository access scopes
- Cloud-based: continuous communication with GitHub Copilot servers

**Security model:**
- Code suggestions generated remotely; LLM never has local filesystem access
- No code retention (for business plans)
- Repository-scoped access in agent mode
- Telemetry concerns exist (opaque data pipeline)

### 4.2 Cursor IDE

**Architecture:**
```
+-------------------+     +-----------------------+     +----------+
| VS Code Fork      |     | Cursor AWS Backend    |     | LLMs     |
| + Code Indexing    |---->| - Prompt engineering   |---->| OpenAI   |
| + AST Analysis     |     | - Context orchestration|     | Anthropic|
| + Embeddings       |<----| - Transformation engine|<----| Google   |
+-------------------+     +-----------------------+     +----------+
```

- AI-native IDE rebuilt from VS Code fork
- Local code indexing: embeddings, AST graphs, cross-references
- AI requests routed through Cursor's AWS infrastructure for prompt engineering
- Multi-model: GPT-4, Claude 3.5 Sonnet, Gemini, custom models

**Security model:**
- Zero-data-retention agreements with OpenAI and Anthropic
- Privacy Mode prevents code storage
- SOC 2 Type II certification
- SAML 2.0 SSO for enterprise
- Code transformation validated via static analysis before application
- `.cursorrules` files embed security requirements into AI's decision-making

**Key insight:** Cursor treats the LLM as a **suggestion engine**. The transformation engine validates proposed edits against static analysis before applying them. This is a Deterministic Shell pattern in practice.

### 4.3 Notion AI / Slack AI

**Architecture:**
```
+------------------+     +-------------------+     +------------+
| SaaS Application |---->| Internal LLM      |---->| OpenAI /   |
| (user data layer)|     | Orchestration     |     | Anthropic  |
|                  |<----| (permission-aware) |<----| APIs       |
+------------------+     +-------------------+     +------------+
```

- LLMs used both on Notion-hosted infrastructure and via third-party APIs
- **Permission-aware:** AI responses only reflect data the requesting user can already access
- Enterprise: zero data retention with LLM providers
- Non-enterprise: 30-day data retention before deletion
- Annual subprocessor security reviews
- Technology security questionnaires for vendors

**Key insight:** The SaaS product's existing permission model becomes the LLM's permission boundary. The LLM cannot escalate beyond the user's existing access.

### 4.4 AWS Bedrock AgentCore

**Architecture:**
```
+-----------------------------------------------------------------+
|  AgentCore Runtime                                              |
|                                                                 |
|  +------------------+  +------------------+  +---------------+  |
|  | Session Isolation |  | Code Interpreter |  | AgentCore     |  |
|  | (per-invocation  |  | (sandbox per     |  | Gateway       |  |
|  |  exec context)   |  |  session, Python  |  | (policy       |  |
|  |  up to 8 hours)  |  |  JS, TS)         |  |  enforcement) |  |
|  +------------------+  +------------------+  +---------------+  |
|                                                                 |
|  +------------------+  +------------------+  +---------------+  |
|  | AgentCore        |  | VPC / PrivateLink|  | Observability |  |
|  | Identity         |  | (network         |  | (step-by-step |  |
|  | (SigV4, OAuth,   |  |  isolation)      |  |  visualization|  |
|  |  API keys)       |  |                  |  |  + metadata)  |  |
|  +------------------+  +------------------+  +---------------+  |
+-----------------------------------------------------------------+
```

- **Session isolation:** each invocation gets its own execution context
- **Code sandbox:** dedicated, isolated environment per session, complete isolation from other workloads
- **Deterministic policy enforcement:** policies defined in natural language but executed outside the LLM reasoning loop via the Gateway
- **Cedar policy language:** natural language rules compiled to deterministic enforcement
- Resource constraints: memory/CPU limits per session

**Key insight:** Policy enforcement is **deterministic, not probabilistic** -- it operates outside the LLM's reasoning loop. No matter how cleverly an agent tries to reason around a constraint, the gateway enforces it at runtime before the action executes. This is the clearest production implementation of the Deterministic Shell pattern at enterprise scale.

### 4.5 Google Vertex AI / GKE Agent Sandbox

**Architecture:**
```
+------------------------------------------------------------------+
|  GKE Cluster                                                     |
|                                                                  |
|  +------------------------------------------------------------+ |
|  | Agent Sandbox (gVisor)                                      | |
|  | - User-space kernel intercepts ALL syscalls                 | |
|  | - Sentry acts as "fake kernel"                              | |
|  | - Pre-warmed pools for sub-second latency                   | |
|  | - Pod Snapshots for checkpoint/restore                      | |
|  +------------------------------------------------------------+ |
|                                                                  |
|  +------------------+  +------------------+  +----------------+  |
|  | Workload Identity|  | Network Policy   |  | VPC-SC         |  |
|  | Federation       |  | (default-deny,   |  | (data          |  |
|  | (ephemeral,      |  |  explicit allow   |  |  exfiltration  |  |
|  |  least-priv IAM) |  |  for DNS/API)    |  |  prevention)   |  |
|  +------------------+  +------------------+  +----------------+  |
+------------------------------------------------------------------+
```

- **gVisor:** open-source application kernel providing VM-level isolation with container-level footprint
- **Pre-warmed pools:** sub-second latency for fully isolated sandboxes (90% improvement over cold starts)
- **Pod Snapshots:** checkpoint and restore running pods for instant sandbox provisioning
- **Workload Identity Federation:** ephemeral, least-privilege IAM per agent
- **Default-deny networking:** explicit allow-lists for DNS, metadata, and API endpoints

---

## 5. The Deterministic Shell, Generative Core Pattern

### 5.1 Core Concept

This is the single most important pattern for embedding LLMs in trusted systems. The system is split into two layers:

```
+------------------------------------------------------------------+
|                                                                  |
|  DETERMINISTIC SHELL (trusted, verified, testable)               |
|  +---------------------------------------------------------+    |
|  | - Authentication & authorization                         |    |
|  | - Input validation & sanitization                        |    |
|  | - Permission checking                                    |    |
|  | - State machine transitions                              |    |
|  | - Output validation & filtering                          |    |
|  | - Side effect execution                                  |    |
|  | - Audit logging                                          |    |
|  | - Tool execution mediation                               |    |
|  +---------------------------------------------------------+    |
|       ^              |              ^              |             |
|       | user input   | validated    | validated    | validated   |
|       |              | request      | response     | output      |
|       |              v              |              v             |
|  +---------------------------------------------------------+    |
|  | GENERATIVE CORE (untrusted, non-deterministic)           |    |
|  |                                                          |    |
|  | - Reasoning & planning                                   |    |
|  | - Language understanding                                 |    |
|  | - Code generation                                        |    |
|  | - Content synthesis                                      |    |
|  | - Pattern recognition                                    |    |
|  +---------------------------------------------------------+    |
|                                                                  |
+------------------------------------------------------------------+

INVARIANT: The shell NEVER trusts the core's output for
           security decisions. The core PROPOSES; the shell DISPOSES.
```

### 5.2 Dave Morissette's "Deterministic Core, Agentic Shell" (2026)

Published February 2026, this formulation inverts the naming but preserves the architecture:

- **Deterministic Core:** XState v5 state machine managing workflow logic with explicit states, transitions, guards, and pure-function actions
- **Agentic Shell:** LLM agent handling natural language interaction

Communication through a narrow bridge of tools:
- `get_current_state`: Agent queries machine for available actions
- `take_action`: Agent requests transitions; machine validates and executes
- **Dynamic tool swapping:** available tools change based on machine state

> "The agent is creative about _how_ to have the conversation; the machine is authoritative about _what happens next_."

**Security properties:**
1. Guards reject invalid transitions regardless of agent requests
2. Tools become available/unavailable based on workflow state
3. Non-determinism isolated to conversational layer
4. Core logic remains testable and verifiable

### 5.3 Google DeepMind's CaMeL (2025)

**Paper:** "Defeating Prompt Injections by Design" (arXiv:2503.18813)
**Authors:** Google, DeepMind, ETH Zurich

CaMeL (CApabilities for MachinE Learning) is the most rigorous formal instantiation of the Deterministic Shell pattern.

```
+------------------------------------------------------------------+
|                     CaMeL Architecture                           |
|                                                                  |
|  User Query (trusted)                                            |
|       |                                                          |
|       v                                                          |
|  +------------------------+                                      |
|  | P-LLM (Privileged)     |  Sees ONLY user query               |
|  | - Plans execution       |  Generates restricted Python code   |
|  | - No untrusted data     |  Has tool-calling authority         |
|  +------------------------+                                      |
|       |                                                          |
|       | Python code with capability tags                         |
|       v                                                          |
|  +------------------------+                                      |
|  | Custom Interpreter      |  Tracks data provenance             |
|  | - AST parsing           |  Enforces capability policies       |
|  | - Capability tracking   |  Gates side effects                 |
|  | - Data flow control     |                                     |
|  +------------------------+                                      |
|       |                                                          |
|       | Untrusted data processing requests                       |
|       v                                                          |
|  +------------------------+                                      |
|  | Q-LLM (Quarantined)    |  Processes untrusted content         |
|  | - No tool access        |  Cannot affect control flow         |
|  | - Suggestions only      |  Output tagged as untrusted         |
|  +------------------------+                                      |
|                                                                  |
+------------------------------------------------------------------+
```

**Key mechanisms:**
- **Capability tags:** Every variable carries metadata about provenance and permitted uses
- **Information flow control:** If variable `address` derives from untrusted `email`, it inherits untrusted designation
- **Policy gating:** `send_email()` only permitted if recipient is trusted or user-approved
- **Control flow integrity:** Injected instructions in data can never reach P-LLM or affect program flow

**Performance:** Solves 77% of tasks with provable security (vs 84% for undefended system). Requires ~2.7-2.8x more tokens.

**Limitations:**
- Users must codify security policies
- Approval fatigue from capability prompts
- Side-channel attacks remain possible
- Not a complete solution

### 5.4 Martin Fowler's GenAI Patterns (2025)

The guardrails pattern from Fowler's catalogue maps to the Deterministic Shell:

```
Input Guardrails --> Query Rewriting --> Retrieval --> LLM --> Output Guardrails
   (regex,            (LLM)             (hybrid)    (core)    (regex,
    embedding,                                                 embedding,
    LLM-based)                                                 LLM-based)
```

Three implementation approaches for guardrails:
1. **LLM-based rules:** Another model judges safety
2. **Embedding-based:** Semantic similarity to known-bad patterns
3. **Regex patterns:** Deterministic pattern matching (fastest, most reliable)

### 5.5 Implementing in Rust

Rust's type system enables compile-time enforcement of the Deterministic Shell pattern:

```rust
// Newtype wrappers prevent mixing trusted/untrusted data
struct TrustedInput(String);   // From validated user input
struct UntrustedData(String);  // From LLM or external sources

// Phantom types for capability tracking
struct Capability<Level> {
    data: String,
    _level: PhantomData<Level>,
}
struct Trusted;
struct Untrusted;

// Only trusted capabilities can trigger side effects
fn execute_tool(cap: Capability<Trusted>, tool: &Tool) -> Result<()> { ... }

// Untrusted data must be validated before promotion
fn validate_and_promote(
    data: Capability<Untrusted>,
    policy: &Policy
) -> Result<Capability<Trusted>> { ... }

// The compiler PREVENTS this:
// execute_tool(untrusted_cap, &tool);  // ERROR: type mismatch

// State machine with typestate pattern
struct AgentSession<State> {
    context: SessionContext,
    _state: PhantomData<State>,
}
struct Planning;
struct Executing;
struct Reviewing;

impl AgentSession<Planning> {
    fn approve_plan(self, plan: &Plan) -> AgentSession<Executing> { ... }
}
impl AgentSession<Executing> {
    fn complete(self, result: &ToolResult) -> AgentSession<Reviewing> { ... }
}
// Cannot call execute methods from Planning state -- compile error
```

**Key Rust advantages:**
- `Send + Sync` bounds ensure thread-safe capability passing
- `#![forbid(unsafe_code)]` prevents capability bypass via unsafe
- Ownership prevents capability duplication (move semantics)
- Enums + exhaustive matching ensure all states are handled
- Zero-cost abstractions mean no runtime overhead for type-level safety

---

## 6. Tool Execution Sandboxing

### 6.1 Taxonomy of Sandbox Approaches

```
+------------------------------------------------------------------+
|  Sandbox Approaches for AI Tool Execution                        |
|                                                                  |
|  Strongest Isolation                                             |
|  +--------------------------+                                    |
|  | VM-Level                 | gVisor, Firecracker, Kata         |
|  | (application kernel)     | ~100ms startup, full syscall      |
|  |                          | interception in userspace          |
|  +--------------------------+                                    |
|                                                                  |
|  +--------------------------+                                    |
|  | WASM-Level               | Wasmtime, Wasmer, Wassette        |
|  | (capability-based)       | <1ms startup, deny-by-default     |
|  |                          | portable, Rust-native              |
|  +--------------------------+                                    |
|                                                                  |
|  +--------------------------+                                    |
|  | Container-Level          | Docker, nsjail, bubblewrap         |
|  | (namespace + seccomp)    | ~50ms startup, configurable        |
|  |                          | syscall filtering                  |
|  +--------------------------+                                    |
|                                                                  |
|  +--------------------------+                                    |
|  | Process-Level            | seccomp-bpf, Landlock, AppArmor   |
|  | (kernel security)        | ~0 overhead, unprivileged          |
|  |                          | stackable policies                 |
|  +--------------------------+                                    |
|                                                                  |
|  +--------------------------+                                    |
|  | Library-Level            | cap-std, pledge(OpenBSD)           |
|  | (capability filesystem)  | compile-time, voluntary            |
|  |                          | for trusted code only              |
|  +--------------------------+                                    |
|                                                                  |
|  Weakest Isolation                                               |
+------------------------------------------------------------------+
```

### 6.2 WASM-Based Sandboxing (Wasmtime / Wassette)

**Microsoft's Wassette** (August 2025) is the reference implementation for WASM-sandboxed AI tools:

- Written in Rust, zero runtime dependencies
- Bridges WebAssembly Components with MCP
- **Deny-by-default:** components start with zero permissions
- WASI capability-based security: explicit grants for filesystem, network, environment
- Cryptographic signing via Notation and Cosign
- OCI registry integration for component distribution

**Wasmtime security properties:**
- Memory isolation: each module gets its own linear memory
- No direct access to host filesystem/network
- Capability-based resource grants (WASI)
- Resource metering (CPU cycles, memory allocation)
- Deterministic execution within a module
- JIT compiler bugs are the primary escape vector (keep runtime updated)

**Relevance to Unimatrix:**
- Unimatrix could compile tool implementations to WASM
- Each tool runs in its own sandbox with explicit capability grants
- Wasmtime has first-class Rust support via `wasmtime` crate
- Sub-millisecond startup for tool invocations
- Tools cannot access Unimatrix's internal state unless explicitly granted

```rust
// Example: Sandboxed tool execution with Wasmtime
use wasmtime::*;
use wasmtime_wasi::*;

fn execute_tool_sandboxed(
    wasm_bytes: &[u8],
    allowed_dirs: &[PathBuf],
    input: &ToolInput,
) -> Result<ToolOutput> {
    let engine = Engine::default();
    let module = Module::new(&engine, wasm_bytes)?;

    let mut wasi = WasiCtxBuilder::new();
    for dir in allowed_dirs {
        wasi = wasi.preopened_dir(dir, ".")?;
    }
    // No network, no env vars, no stdin unless explicitly granted

    let mut store = Store::new(&engine, wasi.build());
    let instance = Instance::new(&mut store, &module, &[])?;

    // Call tool function, get result
    let func = instance.get_typed_func::<(i32, i32), i32>(&mut store, "execute")?;
    // ...
}
```

### 6.3 Linux Kernel Sandboxing

**Landlock** (Linux Security Module):
- Unprivileged sandboxing: no root needed
- Stackable: each enforcement adds constraints, never removes them
- Rust crate: `landlock` provides safe abstractions
- Restrict filesystem access, network TCP bind/connect
- Ideal for self-sandboxing Unimatrix's own process

```rust
// Example: Landlock self-sandboxing
use landlock::*;

fn sandbox_unimatrix() -> Result<()> {
    let abi = ABI::V5;
    let ruleset = Ruleset::default()
        .handle_access(AccessFs::from_all(abi))?
        .create()?
        // Allow read/write to data directory
        .add_rule(PathBeneath::new(
            PathFd::new("/data/unimatrix")?,
            AccessFs::from_all(abi),
        ))?
        // Allow read-only to model cache
        .add_rule(PathBeneath::new(
            PathFd::new("/home/user/.cache/unimatrix")?,
            AccessFs::ReadFile | AccessFs::ReadDir,
        ))?
        // Allow network for API calls
        .add_rule(NetPort::new(443, AccessNet::ConnectTcp))?;
    ruleset.restrict_self()?;
    Ok(())
}
```

**seccomp-bpf:**
- Filter system calls at kernel level before they execute
- Docker's default profile blocks ~44 of 300+ syscalls
- Custom profiles per container/process
- Very low overhead

**nsjail** (Google):
- Combines namespaces + seccomp-bpf + cgroups
- Lightweight process isolation
- Used in production by Google
- CPU time, memory, fd, process count limits
- Kafel language for seccomp policy definition

### 6.4 cap-std (Rust Capability-Based Filesystem)

From the Bytecode Alliance (same team as Wasmtime):

- `Dir` type performs path sandboxing (blocks `..`, symlinks, absolute paths)
- Not a sandbox for untrusted code (Rust unsafe can bypass)
- For trusted code declaring its intent to limit access
- Uses `openat2` on Linux 5.6+ for single-syscall sandbox
- Ecosystem: cap-tempfile, cap-fs-ext, cap-net-ext

**Best used:** for Unimatrix's own filesystem access, not for sandboxing LLM-controlled tools. Provides defense-in-depth when combined with Landlock.

### 6.5 Comparison Matrix

| Approach | Startup | Overhead | Isolation | Untrusted Code | Rust-Native |
|---|---|---|---|---|---|
| gVisor | ~100ms | Medium | Strongest | Yes | No (Go) |
| Wasmtime | <1ms | Minimal | Strong | Yes | Yes |
| nsjail | ~10ms | Minimal | Strong | Yes | No (C++) |
| Landlock | 0 | None | Medium | Self only | Yes (crate) |
| seccomp | 0 | None | Medium | Process-level | Via libc |
| cap-std | 0 | None | Weak | Trusted only | Yes |

**Recommendation for Unimatrix:**
1. **Landlock** for self-sandboxing the Unimatrix process
2. **Wasmtime** for sandboxed tool execution (if tools run code)
3. **cap-std** for Unimatrix's own filesystem operations
4. These stack: Landlock restricts the process, cap-std restricts Rust code, Wasmtime restricts tool modules

---

## 7. API-Based vs Local LLM Integration

### 7.1 Architecture Comparison

```
API-Based Integration:
+------------------+     HTTPS      +------------------+
| Unimatrix (Rust) |--------------->| Anthropic API    |
| - Knowledge store|     JSON       | - Claude models  |
| - Tool execution |<---------------| - No local access|
| - Policy enforce |                | - Stateless      |
+------------------+                +------------------+
    Trust boundary: network edge
    LLM has: zero local access
    Attack surface: API responses (text only)

Local Model Integration:
+----------------------------------------------------------+
| Unimatrix Process                                        |
|                                                          |
| +------------------+     +-----------------------------+ |
| | Knowledge store  |     | LLM Runtime (ort/llama.cpp) | |
| | Tool execution   |<--->| - Model weights in memory   | |
| | Policy enforce   |     | - Direct memory access      | |
| +------------------+     | - Shared process space      | |
|                          +-----------------------------+ |
+----------------------------------------------------------+
    Trust boundary: none (same process)
    LLM has: full process memory access
    Attack surface: model output + memory + side channels
```

### 7.2 Security Comparison

| Property | API-Based | Local Model |
|---|---|---|
| Data exposure to LLM | Prompt text only | Full process memory |
| LLM filesystem access | None | Shared with host |
| Network dependency | Required | None |
| Latency | ~100-500ms per call | ~10-50ms per call |
| Model quality | Frontier models | Smaller models |
| Cost | Per-token pricing | GPU/CPU cost |
| Privacy | Data sent to provider | Data stays local |
| Supply chain risk | API availability | Model provenance |
| Prompt injection impact | Limited to API response | Could affect host process |
| Formal trust boundary | Network edge (clear) | None (same process) |

### 7.3 Hybrid Approach

```
+----------------------------------------------------------+
| Unimatrix Process                                        |
|                                                          |
| +-------------------+  +-----------------------------+   |
| | Local Embed Model  |  | Remote Claude API          |   |
| | (ort, 384-d,       |  | (reasoning, planning,      |   |
| |  already in nxs-003|  |  language understanding)    |   |
| |  Low-risk: vectors)|  |  High-value: decisions)    |   |
| +-------------------+  +-----------------------------+   |
|         |                          |                     |
|         v                          v                     |
| +----------------------------------------------------+  |
| | Deterministic Shell                                 |  |
| | (validates ALL outputs before action)               |  |
| +----------------------------------------------------+  |
+----------------------------------------------------------+
```

**Recommended: Hybrid**
- **Local embeddings** (already implemented in nxs-003): embedding models produce vectors, not text. They cannot hallucinate actions, propose tool calls, or inject instructions. Safe for in-process execution.
- **Remote Claude API** for reasoning: maintains a clear trust boundary. LLM output is text that Unimatrix parses and validates before acting on. The LLM never has direct access to Unimatrix's data store, filesystem, or tools.

### 7.4 Vulnerability Profiles

Research from Liu et al. (AIware 2025) shows:
- **API-integrated apps** are more prone to integration-level vulnerabilities (input validation, prompt injection)
- **Local inference frameworks** exhibit memory-level vulnerabilities tied to C/C++ native code
- **Mitigation differs:** API apps need hardening at interface/orchestration level; local apps need fuzz testing and memory-safe language adoption

Unimatrix's Rust foundation eliminates the memory safety concern for local models, but the integration-level risks remain for both approaches. The API approach provides a cleaner blast radius because a compromised LLM response is just text that the Deterministic Shell can reject.

---

## 8. The Unimatrix as GitHub App Pattern

### 8.1 GitHub App Architecture

```
+------------------------------------------------------------------+
|  GitHub Platform                                                 |
|                                                                  |
|  +---------------------+                                        |
|  | GitHub App Registry  |  - App registration (name, perms)     |
|  | (global entity)      |  - Private key (PEM file)             |
|  |                      |  - Webhook URL                        |
|  +---------------------+                                        |
|           |                                                      |
|           | install                                              |
|           v                                                      |
|  +---------------------+                                        |
|  | Installation         |  - Bound to org/user                  |
|  | (instance)           |  - Scoped to repos (selected or all)  |
|  |                      |  - Intersection: app perms AND repos  |
|  +---------------------+                                        |
|           |                                                      |
|           | authenticates                                        |
|           v                                                      |
|  +---------------------+                                        |
|  | Access Tokens        |  - JWT (app identity, 10min)          |
|  | (short-lived)        |  - Installation token (1hr, scoped)   |
|  |                      |  - Can further restrict perms + repos |
|  +---------------------+                                        |
+------------------------------------------------------------------+
```

### 8.2 Authentication Flow

```
1. App generates JWT (signed with private key)
   +----------+     JWT       +----------+
   | Unimatrix|------------->| GitHub   |
   | (App)    |               | API      |
   +----------+               +----------+

2. Exchange JWT for installation access token
   +----------+  POST /installations/{id}/access_tokens  +----------+
   | Unimatrix|----------------------------------------->| GitHub   |
   | (App)    |<-----------------------------------------| API      |
   +----------+     installation_token (1hr)             +----------+

3. Use installation token for API calls
   +----------+  Authorization: token <inst_token>       +----------+
   | Unimatrix|----------------------------------------->| GitHub   |
   | (App)    |     (scoped to installed repos)          | API      |
   +----------+                                          +----------+
```

### 8.3 Permission Model

GitHub Apps support fine-grained permissions across categories:

**Repository permissions (per-repo):**
- Contents (read/write) — file access
- Issues (read/write) — issue management
- Pull requests (read/write) — PR management
- Metadata (read) — always granted
- Actions, Checks, Deployments, etc.

**Organization permissions:**
- Members, Teams, Projects
- Administration

**Scoping:**
- App declares maximum permissions at registration
- Installation can further restrict to subset of repos
- Installation tokens can be generated with reduced permissions
- Effective permissions = intersection of all layers

### 8.4 Security Properties

**Token management:**
- Installation tokens expire after 1 hour
- JWTs expire after 10 minutes
- Maximum 10 tokens per user/app/scope combination
- Oldest tokens auto-revoked when limit exceeded
- Credential Revocation API for exposed tokens (GA April 2025)

**Audit and monitoring:**
- 180-day audit log retention
- Events: `oauth_authorization.destroy`, token creation/revocation
- Organization-level audit with search/filter
- Enterprise audit API

**Blast radius on compromise:**
- App scoped to installed repos only
- Cannot access repos outside installation
- Short-lived tokens limit exposure window
- Can be instantly revoked by org admin
- December 2025: org admins can block repo-level installations

### 8.5 Unimatrix as GitHub App — Trust Chain

```
+------------------------------------------------------------------+
|                                                                  |
|  GitHub (Root of Trust)                                          |
|  - Verifies Unimatrix App identity                              |
|  - Grants scoped installation tokens                            |
|  - Maintains audit log                                          |
|       |                                                          |
|       | Installation token (1hr, repo-scoped)                    |
|       v                                                          |
|  Unimatrix (Trusted Mediator)                                   |
|  - Authenticates via JWT + installation token                   |
|  - Reads repo content via GitHub API                            |
|  - Manages knowledge base (redb/hnsw)                           |
|  - Mediates ALL agent access to repos                           |
|       |                                                          |
|       | Attenuated context (knowledge entries, not raw files)    |
|       v                                                          |
|  Claude API (Generative Core)                                   |
|  - Receives curated context from Unimatrix                      |
|  - Proposes actions (tool calls)                                |
|  - Has NO direct GitHub access                                  |
|       |                                                          |
|       | Proposed actions (text)                                  |
|       v                                                          |
|  Unimatrix (Deterministic Shell)                                |
|  - Validates proposed actions against policy                    |
|  - Executes approved actions via GitHub API                     |
|  - Logs all actions to audit trail                              |
|  - Reports results back to agent                               |
|                                                                  |
+------------------------------------------------------------------+

INVARIANT: The Claude API never receives a GitHub token.
           It receives knowledge entries and proposes actions.
           Unimatrix executes or rejects those actions.
```

**This creates a three-level trust chain:**
1. **GitHub** trusts Unimatrix (installed App with scoped permissions)
2. **Unimatrix** mediates Claude (sends context, validates proposals)
3. **Claude** proposes, never acts (no tokens, no direct access)

At each level, permissions attenuate (narrow). The LLM at the bottom has the least privilege.

---

## 9. Container-as-Trust-Boundary

### 9.1 Container Security Primitives

```
+------------------------------------------------------------------+
|  Container Runtime                                               |
|                                                                  |
|  +---------------------------+                                   |
|  | Namespaces                |                                   |
|  | - PID (separate process   |                                   |
|  |   tree)                   |                                   |
|  | - Mount (separate fs)     |                                   |
|  | - Network (separate       |                                   |
|  |   net stack)              |                                   |
|  | - User (UID mapping)      |                                   |
|  | - UTS (hostname)          |                                   |
|  | - IPC (separate IPC)      |                                   |
|  +---------------------------+                                   |
|                                                                  |
|  +---------------------------+                                   |
|  | seccomp-bpf               |  Default Docker: blocks ~44/300+ |
|  | (syscall filtering)       |  syscalls. Custom profiles per   |
|  |                           |  container for different trust.  |
|  +---------------------------+                                   |
|                                                                  |
|  +---------------------------+                                   |
|  | cgroups                   |  CPU, memory, I/O, process limits|
|  | (resource limits)         |                                   |
|  +---------------------------+                                   |
|                                                                  |
|  +---------------------------+                                   |
|  | AppArmor / SELinux        |  Mandatory access control         |
|  | (LSM profiles)            |  per-container profiles          |
|  +---------------------------+                                   |
+------------------------------------------------------------------+
```

### 9.2 Processes with Different Trust Levels in One Container

**Yes, it is possible** but requires explicit configuration:

- **Custom seccomp profiles** can be applied per-process
- **Linux capabilities** can be dropped per-process (`cap_drop`)
- **User namespaces** can map different UIDs for different processes
- **Landlock** can self-sandbox individual processes within the container

However, shared kernel means:
- Side-channel attacks possible (timing, cache)
- Kernel vulnerabilities affect all processes
- Shared `/proc` filesystem (unless isolated)

### 9.3 Dev Containers as Deployment Model

**GitHub Codespaces architecture:**
- Each codespace = isolated VM with its own virtual network
- Firewalls block incoming connections from internet
- Codespaces cannot communicate with each other on internal networks
- `.devcontainer.json` controls environment configuration

**Security concern:** Repository-defined `.devcontainer/` files can execute arbitrary code (postCreateCommand). This is an attacker-controlled execution path.

**Unimatrix + Claude Code in a Dev Container:**

```
+------------------------------------------------------------------+
|  Dev Container                                                   |
|                                                                  |
|  +-----------------------------+  +---------------------------+  |
|  | Unimatrix (Rust binary)     |  | Claude Code (Node.js)     |  |
|  | - redb knowledge store      |  | - Agent loop              |  |
|  | - HNSW vector index         |  | - Tool execution          |  |
|  | - MCP server (existing)     |  | - File operations         |  |
|  | - Policy enforcement        |  |                           |  |
|  +-----------------------------+  +---------------------------+  |
|       ^                                    |                     |
|       | MCP (stdio/SSE)                    | Tool calls          |
|       |                                    v                     |
|  +----------------------------------------------------------+   |
|  | Shared Filesystem (workspace)                             |   |
|  +----------------------------------------------------------+   |
|                                                                  |
|  +----------------------------------------------------------+   |
|  | Shared Network (localhost)                                |   |
|  +----------------------------------------------------------+   |
+------------------------------------------------------------------+
```

**In this model:**
- Unimatrix and Claude Code are peers within the container
- They communicate via MCP (Unimatrix is already an MCP server)
- Both have filesystem access (shared workspace)
- Container boundary isolates from host system
- Unimatrix provides knowledge; Claude Code provides reasoning

**Trust implications:**
- No additional isolation between Unimatrix and Claude Code
- Claude Code has same filesystem access as Unimatrix
- Container provides isolation from host, not between services
- For stronger isolation: run in separate containers with network bridge

### 9.4 Recommended Container Architecture

```
+------------------------------------------------------------------+
|  Host System                                                     |
|                                                                  |
|  +-------------------------------+  +--------------------------+ |
|  | Container 1: Unimatrix        |  | Container 2: Agent       | |
|  |                               |  | Sandbox                  | |
|  | - redb, HNSW, embeddings      |  | - Claude Code / Agent SDK| |
|  | - MCP server                  |  | - Restricted filesystem  | |
|  | - GitHub App auth             |  | - No GitHub tokens       | |
|  | - Policy engine               |  | - No direct network      | |
|  | - Audit logging               |  | - Tools via MCP only     | |
|  |                               |  |                          | |
|  | Landlock self-sandboxed       |  | seccomp + Landlock       | |
|  +-------------------------------+  +--------------------------+ |
|              ^                               |                   |
|              | MCP over localhost             |                   |
|              +-------------------------------+                   |
+------------------------------------------------------------------+
```

---

## 10. Identity Delegation Chains

### 10.1 The Full Chain

```
GitHub (identity provider)
    |
    | JWT + Installation Token (1hr, repo-scoped)
    v
Unimatrix (GitHub App / trusted mediator)
    |
    | Internal session token (capability-restricted)
    v
Claude API (LLM provider, stateless)
    |
    | Text response with proposed tool calls
    v
Unimatrix (tool executor, validates all proposals)
    |
    | Attenuated tool results
    v
Agent (logical entity, no credentials)
```

Each link in the chain **attenuates** (narrows) permissions:
- GitHub grants repo-scoped access to Unimatrix
- Unimatrix sends only relevant knowledge to Claude (no tokens)
- Claude's proposals are validated against policy before execution
- Agents receive only the information Unimatrix releases to them

### 10.2 OAuth 2.0 Token Exchange (RFC 8693)

RFC 8693 defines a protocol for exchanging security tokens:

**Impersonation vs Delegation:**
- **Impersonation:** A acts as B; receivers see B (A is invisible)
- **Delegation:** A acts for B; both identities visible; A has B's delegated rights

For AI agents, **delegation** is the correct semantic. The agent acts for the user, and both identities are visible in the audit trail.

**Token Exchange for AI:**
- `subject_token`: represents the original user
- `actor_token`: represents the agent requesting to act
- Response: scoped token with delegation semantics

**IETF Draft (2025):** "OAuth 2.0 Extension: On-Behalf-Of User Authorization for AI Agents" introduces:
- `requested_actor` parameter to identify the specific agent
- `actor_token` parameter to authenticate the agent during token exchange

### 10.3 SPIFFE for Workload Identity

SPIFFE (Secure Production Identity Framework for Everyone):

```
+------------------------------------------------------------------+
|  SPIFFE Trust Domain                                             |
|                                                                  |
|  +-------------------+                                           |
|  | SPIRE Server      |  Root of trust for the domain             |
|  | (Control Plane)   |  Issues SVIDs (SPIFFE Verifiable          |
|  |                   |  Identity Documents)                      |
|  +-------------------+                                           |
|           |                                                      |
|           | Attestation                                          |
|           v                                                      |
|  +-------------------+                                           |
|  | SPIRE Agent       |  Per-node agent                           |
|  | (Data Plane)      |  Workload attestation                     |
|  |                   |  Credential rotation                      |
|  +-------------------+                                           |
|           |                                                      |
|           | SVID (X.509 or JWT)                                  |
|           v                                                      |
|  +-------------------+                                           |
|  | Workload          |  spiffe://trust-domain/workload-name      |
|  | (Unimatrix)       |  Cryptographically verifiable identity    |
|  +-------------------+                                           |
+------------------------------------------------------------------+
```

**Application to Unimatrix:**
- SPIFFE ID: `spiffe://unimatrix.dev/unimatrix-server`
- Agent IDs: `spiffe://unimatrix.dev/agent/{agent-name}`
- Mutual TLS between Unimatrix and any external services
- Short-lived, automatically rotated credentials
- No secrets in configuration files

**However:** SPIFFE is infrastructure for distributed systems. For Unimatrix's single-machine, local-first architecture, it is over-engineered. SPIFFE becomes relevant when Unimatrix evolves to multi-node or cloud deployment.

### 10.4 Practical Identity Chain for Unimatrix

```
+------------------------------------------------------------------+
|  Identity Chain (Single-Machine)                                 |
|                                                                  |
|  Layer 1: GitHub Identity                                        |
|  - GitHub App private key (file, protected)                      |
|  - Installation token (runtime, 1hr, repo-scoped)                |
|                                                                  |
|  Layer 2: Unimatrix Internal Identity                            |
|  - Agent Registry (redb, already in vnc-001)                     |
|  - Trust levels: System > Privileged > Internal > Restricted     |
|  - Content hash chain for audit integrity                        |
|                                                                  |
|  Layer 3: Session Identity                                       |
|  - Per-session capability set (derived from agent trust level)   |
|  - Immutable after creation                                      |
|  - Logged in audit trail                                         |
|                                                                  |
|  Layer 4: LLM Identity                                           |
|  - Anthropic API key (Unimatrix holds, never shared)             |
|  - LLM has no identity of its own                                |
|  - All LLM actions attributed to the requesting agent            |
+------------------------------------------------------------------+
```

**Key principle:** The LLM has no identity. It is a tool. Actions taken based on LLM proposals are attributed to the agent that initiated the request, mediated by Unimatrix.

---

## 11. Real-World Architectures

### 11.1 AWS Bedrock AgentCore Gateway

The most mature production implementation of "Deterministic Shell for AI":
- Natural language policies compiled to Cedar (deterministic policy language)
- Gateway enforces policies **outside** the LLM reasoning loop
- Agent cannot reason around constraints
- Deterministic, not probabilistic enforcement

### 11.2 Microsoft Wassette

Reference implementation for WASM-sandboxed AI tools:
- Rust binary with zero runtime dependencies
- Bridges WASM Components with MCP
- Deny-by-default capability system
- Compatible with Claude Code, GitHub Copilot, Cursor

### 11.3 Google GKE Agent Sandbox

Most advanced container-based isolation:
- gVisor intercepts ALL syscalls in userspace
- Pre-warmed sandbox pools for sub-second startup
- Pod Snapshots for instant checkpoint/restore
- Workload Identity Federation for least-privilege IAM

### 11.4 Kubiya (DevOps AI Platform)

Multi-agent orchestration for DevOps:
- Agents with defined scopes and permissions
- Tool execution through controlled pipelines
- RBAC integration with existing IAM
- Audit logging of all agent actions

### 11.5 Windmill (Code Execution Platform)

Uses nsjail for sandboxed execution:
- Per-execution isolation with namespaces + seccomp
- Resource limits via cgroups
- Lightweight (minimal overhead)
- Production-proven for Python/Go execution

### 11.6 Common Patterns Across All Systems

Every production system that embeds LLMs follows the same fundamental pattern:

1. **The LLM never has direct access to resources.** It proposes; the system executes.
2. **Policy enforcement is deterministic.** Not another LLM, not probabilistic.
3. **Audit trail is immutable.** Every action logged with attribution.
4. **Permissions attenuate through the chain.** Never escalate.
5. **Short-lived credentials.** Tokens expire quickly, minimize blast radius.
6. **The system trusts itself, not the LLM.** The LLM is an untrusted input source.

---

## 12. Feasibility Assessment for Unimatrix

### 12.1 Constraints

- **Language:** Rust (edition 2024, MSRV 1.89)
- **Architecture:** Local-first, single machine (initial deployment)
- **Existing infrastructure:** redb store, HNSW vector index, embedding pipeline, MCP server
- **Deployment model:** Dev Container alongside Claude Code
- **`#![forbid(unsafe_code)]`** in all crates

### 12.2 What Unimatrix Already Has

| Component | Status | Relevance to LLM Integration |
|---|---|---|
| Knowledge store (redb) | Implemented (nxs-001) | Context source for LLM prompts |
| Vector index (HNSW) | Implemented (nxs-002) | Semantic search for RAG |
| Embedding pipeline | Implemented (nxs-003) | Local embeddings (safe, in-process) |
| Core traits | Implemented (nxs-004) | Async wrappers, unified error handling |
| MCP server | Implemented (vnc-001) | Agent communication protocol |
| Tool implementations | Implemented (vnc-002) | context_search, context_store, etc. |
| Agent registry | Implemented (vnc-001) | 4 trust levels, auto-enrollment |
| Audit log | Implemented (vnc-001) | Append-only, monotonic IDs |
| Content scanning | Implemented (vnc-002) | ~35 regex patterns |
| Category allowlist | Implemented (vnc-002) | Runtime-extensible |

### 12.3 What Needs to Be Built

**Phase 1: API Integration (Minimal)**
- HTTP client for Anthropic Messages API (`reqwest`)
- Message serialization/deserialization (`serde`)
- Tool-calling loop (parse tool_use blocks, execute via existing handlers, return results)
- Rate limiting and retry logic
- API key management (environment variable or secure store)

**Phase 2: Deterministic Shell**
- Policy engine: define allowed/denied tool patterns per agent trust level
- Input validation layer (beyond existing content scanning)
- Output validation layer (validate LLM proposals before execution)
- Capability tokens (Rust newtypes for compile-time enforcement)
- State machine for agent session lifecycle

**Phase 3: GitHub App**
- JWT generation (using `jsonwebtoken` crate)
- Installation token management (1hr refresh cycle)
- Repository content access via GitHub API
- Webhook receiver for push/PR events

**Phase 4: Sandboxing (if needed)**
- Landlock self-sandboxing for Unimatrix process
- Wasmtime integration for sandboxed tool execution
- cap-std for filesystem access restriction

### 12.4 Risk Assessment

| Risk | Severity | Mitigation |
|---|---|---|
| LLM proposes destructive action | High | Deterministic Shell validates all proposals |
| API key exposure | High | Environment variable, never in config files |
| Prompt injection via knowledge entries | Medium | CaMeL-inspired data provenance tracking |
| Token exhaustion (rate limits) | Medium | Retry with backoff, request queuing |
| Community Rust SDK instability | Medium | Direct API integration (no SDK dependency) |
| Audit log tampering | Low | Append-only redb table, content hash chain |
| Model availability (API outage) | Low | Graceful degradation, queue pending work |

### 12.5 Effort Estimate

| Phase | Effort | Dependencies |
|---|---|---|
| Phase 1: API Integration | ~2 features (nxs-level) | reqwest, serde_json, tokio |
| Phase 2: Deterministic Shell | ~2 features | Phase 1 |
| Phase 3: GitHub App | ~1 feature | jsonwebtoken, Phase 1 |
| Phase 4: Sandboxing | ~1 feature (optional) | landlock, wasmtime |

---

## 13. Recommended Integration Pattern

### 13.1 Architecture: "Unimatrix as Deterministic Oracle with Generative Core"

```
+====================================================================+
|  UNIMATRIX SYSTEM                                                  |
|                                                                    |
|  +--------------------------------------------------------------+  |
|  | DETERMINISTIC SHELL (Rust, compile-time enforced)             |  |
|  |                                                               |  |
|  |  +------------------+  +------------------+  +-------------+ |  |
|  |  | Input Validator   |  | Policy Engine    |  | Output      | |  |
|  |  | - Sanitization    |  | - Per-agent caps |  | Validator   | |  |
|  |  | - Schema check    |  | - Tool allowlist |  | - Schema    | |  |
|  |  | - Content scan    |  | - Action limits  |  | - Safety    | |  |
|  |  +------------------+  +------------------+  +-------------+ |  |
|  |                                                               |  |
|  |  +------------------+  +------------------+  +-------------+ |  |
|  |  | Session Manager   |  | Tool Executor    |  | Audit Log   | |  |
|  |  | - State machine   |  | - Mediated exec  |  | - Append    | |  |
|  |  | - Cap tokens      |  | - Sandboxed      |  | - Hash chain| |  |
|  |  | - Trust levels    |  | - Rate limited   |  | - Attributed| |  |
|  |  +------------------+  +------------------+  +-------------+ |  |
|  +--------------------------------------------------------------+  |
|       ^                          |                                  |
|       | Validated request        | Validated proposal               |
|       |                          v                                  |
|  +--------------------------------------------------------------+  |
|  | GENERATIVE CORE (Claude API, remote, untrusted)               |  |
|  |                                                                |  |
|  |  Unimatrix sends:          Claude returns:                     |  |
|  |  - System prompt            - Reasoning                        |  |
|  |  - Knowledge context        - Tool call proposals              |  |
|  |  - Available tools          - Content                          |  |
|  |  - Conversation history                                        |  |
|  |                                                                |  |
|  |  Claude NEVER receives:                                        |  |
|  |  - GitHub tokens            - API keys                         |  |
|  |  - Database handles         - File paths (only content)        |  |
|  |  - Raw credentials          - Unimatrix internal state         |  |
|  +--------------------------------------------------------------+  |
|                                                                    |
|  +--------------------------------------------------------------+  |
|  | KNOWLEDGE LAYER (existing, unchanged)                         |  |
|  |  redb + HNSW + embeddings + MCP server                       |  |
|  +--------------------------------------------------------------+  |
|                                                                    |
|  +--------------------------------------------------------------+  |
|  | TRUST CHAIN                                                   |  |
|  |  GitHub (root) --> Unimatrix (mediator) --> Claude (proposer) |  |
|  |  Each link attenuates. Claude has least privilege.            |  |
|  +--------------------------------------------------------------+  |
+====================================================================+
```

### 13.2 Integration Approach: Direct API (Not SDK Wrapping)

**Rationale:**
1. No official Anthropic Rust SDK exists
2. Community Rust SDKs wrap the CLI as subprocess (fragile)
3. The Messages API is simple: POST JSON, receive JSON
4. Rust's type system provides stronger guarantees than SDK hooks
5. Full control over the tool-calling loop
6. No subprocess management complexity

**Implementation sketch:**

```rust
// Tool-calling loop in Rust
pub async fn run_agent_turn(
    client: &AnthropicClient,
    session: &mut AgentSession<Executing>,
    policy: &PolicyEngine,
) -> Result<AgentResponse> {
    let response = client.create_message(&session.to_request()).await?;

    for content_block in &response.content {
        match content_block {
            ContentBlock::Text(text) => {
                session.record_response(text);
            }
            ContentBlock::ToolUse(tool_call) => {
                // DETERMINISTIC SHELL: validate before execution
                let decision = policy.evaluate(&session.agent, &tool_call)?;

                match decision {
                    PolicyDecision::Allow => {
                        let result = session.execute_tool(&tool_call).await?;
                        session.record_tool_result(tool_call.id, result);
                    }
                    PolicyDecision::Deny(reason) => {
                        session.record_tool_denied(tool_call.id, reason);
                    }
                    PolicyDecision::Escalate => {
                        // Queue for human review
                        session.queue_for_review(tool_call)?;
                    }
                }
            }
        }
    }

    // Continue if there were tool calls; stop if text-only response
    if session.has_pending_tool_results() {
        Box::pin(run_agent_turn(client, session, policy)).await
    } else {
        Ok(session.finalize())
    }
}
```

### 13.3 Security Properties of Recommended Architecture

| Property | Mechanism | Enforcement Level |
|---|---|---|
| LLM cannot access files | No file paths in prompts; tools mediated | Architecture |
| LLM cannot access tokens | Tokens held by Unimatrix; never in context | Architecture |
| LLM proposals validated | Policy engine checks every tool call | Runtime (Rust) |
| Capability tokens | Newtype + PhantomData | Compile-time |
| State machine integrity | Typestate pattern | Compile-time |
| Audit completeness | Append-only log, hash chain | Runtime (redb) |
| Permission attenuation | Trust level -> capability set mapping | Configuration + runtime |
| Blast radius | API boundary (LLM is remote text) | Architecture |

### 13.4 Migration Path from Current Architecture

**Current:** Unimatrix is a passive MCP server. Claude Code connects to it as a tool provider.

**Proposed:** Unimatrix becomes an active orchestrator that calls the Claude API itself.

**These are not mutually exclusive.** Unimatrix can:
1. Continue serving as an MCP server (existing vnc-001/002 functionality)
2. Additionally call the Claude API for internal reasoning tasks
3. Gradually move orchestration logic from Claude Code to Unimatrix

```
Phase 0 (Current):
  Claude Code --MCP--> Unimatrix (passive server)

Phase 1 (Hybrid):
  Claude Code --MCP--> Unimatrix --API--> Claude (for knowledge tasks)
  Claude Code still orchestrates; Unimatrix enriches context

Phase 2 (Active):
  Unimatrix --API--> Claude (for all reasoning)
  Unimatrix --GitHub API--> repos (for code access)
  Unimatrix --MCP--> Claude Code (as one of many tools)

Phase 3 (Full Orchestrator):
  Unimatrix orchestrates everything
  Claude Code becomes a sandboxed tool executor
  GitHub App provides authenticated repo access
```

---

## 14. Bibliography

### Claude Agent SDK
1. Anthropic. "Building agents with the Claude Agent SDK." [https://www.anthropic.com/engineering/building-agents-with-the-claude-agent-sdk](https://www.anthropic.com/engineering/building-agents-with-the-claude-agent-sdk)
2. Anthropic. "Agent SDK overview." [https://platform.claude.com/docs/en/agent-sdk/overview](https://platform.claude.com/docs/en/agent-sdk/overview)
3. Anthropic. "Intercept and control agent behavior with hooks." [https://platform.claude.com/docs/en/agent-sdk/hooks](https://platform.claude.com/docs/en/agent-sdk/hooks)
4. Anthropic. "Subagents in the SDK." [https://platform.claude.com/docs/en/agent-sdk/subagents](https://platform.claude.com/docs/en/agent-sdk/subagents)
5. Anthropic. "Handling Permissions." [https://docs.anthropic.com/en/docs/claude-code/sdk/sdk-permissions](https://docs.anthropic.com/en/docs/claude-code/sdk/sdk-permissions)
6. Anthropic. "Headless mode." [https://code.claude.com/docs/en/headless](https://code.claude.com/docs/en/headless)
7. Anthropic. "Create custom subagents." [https://code.claude.com/docs/en/sub-agents](https://code.claude.com/docs/en/sub-agents)
8. anthropics/claude-agent-sdk-python. GitHub. [https://github.com/anthropics/claude-agent-sdk-python](https://github.com/anthropics/claude-agent-sdk-python)

### Rust SDKs
9. anthropic-agent-sdk (crate). [https://crates.io/crates/anthropic-agent-sdk](https://crates.io/crates/anthropic-agent-sdk)
10. claude-agent-sdk (Rust). [https://github.com/louloulin/claude-agent-sdk](https://github.com/louloulin/claude-agent-sdk)
11. claude_agent_sdk_rust. [https://github.com/Wally869/claude_agent_sdk_rust](https://github.com/Wally869/claude_agent_sdk_rust)
12. Claudius (crate). [https://lib.rs/crates/claudius](https://lib.rs/crates/claudius)

### Deterministic Shell / CaMeL
13. Debenedetti, E. et al. "Defeating Prompt Injections by Design." arXiv:2503.18813, 2025. [https://arxiv.org/abs/2503.18813](https://arxiv.org/abs/2503.18813)
14. Willison, S. "CaMeL offers a promising new direction for mitigating prompt injection attacks." 2025. [https://simonwillison.net/2025/Apr/11/camel/](https://simonwillison.net/2025/Apr/11/camel/)
15. Morissette, D. "Deterministic Core, Agentic Shell." 2026. [https://blog.davemo.com/posts/2026-02-14-deterministic-core-agentic-shell.html](https://blog.davemo.com/posts/2026-02-14-deterministic-core-agentic-shell.html)
16. Fowler, M. & Subramaniam, B. "Emerging Patterns in Building GenAI Products." 2025. [https://martinfowler.com/articles/gen-ai-patterns/](https://martinfowler.com/articles/gen-ai-patterns/)
17. "Blueprint First, Model Second." arXiv:2508.02721. [https://arxiv.org/html/2508.02721v1](https://arxiv.org/html/2508.02721v1)

### Sandboxing
18. Microsoft. "Introducing Wassette: WebAssembly-based tools for AI agents." 2025. [https://opensource.microsoft.com/blog/2025/08/06/introducing-wassette-webassembly-based-tools-for-ai-agents](https://opensource.microsoft.com/blog/2025/08/06/introducing-wassette-webassembly-based-tools-for-ai-agents)
19. Wasmtime. "Security." [https://docs.wasmtime.dev/security.html](https://docs.wasmtime.dev/security.html)
20. Bytecode Alliance. cap-std. [https://github.com/bytecodealliance/cap-std](https://github.com/bytecodealliance/cap-std)
21. Google. nsjail. [https://github.com/google/nsjail](https://github.com/google/nsjail)
22. Landlock documentation. [https://landlock.io/](https://landlock.io/)
23. rust-landlock. [https://github.com/landlock-lsm/rust-landlock](https://github.com/landlock-lsm/rust-landlock)
24. NVIDIA. "Sandboxing Agentic AI Workflows with WebAssembly." [https://developer.nvidia.com/blog/sandboxing-agentic-ai-workflows-with-webassembly/](https://developer.nvidia.com/blog/sandboxing-agentic-ai-workflows-with-webassembly/)
25. restyler/awesome-sandbox. GitHub. [https://github.com/restyler/awesome-sandbox](https://github.com/restyler/awesome-sandbox)

### Cloud Agent Platforms
26. AWS. "Amazon Bedrock AgentCore." [https://aws.amazon.com/bedrock/agentcore/](https://aws.amazon.com/bedrock/agentcore/)
27. AWS. "Introducing Amazon Bedrock AgentCore Identity." [https://aws.amazon.com/blogs/machine-learning/introducing-amazon-bedrock-agentcore-identity-securing-agentic-ai-at-scale/](https://aws.amazon.com/blogs/machine-learning/introducing-amazon-bedrock-agentcore-identity-securing-agentic-ai-at-scale/)
28. AWS. "Bedrock AgentCore -- The Trust Layer for Enterprise AI." [https://www.refactored.pro/blog/2025/12/4/aws-reinvent-2025-bedrock-agentcorethe-deterministic-guardrails-that-make-autonomous-ai-safe-for-the-enterprise](https://www.refactored.pro/blog/2025/12/4/aws-reinvent-2025-bedrock-agentcorethe-deterministic-guardrails-that-make-autonomous-ai-safe-for-the-enterprise)
29. Google Cloud. "GKE Agent Sandbox and GKE Pod Snapshots." [https://medium.com/google-cloud/gke-agent-sandbox-and-gke-pod-snapshots-zero-trust-security-for-ai-agents-at-scale-559261ee20b5](https://medium.com/google-cloud/gke-agent-sandbox-and-gke-pod-snapshots-zero-trust-security-for-ai-agents-at-scale-559261ee20b5)
30. Google Cloud. "Vertex AI Agent Engine overview." [https://docs.cloud.google.com/agent-builder/agent-engine/overview](https://docs.cloud.google.com/agent-builder/agent-engine/overview)

### GitHub Apps & Identity
31. GitHub. "Generating an installation access token for a GitHub App." [https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-an-installation-access-token-for-a-github-app](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/generating-an-installation-access-token-for-a-github-app)
32. GitHub. "About authentication with a GitHub App." [https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/about-authentication-with-a-github-app](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/about-authentication-with-a-github-app)
33. GitHub. "Token expiration and revocation." [https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/token-expiration-and-revocation](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/token-expiration-and-revocation)
34. GitHub. "Security in GitHub Codespaces." [https://docs.github.com/en/codespaces/reference/security-in-github-codespaces](https://docs.github.com/en/codespaces/reference/security-in-github-codespaces)
35. GitHub. "Best practices for creating a GitHub App." [https://docs.github.com/en/apps/creating-github-apps/about-creating-github-apps/best-practices-for-creating-a-github-app](https://docs.github.com/en/apps/creating-github-apps/about-creating-github-apps/best-practices-for-creating-a-github-app)

### Identity & Delegation
36. IETF. RFC 8693: OAuth 2.0 Token Exchange. [https://datatracker.ietf.org/doc/html/rfc8693](https://datatracker.ietf.org/doc/html/rfc8693)
37. IETF. "OAuth 2.0 Extension: On-Behalf-Of User Authorization for AI Agents." [https://www.ietf.org/archive/id/draft-oauth-ai-agents-on-behalf-of-user-01.html](https://www.ietf.org/archive/id/draft-oauth-ai-agents-on-behalf-of-user-01.html)
38. Auth0. "Auth0 Token Vault: Secure Token Exchange for AI Agents." [https://auth0.com/blog/auth0-token-vault-secure-token-exchange-for-ai-agents/](https://auth0.com/blog/auth0-token-vault-secure-token-exchange-for-ai-agents/)
39. SPIFFE. "SPIFFE Concepts." [https://spiffe.io/docs/latest/spiffe-about/spiffe-concepts/](https://spiffe.io/docs/latest/spiffe-about/spiffe-concepts/)
40. Spherical Cow Consulting. "Delegation in a Multi-Actor World." [https://sphericalcowconsulting.com/2025/06/27/delegation-part-two/](https://sphericalcowconsulting.com/2025/06/27/delegation-part-two/)

### IDE & SaaS Integrations
41. GitHub Blog. "Under the hood: Exploring the AI models powering GitHub Copilot." [https://github.blog/ai-and-ml/github-copilot/under-the-hood-exploring-the-ai-models-powering-github-copilot/](https://github.blog/ai-and-ml/github-copilot/under-the-hood-exploring-the-ai-models-powering-github-copilot/)
42. GitHub Blog. "Inside GitHub: Working with the LLMs behind GitHub Copilot." [https://github.blog/ai-and-ml/github-copilot/inside-github-working-with-the-llms-behind-github-copilot/](https://github.blog/ai-and-ml/github-copilot/inside-github-working-with-the-llms-behind-github-copilot/)
43. Cursor. "LLM Safety and Controls." [https://cursor.com/docs/enterprise/llm-safety-and-controls](https://cursor.com/docs/enterprise/llm-safety-and-controls)
44. Notion. "Notion AI security & privacy practices." [https://www.notion.com/help/notion-ai-security-practices](https://www.notion.com/help/notion-ai-security-practices)

### LLM Security Research
45. Liu, B. et al. "Security in the Wild: An Empirical Analysis of LLM-Powered Applications." AIware 2025. [https://bozhen-liu.github.io/assets/pdf/LPA_AIware25-preprint.pdf](https://bozhen-liu.github.io/assets/pdf/LPA_AIware25-preprint.pdf)
46. AFINE. "Prompt Injections, design patterns and a CaMeL." [https://afine.com/prompt-injections-design-patterns-and-a-camel/](https://afine.com/prompt-injections-design-patterns-and-a-camel/)

### Rust Type System
47. Rust Book. "Advanced Types." [https://doc.rust-lang.org/book/ch20-03-advanced-types.html](https://doc.rust-lang.org/book/ch20-03-advanced-types.html)
48. Rust Design Patterns. "Newtype." [https://rust-unofficial.github.io/patterns/patterns/behavioural/newtype.html](https://rust-unofficial.github.io/patterns/patterns/behavioural/newtype.html)
49. Crichton, W. "Type-level Programming in Rust." [https://willcrichton.net/notes/type-level-programming/](https://willcrichton.net/notes/type-level-programming/)

### Container Security
50. Docker. "Seccomp security profiles for Docker." [https://docs.docker.com/engine/security/seccomp/](https://docs.docker.com/engine/security/seccomp/)
51. Landlock kernel documentation. [https://docs.kernel.org/userspace-api/landlock.html](https://docs.kernel.org/userspace-api/landlock.html)

---

*Research conducted 2026-02-24. All URLs verified at time of research.*
