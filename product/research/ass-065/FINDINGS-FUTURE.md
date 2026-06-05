# FINDINGS: rmcp Future Opportunities Deep-Dive (1.0→1.7)

**Spike**: ass-065 (extension)
**Date**: 2026-05-29
**Approach**: investigation + evaluation
**Confidence**: directional

---

## Part 1 — rmcp 1.5, 1.6, 1.7 Changelog

### rmcp 1.5.0 (2026-04-16)

| Change | PR | Category | Breaking |
|--------|-----|----------|----------|
| Constructors for `#[non_exhaustive]` error types | #806 | API addition | No |
| MCP protocol version 2025-11-25 support | #802 | Protocol | No |
| Soft error on resource metadata JSON parse failure | #810 | Robustness | No |
| `http_request_id` in request-wise priming event IDs | #799 | Observability | No |
| SSE stream drain for HTTP connection reuse | #790 | Performance | No |

### rmcp 1.6.0 (2026-05-01)

| Change | PR | Category | Breaking |
|--------|-----|----------|----------|
| Origin header validation (`allowed_origins` on config) | #823 | Security | No (defaults disabled) |
| Log Host/Origin rejections at appropriate level | #826 | Observability | No |
| Runtime tool disabling (`disable_route`, `enable_route`, etc.) | #809 | API addition | Behavioral: `has_route()` returns false for disabled |
| Session persistence (`SessionStore` trait) | #775 | API addition | No (opt-in) |
| `init_timeout` for sessions (default 60s) | #811 | Security | No |
| HTTP/2 `:authority` fallback for Host validation | #827 | Robustness | No |

### rmcp 1.7.0 (2026-05-13)

| Change | PR | Category | Breaking |
|--------|-----|----------|----------|
| Reply -32700 on stdio parse errors instead of closing | #833 | Robustness | No |
| Fix: flatten Resource variant of PromptMessageContent | #843 | Bug fix | Wire-format |
| Idle timeout logged at DEBUG instead of ERROR | #824 | Observability | No |
| Remove chrono default features dependency | #829 | Maintenance | No |

**Zero additional breaking changes beyond 1.4.0. Upgrade to 1.7.0, not 1.4.0.**

---

## Part 2 — Deep Assessment of ALL Opportunities

### Opportunity 1: Trait-Based Tool Declaration (v0.17.0+)

**What it is**: Tools as standalone structs implementing `SyncTool` or `AsyncTool` traits, composed into `ToolRouter` via `with_sync_tool::<T>()`. Each tool is self-contained with its own input type, description, and handler.

**What it enables for Unimatrix**: `tools.rs` is ~12K+ lines with 14 handlers in one `#[tool_router]` impl block — far exceeds the 500-line convention. Trait-based declaration enables splitting into `tools/search.rs`, `tools/store.rs`, `tools/graph.rs`, etc. Parameter structs already exist as standalone types mapping to `SyncTool::Input`.

**Vision alignment**: High. Enables domain-specific tool packs that register without modifying core router. A research domain could add `research_search` alongside `context_search` via `router.with_async_tool::<ResearchSearchTool>()`.

**Complexity**: 2-4 days. Each handler extracted to its own struct holding `Arc<UnimatrixServer>` or a purpose-built `ToolContext`. `RequestContext<RoleServer>` threading needs verification.

**Risk**: Medium. Changes how tool metadata is generated. Test coverage for all 14 tools must be re-validated.

**Recommendation**: **Strategic investment** (1-2 weeks after migration). Single highest-value codebase health refactor. Do not attempt during base migration.

---

### Opportunity 2: Auto-Generated `get_info` (v1.4.0+)

**What it is**: `#[tool_handler]` macro auto-generates `get_info()` from `Cargo.toml` metadata when not explicitly provided.

**Why skip**: Our manual `get_info()` uses custom `name` ("unimatrix" not "unimatrix-server"), runtime-configurable `instructions`, and cached construction. Macro requires compile-time string literals for instructions, contradicting config externalization design (dsn-001). Macro detects existing method and does not conflict.

