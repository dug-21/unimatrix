# FINDINGS: Root-of-trust establishment + secure edge binding

**Spike**: ass-101
**Date**: 2026-07-17
**Approach**: design-research — ecosystem/prior-art evaluation + feasibility read of the Unimatrix server's TLS/transport/identity seams (no build)
**Confidence**: directional (seam facts code-verified at file:line; library maturity/versions verified against primary sources; recommendations are options + sizing, not a build)
**Mode**: SYNTHESIS of dual-track Case 3 — FINDINGS-INTERNAL.md (codebase/seam half) + FINDINGS-EXTERNAL.md (ecosystem/landscape half).

> **Framing (locked, unchanged from SCOPE):** This is options + sizing that **feeds a later decision alongside ass-100 and Jurati input. It does not settle the mechanism and does not amend any ADR** (ADR-007/ADR-008 untouched). Whatever anchor this produces is **optional, behind the same `enabled` gate as ass-100's ceiling**; default = current behavior (bearer channel auth, identity audit-only per ADR-008). Purely opt-in enterprise seam, no OSS-default posture change.

---

## Consensus (both tracks converge)

The two tracks independently arrive at the same v1 shape:

- **Anchor B — a pinned Ed25519 short-lived edge-signed assertion, verified at the auth / identity-resolution seam — is the smallest asymmetric, channel-pinned, transport-portable v1 anchor.** External: smallest asymmetric anchor (32-byte public key server-side, `jsonwebtoken` EdDSA / `ed25519-dalek`), the only one that spans HTTP **and** STDIO, proxy-immune. Internal: lands cheapest at the existing bearer/`auth.rs:198` layer feeding `external_identity=Some`, verified once upstream, transport-portable.
- **Anchor A — mTLS client cert (`rustls::WebPkiClientVerifier`) — is an optional HTTP-only channel-hardening tier.** Smallest *channel* anchor, but heavier cert lifecycle and fragile under a TLS-terminating reverse proxy. Internal: only sound with **in-process** TLS termination (`tls.rs:69` flip).
- **Anchor C — OS/process trust — is a non-asymmetric STDIO acknowledgement, not the anchor.** UDS peer-cred UID (`engine/auth.rs:97`) proves "same host, this uid," never "holds a private key." If STDIO needs a portable asymmetric anchor, that is B again.
- **Delegation / one-root is confirmed for v1, collapsing Q2 and Q8.** One credential for one edge process (Jurati controller-proxy); every `_attested_identity` it stamps becomes a trusted delegated claim by construction. Per-agent N-roots is an architectural PKI escalation with no supporting scaffolding — the paid upgrade, not v1.

**Two open channel-binding questions carried to the decision (both tracks flag them, neither settles):**
1. **Channel-binding claim mechanism** — server-cert fingerprint claim vs a fresh per-connection server nonce echoed in the assertion. Both viable; choosing needs the internal read of how the connection exposes the server cert / whether a challenge round-trip fits the handshake.
2. **mTLS proxy-termination trust story** — if mTLS (A) is ever adopted behind a TLS-terminating proxy, the client-cert identity must be forwarded via header, a first-class injection surface. Requires an authenticated proxy→backend hop and root-scope header strip. Anchor B is immune; A's proxy story is documented operator responsibility.

---

## Findings

### Q1 — Anchor recommendation × transport (with sizing)

**Answer**: Two distinct anchor surfaces; **a single mechanism does not cleanly cover both transports at the channel layer** — the scope's transport-dependence claim is confirmed both in the ecosystem and in code.

- **HTTP TLS terminates in-process** in the hyper accept loop on a first-boot self-signed rustls cert; trust is one-directional (client pins server leaf; server does `with_no_client_auth` — no mTLS). Bearer is a separate per-request layer above TLS that already mints `ResolvedIdentity{"http-bearer",Restricted}` into request extensions.
  - *Internal evidence*: TLS accept `http/listener.rs:193-196`; acceptor `http/tls.rs:34-80`; leaf-DER SHA-256 pin `http/tls.rs:130-136`; no-mTLS `http/tls.rs:69`; bearer `http/auth.rs:143,198-262`, mint `auth.rs:250-253`; outermost layer `main.rs:1489-1490`.
