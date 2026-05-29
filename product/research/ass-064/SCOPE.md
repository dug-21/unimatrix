# ASS-064: Remote Telemetry + MCP Transport Unification

**Date**: 2026-05-29
**Type**: Architecture / Transport design
**Trigger**: vnc-021 scope review revealed that HTTPS transport (W2-2) without remote telemetry delivers a degraded product — MCP tool access without the intelligence pipeline's learning layer
**Blocks**: vnc-021 (W2-2) scope finalization

---

## Problem Statement

Unimatrix's value is not the knowledge API — it is the self-learning intelligence pipeline built on top of it. Behavioral signals, observation events, proactive injection, and PreCompact transcript restoration are what make Unimatrix a self-improving engine rather than a remote key-value store with search.

The current architecture splits communication across two UDS paths:

1. **MCP socket** (`unimatrix-mcp.sock`): Tool calls — search, store, lookup, briefing, cycle management. Bidirectional via rmcp.
2. **Hook IPC socket** (`unimatrix-hook.sock`): Lifecycle events — PreToolUse, PostToolUse, PreCompact, SubagentStart, etc. Client-pushed, 4-byte length-prefix framing. Proactive injection returns via stdout write during hook handling (server→client).

Both paths are local-only (Unix Domain Sockets). vnc-021 (W2-2) as currently scoped adds HTTPS transport for MCP tools but leaves Hook IPC local. In a remote personal cloud deployment (Unimatrix on a VPS, client on a laptop), MCP tools work over HTTPS but:

- No observation events reach the server → behavioral signal collection is blind
- No PreCompact events → transcript restoration disabled
- No proactive UDS injection → agents never receive unsolicited knowledge
- No hook-driven phase detection → session context vector is empty

This is not a minor degradation. The intelligence pipeline — the product's core differentiator — is offline for remote sessions.

**Enterprise dimension**: This same problem exists at enterprise scale, amplified. Enterprise deployments serve multiple developers across teams, all remote. The transport architecture chosen here must port cleanly to the enterprise private repository — OAuth-authenticated telemetry, per-team observation routing, centralized behavioral signal aggregation. A personal-cloud-only solution that requires rearchitecting for enterprise is the wrong investment.

---

## Research Questions

### RQ-1: Hook IPC Contract Audit

Catalog every event that flows over the hook IPC socket today. For each event:

- Direction: client→server, server→client, or bidirectional
- Frequency: per-tool-call, per-session, per-phase-transition, periodic
- Intelligence pipeline dependency: which downstream system consumes it (observation pipeline, proactive injection, PreCompact restoration, phase detection)
- Latency sensitivity: must the client block on the response, or is fire-and-forget acceptable?

Classify events into tiers:
- **Critical**: Intelligence pipeline is materially degraded without this event
- **Important**: Reduced signal quality but pipeline still functions
- **Nice-to-have**: Incremental improvement, can be deferred

### RQ-2: Transport Architecture Options

Evaluate these options for delivering both MCP and telemetry to a remote instance:

**(a) Dual-endpoint HTTP** — MCP on rmcp streamable HTTP; telemetry on a separate authenticated HTTP endpoint (POST /observe). Same TCP listener, same auth token, different paths. Mirrors the current dual-socket architecture over HTTP.

**(b) MCP tool tunneling** — Replace implicit hook dispatch with an explicit `context_observe` MCP tool. Clients call it to report lifecycle events. Eliminates the second transport surface entirely. Telemetry becomes part of the MCP protocol.

**(c) Bidirectional over rmcp streamable HTTP** — rmcp 0.16 streamable HTTP uses SSE for server→client notifications. Investigate whether observation events can ride client→server on the same connection (HTTP/2 multiplexing or chunked POST), and whether proactive injection can ride server→client SSE.

**(d) WebSocket sidecar** — A separate WebSocket connection for bidirectional telemetry alongside MCP HTTP. Two connections per client session, shared auth.

**(e) Unified MCP transport with notification extensions** — Extend the MCP session to carry observation events as MCP notifications (if the protocol supports it) and proactive injection as server-initiated notifications.

For each option, assess:
- Implementation complexity (rmcp 0.16.0 pinned, no axum, tower-native only)
- Client integration burden per target client (Claude Code, Codex CLI, Gemini CLI)
- Auth model fit (static bearer token for personal cloud, OAuth JWT for enterprise)
- Latency characteristics (blocking vs. fire-and-forget)
- Failure modes (what happens when the telemetry channel drops but MCP stays up?)
- Enterprise portability (does this architecture survive OAuth, multi-tenant routing, per-team isolation?)