**Recommendation**: **Skip.**

---

### Opportunity 3: `IntoCallToolResult` (v1.4.0)

**What it is**: Tool handlers can return any type implementing `IntoCallToolResult` — blanket impls for `String`, `Vec<Content>`, `ErrorData`, `Result<T, E>`.

**Current state**: ~40+ `.map_err(rmcp::ErrorData::from)?` sites. Gain is marginal because: (1) `From<ServerError> for ErrorData` already makes `?` work, (2) handlers construct `CallToolResult::success(vec![Content::text(...)])` — not simple `String` returns.

**Recommendation**: **Evaluate later.** Revisit when trait-based tools (Opp 1) are adopted — at that point each tool's return type is individually defined.

---

### Opportunity 4: OAuth 2.0 Client Credentials (v1.1.0)

**Vision alignment**: Wave 2 roadmap (W2-3) explicitly scopes OAuth as enterprise extension, built in private repo on top of OSS `StaticTokenAuth` seam.

**Recommendation**: **Watch list.** Enterprise-tier scope.

---

### Opportunity 5: UDS Client Transport (v1.3.0)

**Recommendation**: **Watch list.** No MCP client use case currently.

---

### Opportunity 6: Transparent Session Re-Init (v1.3.0)

**Recommendation**: **Watch list.** Relevant only when Unimatrix acts as MCP client.

---

### Opportunity 7: `json_response` Mode (v0.17.0)

**What it is**: `StreamableHttpServerConfig { json_response: true }` returns plain JSON instead of SSE for non-streaming responses.

**What it enables**: All 14 tools are synchronous request-response. JSON mode eliminates SSE framing overhead, improves curl debugging, simplifies HTTP client compatibility.

**Constraint**: Only takes effect when `stateful_mode: false`. Current defaults set `stateful_mode: true` (sessions enabled). Interaction with `client_type_map` (keyed on `Mcp-Session-Id`) must be verified.

**Recommendation**: **Fast follow-on** (0.5 day). Evaluate `stateful_mode` interaction. Add config.toml knob if viable.

---

### Opportunity 8: `local` Feature for `!Send` Handlers (v1.3.0)

**Recommendation**: **Skip.** `RayonPool` + `spawn_blocking` pattern already solves `!Send` model sessions. Changing Send/Sync bounds would be a fundamental architectural change.

---

### Opportunity 9: Runtime Tool Disabling (v1.6.0) — NEW

**What it is**: `ToolRouter` gains `disable_route(name)`, `enable_route(name)`, `is_disabled(name)`, `with_disabled(name)`. Disabled tools hidden from listing, lookup, execution. Automatic `notifications/tools/list_changed` sent to clients.

**What it enables for Unimatrix**:
- Disable `context_store`/`context_correct` in read-only mode (maintenance/backup)
- Disable `context_cycle`/`context_cycle_review` for agents without Privileged capability
- Config-driven tool visibility per deployment domain
- Reduced prompt noise — invisible tools don't appear in `tools/list`

Currently, tools always visible regardless of caller capabilities. Denial happens at invocation time with error. Runtime disabling prevents wasted tool calls entirely.

**Vision alignment**: High for `personal-cloud` (clean tool presentation) and `domain-agnostic` (domain-specific tool sets).

**Complexity**: Low for startup-time `with_disabled`. Runtime disabling needs ownership adjustment (`Arc<Mutex<ToolRouter>>` or similar).

**Recommendation**: **Fast follow-on** (1 day). Start with config-driven startup-time disabling.

---

### Opportunity 10: Session Persistence / Resumability (v1.6.0) — NEW

**What it is**: `SessionStore` trait with `load`/`store`/`delete` methods. Plugs into `StreamableHttpServerConfig`. Enables session state to survive server restarts and load balancer routing.

**What it enables for Unimatrix**:
1. **Horizontal scaling**: Multiple instances behind load balancer share sessions via Redis/SQLite
2. **Graceful restarts**: Daemon restart doesn't force client re-initialization
3. **Container orchestration**: K8s pod restarts / rolling deploys don't break active sessions