- **STDIO has no TLS and no bearer** — raw JSON-RPC (`main.rs:1991`), bridged stdio→daemon UDS (`bridge.rs:146-156`). Its only verifiable channel anchor is OS/process-level (UDS `SO_PEERCRED` UID at `engine/auth.rs:97`), which guards the socket, not stdio itself.
- **Ecosystem maturity**: all three anchors backed by mature Rust crates. B is the smallest asymmetric anchor (one 32-byte Ed25519 pubkey in config; payload-level verification identical over HTTP and STDIO). A is the smallest *channel* anchor for HTTP-direct but proxy-fragile. C is OS-native, near-zero weight, but not asymmetric and not cryptographically channel-pinned.

**Recommendation per transport**:
- **HTTP**: primary = **(B) pinned Ed25519 short-lived signed assertion**, verified in/adjacent to the bearer layer (`auth.rs:198`) feeding `external_identity=Some`. **Change-size: schema+handler. Seam: `http/auth.rs:198` + `server.rs:526`.** Optional hardening tier = **(A) mTLS** by flipping `tls.rs:69` `with_no_client_auth`→`with_client_cert_verifier`. **Change-size: transport. Seam: `http/tls.rs:69`.**
- **STDIO**: OS/process trust (C) supplies channel confidence but **does not suffice** as an asymmetric anchor; **(B) is still required** for a portable asymmetric anchor, verified in-body at the `build_context_with_external_identity` seam. **Change-size: schema+handler. Seam: `server.rs:526`.** (C acknowledged as the shipped local reality at `engine/auth.rs:97`; STDIO otherwise remains a separate transport track.)

### Q2 — Delegation vs per-agent root (carries ass-100 Q8)

**Answer**: **Delegation / one-root** — the edge is a delegating authority; the server trusts one root, the edge vouches for the per-agent `sub`s it stamps. Both the ecosystem precedent and every relevant Unimatrix seam favor it. Per-agent / N-roots is an architectural PKI build with no supporting scaffolding.

- **Ecosystem**: OIDC (trust the issuer, not each user), SPIFFE trust domains, SSH certificate authorities, K8s service-account issuance all trust one *authority* and treat per-principal identity as signed data inside the envelope. Delegation = one provisioning event, one rotation lifecycle; N agents add zero marginal credential cost (the per-agent claim is a `sub` field, not a new key). Failure mode: edge-key compromise impersonates all agents (blast radius = whole edge authority), mitigated by short-lived signing + rotation; no per-agent crypto revocation granularity. Per-agent-at-server = N provisioning events, N rotation lifecycles, a CSR/EST/ACME-shaped enrollment workflow — this **is** a PKI, with tight per-agent blast radius as the only upside, justified only when a regulated requirement demands per-agent crypto revocation.
- **Internal seams (all single-principal-shaped)**: one static bearer minting one `ResolvedIdentity` (`auth.rs:110-137,250`); a single `external_identity: Option<&ResolvedIdentity>` context param and one `ctx.agent_id` (`server.rs:526-533,599`); one pinned cert; `TrustLevel` carried per identity for audit only, never read at the gate (`registry.rs:81-100`); one Jurati process. Delegation drops into these seams; N-roots needs the context seam to verify per-call credentials and manage a set, must additionally *close* the auto-enroll fail-open per agent (`store/registry.rs:114-155`), and take per-agent trust live at the gate — ass-100 Q5's architectural rewrite.

**Sizing**:
- **Delegation / one-root**: reuse bearer channel root (or add one pinned controller cert/key), thread the one vouched identity into `external_identity`. Binding = **handler-only** (bearer-as-root, symmetric caveat) to **transport** (mTLS cert); attribution capture = **schema+handler**. Small, Jurati-aligned. **Seam: `server.rs:526`, `auth.rs`, `registry.rs`.**
- **Per-agent / N-roots**: new per-agent key management, issuance/verification lifecycle, per-agent registry provisioning, live-trust gate rewrite. **Change-size: architectural** across subsystems; no current scaffolding.