### RQ-3: Proactive Injection Over HTTP

UDS injection currently writes to stdout during hook handling — the server pushes knowledge into the client's context window. Over HTTP:

- Does rmcp streamable HTTP support server-initiated messages (SSE notifications)?
- Can the proactive injection payload ride those notifications?
- If not, what is the alternative — client polling via a tool call? Piggyback on tool responses?
- What do Claude Code, Codex CLI, and Gemini CLI each support for receiving unsolicited server content?

This is the hardest part of the problem. MCP and telemetry are request/response patterns. Proactive injection is server-push.

### RQ-4: Client Integration Constraints

For each target client (Claude Code, Codex CLI, Gemini CLI):

- What hook/plugin architecture exists? Can it send observation events to a remote endpoint?
- What transport does the MCP client implementation support (stdio, HTTP, SSE)?
- Does the client support receiving server-initiated notifications?
- Known bugs or limitations (e.g., Claude Code #28293 — headers not forwarded on tool call POSTs)
- What configuration is required to connect to a remote MCP server with auth?

The transport architecture must work for all three clients. A solution that works for Claude Code but requires custom integration for Codex/Gemini is acceptable only if the custom integration is thin.

### RQ-5: Single-Connection vs. Dual-Connection Trade-offs

Is there architectural value in a single transport that carries everything (MCP + telemetry + proactive injection) vs. separating concerns?

**Single-connection arguments**: One port, one auth token, one connection lifecycle, simpler firewall/proxy config, atomic session identity.

**Dual-connection arguments**: Independent failure domains, telemetry can be fire-and-forget without blocking MCP, independent scaling, cleaner separation of concerns.

**Enterprise lens**: Multi-tenant routing is simpler with a single connection (one auth handshake, one tenant resolution) but dual connections allow telemetry to flow to a dedicated analytics pipeline without touching the knowledge API path.

### RQ-6: Impact on vnc-021 Scope

Given the recommended architecture:

- Does vnc-021 need to be restructured, or can telemetry transport be additive (shipped alongside or immediately after)?
- Are there structural decisions in vnc-021 (listener setup, auth middleware, session lifecycle) that would be done differently if telemetry transport is considered from the start?
- What is the minimum viable vnc-021 that avoids rework — the smallest scope that doesn't paint us into a corner?

---

## Constraints

1. **rmcp 0.16.0 pinned** — cannot upgrade without workspace-wide validation
2. **No axum** — rmcp 0.16 HTTP transport is tower-native; tower + hyper only
3. **`#![forbid(unsafe_code)]`** — all dependencies must be safe Rust or already audited
4. **Three target clients** — Claude Code, Codex CLI, Gemini CLI. Architecture must work for all three.
5. **Enterprise portability** — the chosen architecture must support OAuth JWT auth, multi-tenant routing, and per-team telemetry isolation without structural rearchitecture. The personal cloud implementation is the first consumer; enterprise is the second.
6. **Zero regression on local paths** — UDS MCP + UDS Hook IPC continue to work unchanged for local deployments
7. **Single binary** — no sidecar processes or separate telemetry services

---

## Approach

**Breadth**: Medium — focused on transport architecture, not implementation detail. Codebase investigation of hook IPC contract + rmcp 0.16 HTTP capabilities + client MCP implementations. No external API calls or proof-of-concept code.

**Researcher should**:
1. Audit `uds/listener.rs` and `uds/hook.rs` for the complete hook event catalog
2. Read rmcp 0.16 source for streamable HTTP server capabilities (SSE, notifications, session lifecycle)
3. Review Claude Code, Codex CLI, and Gemini CLI MCP client documentation/source for transport and notification support
4. Assess each RQ-2 option against the enterprise portability constraint
5. Produce a ranked recommendation with clear trade-off rationale

---

## Output

1. Hook event catalog with tier classification (RQ-1)
2. Transport option comparison matrix (RQ-2)
3. Proactive injection feasibility assessment (RQ-3)
4. Client compatibility matrix (RQ-4)
5. Architecture recommendation with enterprise portability assessment
6. vnc-021 scope impact statement (RQ-6) — restructure, defer, or proceed-with-changes

---

## Tracking

GitHub Issue: (to be created)
Blocks: #658 (vnc-021: HTTPS transport)