**Vision alignment**: High for `personal-cloud`. Container deployment currently loses all MCP sessions on restart.

**Complexity**: Moderate. Implement `SessionStore` backed by SQLite (new table: `mcp_sessions`), wire into config, handle expired session cleanup.

**Recommendation**: **Strategic investment** (3-5 days). Critical enabler for container deployments. Plan as dedicated feature.

---

### Opportunity 11: Origin Header Validation (v1.6.0) — NEW

**What it is**: `allowed_origins: Vec<String>` on config. Complements Host header validation from v1.4.0.

**Recommendation**: **Adopt during migration** (15 min). Add config.toml knob while touching `router.rs`.

---

### Opportunity 12: Init Timeout Protection (v1.6.0) — NEW

**What it is**: `init_timeout: Option<Duration>` defaults to 60s. Terminates sessions that don't send `initialize` in time. Prevents zombie session resource exhaustion.

**Recommendation**: **Adopt during migration** (automatic with version bump).

---

### Opportunity 13: Protocol Version 2025-11-25 (v1.5.0) — NEW

**What it is**: Current MCP spec. Claude Desktop/Code negotiate 2025-11-25; advertising older version may trigger compatibility warnings.

**Recommendation**: **Adopt during migration** (automatic).

---

### Opportunity 14: Stdio Parse Resilience (v1.7.0) — NEW

**What it is**: Reply -32700 on malformed input instead of silently closing connection. UTF-8 BOM stripping for Windows.

**What it enables**: Bridge mode (stdio primary transport) no longer drops entire session on a single bad byte.

**Recommendation**: **Adopt during migration** (automatic).

---

### Opportunity 15: Idle Timeout Log Noise Reduction (v1.7.0) — NEW

**What it is**: Session idle timeouts at DEBUG instead of ERROR. Dedicated `WorkerQuitReason::IdleTimeout`.

**Recommendation**: **Adopt during migration** (automatic).

---

### Opportunity 16: SSE Connection Reuse (v1.5.0) — NEW

**What it is**: SSE streams properly drained on completion, enabling HTTP connection reuse.

**Recommendation**: **Adopt during migration** (automatic).

---

### Opportunity 17: Elicitation Support (v1.5.0 protocol) — NEW

**What it is**: Server can request interactive input from user during tool execution. Types: `ElicitationAction`, `CreateElicitationRequestParams`, `ElicitationSchema`.

**What it enables**: Interactive knowledge curation. `context_store` could ask confirmation before storing duplicates. `context_cycle` could present entries for review.

**Vision alignment**: High for `self-learning` and `proactive-delivery`.

**Risk**: High. Client support not guaranteed. Must verify Claude Code elicitation support first.

**Recommendation**: **Watch list.** Verify client support before investing.

---

### Opportunity 18: Task Capabilities / Long-Running Ops (v1.5.0 protocol) — NEW

**What it is**: Task capabilities for long-running tool execution with progress reporting.

**Recommendation**: **Watch list.** No operations >10s currently.

---

### Opportunity 19: Extension Capabilities (v1.5.0 protocol) — NEW

**What it is**: `ServerCapabilities.extensions` for vendor-prefixed capability advertisement (e.g., `io.unimatrix/knowledge-graph`).

**Recommendation**: **Evaluate later.** No client consumes custom extensions yet.

---

### Opportunity 20: `Implementation` Struct Enrichment (v1.0.0+) — NEW

**What it is**: `Implementation` gained `title`, `description`, `icons`, `website_url` fields. `Implementation::new(name, version)` constructor.

**What it enables**: Richer metadata in MCP handshake. Claude Desktop could display Unimatrix description/icon.

**Recommendation**: **Adopt during migration** (10 min). Already rewriting struct literal. Add `.with_description("Self-learning knowledge engine for agentic workflows")`.

---

### Opportunity 21: Error Type Constructors (v1.5.0) — NEW

**What it is**: Public constructors for `#[non_exhaustive]` error types for test construction.

