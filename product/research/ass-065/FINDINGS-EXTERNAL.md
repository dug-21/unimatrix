# FINDINGS: rmcp 0.16 to 1.4 Migration — External Ecosystem Investigation

**Spike**: ass-065
**Track**: External
**Date**: 2026-05-29
**Approach**: investigation + evaluation
**Confidence**: validated

---

## Findings

### Q2: What breaking changes occurred in rmcp between 0.16.0 and 1.4.0?

**Answer**: 87 commits span the 0.16.0 to 1.4.0 range across 8 releases (0.17.0, 1.0.0-alpha, 1.0.0, 1.1.0, 1.1.1, 1.2.0, 1.3.0, 1.4.0). The breaking changes fall into five categories: API signature changes, `#[non_exhaustive]` additions, behavioral changes, type/trait bound changes, and new default values.

**Evidence**: GitHub releases API, PR bodies, and source code comparison between `rmcp-v0.16.0` and `rmcp-v1.4.0` tags at https://github.com/modelcontextprotocol/rust-sdk.

---

#### Category 1: API Signature Changes (Breaking)

**1.1 StreamableHttpClient trait — `get_stream` and `delete_session` gained `custom_headers` parameter (PR #675, v0.17.0)**

Hard breaking change for anyone implementing `StreamableHttpClient` directly.

Before (0.16.0):
```rust
fn get_stream(&self, uri: Arc<str>, session_id: Arc<str>, last_event_id: Option<String>, auth_header: Option<String>) -> impl Future<...>;
fn delete_session(&self, uri: Arc<str>, session_id: Arc<str>, auth_header: Option<String>) -> impl Future<...>;
```

After (0.17.0+):
```rust
fn get_stream(&self, uri: Arc<str>, session_id: Arc<str>, last_event_id: Option<String>, auth_header: Option<String>, custom_headers: HashMap<HeaderName, HeaderValue>) -> impl Future<...>;
fn delete_session(&self, uri: Arc<str>, session_id: Arc<str>, auth_header: Option<String>, custom_headers: HashMap<HeaderName, HeaderValue>) -> impl Future<...>;
```

Impact: Only affects code implementing `StreamableHttpClient` trait. Built-in reqwest client is unchanged.

**1.2 Auth token exchange return type changed (PR #700, v1.0.0-alpha)**

`exchange_code_for_token` and `refresh_token` now return `StandardTokenResponse`. Only affects `auth` feature users.

**1.3 Builder `with_*` methods now take `T` instead of `Option<T>` (PR #720, v1.0.0)**

Before: `.with_title(Some("my tool".to_string()))`
After: `.with_title("my tool")`

Also: `with_logger` and `with_content` changed from standalone constructors to builder setters.

**1.4 StreamableHttpServerConfig gained `allowed_hosts` field (PR #764, v1.4.0)**

```rust
pub struct StreamableHttpServerConfig {
    pub sse_keep_alive: Option<Duration>,
    pub sse_retry: Option<Duration>,
    pub stateful_mode: bool,
    pub json_response: bool,             // NEW in 0.17.0
    pub cancellation_token: CancellationToken,
    pub allowed_hosts: Vec<String>,      // NEW in 1.4.0
}
```

Default `allowed_hosts`: `["localhost", "127.0.0.1", "::1"]`. New builder methods: `with_allowed_hosts()` and `disable_allowed_hosts()`.

**1.5 StreamableHttpServerConfig gained `json_response` field (PR #683, v0.17.0)**

New field `pub json_response: bool` (defaults to `false`).

**1.6 StreamableHttpService lost default type parameter for M (PR #758, v1.3.0)**

`StreamableHttpService<S, M>` previously defaulted `M` to `LocalSessionManager`. Default removed.

---

#### Category 2: `#[non_exhaustive]` Additions (Breaking for struct-literal construction)

PR #715 (v1.0.0-alpha) added `#[non_exhaustive]` to most public structs and enums. **Single most impactful change** — any struct-literal construction will no longer compile.

Types confirmed `#[non_exhaustive]`:

**Model types:**
- `Request<M, P>`, `Notification<M, P>`
- `InitializeRequestParams` (= `ClientInfo`)
- `InitializeResult` (= `ServerInfo`)
- `Icon` (also gained `theme: Option<IconTheme>` field)
- `Implementation`, `ServerCapabilities`, `ClientCapabilities`
- `Tool`, `ToolAnnotations`, `ToolUseContent`, `ToolResultContent`

**Transport types:**
- `LocalSessionManager`
- `ServerSseMessage` (also gained `Default`, `new()`, `priming()` constructors in PR #794, v1.4.0)

PR #739 (v1.2.0) added missing constructors for types made unconstructable by `#[non_exhaustive]` (`Root`, `ListRootsResult`, `UnsubscribeRequestParams`, `PromptReference`).

---

#### Category 3: Behavioral Changes (Potentially Breaking)

**3.1 Initialized notification gate removed (PR #788, v1.4.0)**

Server no longer requires `notifications/initialized` before accepting requests. `ExpectedInitializedNotification` error variant removed.

**3.2 Default session keep_alive changed from None to 5 minutes (PR #780, v1.4.0)**

Sessions inactive for 5 minutes auto-cleanup. Override with `keep_alive: None`.

**3.3 MCP-Protocol-Version header validation on server (PR #675, v0.17.0)**

Server validates `MCP-Protocol-Version` headers, rejects unsupported values with HTTP 400. Requests without header accepted for backwards compatibility.

**3.4 DNS rebinding Host header validation enabled by default (PR #764, v1.4.0)**

Server validates `Host` headers against allowlist defaulting to `["localhost", "127.0.0.1", "::1"]`. Non-matching → HTTP 403. **Critical for deployment** if behind reverse proxy with different Host header.

---

#### Category 4: Trait Bound Changes

**4.1 Service and ServerHandler — cfg-gated Send+Sync (PRs #740, #757, v1.3.0)**

New `local` feature relaxes `Send + Sync` bounds. Without it (default), bounds identical to 0.16.0. Non-breaking for default feature set.

---

#### Category 5: New Features (Non-Breaking but Relevant)

- **Trait-based tool declaration** (PR #677, v0.17.0) — New `Tool` trait for organizing tools in separate modules
- **Auto-generated get_info and default router** (PR #785, v1.4.0) — `#[tool_handler]` and `#[tool_router]` macros can auto-generate `get_info()`
- **Transparent session re-init configuration** (PR #760, v1.3.0) — New `enable_reinit_on_expired_session` on client config
- **Unix domain socket client** (PR #749, v1.3.0) — New transport behind `transport-streamable-http-client-unix-socket` feature
- **OAuth 2.0 Client Credentials flow** (PR #707, v1.1.0) — New auth flow behind `auth` feature
- **IntoCallToolResult unified** (PR #787, v1.4.0) — Wider set of error types accepted in tool handlers

---

#### Release-by-Release Summary

| Version | Date | Breaking Changes | Key Non-Breaking Changes |
|---------|------|-----------------|------------------------|
| 0.17.0 | 2026-02-27 | `StreamableHttpClient` trait params; `json_response` field | Trait-based tools, MCP-Protocol-Version |
| 1.0.0-alpha | 2026-03-03 | Auth return type; `#[non_exhaustive]` on ~14 types | Builder/mutation methods |
| 1.0.0 | 2026-03-03 | `with_*` methods take `T` not `Option<T>` | Stale session 401 mapping |
| 1.1.0 | 2026-03-04 | None | OAuth Client Credentials |
| 1.1.1 | 2026-03-09 | None | Pre-init logging/ping |
| 1.2.0 | 2026-03-11 | None | Missing constructors, jsonwebtoken 9→10 |
| 1.3.0 | 2026-03-24 | `StreamableHttpService` lost default type param for M | Local feature, UDS client, transparent re-init |
| 1.4.0 | 2026-04-09 | `allowed_hosts` (DNS rebinding); `ExpectedInitializedNotification` removed; keep_alive default 5min | Auto-generated get_info, ServerSseMessage constructors |

---

### Q6: Can the DNS rebinding fix (CVE-2026-42559) be cherry-picked or backported to 0.16.x without the full migration?

**Answer**: A manual backport is **technically feasible but practically inadvisable**. The fix is contained in 3 files with ~279 lines added and only 2 lines removed. However, no 0.16.x patch release exists or is planned, and backporting creates an unmaintainable fork.

**Evidence**:

#### Fix Details

- **CVE**: CVE-2026-42559 (GHSA-89vp-x53w-74fx)
- **Severity**: High (CVSS 8.8)
- **Published**: 2026-04-29
- **Fix PR**: #764, merged 2026-04-01
- **Fix commit**: `8e22aa2de28df5a285eed87c11cd89bf15fa90d3`
- **Files changed**: 3 files
  1. `crates/rmcp/src/transport/common/http_header.rs` — minor annotation (2 lines)
  2. `crates/rmcp/src/transport/streamable_http_server/tower.rs` — host validation logic (121 lines added)
  3. `crates/rmcp/tests/test_custom_headers.rs` — integration tests (158 lines added)

#### What the Fix Does

Host header validation in `StreamableHttpService::handle()`:
1. `validate_dns_rebinding_headers()` parses incoming `Host` header
2. Checks against configurable allowlist (default: `["localhost", "127.0.0.1", "::1"]`)
3. Returns HTTP 403 for non-matching hosts
4. New config methods: `with_allowed_hosts()` and `disable_allowed_hosts()`

#### Dependencies on Intervening Changes

The fix modifies `tower.rs` and depends on:
1. `StreamableHttpServerConfig` struct layout — fix adds `allowed_hosts: Vec<String>`. Straightforward addition to 0.16.0's 4-field version.
2. `handle()` method — fix inserts validation call at top. Method changed between versions but insertion point is adjustable.
3. Helper functions — `validate_dns_rebinding_headers`, `parse_host_header`, `host_is_allowed`, `normalize_authority`, `normalize_host` — self-contained, no post-0.16.0 type dependencies.

#### Why Backport is Inadvisable

1. **No 0.16.x branch exists upstream.** No maintenance branches for pre-1.0 versions.
2. **No community backport precedent.** GitHub issue #815 confirms fix only in v1.4.0+. Maintainers explicitly declined to backport a different fix in 0.14→0.15.
3. **Creates a maintenance fork.** Future CVEs require re-patching against a diverging codebase.
4. **Gap widens continuously.** Current latest is 1.7.0 (2026-05-29).
5. **Alternative mitigation exists.** Reverse proxy with Host header validation mitigates DNS rebinding at infra layer.

#### Risk Comparison

| Risk Factor | Manual Backport | Full Upgrade to 1.4.0+ |
|-------------|----------------|----------------------|
| CVE-2026-42559 | Addressed if correct | Addressed |
| Future CVEs | Not covered | Covered by semver updates |
| Regression risk | Low (additive) | Moderate (87 commits) |
| Testing confidence | Low (custom fork) | High (upstream CI) |
| Long-term maintenance | High ongoing | One-time migration |
| Time to deploy | ~1 day | ~3-5 days |

**Recommendation**: Do not backport. Upgrade to 1.4.0 (or latest 1.7.0). Deploy Host header validation at reverse proxy as interim mitigation if needed before migration completes.

---

## Unanswered Questions

1. **Exact `ServerInitializeError` enum variants diff 0.16.0 vs 1.4.0**: `ExpectedInitializedNotification` removed; full variant list not enumerated.
2. **Whether `Icon` gained additional fields beyond `theme`**: Confirmed `theme: Option<IconTheme>`, other fields may exist.
3. **Complete `#[non_exhaustive]` additions**: ~14 types confirmed. Some submodule files (`prompt.rs`, `task.rs`, `elicitation_schema.rs`) not individually inspected.

---

## Out-of-Scope Discoveries

1. **rmcp is now at 1.7.0** (2026-05-29). Upgrading to 1.7.0 instead of 1.4.0 picks up 3 additional releases. Worth evaluating.
2. **MCP protocol 2025-06-18 compliance**: v0.17.0 conformance work aligns with latest MCP spec. Staying on 0.16.0 means non-conformance.
3. **jsonwebtoken dependency bump 9→10 in v1.2.0** (PR #737): Potential version conflicts if used elsewhere.
4. **Rust toolchain 1.92 required**: v1.4.0 updated to Rust 1.92. Toolchain compatibility required.
5. **`local` feature for `!Send` handlers**: New in 1.3.0. May be relevant for future architecture.
