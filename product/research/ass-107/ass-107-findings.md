# FINDINGS: MCP 2026-07-28 / rmcp 3.x adoption revisit

**Spike**: ass-107 (GH #1019)
**Date**: 2026-11-25
**Approach**: investigation (no PoC)
**Confidence**: directional

**Overall recommendation: DEFER the stateless wire. GO on a decoupled rmcp 1.7 → 3.x upgrade, with the server still serving only `2025-11-25` (legacy session mode), scheduled after vnc-050 lands.** The revisit trigger for enabling `2026-07-28` serving: Claude Code ships GA support for `2026-07-28`, **and** a stateless replacement for vnc-014 attribution has been designed.

Sources: [rmcp 3.0 migration guide (Discussion #969)](https://github.com/modelcontextprotocol/rust-sdk/discussions/969), [rmcp-v3.0.0 release](https://newreleases.io/project/github/modelcontextprotocol/rust-sdk/release/rmcp-v3.0.0), [rust-sdk ROADMAP](https://github.com/modelcontextprotocol/rust-sdk/blob/main/ROADMAP.md), [Anthropic: bringing MCP 2026-07-28 to Claude](https://claude.com/blog/bringing-mcp-2026-07-28-to-claude), [openai/codex #33952](https://github.com/openai/codex/issues/33952), [Codex CLI guide (unofficial)](https://codex.danielvaughan.com/2026/08/31/mcp-2026-07-28-stateless-protocol-codex-cli-operators-guide/), [renovate rmcp v3 PR example](https://github.com/factorial-io/imap-mcp/pull/118). Code read: `crates/unimatrix-server/src/{server.rs, http/auth.rs, http/router.rs, mcp/context.rs, mcp/identity.rs}`, `Cargo.toml`.

---

## Findings

### Q: A. Readiness — rmcp 3.x stable? spec final? MSRV? conformance?
**Answer**: Yes, both are ready.
- `2026-07-28` was published final on 2026-07-28. Anthropic's announcement and secondary sources describe a stateless core: no handshake, no `Mcp-Session-Id`, no GET stream. That matches the ass-105 RC delta. I found no report of SEPs dropped between the RC and the final, though I did not diff the final text against the RC, so the auth SEP wording is not re-verified.
- **rmcp 3.0.0 is stable**, released 2026-07-28 after beta.1 through beta.5. The release-PR number is inconsistent across sources (#1077 vs #964), which does not matter here. Renovate PRs cite later 3.x minors (up to 3.11.0). I did not confirm that number on crates.io, so pin to the current 3.x patch after checking.
- **MSRV is 1.88**, declared in Cargo metadata and enforced in CI. Our workspace `rust-version = "1.89"`, so there is **no toolchain bump**.
- `ProtocolVersion::LATEST` in 3.0 still points to `2025-11-25`, so opting into `2026-07-28` is explicit.
- **Conformance results: not found** (see Unanswered).
**Recommendation**: Treat rmcp 3.x as an adoptable stable line. The readiness gate ass-105 set is met.

### Q: B. Backward compatibility (gating) — one server, both versions, zero client changes?
**Answer**: **Yes, if the server keeps legacy session mode on.** The migration guide states the wire changes are "additive or version-gated":
- `serve()` keeps the legacy `initialize` handshake.
- `with_legacy_session_mode` governs sessions for pre-2026-07-28 protocols.
- 2026-07-28 requests are always stateless.
- Legacy results keep their historical shape.

So today's bridge, hook client and Claude/Codex/OpenCode need no changes. The Rust API is breaking, which only affects our code (see E).

1. **`peer_info()` / clientInfo under stateless serving.** For legacy callers it stays populated, because the handshake still happens. For stateless callers there is no `initialize`, so clientInfo travels in per-request `_meta` (SEP-2575). The connection-level `peer_info()` cannot be assumed populated. Evidence: the migration guide's split of `RequestMetaObject` and the absence of a handshake. Not PoC-verified. **The attribution anchor holds in legacy mode and must move to `RequestContext.meta` for stateless mode.**
2. **Replacement for the `Mcp-Session-Id`-keyed `client_type_map`.** Nothing in rmcp replaces it, because the header does not exist for stateless callers. Today `server.rs:1279-1292` inserts on `initialize`, and `server.rs:579-590` looks up via the `mcp-session-id` header in `http::request::Parts`. Stateless replacement: read clientInfo from each request's `_meta` and derive `client_type` per request. No map is needed, since every request is self-describing. Keep the map for legacy callers.
3. **Does `ResolvedIdentity` reach `build_context` in both modes?** Very likely yes. `auth.rs:253` inserts it into the `http::Request` extensions before rmcp runs, and rmcp injects `Parts`, including extensions, into the message extensions. The R-01 spike validated this at `router.rs:301,435` on 1.7. The 3.x guide changes `RequestContext.meta`, not `extensions`, and the injection sits in `StreamableHttpService` regardless of session mode. **I re-checked the security boundary claims rather than inheriting them:**
   - The bearer is validated per HTTP request in the auth middleware, independent of sessions.
   - The TLS pin is at the transport layer and unaffected.
   - Capability-after-identity is unchanged as long as `build_context` still receives the extension.

   This stays a **mandatory test** at upgrade time: an integration test asserting that `ResolvedIdentity` arrives on both a legacy and a stateless request.
4. **Harness client status.**
   - **Claude (Anthropic):** "rolling out across Claude products". I found no Claude Code changelog confirming support, so treat it as still on `2025-11-25`.
   - **Codex CLI:** opt-in per server. v0.147.0 reportedly ships it behind config or an experimental flag. The source is unofficial and the default is legacy.
   - **OpenCode:** not determined.
   - **Our bridge and hook client:** `2025-11-25`.

   **All of our real callers are effectively on the old spec today.**
**Recommendation**: Back-compat is **YES**, conditional on `with_legacy_session_mode` staying enabled. Do not enable `2026-07-28` serving until items 1 and 2 have a stateless design. Item 3 needs a test.

### Q: C. Simplification for vnc-050
**Answer**: In the long term, the stateless wire removes:
- the server session store (the `transport-streamable-http-server-session` feature and `LocalSessionManager`);
- `Mcp-Session-Id` passthrough in the bridge;
- the GET SSE stream endpoint;
- sticky routing behind Tailscale or Cloudflare, so round-robin works.

But **none of that is removable while any legacy client exists**. Every harness client is legacy (B4), so for vnc-050's lifetime the server and bridge must stay session-capable. That means vnc-050 gains nothing from the new spec now. **The setup surface is unchanged:** bundle, scoped tokens and CA trust are transport- and auth-layer concerns, and no SEP touches static bearer or a pinned private CA. The expected answer is confirmed.
**Recommendation**: Write vnc-050's bridge **agnostic to the spec version**:
- forward `Mcp-Session-Id` only when it is present;
- never synthesize a session or require one;
- pass `MCP-Protocol-Version` and the body `_meta` through opaquely;
- put no session affinity in bridge logic.

That costs nothing now and makes the stateless switch a no-op for the bridge. Do not block vnc-050 on rmcp 3.x.

### Q: D. #1014 impact (handoff to ass-108)
**Answer / handoff**: The `2026-07-28` authorization hardening is OAuth-specific: OAuth 2.1, protected-resource metadata, and the OIDC-aligned SEPs. ass-105 flagged those auth SEPs as provisional in the RC, and I did not re-diff them against the final. They do not touch our static-bearer path. The claude.ai connector, however, uses the MCP OAuth flow, so #1014 lands squarely on these SEPs. Hand ass-108 four points:
- the connector path requires implementing an OAuth authorization or resource server (resource metadata), which Unimatrix does not have today;
- rmcp 3.0 changed its "OAuth startup and metadata APIs", so ass-108 should design against 3.x, not 1.7;
- the Claude connector rollout of `2026-07-28` may make the connector the first stateless client we see;
- the final auth SEP text needs a fresh read.
**Recommendation**: ass-108 should assume rmcp 3.x as its baseline. That gives the 3.x upgrade (E) a concrete forcing function.

### Q: E. Migration cost
**Answer**: **One step, 1.7 → 3.x, with no intermediate 2.x release.** The 2.x breaks have to be absorbed anyway, and a staged path only doubles the verification cycles. Breaks that land at our call sites:
- **1.8:** `peer_info()` returns `Arc<PeerInfo>` on the attribution path. `Deref` keeps field access mostly intact.
- **2.0:** model types realigned to `2025-11-25`. `mcp/tools.rs` alone has 149 `rmcp::` references, mostly model and result types, so the coupling is wider than the "~3 files" in pattern #4699 for type churn, though still mechanical.
- **3.0:**
  - `Meta` is split, so `RequestContext.meta` becomes a `RequestMetaObject`;
  - `ServerResult` and six union enums are now `#[non_exhaustive]`, so exhaustive matches need wildcard arms;
  - `structured_content` is a `Value` and `Annotations.last_modified` is a `String`;
  - `StreamableHttpService` requires `S: ServerHandler`;
  - `stateful_mode` is renamed `with_legacy_session_mode`;
  - manual `ServerHandler` impl signatures changed, including our `initialize` override at `server.rs:~1251`;
  - `#[tool]` methods compile unchanged because the router and handler macros absorb the changes.
- Re-pin `rmcp` and `rmcp-macros` `=` in lockstep (#902).

**Sketch:**
1. Bump both pins and fix compile errors in `server.rs`, `mcp/*`, `http/router.rs` and `uds/mcp_listener.rs`.
2. Explicitly set legacy session mode and keep the `-session` feature.
3. Add the `ResolvedIdentity`-propagation and `client_type_map` regression tests (legacy path).
4. Run the full integration suite with the bridge, hook client and a Claude Code smoke test.

Enabling the stateless path, with `_meta` clientInfo attribution, is a separate later feature.

**Estimate**: **2–3 delivery sessions** for the compatible upgrade, and **1–2 more** for stateless enablement and attribution.
**Recommendation**: Run it as one design+delivery feature (nxs or vnc), sequenced after vnc-050 merges and before or alongside ass-108 implementation.

---

## Unanswered Questions
- **Conformance-suite results for rmcp 3.x.** I found no published results. Directional confidence does not need them, but check the rust-sdk CI or the conformance repo during upgrade design.
- **Whether `peer_info()` is populated for stateless requests in rmcp 3.x.** I inferred it is not. Confirming needs a source read or a PoC, which is out of scope for directional confidence, so do it in upgrade design.
- **Claude Code's and OpenCode's `2026-07-28` support status.** There is no official changelog evidence. External, time-dependent.
- **The exact current rmcp 3.x patch version on crates.io** (3.11.0 is unverified).

## Out-of-Scope Discoveries
- Pattern #4699's "~3 files" claim holds for transport and handler wiring but undercounts model-type coupling: `mcp/tools.rs` has 149 refs. Worth correcting after the upgrade validates it.
- The Codex CLI per-server opt-in means a Codex user could point `2026-07-28` at us. With legacy-only serving, negotiation should fall back to `2025-11-25`. Verify this in the upgrade test matrix.

## Recommendations Summary
- **Overall**: DEFER the stateless wire, GO on a compatible rmcp 3.x upgrade after vnc-050. Revisit stateless when Claude Code GA-supports `2026-07-28` and stateless attribution has a design.
- **A**: The spec is final (2026-07-28) and rmcp 3.0.0 is stable (2026-07-28, now 3.x minors). MSRV 1.88 is within our 1.89. No conformance data was found.
- **B**: YES, back-compat with zero client changes, conditional on legacy session mode. `peer_info` holds for legacy only. Stateless needs `_meta`-derived `client_type` in place of `client_type_map`. `ResolvedIdentity` propagation is expected to hold but must be tested. All our clients are legacy today.
- **C**: Write vnc-050's bridge spec-agnostic: optional session-id passthrough, no affinity, opaque headers and `_meta`. The setup surface (bundle, tokens, CA) is unaffected. Do not block vnc-050 on 3.x.
- **D**: Hand ass-108 the point that the claude.ai connector path is exactly where the OAuth 2.1 / resource-metadata SEPs bite. Baseline on rmcp 3.x OAuth APIs.
- **E**: A single-step 1.7 → 3.x upgrade: about 2–3 sessions compatible, plus 1–2 for stateless enablement. Lockstep re-pin and `ResolvedIdentity` tests are mandatory.
