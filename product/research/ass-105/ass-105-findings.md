# FINDINGS: New MCP Specification (2026-07-28) — Spec Delta, Security-Model Impact, rmcp Upgrade Path, Adoption Assessment

**Spike**: ass-105
**Date**: 2026-07-26
**Approach**: investigation (external web research on the spec/rmcp + internal codebase & prior-art analysis)
**Confidence**: directional (go/no-go-now posture + upgrade-cost sizing; no PoC)

> **TIMING / CONFIRMED-vs-DRAFT CAVEAT (read first).** As of the research date (2026-07-26):
> - The **2026-07-28 spec is a Release Candidate, not final.** The RC was locked **2026-05-21**; final publication is targeted **2026-07-28** (~2 days out). Primary + secondary sources agree it is not yet settled, and the **authorization SEPs specifically are called out as "still settling — treat as provisional until the July 28 final."**
> - **CONFIRMED (published/merged):** the RC document itself and its SEP list (modelcontextprotocol blog, dated post); all **rmcp releases up to 2.2.0 (stable)** and **3.0.0-beta.2 (pre-release)**, with dated crates.io entries and merged GitHub release notes.
> - **DRAFT / UNCONFIRMED:** exact final wording of the auth SEPs; whether any SEP is dropped/changed before 07-28; the numeric MSRV rmcp 3.x will declare.
> - Nothing below treats unpublished draft text as settled. rmcp's **3.0.0-beta churn is used as the leading indicator** of what is landing, and is labelled pre-release throughout.

---

## Findings

### A. Understand the new spec

#### Q: What version/date, what changed vs. the version rmcp 1.7.0 implements? Concrete delta (added/changed/deprecated/removed).

**Answer**: New revision = **`2026-07-28`** (RC locked 2026-05-21, final targeted 2026-07-28). It is described by the MCP maintainers as *"one of the most substantial changes since adding authorization,"* and it **contains breaking changes**. Our pinned **rmcp 1.7.0 (published 2026-05-13)** predates rmcp's own 2.0.0 realignment *"align model types with MCP 2025-11-25 spec"* — so our server is effectively on the **prior finalized revision (2025-06-18 lineage), roughly two finalized spec revisions behind the RC** (2025-06-18 → 2025-11-25 → 2026-07-28).

Concrete delta (RC vs. our 1.7.0 baseline), grouped by SEP:

**ADDED / CHANGED — statelessness (the structural core):**
- **SEP-2575** — removes the `initialize`/`initialized` handshake; **protocol version, clientInfo, and client capabilities now ride in `_meta` on every request** instead of being exchanged once at connect. Adds `server/discover` negotiation.
- **SEP-2567** — **eliminates the `Mcp-Session-Id` header and protocol-level sessions** → enables round-robin load balancing / horizontal scale with no sticky sessions.
- **SEP-2260** — server-initiated requests permitted **only during active client-request processing**.
- **SEP-2322** — Multi Round-Trip Requests (MRTR) with `InputRequiredResult` for elicitation; replaces SSE streams for that pattern.

**ADDED — operability:**
- **SEP-2243** — mandatory `Mcp-Method` / `Mcp-Name` HTTP headers (route without body inspection).
- **SEP-2549** — `ttlMs` + `cacheScope` cache hints on list/resource responses.
- **SEP-414** — W3C Trace Context (`traceparent`/`tracestate`/`baggage`) in `_meta`.

**ADDED — authorization hardening (all OAuth 2.1 / OIDC — see Section B):**
- **SEP-2468** (`iss` validation per RFC 9207), **SEP-837** (OIDC `application_type` in DCR), **SEP-2352** (bind credentials to issuer), **SEP-2207** (OIDC refresh), **SEP-2350** (scope accumulation on step-up), **SEP-2351** (`.well-known` discovery suffix). PKCE **S256 now required** of auth servers.

**ADDED — schema/tooling:**
- **SEP-2106** — tool `inputSchema`/`outputSchema` lifted to **full JSON Schema 2020-12** (composition/conditionals/refs); `structuredContent` / non-object output types now allowed.
- **SEP-2164** — missing-resource error changes from MCP-custom `-32002` to JSON-RPC standard `-32602`.

**ADDED — extensions framework (features leaving the core):**
- **SEP-2133** (reverse-DNS extension IDs, independent versioning), **SEP-2663** (Tasks graduates from experimental core to an **opt-in extension**), **SEP-1865** (**MCP Apps** — server-rendered HTML UI in sandboxed iframes).

