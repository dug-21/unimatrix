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

Named volume *(updated to reflect nan-014 shipped design)*:
- `unimatrix-data` — databases, vector indexes, config, and logs (integrity-critical, back up frequently). ONNX models baked into image.

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

### W2-3: Security Model — OSS Foundation (🔬 ASS-050 — COMPLETE)
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

**Resolved by ASS-050**: Full implementation audit confirms no reconstruction needed. `BearerValidator` + `StaticTokenAuth` trait signatures specified. `audit_log` schema migration (4 new columns + append-only triggers) designed and classified. Seam map identifies 5 injectable identity seams — Seams 1 and 2 are low-cost additive now, high-cost retrofit later. Don't-foreclose list (7 invariants) documented as code review gates. OQ-01 and OQ-03 resolved via rmcp 0.16 source read: `clientInfo.name` accessible via `ctx.peer.peer_info()` (not extensions); rmcp session UUID accessible via `extensions.get::<Parts>()` + `Mcp-Session-Id` header — server-assigned, non-spoofable, no upstream changes needed. vnc-014 is fully unblocked.

---

### W2-4: Multi-LLM Compatibility (🔬 ASS-049 — COMPLETE)
**Goal**: Unimatrix works correctly out-of-the-box with Codex (OpenAI) and Gemini (Google) MCP clients. Same HTTPS transport, same tool API, same behavioral contract. "Works with Claude, Codex, and Gemini" as an empirical claim, not a theoretical one.

**Delivery items** (researched, ready for implementation):