**Recommendation**: **Adopt during migration** (automatic).

---

## Part 3 — Architecture Cross-Reference

### MCP Tool Structure Impact

| File | rmcp Surface | Opportunity Impact |
|------|-------------|-------------------|
| `tools.rs` (~12K lines) | `#[tool_router]`, `#[tool]`, `Parameters<T>`, `CallToolResult`, `ErrorData` | **Opp 1** (trait-based tools): primary refactor target. **Opp 3** (IntoCallToolResult): return type simplification |
| `server.rs` | `ServerHandler`, `ServerInfo`, `Implementation`, `#[tool_handler]` | **Opp 20** (Implementation enrichment): adopt during migration |
| `graph_read*.rs` (6 files) | `ErrorData` | No opportunity impact |
| `response/*.rs` (5 files) | `CallToolResult`, `Content` | **Opp 3**: could simplify if adopted later |

### Transport Architecture Impact

| Component | Opportunity Impact |
|-----------|-------------------|
| `http/router.rs` (McpAdapter) | **Opp 7** (json_response), **Opp 10** (session store), **Opp 11** (origin validation), **Opp 12** (init timeout) |
| `uds/mcp_listener.rs` | No opportunity impact |
| `main.rs` (stdio) | **Opp 14** (stdio resilience): automatic |

### Vision Alignment Map

| Vision Goal | Aligned Opportunities |
|-------------|----------------------|
| Self-learning intelligence | Opp 17 (elicitation), Opp 18 (task progress) |
| Proactive delivery | Opp 17 (elicitation for interaction), Opp 19 (extensions) |
| Developer-friendly deployment | Opp 7, 10, 11, 12, 14, 15, 16, 20 |
| Domain-agnostic platform | Opp 1 (pluggable tool packs), Opp 9 (runtime disabling), Opp 19 |

---

## Part 4 — Prioritized Roadmap

### Tier 1: Adopt During Migration (zero/minimal marginal effort)

**Automatic with version bump (zero effort):**

| # | Opportunity | Value |
|---|------------|-------|
| 13 | Protocol Version 2025-11-25 | MCP spec compliance |
| 14 | Stdio Parse Resilience | Production robustness |
| 15 | Idle Timeout Log Reduction | Clean observability |
| 16 | SSE Connection Reuse | HTTP performance |
| 12 | Init Timeout Protection | Security hardening |
| 21 | Error Type Constructors | Test infrastructure |

**Minimal source changes in already-modified files:**

| # | Opportunity | Effort | Value |
|---|------------|--------|-------|
| 20 | Implementation Enrichment | 10 min | Richer server metadata |
| 11 | Origin Header Validation | 15 min | Defense-in-depth security |

### Tier 2: Fast Follow-On (1-2 days, clear value)

| # | Opportunity | Effort | Value |
|---|------------|--------|-------|
| 9 | Runtime Tool Disabling | 1 day | Config-driven tool visibility, reduced prompt noise |
| 7 | `json_response` Mode | 0.5 day | Evaluate viability, add config knob |

### Tier 3: Strategic Investment (larger effort, architectural improvement)

| # | Opportunity | Effort | Value |
|---|------------|--------|-------|
| 1 | Trait-Based Tool Declaration | 1-2 weeks | Codebase health, domain-agnostic tool packs |
| 10 | Session Persistence | 3-5 days | Container restart resilience, horizontal scaling |

### Tier 4: Watch List (not yet actionable)

| # | Opportunity | Gating Condition |
|---|------------|-----------------|
| 17 | Elicitation | Verify Claude Code client support |
| 18 | Task Capabilities | Need operations >10s |
| 19 | Extension Capabilities | Need client that consumes them |
| 4 | OAuth 2.0 | Enterprise repo scope |
| 5 | UDS Client | Need federated knowledge use case |
| 6 | Session Re-Init | Need MCP client use case |
| 3 | IntoCallToolResult | Revisit with trait-based tools |
| 2 | Auto-Generated get_info | Config-driven instructions require manual override |
| 8 | `local` Feature | RayonPool already solves this |