**DEPRECATED (SEP-2577 / SEP-2596 lifecycle — annotation-only, functional ≥12 months):**
- **Roots**, **Sampling**, **Logging** all deprecated. Minimum 12 months deprecation→removal; expedited removal needs 90-day notice.

**REMOVED:** nothing yet in the RC (deprecations precede removals by ≥12 months); the *initialize handshake* and *Mcp-Session-Id* are the effective structural removals introduced by the stateless SEPs.

**Evidence**: modelcontextprotocol blog RC post (2026-07-28-release-candidate); rmcp GitHub release notes for 2.0.0/2.1.0/2.2.0/3.0.0-beta.1 (each SEP is a merged PR, dated Jun–Jul 2026); The Register (2026-07-23); Stacktree (2026-07-13). See Sources.

**Recommendation**: Treat `2026-07-28` as a **structural (stateless) + authorization + schema** revision, not a point release. Track it against rmcp's **3.0.0-beta line** (the only line implementing it) rather than the RC prose, because the beta is the concrete, testable form of what lands.

#### Q: Which changes are mandatory for compliance vs. optional?

**Answer**:
**Mandatory to claim `2026-07-28` compliance** (per Stacktree's compliance read of the RC):
- Stateless operation — no session-based routing; read protocol version/capabilities from `_meta` per request (SEP-2575/2567).
- `server/discover` RPC.
- `Mcp-Method` / `Mcp-Name` headers on Streamable HTTP (SEP-2243).
- Tasks callers move to polling `tasks/get` (blocking `tasks/result` is breaking) — only if Tasks is used.

**Optional / opt-in:** MCP Apps (SEP-1865), the Tasks extension itself (SEP-2663), custom `x-mcp-header` support, cache hints (SEP-2549), trace context (SEP-414). **The entire OAuth/OIDC authorization suite is optional** — it applies only to servers that implement MCP's OAuth flow.

**Crucially:** the **prior finalized revision `2025-11-25` is NOT superseded** and remains valid for **≥12 months**. There is **no compliance forcing function** to adopt `2026-07-28` on any near-term clock.

**Recommendation**: We are under no obligation to adopt `2026-07-28` to stay compliant. Adoption is a **capability/opportunity decision** (stateless horizontal scale, richer schema), not a compliance deadline.

---

### B. Security-model impact (human-flagged priority) — GO/NO-GO POSTURE

**Bottom line: the new spec's *authorization* hardening does NOT force changes on Unimatrix's security model, because we do not implement MCP's OAuth flow. The real security-relevant impact is the *stateless* SEPs (2575/2567), which touch two *attribution* anchors — not the auth boundary itself. Posture: NO structural security threat; NO-adopt-now on the wire; a bounded attribution-continuity item to verify before any 3.x move.**

#### Q: Does it change the auth story (OAuth 2.1 / resource-server metadata, bearer handling, protocol-version headers, session binding) vs. our model?

Our model (code-confirmed at `crates/unimatrix-server/src/http/auth.rs`): **sole static 256-bit bearer token**, constant-time (`subtle::ConstantTimeEq`) validation in a **tower `StaticTokenAuthLayer`** wrapping rmcp's `StreamableHttpService`; **`BearerValidator` trait** as the extension seam; on success it **mints a `ResolvedIdentity` into request extensions** (`auth.rs:250-253`); **capability-check-after-identity (Principle 3)** downstream; **TLS leaf-DER SHA-256 fingerprint pinning** (`http/tls.rs`, in-process termination, `with_no_client_auth`); **per-slug routing** above the auth layer.

**Answer — mapped SEP by SEP:**
- **OAuth 2.1 / OIDC / DCR / resource-server metadata (SEP-2468/837/2352/2207/2350/2351):** **No impact.** These harden MCP's *OAuth authorization-server* interaction. Unimatrix authenticates with a **static bearer over pinned TLS**, entirely **outside the MCP OAuth spec** — which is *optional*. Bearer-over-TLS remains a permitted, compliant posture. Our `BearerValidator`/`StaticTokenValidator` are untouched. (Note: rmcp's OAuth fixes in 1.8–2.2 — issuer validation, SSRF/resource-spoofing blocks, S256-PKCE enforcement — live in rmcp's **client-side OAuth**, which we do not use. They are irrelevant to our server auth.)
- **Protocol-version header:** the version moves **into `_meta` per request** (SEP-2575). rmcp abstracts negotiation (2.1.0 *"negotiate protocol version in handler"*). No change to our auth layer, which never inspects the protocol version.
- **Session binding:** **SEP-2567 eliminates `Mcp-Session-Id`.** Our **auth boundary does not depend on sessions** — the bearer is validated per-request and `ResolvedIdentity` is inserted per-request into extensions. So the **auth boundary is unaffected**. (The *attribution* layer does lean on session identifiers — see the next two items; that is where the real impact sits.)

**Attribution-continuity impact (this is the concrete finding, not the auth boundary):**
1. **`clientInfo.name` as the non-spoofable attribution primary source** (ass-050 OQ-01: `ctx.peer.peer_info().map(|ci| ci.client_info.name)`). SEP-2575 relocates clientInfo from a one-time handshake into per-request `_meta`. rmcp already re-shaped this: **1.8.0 changed `Peer::peer_info()` from `&PeerInfo` to `Arc<PeerInfo>`** (re-settable on duplicate initialize). Field access still works via `Arc` `Deref`, but **whether `peer_info()` is reliably populated in stateless serving must be verified** before we trust it as the attribution anchor under 3.x.
2. **rmcp session UUID / `Mcp-Session-Id`** (ass-050 OQ-03; vnc-014 `client_type_map` keyed on it via `extract_rmcp_session_id` reading the `mcp-session-id` header). **SEP-2567 removes that header; rmcp 3.0.0-beta serves the draft statelessly with no `Mcp-Session-Id` (PR #999).** This **breaks the OQ-03 mechanism** — `client_type_map` attribution and any header-derived session key evaporate in stateless mode. **This is the single highest-value security/attribution-model impact of the new spec.**

**Recommendation**: **GO** on the assessment that the **auth boundary (bearer + TLS pin + BearerValidator + Principle 3) is not threatened or forced to change** by `2026-07-28`. **NO-GO on adopting the stateless wire now** without first re-deriving the two attribution anchors: (a) confirm `peer_info()`/clientInfo survives stateless serving, (b) replace the `Mcp-Session-Id`-derived session key (OQ-03/vnc-014) with an in-`_meta` or in-payload identifier. Both are **attribution/provenance** fixes, not auth-boundary rebuilds — bounded, and only due when/if we move to stateless.

#### Q: Does it affect the anticipated L2 identity seam (relying-party verifier, ass-100/101)?

**Answer**: **Net-neutral-to-positive; no rework of the ass-100/101 direction.** The chosen v1 anchor is **(B) a pinned Ed25519 short-lived signed assertion verified in-band** at the bearer/identity-resolution seam (`auth.rs:198` → `external_identity=Some` → `build_context_with_external_identity`, `server.rs:526`), transport-portable and **payload-carried**. Because the new spec pushes identity/version metadata **into per-request `_meta`**, a payload-carried signed assertion **aligns naturally with the stateless model** — it never depended on the handshake or on `Mcp-Session-Id`, so SEP-2575/2567 do not disturb it. The `enforce_external_identity` gate (ass-100/101 Q6, default `false`, ADR-008 audit-only posture) is untouched.

Two nuances to carry:
- The new **OIDC/DCR hardening is a forward-compat positive** for the *enterprise* JWT path (ADR-007 JWT-`sub`): if that tier ever adopts real OIDC, SEP-2352 (bind creds to issuer) and SEP-837 make it more standards-aligned. Not a forcing function; a future convergence point.
- ass-101's open **channel-binding-via-server-cert-fingerprint** question gets *harder* under SEP-2567's round-robin/horizontal-scale intent (the terminating endpoint may differ per request). This strengthens ass-101's already-stated lean toward the **payload signature (B), which is immune**, over channel-fingerprint binding.

**Recommendation**: The ass-100/101 anchor choice **holds and is reinforced** by the stateless direction. No new spike needed; feed this alignment note into the eventual L2 decision.

#### Q: Any new attack surface (elicitation, sampling, resource fetch, async tasks) touching integrity / poison-resistance?

**Answer**: **Net reduction of relevant surface, plus opt-in features we simply don't enable.**
- **Sampling — DEPRECATED (SEP-2577).** Server-solicited LLM sampling was the classic prompt-injection/exfil vector; its deprecation *reduces* surface. We don't use it.
- **Server-initiated requests constrained (SEP-2260)** to active client-request windows — tightens, doesn't loosen.
- **Tasks → opt-in extension (SEP-2663); MRTR elicitation (SEP-2322); MCP Apps (SEP-1865, HTML-in-iframe).** All **opt-in**. Not enabling them = no new surface. If MCP Apps is ever adopted for a dashboard (mtx phase), the sandboxed-iframe HTML path becomes a genuine new surface warranting its own review — flag, don't adopt implicitly.
- **Integrity / poison-resistance posture is orthogonal to the wire spec.** Our threat model is content-poisoning of *stored knowledge*, defended at the knowledge/eval layer — not at transport. Statelessness does not touch it.

**Recommendation**: No integrity/poison-resistance regression from `2026-07-28`. Keep Tasks/MCP-Apps/MRTR **disabled by default**; treat any future MCP Apps adoption as a net-new security review item.

---

### C. rmcp / Rust upgrade path

#### Q: What rmcp version implements the new spec, and is it released or pre-release on ~2026-07-28?

**Answer (version → spec map, from crates.io dates + merged release notes):**

| rmcp | Published | Spec target | Status |
|---|---|---|---|
| **1.7.0 (our pin)** | 2026-05-13 | prior finalized (2025-06-18 lineage; pre-2.0 realignment) | stable |
| 1.8.0 | 2026-06-23 | +SEP-2164/2468/837/2577 (still 2025-11-25-era) | stable — **source-breaking despite minor bump** |
| 2.0.0 | 2026-06-29 | **realigned to 2025-11-25** | stable, breaking |
| 2.1.0 | 2026-07-02 | 2025-11-25 +SEP-414/2575 meta helpers | stable |
| **2.2.0** | 2026-07-08 | **2025-11-25, conformance-passed** | **latest stable** |
| 3.0.0-beta.1 | 2026-07-23 | **implements 2026-07-28 draft (stateless)** | **pre-release** |
| 3.0.0-beta.2 | 2026-07-24 | 2026-07-28 draft; declares MSRV | pre-release |

**The `2026-07-28` spec is implemented ONLY in the rmcp 3.0.0-beta line — pre-release as of 2026-07-26. No stable rmcp implements it. Latest stable (2.2.0) implements the prior finalized `2025-11-25`.** 3.0.0-beta serves 2025-11-25 stably **and** the draft statelessly (PR #999, `.with_stateful_mode` → `.with_legacy_session_mode` #1015) — i.e. **dual-version** in one binary.

**Delta from 1.7.0 to the new spec (the breaking waypoints we'd cross):**
- **1.8.0:** `Peer::peer_info()` → `Arc<PeerInfo>` (source-breaking; **directly on our attribution path**, `server.rs`). Fix: `.as_deref()` where a `&PeerInfo` was bound.
- **2.0.0:** model-type realignment to 2025-11-25 (breaking); `Audio` PromptMessageContent variant; streamable-HTTP **session-leak fix (#934)**. Migration guide: discussions/926.
- **2.1.0/2.2.0:** protocol-version negotiation in handler; **streamable-HTTP memory bound (#970)**; S256-PKCE enforcement (client-OAuth, N/A to us); conformance-suite pass.
- **3.0.0-beta:** many `[breaking]` SEPs — server discovery/negotiation (2575), stateless serving (2567), Tasks extension (2663), cache hints (2549), MRTR (2322), HTTP standard headers (2243), `outputSchema`/`structuredContent` relaxation (2106).

**Our pinned features** (`server, client, transport-io, macros, transport-streamable-http-server, transport-streamable-http-server-session`): the **`-session` feature becomes legacy under statelessness** — SEP-2567 removes sessions, and 3.x renames stateful config to `.with_legacy_session_mode`. Long-term, adopting the new spec **simplifies** our transport (no session store).

- **`#[tool_router]` / `#[tool_handler]` / `#[tool]` macros:** **no breaking changes in any release note across 1.7→3.0-beta** — consistent with pattern #4699 (proc-macro surfaces stable across the whole range). Low risk.
- **schemars schema-gen:** no schemars version bump called out; **SEP-2106 relaxes output schema to any JSON Schema 2020-12 type** and `structuredContent` typing (#933, breaking) — verify our `schemars`-derived tool schemas still generate under the relaxed types, but this is a compile-surface check, not a redesign.
- **Streamable HTTP transport:** stateless-by-default in 3.x; session config renamed; leak/memory fixes in 2.x.

**Recommendation**: **Do not pin a security-critical wire to a pre-release.** The new spec's rmcp support is **beta-only**; wait for a **stable rmcp 3.x**. Separately, note that even the *stable* path to today's spec (2.2.0) already crosses the 1.8.0 `peer_info` break and 2.0.0 realignment — so any move off 1.7.0 is a real (bounded) migration, not a version-number bump.

#### Q: Rust edition / toolchain / MSRV bumps; breaking API changes at our call sites; lockstep re-pin (#902).

**Answer:**
- **MSRV:** rmcp **first declares/checks an MSRV in 3.0.0-beta.2 (#1034)**. The numeric value isn't in the release note — **VERIFY our workspace `rust-version` meets rmcp 3.x's declared MSRV** before adopting. **No edition bump** is mentioned in any release note (UNCONFIRMED beyond release-note absence; verify against `Cargo.toml` at adoption).
- **Breaking API at our call sites** (bounded by ADR-003 McpAdapter isolation, pattern #4699 — rmcp coupling concentrated in ~3 files: `server.rs`, `http_provision`/router, `mcp_listener`): the concrete hits are (1) **`peer_info()` Arc** on our attribution path, (2) **streamable-HTTP session config rename** if we configure stateful mode, (3) **`structuredContent`/`outputSchema` type relaxation** on schema-gen. The #4699 attention list still governs: **verify `ServerHandler` trait signature, extension propagation (that our `ResolvedIdentity` inserted at `auth.rs:250` still survives to `build_context`), IntoTransport blanket impls (UDS tuple), and config default changes.** Under statelessness, **extension propagation of `ResolvedIdentity` is the top verify-item** — our entire bearer→identity wiring depends on `request.extensions_mut().insert(identity)` surviving stateless request flow.
- **Lockstep re-pin (#902 / ADR-001, #5455):** unchanged and mandatory — **bump `rmcp` and `rmcp-macros` `=`pins together** in the same change, and keep `--locked` on documented `cargo install`. Any 1.8+/2.x/3.x move re-pins both.

**Recommendation**: Size a future upgrade as **schema+handler-plus-transport-verify**, concentrated in the 3 McpAdapter files, with **extension-propagation of `ResolvedIdentity` as the make-or-break check**. Re-pin both crates in lockstep. Confirm MSRV before committing.

---

### D. Other impacts

#### Q: Wire-contract stability for existing clients (Claude Code, Codex CLI, Gemini CLI) + multi-LLM parity (C14); backward-compat / dual-version window.

**Answer**: **Strong backward-compat cushion.** `2025-11-25` stays finalized and valid **≥12 months**; deprecated features live ≥12 months. rmcp 3.x **serves 2025-11-25 stably alongside the 2026-07-28 draft** in one binary — so we can adopt rmcp 3.x (when stable) and **keep serving 2025-11-25 to existing clients** while draft-capable clients negotiate the new revision. Multi-LLM parity (C14) is preserved **as long as we do not force stateless-only**. The clients themselves must add stateless support to consume `2026-07-28`; that is their clock, not ours, and the long window means no scramble.

**Recommendation**: When we do adopt rmcp 3.x, **keep legacy/2025-11-25 serving enabled** through the transition; do not force stateless-only until Claude Code / Codex / Gemini demonstrably support it.

#### Q: New protocol capabilities as opportunities mapped to Unimatrix tools (flag, don't design).

- **SEP-2106 full JSON Schema output / `structuredContent`** → richer, typed returns for `context_get`/`context_search`/`context_briefing` (today JSON-in-text). Opportunity.
- **SEP-2549 cache hints (`ttlMs`/`cacheScope`)** → cache briefing/search results client-side; cheap latency win.
- **SEP-2322 MRTR elicitation** → interactive `context_enroll`/`context_correct` confirmation flows without bespoke round-trips.
- **SEP-1865 MCP Apps** → server-rendered dashboard UI (mtx phase) — but a net-new **security surface** (sandboxed-iframe HTML); adopt only with its own review.
- **SEP-2567 statelessness** → **horizontal-scale story for `personal-cloud`/L3 platform** (round-robin, no session store) — directly serves the platform-goal deploy contract.

**Recommendation**: Flag all as opportunities for a later design cycle; **do not design here**. The stateless horizontal-scale and structured-output items are the highest-value for the platform goal.

#### Q: Client-SDK semver co-evolution (platform L3 seam).

**Answer**: A wire revision co-evolves the **JS/TS edge client** (single edge language, per standing decision). A `2026-07-28` move means the edge SDK must track stateless framing + `_meta`-carried version/clientInfo. **Flag** as a coupled work item for any adoption; not sized here.

#### Q: Timing recommendation — adopt-now vs. wait-for-stable-rmcp.

**Answer: WAIT for stable rmcp 3.x. Do not adopt now.** Rationale, ranked:
1. **Spec is RC, not final** (until ~2026-07-28); auth SEPs explicitly *"still settling."* Adopting pre-final risks re-work.
2. **Only rmcp 3.0.0-beta implements it** — pinning a **security-critical MCP wire to a pre-release** violates the spirit of ADR-001's exact-pin discipline.
3. **No compliance forcing function** — `2025-11-25` valid ≥12 months; existing clients unaffected.
4. **Low security value to *us*** — the spec's headline hardening is OAuth/OIDC, which our static-bearer model doesn't use; the parts that *do* touch us (statelessness) hit **attribution anchors**, adding near-term work rather than security.

**Decoupled near-term option (flag for the decision-maker):** the *streamable-HTTP session-leak (#934)* and *memory-bound (#970)* fixes landed in the **stable 2.x** line. If those defects affect our long-running `transport-streamable-http-server` deployment, a **bump to stable 2.2.0** (still `2025-11-25`, no stateless disruption) is a **separate, defensible decision** — but it still crosses the 1.8.0 `peer_info` break and 2.0.0 realignment, so it is a real bounded migration, not free. Revisit rmcp 3.x when it goes **stable** *and* when we actually want stateless horizontal scale for the platform goal.

---

## Unanswered Questions

- **Does `peer_info()`/clientInfo remain populated under rmcp 3.x stateless serving?** Not determinable from release notes/RC prose; requires a code read of rmcp 3.0.0-beta or a spike PoC. Blocks trusting the ass-050 OQ-01 attribution anchor under statelessness. (Requires: rmcp 3.x source/PoC.)
- **rmcp 3.x numeric MSRV** — declared in 3.0.0-beta.2 (#1034) but the value isn't in the note. Verify against our workspace `rust-version` before adoption. (Requires: read `Cargo.toml` of a 3.x tag.)
- **Final vs. RC auth-SEP wording** — the authorization SEP set is "still settling"; final `2026-07-28` text may differ. Re-check after 2026-07-28. (Blocked on: spec finalization, ~2 days.)
- **Do #934/#970 streamable-HTTP fixes affect our deployment specifically?** Needs a read of whether our session-mode config path is exposed to the leak. (Requires: targeted code + rmcp 2.x diff read.)

## Out-of-Scope Discoveries

- **The `-session` transport feature is on a deprecation trajectory.** SEP-2567 removes sessions; rmcp 3.x renames stateful config to `.with_legacy_session_mode`. Our `transport-streamable-http-server-session` feature becomes legacy under the new spec — a future simplification (drop the session store), not a spike. Flag for the eventual delivery cycle.
- **rmcp 1.8.0 shipped a source-breaking change under a *minor* version bump** (`peer_info` Arc). This validates ADR-001's `=`-pin exactly — a caret `^1.7` would have broken our build on a routine resolve. Reinforces the existing pin discipline; no action beyond noting the vindication.
- **Sampling deprecation (SEP-2577) removes a classic injection vector from the protocol.** If Unimatrix ever considered server-solicited sampling, the spec now steers away from it — a data point for any future integrity design, not a spike.
- **MCP Apps (SEP-1865) is a latent new attack surface** should the mtx dashboard phase adopt server-rendered iframe UI. Warrants its own security review *if* adopted — flag, not pursued.

## Recommendations Summary

*(This section is posted to issue #971.)*

- **A — Spec delta**: `2026-07-28` is an RC (final ~07-28), a **structural (stateless) + auth + schema** revision; our rmcp 1.7.0 is ~2 finalized revisions behind. Track it via rmcp's 3.0.0-beta line, the only implementation.
- **A — Mandatory vs optional**: Statelessness (SEP-2575/2567), `server/discover`, and `Mcp-Method/Name` headers are mandatory *for compliance*; OAuth/OIDC suite, Tasks, MCP Apps, cache hints are optional. **`2025-11-25` stays valid ≥12 months — no forcing function.**
- **B — Security posture (GO/NO-GO)**: **GO** — the auth boundary (bearer + TLS pin + `BearerValidator` + Principle 3) is **not threatened or forced to change**; the OAuth hardening is optional and we don't use MCP OAuth. **NO-GO on adopting the stateless wire now**: SEP-2567 removes `Mcp-Session-Id` (breaks ass-050 OQ-03 / vnc-014 `client_type_map` key) and SEP-2575 relocates clientInfo (ass-050 OQ-01 attribution anchor). Both are bounded **attribution-continuity** fixes, due only when/if we go stateless.
- **B — L2 seam (ass-100/101)**: **Reinforced, not reworked.** The payload-carried Ed25519 signed-assertion anchor is immune to statelessness and aligns with the `_meta`-per-request model; round-robin scale strengthens the lean away from channel-cert-fingerprint binding. OIDC hardening is a forward-compat positive for the enterprise JWT path.
- **B — Attack surface**: **Net reduction** — Sampling deprecated, server-initiated requests constrained; Tasks/MRTR/MCP-Apps are opt-in (keep disabled). Integrity/poison-resistance is orthogonal to the wire — unaffected.
- **C — rmcp version**: New spec is implemented **only in rmcp 3.0.0-beta (pre-release)**; latest **stable is 2.2.0 (`2025-11-25`)**. **Wait for stable rmcp 3.x.**
- **C — Upgrade cost**: Bounded to the 3 McpAdapter files (ADR-003/#4699). Concrete breaks: `peer_info()`→`Arc` (1.8.0, on our attribution path), session-config rename, `outputSchema`/`structuredContent` relaxation. **Top verify-item: `ResolvedIdentity` extension propagation survives stateless flow.** Re-pin `rmcp` + `rmcp-macros` `=`pins in lockstep (ADR-001/#5455); confirm MSRV (declared in 3.0.0-beta.2).
- **D — Client compat**: Long dual-version window; rmcp 3.x serves `2025-11-25` + draft in one binary. Keep legacy serving on through any transition; don't force stateless-only until Claude Code/Codex/Gemini support it (C14 parity).
- **D — Opportunities (flag, don't design)**: structured/typed tool output (SEP-2106), cache hints (SEP-2549), MRTR elicitation flows, and especially **stateless horizontal scale (SEP-2567) for the personal-cloud/L3 platform goal**.
- **D — Timing**: **WAIT for stable rmcp 3.x** + spec finalization; adopt when we want stateless scale, not on a compliance clock. **Decoupled option**: if streamable-HTTP session-leak (#934)/memory (#970) fixes matter to our deployment, a bump to **stable 2.2.0** (`2025-11-25`, no stateless disruption) is a separate defensible decision — but still a real bounded migration across the 1.8.0/2.0.0 breaks.

---

## Sources (date-stamped)

- MCP RC announcement — *The 2026-07-28 MCP Specification Release Candidate*, blog.modelcontextprotocol.io (RC locked 2026-05-21; final targeted 2026-07-28). CONFIRMED RC status.
- *MCP prepares to break with its stateful past*, The Register, 2026-07-23. Secondary, stateless framing.
- *MCP 2026-07-28 spec: what changed, what breaks*, Stacktree, 2026-07-13. Compliance mandatory-vs-optional read; "auth SEPs still settling."
- rmcp crate versions + dates — crates.io API (`/crates/rmcp/versions`), retrieved 2026-07-26: 1.7.0 (05-13) … 2.2.0 stable (07-08), 3.0.0-beta.2 (07-24).
- rmcp GitHub release notes (modelcontextprotocol/rust-sdk) for tags 1.8.0/2.0.0/2.1.0/2.2.0/3.0.0-beta.1/beta.2, retrieved 2026-07-26 — per-SEP merged PRs; `peer_info` Arc break (#862); stateless serving (#999); session-mode rename (#1015); MSRV (#1034).
- Internal: `crates/unimatrix-server/src/http/auth.rs`, `Cargo.toml`; ass-050 FINDINGS (bearer model, OQ-01/OQ-03); ass-100/101 FINDINGS (L2 anchor); Unimatrix #5455 (ADR-001 lockstep pin), #4699 (rmcp migration blast radius).
