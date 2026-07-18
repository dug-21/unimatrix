# FINDINGS (INTERNAL track): Root-of-trust establishment + secure edge binding

**Spike**: ass-101
**Date**: 2026-07-17
**Approach**: design-research — feasibility read of the Unimatrix server's TLS / transport / identity seams (no build)
**Confidence**: directional (seam facts code-verified at file:line; recommendations are options + sizing, not a build)
**Track**: INTERNAL (codebase/seam-anchored). External track owns the ecosystem landscape — mTLS/JWKS/SPIFFE mechanics and PKI provisioning/rotation (SCOPE Q3) — assumed in FINDINGS-EXTERNAL.md.
**Prior art consumed**: ass-100 FINDINGS (#954); ADR-002/vnc-034 (#4948 cert-fingerprint); ADR-006 bearer; ADR-007 (#4361 two-field attribution); ADR-008 (#5609 audit-only posture).

## Scope of this track
Answers the codebase half of SCOPE bounded questions **Q1 (internal half)**, **Q2 (internal half)**, **Q4**, **Q5**, **Q6**. No external prior art, no SCOPE Q3 (provisioning/rotation). Every recommendation carries a change-size (`handler-only / schema+handler / transport / architectural`) and names its seam.

---

## Findings

### Q: (SCOPE Q1, internal half) Where does TLS terminate today, and how do HTTP vs STDIO differ in where a channel anchor could be verified? (the shipped fingerprint-pinned HTTPS + single-bearer path — the anchor we extend, not replace)

**Answer**: HTTP TLS terminates **in-process** in the hyper accept loop, on a rustls acceptor built from a first-boot self-signed cert; trust is **one-directional** (client pins the server leaf; the server does not authenticate the client's TLS peer). Bearer is a **separate per-request** layer above TLS. STDIO has **no TLS and no bearer** — its only verifiable channel anchor is OS/process-level (UDS peer-credentials), and that lives on the socket path, not stdio itself. The transports differ fundamentally in *where* an anchor is even expressible.

**Evidence**:
- HTTP TLS terminates in-process at `http/listener.rs:193-196` (`acceptor.accept(...)` in `handle_connection`); acceptor built at `http/tls.rs:34-80`; leaf-DER SHA-256 pin oracle at `http/tls.rs:130-136`; self-signed first-boot cert at `http/cert_provisioner.rs:65,211` (ADR-002/#4948).
- **No mTLS** — sole client-auth site is `http/tls.rs:69` `.with_no_client_auth()` (comment `:63` "no client auth / no mTLS"); no `ClientCertVerifier`/peer-cert read anywhere. The shipped pin is server→client; the mirror (client→server) is what an anchor adds.
- Bearer is per-request: `StaticTokenAuthLayer` (`http/auth.rs:143`) → `StaticTokenAuth::call` (`:198-262`), constant-time compare (`:110-121`), outermost layer at `main.rs:1489-1490`. On success it already builds `ResolvedIdentity{agent_id:"http-bearer", trust_level:Restricted}` into request extensions (`auth.rs:127-137,250-253`). Authenticates the connection/deployment, not per-agent identities (ADR-006).
- STDIO: `main.rs:1991` `serve(rmcp::transport::io::stdio())` — raw JSON-RPC, no TLS/auth (`main.rs:1983`); default path bridges stdio→daemon UDS (`bridge.rs:146-156`), no auth of its own. The only enforceable local anchor is UDS peer-cred (SO_PEERCRED UID) at `unimatrix-engine/src/auth.rs:97` (invoked `uds/listener.rs:640`) — guards the socket, not stdio.

**Recommendation**: Two distinct anchor surfaces; do not force one mechanism (transport-dependence confirmed in code).
- **HTTP**: extend the pinned channel. Two verification points already exist — TLS accept (`tls.rs:69`, flip `with_no_client_auth`→`with_client_cert_verifier` = **transport**) or the bearer layer (`auth.rs:198`, verify an edge-signed assertion/JWT and populate `external_identity` = **schema+handler**). The bearer layer is the cheaper, more portable placement (Q5).
- **STDIO**: no in-channel anchor possible; nearest real mechanism is the shipped UDS peer-cred UID check (`engine/auth.rs:97`). Only an in-body signed assertion (anchor B) is portable, verified at the `build_context_with_external_identity` seam.

### Q: (SCOPE Q4) Minimal binding that turns the one controller-proxy into an authenticated delegating authority; does it collapse Q8/delegation to single-root for v1?

**Answer**: Minimal binding = **one credential for one process** — authenticate the single controller connection and every `_attested_identity` it stamps becomes a trusted delegated claim by construction. **Yes — it collapses SCOPE Q2/Q8 to a single-root delegation model for v1.** One edge process = one root; per-agent (N-root) credentials are unnecessary. The controller vouches, the server trusts the controller, per-agent identities are attributed-within-boundary claims.

**Evidence**:
- Jurati Mode B/P2: one proxy holds the pinned HTTPS session + bearer and stamps `_attested_identity`; server returned `ok:true` (ass-100 SCOPE 50-56, FINDINGS Q4/Q8). One process = one trust boundary.
- Server already resolves the bearer connection to a **single** `ResolvedIdentity` (`auth.rs:250`); `build_context_with_external_identity` takes a **single** `Option<&ResolvedIdentity>` (`server.rs:526-533`) — shaped for one delegated identity per call.
- `_attested_identity` is a **delegation artifact by construction** — no independent per-agent crypto (ass-100 Q8); it is a vouching token, exactly what single-root delegation consumes.
- ADR-007 (#4361): JWT-`sub` replaces the *source* of the single per-connection attribution field — single-root shape.

**Recommendation**: Bind the one controller connection, then trust its stamps. Cheapest-first:
1. **Bearer-as-root (interim)**: accept the already-authenticated bearer connection as the delegating authority; thread its existing extensions `ResolvedIdentity` into `external_identity=Some` and capture `_attested_identity` as the delegated subject. Size **schema+handler** (`tools.rs` params + `server.rs:526`). **Caveat (SCOPE principle 1)**: bearer is symmetric — cannot prove *the edge* spoke; leaked token = full impersonation. Audit-grade delegation, not an asymmetric anchor.
2. **Asymmetric single-root (the real anchor)**: give the controller an asymmetric credential verified against a pinned public half — a controller client-cert pinned server-side (mirror of the shipped leaf pin; flip `tls.rs:69`) = **transport**, or an edge-signed assertion verified at the auth/handler seam = **schema+handler**. One cert/key for one process — the smallest asymmetric, channel-pinned anchor.

**Q2/Q8 collapse — confirmed**: v1 is single-root delegation. Per-agent N-root credentials are not required for Jurati and should not be built for v1.

### Q: (SCOPE Q5) Verifier placement relative to `build_context_with_external_identity` and the 15 `require_cap` chokepoints — transport termination, identity resolution, or both? Reverse-proxy header-injection hazard.

**Answer**: **Verify once, upstream — not at the 15 chokepoints.** The channel anchor verifies at/near transport termination (mTLS at `tls.rs`) or at the identity-resolution layer (bearer/`auth.rs` for an in-band signed assertion); the resolved principal flows through `build_context_with_external_identity` into `ctx.agent_id`, and all 15 `require_cap` sites consume it downstream without re-verifying. Lowest-cost placement is the **existing bearer layer feeding `external_identity=Some`** (identity resolution) — transport-portable and already the single ingress that mints a principal. The 15 chokepoints are **enforcement**, not **verification**, points.

**Evidence**:
- All 15 gates call `require_cap(&ctx.agent_id, cap)` (`tools.rs:756,890,1003,1226,1364,1573,1673,1793,1898,2241,2365,3864,4104,4330`); `ctx.agent_id` set once at `server.rs:599`. An upstream-resolved principal reaches every site — no per-site verifier needed.
- The gate re-resolves caps from the registry by agent_id (`infra/registry.rs:81-100` → `store.agent_get`), ignoring asserted caps/TrustLevel. Verification can't meaningfully live at the gate — the gate trusts the *string*; the string's authenticity must precede it.
- Bearer already mints `ResolvedIdentity` into extensions (`auth.rs:250-253`); `external_identity: Option<&ResolvedIdentity>` (`server.rs:526-533`) is the wire to carry it — but all 15 sites pass `None` (dormant seam, ass-100 Q5).
- Two depths exist: transport termination (`listener.rs:195`/`tls.rs:69`, mTLS = whole connection) vs identity resolution (`auth.rs:198`→`server.rs:526`, per-agent claim inside the connection).

**Recommendation**: Verify at **identity resolution (bearer/auth layer) feeding `external_identity=Some`** as primary; optionally add channel mTLS at transport termination as defense-in-depth for the delegating-authority connection. Verify the edge-signed `_attested_identity` (or controller client-cert identity) in/adjacent to `auth.rs:198`, build the verified `ResolvedIdentity`, thread it as `Some` into all `build_context_with_external_identity` sites (removing hardcoded `None`). Do **not** add verification at the 15 `require_cap` sites. Size **schema+handler** (in-band signed assertion) or **transport** (mTLS). Seam: `http/auth.rs:198` + `server.rs:526`.

**Reverse-proxy header-injection hazard (flagged)**:
- Server supports **proxy-terminated mode** — plain HTTP when acceptor is `None` (`http/listener.rs:198-201`), i.e. TLS terminates upstream. Safe **today**: identity comes only from the validated bearer via extensions (`auth.rs:250-253`, read `handlers.rs:101-111`); **no** `X-Forwarded-*`/`Forwarded`/client-cert-header handling exists (grep-confirmed empty).
- Hazard appears **the instant mTLS is the anchor in proxy-terminated deployments**: TLS terminates upstream, so client-cert identity must be forwarded via a header (`X-SSL-Client-*`) — an **injection surface**; any party reaching the plain-HTTP port directly (bypassing the proxy) can forge it. Internal-seam implication: **mTLS-as-anchor is only sound with in-process TLS termination** (`listener.rs:195`; `tls.rs:69`→client-cert verifier). Proxy-terminated client-cert headers must be treated as untrusted unless the proxy→server hop is authenticated **and** direct connections are refused — neither exists today.
- Consequence: the **edge-signed assertion (anchor B) is immune** — verified end-to-end regardless of TLS termination — a concrete point in its favor behind a reverse proxy. Never read client identity from a forwarded header without an authenticated proxy hop.

### Q: (SCOPE Q6) Same `enabled` gate as ass-100's ceiling; OFF ⇒ bearer-only, audit-only, unchanged (ADR-008). Fail-closed vs fail-open — what the seams make natural.

**Answer**: **Confirmed — same single `enforce_external_identity` flag on `AgentsConfig` (default `false`).** OFF ⇒ verifier never runs, `external_identity` stays `None`, bearer-only channel auth, `_attested_identity` audit-only — ADR-008 posture (#5609) preserved, OSS default unchanged. On failure handling: the existing seams make **fail-OPEN the default gravity (the wrong answer)**, so **fail-closed must be explicit and net-new** — its natural home is the bearer layer's existing uniform-401 reject path.

**Evidence**:
- ass-100 Q6 sized `enforce_external_identity: bool` on `AgentsConfig` (`config.rs:432-442`, struct-level `#[serde(default)]`), backward-compatible, **schema+handler**. Same flag decides whether the verifier runs and whether `external_identity` is `Some` vs `None`.
- **Startup-time, global scope caveat**: `AgentsConfig` is not read per-request — consumed once at startup (`main.rs:853-854,939-942`), frozen into `AgentRegistry.permissive`/`.session_caps` (`registry.rs:30-50`). Per-slug scoping blocked on deferred D2 overlay: `ProjectConfigEntry` is slug-only (`config.rs:123-127`).
- **Fail-open is built-in**: `resolve_or_enroll` auto-enrolls unknown agent_id as `TrustLevel::Restricted` with default caps (permissive ⇒ `[Read,Write,Search]`, `unimatrix-store/src/registry.rs:114-155`). If verification fails but the request still reaches `resolve_agent`, the id is auto-granted. `TrustLevel` never consulted at the gate — no fail-closed backstop.
- **Fail-closed has a natural home**: bearer layer already rejects bad credentials with a uniform 401 *before* context is built (`auth.rs:269-281`).

**Recommendation**: Gate the anchor on the same `enforce_external_identity` flag.
- **OFF (default)**: no verification; `external_identity=None`; bearer-only; `_attested_identity` audit column only (ass-100 floor). Unchanged. **no-change** to posture.
- **ON**: verify the anchor at the **auth layer** and **fail closed there** — absent/invalid/unpinned anchor ⇒ 401/403 reject *before* `build_context` (reuse `auth.rs:269-281`), so failure never reaches `resolve_or_enroll` auto-grant; only a verified identity is threaded as `Some`. Size **schema+handler** (flag + fail-closed wiring). The *enforcement behavior behind ON* (gate honoring the asserted principal / live TrustLevel) stays **architectural** per ass-100 Q5, out of this anchor's scope.
- Per-slug scoping waits on D2; ship global-at-startup first. Seam: `config.rs:432` + `http/auth.rs:198,269`.

### Q: (SCOPE Q2, internal half) Which model — delegation/one-root vs per-agent/N-roots — do the existing seams (TrustLevel, `build_context_with_external_identity`, the bearer path) more naturally support? Size both.

**Answer**: The existing seams overwhelmingly favor **delegation / one-root**. Every relevant seam is single-principal-shaped: one static bearer, one `ResolvedIdentity` per connection, a single `Option<&ResolvedIdentity>` context param, one pinned cert, one Jurati process. **Per-agent / N-roots is an architectural build** with no supporting scaffolding. Smaller/safer first increment: delegation.

**Evidence** (each named seam, sized against current code):
- **Bearer path** — one static token authenticates one connection, mints one `ResolvedIdentity{"http-bearer"}` (`auth.rs:110-137,250`). Single-root; extending it to vouch is **handler-only/schema+handler**. Verifying N per-agent credentials here is not expressible.
- **`build_context_with_external_identity`** — single `external_identity: Option<&ResolvedIdentity>` (`server.rs:526-533`), one `ctx.agent_id` (`:599`). Delegation drops in (**schema+handler**); N-roots need this seam to verify per-call credentials and manage a set (**architectural**).
- **`TrustLevel`** — carried per identity for audit/usage only, never read at the gate (`registry.rs:81-100`; audit `server.rs:568,600`). Under delegation the controller is the anchor and per-agent TrustLevel stays audit-only — no gate change for the binding. Under N-roots, per-agent trust must go live at the gate — ass-100 Q5's **architectural** rewrite.
- **Auto-enroll** — fail-opens for unknown ids (`store/registry.rs:114-155`). N-roots must additionally *close* this per agent (provisioning replaces auto-enroll) — **architectural**.

**Sizing summary**:
- **Delegation / one-root**: reuse bearer channel root (or add one pinned controller cert/key); thread the one vouched identity into `external_identity`. Binding = **handler-only** (bearer-as-root) to **transport** (mTLS cert); attribution capture = **schema+handler**. Small, Jurati-aligned.
- **Per-agent / N-roots**: new per-agent key management, issuance/verification lifecycle, per-agent registry provisioning, live-trust gate rewrite. **Architectural** across subsystems; no current scaffolding.

**Recommendation**: **Delegation / one-root** is the smaller, safer, shape-aligned first increment the seams naturally support and what Jurati controller-first requires (Q4). Present N-roots as the higher-assurance ceiling; do not pre-commit or pre-build. Confirms ass-100 Q8. Seam: `server.rs:526`, `auth.rs`, `registry.rs`.

---

## Unanswered Questions
- **SCOPE Q3 (provisioning & rotation)** is the **external track's** to sketch. Internal constraint that bounds it: cert rotation **invalidates the pin** in the shipped model (ADR-002/#4948, "rotation = re-bundle + re-init"), so any mTLS-client-cert anchor inherits the same cost; the server has **no live cert-reload path** (acceptor built once, `tls.rs:34-80`). Flagged for synthesis, not solved here.
- **STDIO anchor beyond process trust** assessed only to the divergence point (UDS peer-cred UID, `engine/auth.rs:97`); full STDIO trust is a separate transport track, not designed here.

## Out-of-Scope Discoveries
- **The bearer layer already mints a `ResolvedIdentity` the context builder ignores.** `auth.rs:250-253` inserts `ResolvedIdentity{"http-bearer",Restricted}` into extensions, but every `build_context_with_external_identity` site passes `None` (`server.rs:526`), dropping it. This is a *shorter-than-expected* path to wiring `external_identity=Some` for delegation — the identity exists at the auth layer; only threading is missing. *Not pursued.*
- **"Proxy-terminated mode" is a latent identity-trust cliff.** The `None`-acceptor plain-HTTP path (`listener.rs:198-201`) is safe today only because no identity header is read; it becomes an injection surface the instant header-forwarded identity (mTLS-behind-proxy) is introduced. Any client-cert anchor design must decide the proxy-terminated trust story. *Flagged, not pursued.*

## Recommendations Summary
- **Q1 (internal)**: HTTP TLS terminates in-process (`listener.rs:195`), one-directional server-leaf pin, `with_no_client_auth` (no mTLS); bearer per-request above it (`auth.rs:198`). STDIO has no TLS/auth — only UDS peer-cred UID (`engine/auth.rs:97`) is a real anchor, on the socket not stdio. Two distinct anchor surfaces; don't force one mechanism.
- **Q4**: Minimal binding = one credential for one process — collapses Q2/Q8 to single-root delegation for v1 (**confirmed**). Bearer-as-root = handler-only/schema+handler (symmetric caveat); asymmetric controller cert = transport, signed assertion = schema+handler. Seam: `auth.rs`/`server.rs:526`.
- **Q5**: Verify **once upstream** at identity resolution (bearer/auth → `external_identity=Some`), not at the 15 `require_cap` sites. **schema+handler** (signed assertion) / **transport** (mTLS). Reverse-proxy hazard: safe today (no identity header read); mTLS-behind-proxy makes a client-cert header an injection surface — mTLS only sound with in-process TLS termination; signed-assertion anchor is immune. Seam: `http/auth.rs:198` + `server.rs:526`; hazard at `listener.rs:198-201`.
- **Q6**: Same `enforce_external_identity` flag on `AgentsConfig` (OFF ⇒ bearer-only, audit-only, ADR-008 unchanged) — **confirmed**; **schema+handler**, global-at-startup (per-slug blocked on D2). Seams make **fail-OPEN** natural (`resolve_or_enroll` auto-grants); **fail-closed must be explicit** — reuse the bearer 401 reject (`auth.rs:269`) so failure never reaches auto-enroll. Seam: `config.rs:432` + `http/auth.rs:269`.
- **Q2 (internal)**: Every seam (single bearer, single-`Option` context param, single pinned cert, unused-at-gate TrustLevel, one Jurati process) is single-principal-shaped ⇒ **delegation / one-root** naturally supported, smaller/safer first increment (**schema+handler**→**transport**); **per-agent / N-roots** is **architectural** with no scaffolding. Confirms ass-100 Q8.