**Recommendation**: **Delegation / one-root is the smaller, safer first increment** the seams naturally support and Jurati controller-first requires. Present N-roots as the higher-assurance ceiling; do not pre-commit or pre-build. **Confirms ass-100 Q8.**

### Q3 — Provisioning & rotation lifecycle (the primary cost driver)

**Answer**: Rotation, not verification, is where each anchor's cost concentrates. Ranked lightest→heaviest: pinned-key rollover < short-lived assertions (fold revocation into expiry) < JWKS key rotation < mTLS CA/leaf issuance. The recommended pinned-key + short-lived-assertion path reduces the cost driver to **a config paste plus a two-key overlap window — no CA, no JWKS endpoint, no revocation infra**.

- **Pinned-key rollover (lightest)**: edge generates an Ed25519 keypair once at install; operator pastes the public half into the server's `enabled`-gated trust block. Rotation: config accepts a small **set** (current + next) — operator adds the new pubkey, edge cuts over, operator removes the old; overlap window = zero downtime. Matches how the bearer token is already handled and the OSS "coarse" posture.
- **Short-lived assertions**: assertions carry a short `exp` (minutes) — **expiry is the revocation mechanism**, no CRL/OCSP/revocation list. The signing key still rotates via pinned-key rollover; individual assertions self-expire. Bind each to the channel (`aud` = server, optional server-cert-fingerprint claim) plus `exp` and a nonce to defeat replay.
- **JWKS key rotation**: standard (OIDC) but **pull-based** — the verifier fetches from the signer. Topology mismatch: here the **edge dials out to the server**, so the edge often has no address the server can pull from. Pushes toward pinned-key or in-band JWK push (edge presents its JWK on the connection, server pins by fingerprint), not a hosted JWKS endpoint.
- **mTLS CA/leaf (heaviest)**: stand up a CA (rotate leaves without touching the server, but operate a mini-PKI) or skip the CA and pin the leaf/SPKI directly (coordinated pin-rollover with overlap, simpler infra, more coupling). CRL/OCSP is the heavy tail; for a single edge, skip it and rotate the pin.

**Lifecycle sketch for the recommended anchor (pinned Ed25519 + short-lived assertion)**:
1. Edge generates an Ed25519 keypair at install; private key stays on the edge at today's bearer-token file-perm discipline (0600).
2. Operator pastes the edge **public** key into the server's `enabled`-gated trust-anchor block. Server holds only the public half — asymmetric, satisfied.
3. Per session/request the edge mints a short-lived assertion: `sub` = agent id, `iss` = edge id, `aud` = server, `exp` = now + minutes, plus a channel-binding claim (server-cert fingerprint); signs with the private key.
4. Server verifies the signature against the pinned public key, checks `exp`/`aud`/channel-binding, resolves `sub` as external identity. Fail-closed when the gate is on and verification fails.
5. Rotation: add the next pubkey to the trust set → edge cuts over → drop the old. No endpoint, no CA, no downtime.