| Issue | Type | Description |
|-------|------|-------------|
| [#558](https://github.com/dug-21/unimatrix/issues/558) | Bug | Tool description fixes — NLI language in `context_briefing`, hook-path framing in `context_cycle` | ✅ COMPLETE |
| [#559](https://github.com/dug-21/unimatrix/issues/559) | Feature | vnc-013: Canonical event normalization — Gemini `BeforeTool`/`AfterTool`/`SessionEnd` → canonical names | ✅ COMPLETE (ASS-051: keep Claude Code names) |
| [#560](https://github.com/dug-21/unimatrix/issues/560) | Feature | Server-side session attribution via `clientInfo.name` + `Mcp-Session-Id` |
| [#561](https://github.com/dug-21/unimatrix/issues/561) | Feature | Byte-based content size enforcement (`context_store` cap, `context_status format:json` documentation) | ✅ COMPLETE |
| [#574](https://github.com/dug-21/unimatrix/issues/574) | Bug | `context_cycle` must write `cycle_events` + `sessions.feature_cycle` via MCP handler, not hook path — prerequisite for Codex/Gemini behavioral provenance |

**Deferred** (post-Wave 2):
- Provider-neutral eval corpus (20–40 hand-authored scenarios, no harness code changes)
- Gemini MRR baseline (after schema fixes land)
- Zed (revisit when zed-industries/zed#34719 resolves — no native HTTP transport today)

**Codex #5619 — RESOLVED (not a Unimatrix issue)**: rmcp 0.16.0 explicitly pins `LATEST = 2025-03-26` (`ProtocolVersion::LATEST` in `model.rs`). The negotiation logic responds with `min(client, server)` — so Codex's `2025-06-18` request receives a `2025-03-26` response. The #5619 bug is specific to a `2025-06-18 → 2024-11-05` response mismatch; Unimatrix sidesteps it entirely. Remaining Codex blocker: #16732 (MCP tool call hooks don't fire — upstream).

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
| ASS-050 | Security Model Review — OSS + Enterprise Foundation | **COMPLETE** | W2-3 |
| ASS-051 | Hook Event Canonical Naming Strategy | **COMPLETE** | vnc-013 (#559) — keep Claude Code names as canonical |
| ASS-052 | RuVector Component Re-evaluation | **COMPLETE** | W3-1 (negative result — no adoption) |
| ASS-053 | REST API Connectivity + Admin Plane Decoupling Seams | Not started | W2-3, enterprise |
| ASS-055 | ADR DependsOn Graph Relationship (`context_relate`) | **COMPLETE** | crt-NNN (Wave 2), enterprise audit graph |

### ASS-041 Findings Summary — Transport + Auth Stack
rmcp 0.16 `transport-streamable-http-server` is production-ready. Tower middleware for auth. `rustls 0.23` for TLS. `subtle::ConstantTimeEq` for token validation. `Authorization: Bearer` header confirmed for Claude Code HTTP transport. `claude mcp add -H` workaround required for anthropics/claude-code#28293.

### ASS-045 Findings Summary — Licensing
MIT/Apache 2.0 on all core crates. No BSL (creates OSPO procurement friction). DCO on MIT crates; no CLA. Enterprise commercial features in separate private repository under a named commercial license — not in this codebase.

### ASS-047 Findings Summary — Scalability
Write ceiling: ~200 integrity writes/sec (single `write_pool` connection, SQLite WAL). Defensible at 20 concurrent agents at normal usage. Per-repo in-memory envelope: 3–5 MB (small), 30–50 MB (medium). Personal cloud (single-user) operates well within these limits. PostgreSQL upgrade trigger: >50 agents or >300 audit writes/sec sustained.

### ASS-050 Findings Summary — Security Model Review
Current `agent_id`-per-call model is security-through-obscurity (STDIO) and no security at all (HTTPS). Path forward does not require reconstruction — three changes: (1) additive `BearerValidator` tower middleware with `StaticTokenAuth` OSS impl, (2) one schema migration adding 4 fields to `audit_log` (`credential_type`, `capability_used`, `agent_attribution`, `metadata`) plus append-only DDL triggers, (3) non-breaking `build_context_with_external_identity()` overload in `server.rs`. `agent_id` tool param is reclassified as persona metadata; `AgentRegistry` retained for attribution analytics. Zero-enrollment-friction confirmed: bearer token = full access, no `context_enroll` required. Audit log migration is a one-way decision — must land before any Wave 2 auth feature. Enterprise extension surface (`BearerValidator` trait, `EnterpriseAuditWriter` trait) fully specified; enterprise binary swaps in `JwtBearerAuth` without touching OSS code. Seven behavioral provenance invariants documented as code review gates.

**CORRECTION (2026-04-22)**: ASS-050 Seam 5 analysis contained an error. `cycle_events.cycle_id` is the feature identifier (`topic` / `feature_cycle`, e.g. `"crt-027"`) — not the MCP session ID. The behavioral provenance chain is two hops: `audit_log.session_id → sessions.session_id → sessions.feature_cycle = cycle_events.cycle_id`. The `audit_log.session_id` fix must use agent-declared session_id (from `ToolContext.audit_ctx.session_id`), not the rmcp UUID. The rmcp UUID is the `client_type_map` key (vnc-014). Additionally, `context_cycle` MCP handler does not write to `cycle_events` or `sessions.feature_cycle` — both are hook-path only, breaking provenance for non-Claude-Code clients. Issue #574 tracks the fix. ASS-050 FINDINGS.md corrected in place.

### ASS-049 Findings Summary — Multi-LLM Compatibility
Codex CLI and Gemini CLI confirmed as primary Wave 2 targets. `Authorization: Bearer` static token forwarding confirmed for both. Gemini JSON Schema blockers identified (inline `$defs`, union types). Codex #5619 (protocolVersion) **closed** — rmcp 0.16.0 declares `2025-03-26`, sidesteps the bug. Sole remaining Codex blocker: upstream #16732 (MCP tool call hooks). `clientInfo.name` available as agent attribution source across providers.

### ASS-055 Findings Summary — ADR DependsOn Graph Relationship
Retain `Prerequisite` RelationType (no rename). Store A→B (A is prerequisite of B). PPR reverse-walk already correctly surfaces A when B is seeded — confirmed by existing tests `test_prerequisite_incoming_direction` and `test_prerequisite_wrong_direction_does_not_propagate`. graph_expand gap accepted (PPR compensates). Write path: add `depends_on: Option<Vec<u64>>` to both `context_store` and `context_correct` — no new MCP tool (stays at 12), GRAPH_EDGES-only, no schema migration (stays at v25). Reference implementation: `write_graph_edge` in `nli_detection.rs:78–118`. No edge auto-transfer on supersession — add `stale_dependency_edges` count to `context_status` and a `DependencyOnDeprecated` detection rule to `context_cycle_review`. Security: Write cap + source-entry ownership validation (caller must match `created_by` of source_id) + confidence floor on source. Blast radius: ~4 files changed, ~7 benefit for free, 6 new tests. Effort: 2–3 days. **Go, Wave 2.** Dependency is the only named relationship in the vision not yet modeled.

### ASS-051 Findings Summary — Hook Event Canonical Naming Strategy
Keep Claude Code names (`PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `Stop`) as canonical. Switching to neutral names triggers a mandatory `observations` table row-rewrite migration on production data with no behavioral benefit — Gemini CLI maps trivially to existing names via normalization. Blast radius audit: 3 definition sites in `observation.rs`, 14 match-arm references, 6 test fixtures, plus stored `observations.hook` column values. Decision: normalize at ingest boundary (vnc-013), not at storage layer.

### ASS-052 Findings Summary — RuVector Re-evaluation
All four evaluated mechanisms rejected: GNN integration (same PPR feedback loop as ASS-031, no HNSW wiring path), graph-based retrieval (Unimatrix PPR expander strictly surpasses ruvector-graph), HNSW replacement (same underlying library; Unimatrix VectorIndex superior), EWC (uncertain applicability — folded into W3-1 training harness comparison scope). No ruvector components adopted. W3-1 proceeds on existing Unimatrix graph and HNSW foundations.

---

## Dependency Map

```
ASS-053: REST API + Admin Seams ─ depends on ASS-050 ────────► W2-3 (REST path + admin decoupling)
ASS-051: Hook Event Naming ─── COMPLETE ─────────────────────► vnc-013 (#559) — DELIVERED
ASS-050: Security Model Review ─────────────────────────────► W2-3 (extension surface spec)
ASS-041: Transport ─── COMPLETE ────────────────────────────► W2-2 (HTTPS + static token)
ASS-045: Licensing ─── COMPLETE ────────────────────────────► W2-0 (MIT/Apache confirmed)
ASS-047: Scalability ─ COMPLETE ────────────────────────────► W2-2 (connection limits)
ASS-049: Multi-LLM ─── COMPLETE ────────────────────────────► W2-4 (delivery scope confirmed)
ASS-052: RuVector ──── COMPLETE (negative) ──────────────────► W3-1 (no adoption)

ASS-043 ──────────────────────────────────────────────────── ► W2-1 (packaging decisions)
ASS-046 ──────────────────────────────────────────────────── ► W2-5 go/no-go

vnc-013 (#559) DELIVERED: ASS-051 resolved (keep Claude Code names), #559 closed
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
