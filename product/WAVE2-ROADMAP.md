# Wave 2 — Personal Cloud Delivery

**Date**: 2026-04-09 (updated 2026-04-14)
**Prior roadmap**: ASS-040 (self-learning knowledge engine) — COMPLETE
**Schema version**: v22
**Eval baseline**: MRR=0.2558, 2,096 scenarios, snapshot a03bdd8f1fcb (2026-04-08)

**Wave 2 outcome**: Complete, deployable personal Unimatrix cloud. Containerized, HTTPS-accessible, multi-LLM compatible, with a clean security model that an individual developer can operate without friction. Enterprise delivery follows in a separate private repository after Wave 2 ships.

---

## Prior Waves — Completion Status

| Wave | Scope | Status |
|------|-------|--------|
| Wave 0 | Daemon mode, sqlx dual-pool, config externalization | **COMPLETE** |
| Wave 1 | Typed relationship graph, rayon ML pool, eval harness, NLI cross-encoder, observation pipeline generalization | **COMPLETE** |
| Wave 1A | Formula fusion (WA-0), phase signals (WA-1), session context (WA-2), proactive delivery (WA-4), PreCompact restoration (WA-5) | **COMPLETE** |
| ASS-040 Groups 1–6 | Formula cleanup, tick decomp, graph enrichment (S1/S2/S8/cosine Supports), PPR expander (+0.0122 MRR), behavioral signal infra, goal-conditioned briefing | **COMPLETE** |
| ASS-040 Groups 7–8 | Data hygiene, co_access→PPR migration, intelligence-driven retention | **COMPLETE** (carry-forwards: #477, #471 open, non-blocking) |
| ASS-040 Groups 9–10 | Explicit read logging, phase-conditioned category affinity | **COMPLETE** |

Intelligence-pipeline carry-forwards that do not block Wave 2: #477 (quarantine guard at co_access write), #471 (orphaned-edge compaction), #510 (context_purge).

---

## Wave 2 — Delivery Items

### W2-0: OSS Licensing Clarity (🔬 ASS-045 — COMPLETE)
**Goal**: Clean OSS boundary. Core crates (`unimatrix-store`, `unimatrix-vector`, `unimatrix-embed`, `unimatrix-core`, `unimatrix-engine`, `unimatrix-server`) published under MIT/Apache 2.0. No license instrument applied to the OSS personal cloud tier.

**Resolved by ASS-045**: MIT/Apache 2.0 on all core crates (no BSL). DCO on MIT crates (no CLA required). Enterprise commercial features ship from a separate private repository — not from this codebase.

---

### W2-1: Container Packaging (🔬 ASS-043)
**Goal**: Single-image personal cloud deployment. Containerized daemon with ONNX runtime. Air-gap deployable — no runtime internet dependencies.

Named volumes:
- `unimatrix-knowledge` — per-repo knowledge DBs (integrity-critical, back up frequently)
- `unimatrix-analytics` — per-repo analytics DBs (self-healing)
- `unimatrix-shared` — ONNX models + `config.toml` as read-only bind

Non-root container user. HEALTHCHECK on daemon liveness + schema version.

**Resolved by ASS-043**: ONNX Runtime packaging approach. Base image selection. Multi-arch strategy. Secrets injection pattern.

---

### W2-2: HTTPS Transport + Static Token Auth + Observability (🔬 ASS-041 — COMPLETE)
**Goal**: HTTPS personal cloud transport. Static 256-bit bearer token authenticates all clients — the token IS the authorization credential, not an agent identity mechanism. No per-call `agent_id` required for access. Zero enrollment friction for individual developers.

**Transport**: rmcp 0.16's `transport-streamable-http-server` feature. Tower middleware for auth. No Axum required.

**Auth**: 32-byte OsRng hex token (64 lowercase hex chars). Stored at `{data_volume}/token` with mode 0600. Generated and printed once on first run; loaded silently thereafter. Validated by `subtle::ConstantTimeEq`. Presented as `Authorization: Bearer <token>`.

**Two listeners** (personal cloud uses content port only; admin port reserved for enterprise extension):
- Content port: 8443 (personal cloud)
- Admin port: 8444 (reserved — enterprise extension point)

**TLS**: `rustls 0.23` via `tokio-rustls`. Support `tls.enabled = false` for proxy-terminated deployments.

**Observability** (required for production operation):
- Prometheus metrics endpoint: request count per tool, write queue depth, `shed_events_total`, pool acquire latency, tick completion time, audit log write latency. Without this, operators cannot observe `shed_events_total` except as a WARN log.
- Structured logging: `tracing` spans with `project_id` for log routing.

**Client note**: Active Claude Code bug anthropics/claude-code#28293 (headers in `.mcp.json` not forwarded on tool call POSTs). Workaround: `claude mcp add -H`. Client setup documentation must specify this path.

**Resolved by ASS-041**: rmcp HTTP transport readiness confirmed. rustls, jsonwebtoken, tower middleware selections confirmed. `Authorization: Bearer` header support confirmed for Claude Code HTTP transport.

---

### W2-3: Security Model — OSS Foundation (🔬 ASS-050 — IN PROGRESS)
**Goal**: Correct the security model for the personal cloud tier. Revise the current `agent_id`-per-call model (designed around a now-invalid assumption about subagent session isolation) to match the actual personal cloud identity model. Lay the extension surface that the enterprise private repository will build OAuth 2.1 + three-role RBAC on top of.

**Personal cloud identity model**:
- Bearer token = authorization. Any client presenting the valid token has full access.
- `agent_id` for observation/audit attribution comes from MCP `clientInfo.name`. Not a security mechanism — metadata only.
- `AgentRegistry` and `context_enroll` behavior at this tier: TBD by ASS-050 (hypothesis: permissive default mode, no mandatory enrollment).

**Content size enforcement** (from #561 reframing):
- `context_store` enforces a configurable max byte cap (`[store] max_content_bytes` in `config.toml`, default 8,000).
- Error message includes the configured limit and received size.
- Tool description states a limit exists; does not publish the specific value (revealed only at runtime via error).
- `context_get` naturally bounded by store cap — no separate enforcement.
- `context_status format:json` — documented as corpus-size dependent; risk accepted.

**Extension surface for enterprise** (specified by ASS-050):
- `BearerValidator` trait: OSS ships `StaticTokenAuth`. Enterprise private repo ships `JwtBearerAuth`.
- Startup plugin registration pattern for enterprise auth injection.
- Audit log schema designed now to carry `session_id`, `credential_type`, `capability_used`, `agent_attribution`, and extensible `metadata` JSON for future AI governance attributes — immutable decision, get it right in Wave 2.

**Resolved by ASS-050**: Full implementation audit, interface signatures, audit log schema recommendation, don't-foreclose constraints for future session-pinned identity and behavioral provenance analysis.

---

### W2-4: Multi-LLM Compatibility (🔬 ASS-049 — COMPLETE)
**Goal**: Unimatrix works correctly out-of-the-box with Codex (OpenAI) and Gemini (Google) MCP clients. Same HTTPS transport, same tool API, same behavioral contract. "Works with Claude, Codex, and Gemini" as an empirical claim, not a theoretical one.

**Delivery items** (researched, ready for implementation):

| Issue | Type | Description |
|-------|------|-------------|
| [#558](https://github.com/dug-21/unimatrix/issues/558) | Bug | Tool description fixes — NLI language in `context_briefing`, hook-path framing in `context_cycle` | ✅ COMPLETE |
| [#559](https://github.com/dug-21/unimatrix/issues/559) | Feature | vnc-013: Canonical event normalization — Gemini `BeforeTool`/`AfterTool`/`SessionEnd` → canonical names |
| [#560](https://github.com/dug-21/unimatrix/issues/560) | Feature | Server-side session attribution via `clientInfo.name` + `Mcp-Session-Id` |
| [#561](https://github.com/dug-21/unimatrix/issues/561) | Feature | Byte-based content size enforcement (`context_store` cap, `context_status format:json` documentation) |

**Deferred** (post-Wave 2):
- Provider-neutral eval corpus (20–40 hand-authored scenarios, no harness code changes)
- Gemini MRR baseline (after schema fixes land)
- Zed (revisit when zed-industries/zed#34719 resolves — no native HTTP transport today)

**Critical open issue**: Codex #5619 — Codex sends `protocolVersion: "2025-06-18"` but may expect `2024-11-05` response semantics. Verify rmcp `protocolVersion` declaration before any Codex Wave 2 testing.

**Resolved by ASS-049**: Client capability matrix, tool description risk, `clientInfo.name` attribution, injection size analysis, HTTP auth confirmation per client.

---

### W2-5: GGUF Module — Conditional (🔬 ASS-046)
**Goal**: Optional local GGUF inference behind Cargo feature flag (`features = ["infer"]`). When present: upgrades `context_cycle_review` recommendations, `context_status` explanations, contradiction explanation, background synthesis. SHA-256 hash-pinned model required in config.

**Gate**: ASS-046 must return a go recommendation with proof-of-concept validation. If unfavorable, W2-5 defers to post-Wave 2.

---

## Enterprise Tier

Enterprise delivery — OAuth 2.1, three-role RBAC (Admin/Operator/Auditor), structured compliance audit log, control plane DB, admin console, SOC 2 Type I readiness — is **scoped for a separate private repository** after Wave 2 ships.

Wave 2 delivers the OSS extension surface (W2-3 / ASS-050) that the enterprise private repo builds on. No enterprise features ship from this repository.

---

## Research Prerequisites

| Spike | Title | Status | Feeds |
|-------|-------|--------|-------|
| ASS-041 | Transport + Auth Stack | **COMPLETE** | W2-2 |
| ASS-043 | Container + Packaging Strategy | In progress | W2-1 |
| ASS-045 | Licensing Strategy | **COMPLETE** | W2-0 |
| ASS-046 | GGUF Feasibility | Not started | W2-5 go/no-go |
| ASS-047 | Core Scalability Strategy | **COMPLETE** | W2-2 (connection limits) |
| ASS-049 | Multi-LLM MCP Client Compatibility | **COMPLETE** | W2-4 |
| ASS-050 | Security Model Review — OSS + Enterprise Foundation | **IN PROGRESS** | W2-3 |

### ASS-041 Findings Summary — Transport + Auth Stack
rmcp 0.16 `transport-streamable-http-server` is production-ready. Tower middleware for auth. `rustls 0.23` for TLS. `subtle::ConstantTimeEq` for token validation. `Authorization: Bearer` header confirmed for Claude Code HTTP transport. `claude mcp add -H` workaround required for anthropics/claude-code#28293.

### ASS-045 Findings Summary — Licensing
MIT/Apache 2.0 on all core crates. No BSL (creates OSPO procurement friction). DCO on MIT crates; no CLA. Enterprise commercial features in separate private repository under a named commercial license — not in this codebase.

### ASS-047 Findings Summary — Scalability
Write ceiling: ~200 integrity writes/sec (single `write_pool` connection, SQLite WAL). Defensible at 20 concurrent agents at normal usage. Per-repo in-memory envelope: 3–5 MB (small), 30–50 MB (medium). Personal cloud (single-user) operates well within these limits. PostgreSQL upgrade trigger: >50 agents or >300 audit writes/sec sustained.

### ASS-049 Findings Summary — Multi-LLM Compatibility
Codex CLI and Gemini CLI confirmed as primary Wave 2 targets. `Authorization: Bearer` static token forwarding confirmed for both. Gemini JSON Schema blockers identified (inline `$defs`, union types). Codex #5619 (protocolVersion) requires verification before Codex testing. `clientInfo.name` available as agent attribution source across providers.

---

## Dependency Map

```
ASS-050: Security Model Review ─────────────────────────────► W2-3 (extension surface spec)
ASS-041: Transport ─── COMPLETE ────────────────────────────► W2-2 (HTTPS + static token)
ASS-045: Licensing ─── COMPLETE ────────────────────────────► W2-0 (MIT/Apache confirmed)
ASS-047: Scalability ─ COMPLETE ────────────────────────────► W2-2 (connection limits)
ASS-049: Multi-LLM ─── COMPLETE ────────────────────────────► W2-4 (delivery scope confirmed)

ASS-043 ──────────────────────────────────────────────────── ► W2-1 (packaging decisions)
ASS-046 ──────────────────────────────────────────────────── ► W2-5 go/no-go

W2-3 unblocks: W2-2 delivery (auth middleware placement confirmed)
W2-2 + W2-4 can ship concurrently (shared HTTPS transport layer)
W2-1 wraps W2-2 + W2-3 (container packaging after server complete)
W2-5 independent (feature-flagged, does not block other items)
```

---

## Wave 3 — Unchanged

W3-1 (GNN session-conditioned relevance function) and W3-2 (knowledge synthesis) remain deferred pending:
- ASS-029 architecture spike (not yet started — can begin during Wave 2 delivery)
- Behavioral signals accumulating in production (2–4 weeks active daemon use)
- Groups 9 + 10 signal quality confirmed via live retrospectives

Wave 3 scoping can proceed in parallel with Wave 2 delivery. ASS-029 has no Wave 2 dependencies.