**Internal constraint that bounds this sketch**: the shipped mTLS-style pin has **no live cert-reload path** — the rustls acceptor is built once (`tls.rs:34-80`) and **rotation invalidates the pin** (ADR-002/#4948: "rotation = re-bundle + re-init"). Any mTLS-client-cert anchor (A) inherits this restart-to-rotate cost. The pinned-key/signed-assertion anchor (B) avoids it: the trust set lives in the `enabled`-gated config block, not baked into the TLS acceptor, so a two-key overlap rotates without a connection-layer rebuild.

**Recommendation**: Center the **pinned-key + short-lived-assertion** lifecycle. Delivers asymmetric + channel-pinned + rotatable + "revocable via expiry" with no PKI and no hosted endpoint. **Change-size: schema+handler** (trust-anchor config block + verifier), bounded by the no-live-reload constraint (which B sidesteps and A does not).

### Q4 — Extend to Jurati (controller-first): the minimal binding

**Answer**: Minimal binding = **one credential for one process** — authenticate the single controller-proxy connection and every `_attested_identity` it stamps becomes a trusted delegated claim by construction. **Yes — this collapses Q2/Q8 to a single-root delegation model for v1.** One edge process = one root; per-agent (N-root) credentials are unnecessary.

- **Jurati Mode B/P2**: one proxy holds the pinned HTTPS session + bearer and stamps `_attested_identity`; server returned `ok:true` (ass-100). One process = one trust boundary. `_attested_identity` is a **delegation artifact by construction** — no independent per-agent crypto — exactly what single-root delegation consumes.
- **Server shape already matches**: bearer resolves the connection to a single `ResolvedIdentity` (`auth.rs:250`); `build_context_with_external_identity` takes a single `Option<&ResolvedIdentity>` (`server.rs:526-533`) — shaped for one delegated identity per call. ADR-007 (#4361) JWT-`sub` replaces the *source* of that single per-connection attribution field.
- **Forward path (ecosystem)**: SPIFFE's JWT-SVID *is* a signed `sub` assertion = anchor B; keeping a JWT-shaped carrier means a future JWT-SVID drops in as a key-source swap, not a redesign.

**Recommendation** (cheapest-first):
1. **Bearer-as-root (interim)**: accept the already-authenticated bearer connection as the delegating authority; thread its existing extensions `ResolvedIdentity` into `external_identity=Some` and capture `_attested_identity` as the delegated subject. **Change-size: schema+handler** (`tools.rs` params + `server.rs:526`). **Caveat (SCOPE principle 1)**: bearer is symmetric — cannot prove *the edge* spoke; leaked token = full impersonation. Audit-grade delegation, not an asymmetric anchor.
2. **Asymmetric single-root (the real anchor)**: give the controller an asymmetric credential verified against a pinned public half — a pinned Ed25519 signed assertion at the auth/handler seam (**schema+handler**), or a controller client-cert pinned server-side by flipping `tls.rs:69` (**transport**). One cert/key for one process — the smallest asymmetric, channel-pinned anchor.

**Q2/Q8 collapse — confirmed**: v1 is single-root delegation; per-agent N-root credentials are not required for Jurati and should not be built for v1. **Seam: `auth.rs`, `server.rs:526`.**

### Q5 — Verifier placement (relative to `build_context_with_external_identity` and the 15 `require_cap` sites)

**Answer**: **Verify once, upstream — not at the 15 chokepoints.** The channel anchor verifies at/near transport termination (mTLS at `tls.rs`) or at the identity-resolution layer (bearer/`auth.rs` for an in-band signed assertion); the resolved principal flows through `build_context_with_external_identity` into `ctx.agent_id`, and all 15 `require_cap` sites consume it downstream without re-verifying. The 15 chokepoints are **enforcement**, not **verification**, points. Lowest-cost placement is the existing bearer layer feeding `external_identity=Some` (identity resolution) — transport-portable and already the single ingress that mints a principal.

- **Evidence**: all 15 gates call `require_cap(&ctx.agent_id, cap)` (`tools.rs:756,890,1003,1226,1364,1573,1673,1793,1898,2241,2365,3864,4104,4330`); `ctx.agent_id` set once at `server.rs:599`. The gate re-resolves caps from the registry by agent_id (`infra/registry.rs:81-100`), ignoring asserted caps/TrustLevel — it trusts the *string*, so the string's authenticity must precede it. Bearer already mints `ResolvedIdentity` into extensions (`auth.rs:250-253`); `external_identity: Option<&ResolvedIdentity>` (`server.rs:526-533`) is the wire to carry it, but all 15 sites currently pass `None` (dormant seam, ass-100 Q5).
- **Two depths exist**: transport termination (`listener.rs:195`/`tls.rs:69`, mTLS = whole connection) vs identity resolution (`auth.rs:198`→`server.rs:526`, per-agent claim inside the connection).

**Recommendation**: Verify at **identity resolution (bearer/auth layer) feeding `external_identity=Some`** as primary — verify the edge-signed `_attested_identity` in/adjacent to `auth.rs:198`, build the verified `ResolvedIdentity`, thread it as `Some` into all `build_context_with_external_identity` sites (removing hardcoded `None`). Optionally add channel mTLS at transport termination as defense-in-depth. **Do not** add verification at the 15 `require_cap` sites. **Change-size: schema+handler** (in-band signed assertion) or **transport** (mTLS). **Seam: `http/auth.rs:198` + `server.rs:526`.**

**Reverse-proxy header-injection hazard (both tracks flag)**:
- Server supports **proxy-terminated mode** — plain HTTP when acceptor is `None` (`listener.rs:198-201`), TLS terminating upstream. Safe **today**: identity comes only from the validated bearer via extensions (`auth.rs:250-253`); **no** `X-Forwarded-*`/`Forwarded`/client-cert-header handling exists (grep-confirmed empty).
- Hazard appears **the instant mTLS (A) is the anchor in proxy-terminated deployments**: the client cert vanishes from the in-process verifier, so identity must be forwarded via a header (nginx `$ssl_client_*` → `X-Client-Cert`; Envoy XFCC) — an injection surface. Ecosystem pitfalls (Deutsche Telekom header-smuggling research + nginx docs): direct spoof if the proxy doesn't unconditionally strip/reset at **root/server scope**; underscore/hyphen normalization differential; path-parsing (matrix-param) differential; header duplication. Correct pattern: proxy does mTLS + validates, unconditionally strips inbound instances, sets from validated cert vars, **and the proxy→backend hop is itself authenticated** (Envoy XFCC `sanitize_set`/`forward_only` on a private network is canonical).
- **Consequence**: mTLS-as-anchor is only sound with **in-process TLS termination**; proxy-terminated client-cert headers must be treated as untrusted unless the proxy→server hop is authenticated **and** direct connections refused — neither exists today. The **edge-signed assertion (B) is immune** — signed end-to-end inside the payload, a terminating proxy cannot forge it and forwards nothing special. Decisive robustness argument for B; A's proxy story is documented operator responsibility, not server-enforced.

### Q6 — Gate shape (consistency with ass-100)

**Answer**: **Confirmed — same single `enforce_external_identity` flag on `AgentsConfig` (default `false`).** OFF ⇒ verifier never runs, `external_identity` stays `None`, bearer-only channel auth, `_attested_identity` audit-only — ADR-008 posture (#5609) preserved, OSS default unchanged. On failure handling: the existing seams make **fail-OPEN the default gravity (the wrong answer)**, so **fail-closed must be explicit and net-new** — its natural home is the bearer layer's existing uniform-401 reject path.

- **Evidence**: ass-100 Q6 sized `enforce_external_identity: bool` on `AgentsConfig` (`config.rs:432-442`, `#[serde(default)]`), backward-compatible, schema+handler. Same flag decides whether the verifier runs and whether `external_identity` is `Some` vs `None`. **Startup-time, global-scope caveat**: `AgentsConfig` is read once at startup (`main.rs:853-854,939-942`), frozen into `AgentRegistry` (`registry.rs:30-50`); per-slug scoping is blocked on the deferred D2 overlay (`ProjectConfigEntry` is slug-only, `config.rs:123-127`). **Fail-open is built-in**: `resolve_or_enroll` auto-enrolls unknown agent_id as `TrustLevel::Restricted` with default caps (`store/registry.rs:114-155`); `TrustLevel` never consulted at the gate — no fail-closed backstop. **Fail-closed has a natural home**: the bearer layer already rejects bad credentials with a uniform 401 *before* context is built (`auth.rs:269-281`).

**Recommendation**: Gate the anchor on the same `enforce_external_identity` flag.
- **OFF (default)**: no verification; `external_identity=None`; bearer-only; `_attested_identity` audit column only (ass-100 floor). **no-change** to posture.
- **ON**: verify the anchor at the **auth layer** and **fail closed there** — absent/invalid/unpinned anchor ⇒ 401/403 reject *before* `build_context` (reuse `auth.rs:269-281`), so failure never reaches the `resolve_or_enroll` auto-grant; only a verified identity is threaded as `Some`. **Change-size: schema+handler** (flag + fail-closed wiring). Per-slug scoping waits on D2 — ship global-at-startup first. The *enforcement behavior behind ON* (gate honoring the asserted principal / live TrustLevel) stays **architectural** per ass-100 Q5, out of this anchor's scope. **Seam: `config.rs:432` + `http/auth.rs:198,269`.**

---

## Transport matrix (where one mechanism does / doesn't cover both)

| Anchor | HTTP | STDIO | Asymmetric | Channel-pinned | Proxy-safe | v1 role |
|---|---|---|---|---|---|---|
| **(B) Pinned Ed25519 signed assertion** | Yes (verify at `auth.rs:198`→`external_identity`) | Yes (verify in-body at `server.rs:526`) | Yes | Yes (via `aud`/cert-fp claim + `exp`) | Yes (immune — end-to-end signature) | **Recommended v1 anchor, both transports** |
| **(A) mTLS client cert** | Yes (flip `tls.rs:69`) | No (no TLS on STDIO) | Yes | Yes (channel) | No (header-forward injection under terminating proxy) | Optional HTTP-only channel-hardening tier, in-process termination only |
| **(C) OS/process trust** | No | Yes (UDS `SO_PEERCRED` UID `engine/auth.rs:97`) | **No** | No (ambient OS authority) | n/a | STDIO acknowledgement, not the anchor |

**Key finding**: at the *channel* layer, A is HTTP-only and C is STDIO-only — **no single channel mechanism covers both**. Only the signed-assertion layer (B) spans HTTP and STDIO unchanged. This confirms the SCOPE transport-dependence claim: the spike says so explicitly rather than forcing one answer.

---

## Recommended v1 anchor per transport (at a glance)

- **HTTP**: **(B) pinned Ed25519 short-lived signed assertion** at the auth/identity-resolution seam (schema+handler; `auth.rs:198` + `server.rs:526`). Optional: **(A) mTLS** for HTTP-direct hardening (transport; `tls.rs:69`), in-process termination only.
- **STDIO**: **(B)** in-body at `build_context_with_external_identity` (schema+handler; `server.rs:526`). **(C)** UDS peer-cred is the acknowledged local channel reality, not an asymmetric anchor.
- **Delegation model**: single-root (one credential for the one edge/controller process); N-roots deferred as architectural.

---

## Unanswered Questions

- **Channel-binding claim mechanism** — server-cert-fingerprint claim vs a fresh per-connection server nonce echoed by the assertion. Both viable; picking one needs the internal read of how the connection exposes the server cert / whether a challenge round-trip fits the handshake. (Both tracks flag; carried to the decision.)
- **mTLS proxy-termination trust story** — if A is ever adopted behind a terminating proxy, the proxy→backend hop must be authenticated and identity headers stripped at root scope; the server enforces none of this today. Documented operator responsibility if A is chosen; moot if B is the anchor.
- **No live cert-reload path** — the rustls acceptor is built once (`tls.rs:34-80`); rotation invalidates the pin (ADR-002/#4948). Bounds any mTLS-client-cert (A) rotation to restart-to-rotate; B sidesteps it via config-block trust set. Flagged, not solved.
- **STDIO anchor beyond process trust** — assessed only to the divergence point (UDS peer-cred UID, `engine/auth.rs:97`); full STDIO trust is a separate transport track, not designed here.
- **Enforcement behavior behind the ON gate** (gate honoring the asserted principal / live per-agent TrustLevel) stays architectural per ass-100 Q5 — out of this anchor's scope.

---

## Out-of-Scope Discoveries

- **The bearer layer already mints a `ResolvedIdentity` the context builder ignores.** `auth.rs:250-253` inserts `ResolvedIdentity{"http-bearer",Restricted}` into extensions, but every `build_context_with_external_identity` site passes `None` (`server.rs:526`), dropping it. A shorter-than-expected path to wiring `external_identity=Some` for delegation — the identity exists at the auth layer; only threading is missing. *Not pursued.*
- **"Proxy-terminated mode" is a latent identity-trust cliff.** The `None`-acceptor plain-HTTP path (`listener.rs:198-201`) is safe today only because no identity header is read; it becomes an injection surface the instant header-forwarded identity (mTLS-behind-proxy) is introduced. Any client-cert anchor design must decide the proxy-terminated trust story. *Flagged, not pursued.*
- **`spiffe-rustls` as a future integration seam** — bridges SPIFFE SVIDs into the rustls config the server already uses if workload identity is ever adopted. Because a JWT-shaped carrier (B) matches the SPIFFE JWT-SVID, adopting SPIRE later is a key-source swap, not a redesign. Forward-compat data point, not a new spike.
- **Envoy XFCC `sanitize_set`/`forward_only`** is the reference implementation of safe client-cert forwarding — cite verbatim in any future operator doc if mTLS-behind-proxy is ever supported. Doc concern, not a spike.

---

## Recommendations Summary

- **Q1 (anchor × transport)**: Two distinct anchor surfaces; no single channel mechanism covers both. HTTP TLS terminates in-process (`listener.rs:195`, `with_no_client_auth`); STDIO has no TLS/bearer. Recommend **(B) pinned Ed25519 signed assertion** as the portable primary (**schema+handler**, `auth.rs:198`/`server.rs:526`) for both transports; **(A) mTLS** as optional HTTP-only hardening (**transport**, `tls.rs:69`); **(C)** UDS peer-cred acknowledged for STDIO, not the anchor.
- **Q2 (delegation vs N-roots)**: **Delegation / one-root** — every seam is single-principal-shaped and the ecosystem (OIDC/SPIFFE/SSH-CA) starts there. Smaller/safer first increment (**handler-only→transport** binding, **schema+handler** attribution). N-roots is a full PKI, **architectural**, deferred. Confirms ass-100 Q8.
- **Q3 (provisioning & rotation)**: Center the **pinned-key + short-lived-assertion** lifecycle — asymmetric, channel-pinned, rotatable via a two-key config overlap, "revocable via expiry," no CA/JWKS/revocation infra (**schema+handler**). Bounded by the internal no-live-cert-reload constraint (which burdens mTLS-A with restart-to-rotate and which B sidesteps). mTLS CA/leaf is heaviest; JWKS-pull mismatches the edge-dials-out topology.
- **Q4 (Jurati)**: Minimal binding = **one credential for one process**; collapses Q2/Q8 to **single-root delegation for v1** (**confirmed**). Bearer-as-root interim = **schema+handler** (symmetric caveat); asymmetric controller cert = **transport**, signed assertion = **schema+handler**. Seam: `auth.rs`/`server.rs:526`.
- **Q5 (verifier placement)**: Verify **once upstream** at identity resolution (bearer/auth → `external_identity=Some`), **not** at the 15 `require_cap` enforcement sites (**schema+handler** / **transport**; `auth.rs:198` + `server.rs:526`). Reverse-proxy hazard: safe today (no identity header read); mTLS-behind-proxy makes a client-cert header an injection surface — A sound only with in-process termination; **B is immune**.
- **Q6 (gate shape)**: Same `enforce_external_identity` flag on `AgentsConfig` (OFF ⇒ bearer-only, audit-only, ADR-008 unchanged) — **confirmed**, **schema+handler**, global-at-startup (per-slug blocked on D2). Seams make **fail-OPEN** natural (`resolve_or_enroll` auto-grants); **fail-closed must be explicit** — reuse the bearer 401 reject (`auth.rs:269`) so failure never reaches auto-enroll. Seam: `config.rs:432` + `http/auth.rs:269`.

*Options + sizing — feeds the later decision alongside ass-100 and Jurati input. Does not settle the mechanism or amend any ADR.*
