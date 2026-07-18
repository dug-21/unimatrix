# FINDINGS (EXTERNAL): Root-of-trust establishment + secure edge binding

**Spike**: ass-101
**Track**: EXTERNAL (dual-track Case 3 — ecosystem/prior-art half; Unimatrix seams land in FINDINGS-INTERNAL.md)
**Date**: 2026-07-17
**Approach**: design-research — ecosystem/prior-art evaluation
**Confidence**: directional (library maturity/versions verified against primary sources; no PoC per scope's "no build")

## Scope of this track

Answers the ecosystem/landscape half only: anchor maturity + Rust library support (Q1 external), provisioning/rotation lifecycle (Q3), delegation-vs-per-agent as a general PKI/token pattern (Q2 external), SPIFFE/SPIRE weight assessment, and the reverse-proxy header-injection hazard. Verifier placement inside `server.rs`, the 15 `require_cap` sites, and the actual TLS-termination seam are the internal track's — not read here.

## Findings

### Q1 (external half): Per-anchor maturity, Rust library support, weight, tradeoffs — and the smallest asymmetric, channel-pinned anchor per transport

**Answer**: All three anchors are backed by mature, actively-maintained Rust crates. They are not equivalent in what they prove or where they bind.

**(A) mTLS client certificate — HTTP, binds the channel**
- *Maturity*: highest. TLS 1.3 mutual auth (RFC 8446), ubiquitous.
- *Rust*: `rustls::server::WebPkiClientVerifier`, built via `builder(roots)` from a `RootCertStore` of trusted edge CA(s) (or a single pinned leaf). Server reads the validated chain from the connection's `peer_certificates()`. CRL support is built in; `allow_unauthenticated()` gates anonymous clients. Backend crypto via `aws-lc-rs` or `ring`. This is the mirror image of the server-cert pinning the project already ships — same rustls machinery, pointed at the client half.
- *Weight*: light **iff TLS terminates in-process** — a `RootCertStore` + verifier builder + one peer-cert read. No new dependency if already on rustls.
- *Tradeoff*: authenticates the **whole connection as one identity** (the edge), not per-agent `sub`. Fragile under a TLS-terminating reverse proxy (cert vanishes from the in-process verifier → the header-forwarding hazard, below). Cert issuance/rotation is the real cost.

**(B) Edge-signed assertion (JWT-`sub` / detached JWS) — HTTP *and* STDIO, portable, binds per-agent inside the channel**
- *Maturity*: high. JWT/JWS/JWK/JWKS are RFCs 7519/7515/7517; Ed25519 signatures via RFC 8037 (EdDSA).
- *Rust*: `jsonwebtoken` v10.4.0 (released 2026-05-11, ~2k stars, releasing steadily through 2025–2026), with a built-in `jwk` module for JWK/JWKS decode and `aws-lc-rs`/`rust_crypto` backends. **For a single *pinned* key you do not need JWKS-fetch machinery at all** — verify a signature against a configured Ed25519 public key (via `jsonwebtoken` EdDSA, or `ed25519-dalek` directly). Dedicated JWKS crates (`jwks-client`, `jwt-simple-jwks`) exist if a fetch endpoint is ever wanted.
- *Weight*: **the smallest asymmetric anchor.** Server-side state = one 32-byte Ed25519 public key in config. No CA, no chain, no revocation list, no endpoint. Verification is payload-level, so it is **identical over HTTP and STDIO** — the only anchor genuinely transport-portable.
- *Tradeoff*: verifies per-message, not per-connection (negligible CPU for Ed25519). Does not authenticate the channel by itself — rides on top of the existing bearer + server-cert-pin for channel auth. Needs replay/binding discipline: bind the assertion to the channel (`aud` = server identity or server-cert fingerprint) and give it a short `exp`.

**(C) OS / process trust — STDIO only, binds the channel via the OS**
- *Maturity*: OS-native, not a crate ecosystem. UDS peer creds (`SO_PEERCRED` Linux, `LOCAL_PEERCRED`/`getpeereid` BSD/macOS), Windows named-pipe `GetNamedPipeClientProcessId`; for inherited stdio pipes the trust is process lineage (the parent spawned the child). Rust via `tokio::net::UnixStream` + `nix`.
- *Weight*: near-zero — no crypto, no keys.
- *Tradeoff*: **not asymmetric and not cryptographically channel-pinned.** It is ambient OS authority — proves "same host, this uid/process," never "holds a private key." Against the scope's locked principle (asymmetric + channel-pinned), C does **not** satisfy the anchor requirement on its own; it is the "channel is private by construction, so no network attacker" argument. If STDIO needs a *portable asymmetric* anchor, that is B again.

**Smallest asymmetric, channel-pinned anchor per transport**
- **HTTP**: (B) pinned-key signed assertion is the smallest *asymmetric* anchor. (A) mTLS is the smallest *channel* anchor but carries a heavier cert lifecycle and proxy fragility.
- **STDIO**: (B) is the only asymmetric option. (C) supplies channel confidence but not asymmetry.
- A single mechanism does **not** cleanly cover both transports at the *channel* layer (A is HTTP-only, C is STDIO-only) — **only the signed-assertion layer (B) spans both**, confirming the scope's transport-dependence claim.

**Recommendation**: For a portable v1 anchor, use **(B) a pinned Ed25519 public key verifying a short-lived edge-signed assertion**, via `jsonwebtoken` (EdDSA) or `ed25519-dalek`. Smallest asymmetric anchor, spans HTTP and STDIO unchanged, holds only a 32-byte public key server-side. Offer **(A) mTLS via `rustls::WebPkiClientVerifier`** as an optional heavier channel-hardening tier for HTTP-direct (non-proxied) deployments. Treat **(C)** as an acknowledged STDIO reality, not an asymmetric anchor.

### Q3: Provisioning & rotation lifecycle — the primary cost driver

**Answer**: Rotation, not verification, is where each anchor's cost concentrates. Ranked lightest to heaviest:

**Pinned-key rollover (lightest)** — provisioning: edge generates an Ed25519 keypair once at install; operator pastes the public half into the server's `enabled`-gated trust block. Server holds only the public key. Rotation: config accepts a small **set** (current + next); operator adds the new pubkey, edge cuts over to the new private key, operator removes the old. Overlap window = both trusted → zero downtime. Manual but trivial; matches the OSS "coarse" posture and how the bearer token is already handled.

**Short-lived signed assertions (folds revocation into expiry)** — assertions carry a short `exp` (minutes). **Expiry is the revocation mechanism** — no CRL, no OCSP, no revocation list to host or distribute. The *signing key* still rotates via pinned-key rollover, but individual assertions self-expire. Cheapest route to "revocation" with zero infrastructure. Bind each assertion to the channel (`aud` = server, optional server-cert-fingerprint claim) plus `exp` and a nonce to defeat replay.

**JWKS key rotation (standard but assumes a reachable signer endpoint)** — edge publishes public JWKs at a URL keyed by `kid`; rotate by publishing new-alongside-old, signing with the new, matching by `kid`, retiring the old. Zero-downtime, well-trodden (OIDC). **Topology mismatch**: JWKS is *pull-based* — the verifier fetches from the signer. Here the **edge dials out to the server**, so the edge often has no address the server can pull from. Pushes toward pinned-key or **in-band push** (edge presents its JWK on the connection, server pins by fingerprint) rather than a hosted JWKS endpoint.

**mTLS CA/leaf issuance (heaviest)** — provisioning: either stand up a CA (even a one-cert self-signed root), issue an edge leaf under it, load the CA into the server's `RootCertStore`; *or* skip the CA and pin the leaf's cert/SPKI directly. Rotation: **with a CA**, rotate leaves without touching the server — but you now operate a mini-PKI (expiry monitoring, key storage, optionally CRL/OCSP). **Without a CA (leaf-pin)**, rotation is a coordinated pin-rollover on both sides with an overlap window — simpler infra, more coupling. CRL/OCSP is the heavy tail; for a single edge, skip it and rotate the pin instead.

**Lifecycle sketch for the recommended anchor (pinned Ed25519 + short-lived assertion):**
1. Edge generates an Ed25519 keypair at install; private key stays on the edge at today's bearer-token file-perm discipline (0600).
2. Operator pastes the edge **public** key into the server's `enabled`-gated trust-anchor block. Server holds only the public half — asymmetric, satisfied.
3. Per session/request the edge mints a short-lived assertion: `sub` = agent id, `iss` = edge id, `aud` = server, `exp` = now + minutes, plus a channel-binding claim (server-cert fingerprint); signs with the private key.
4. Server verifies the signature against the pinned public key, checks `exp`/`aud`/channel-binding, resolves `sub` as external identity. Fail-closed when the gate is on and verification fails.
5. Rotation: add the next pubkey to the trust set → edge cuts over → drop the old. No endpoint, no CA, no downtime.

**Recommendation**: Center the pinned-key + short-lived-assertion lifecycle. Delivers asymmetric + channel-pinned + rotatable + "revocable via expiry" with **no PKI and no hosted endpoint** — the primary cost driver reduced to a config paste plus a two-key overlap window.

### Q2 (external half): Delegation (one root) vs per-agent credentials at the server (N roots) — cost delta and failure modes

**Answer**: The ecosystem near-universally chooses delegation as the first increment; per-agent-at-verifier is an enterprise escalation, not a starting point.

**Delegation — one root, the edge vouches for the per-agent `sub`s it stamps**
- *Precedent*: OIDC (trust the issuer, not each user), SPIFFE trust domains, SSH certificate authorities, Kubernetes service-account token issuance. All trust one *authority* and treat per-principal identity as signed data inside the envelope.
- *Cost*: **one** provisioning event, **one** rotation lifecycle. N agents add zero marginal credential cost — the per-agent claim is a `sub` field, not a new key.
- *Failure mode*: compromise of the edge key impersonates **all** agents (blast radius = the whole edge authority). Mitigated by short-lived signing + rotation. No per-agent *cryptographic* revocation granularity — revoking one agent is an application-layer decision.

**Per-agent credentials terminating at the server — N roots**
- *Pattern*: each agent holds its own key/cert; server trusts N distinct anchors. (Introduce a CA to tame that and you are back to delegation-via-CA.)
- *Cost*: **N** provisioning events, **N** rotation lifecycles, an issuance/enrollment workflow (CSR/EST/ACME-shaped), per-agent key distribution. This **is** a PKI, with lifecycle automation a single-binary OSS server should not carry.
- *Failure mode*: compromise of one agent key impersonates only that agent (tight blast radius, per-agent crypto revocation possible) — the upside justifying the N× surface, but only when actually needed.

**Verdict**: **Delegation is the smaller and safer first increment.** Matches the topology exactly — one edge process (hook client / Jurati controller-proxy) is naturally the single delegating authority — and defers the whole per-agent issuance lifecycle until a regulated/enterprise requirement demands per-agent crypto revocation. Per-agent-at-server is the paid upgrade, not v1.

### SPIFFE/SPIRE assessment: too heavy for a single-binary server?

**Answer**: Yes for v1 — assessed concretely. Borrow the ideas, not the machinery.

- *What it is*: SPIFFE is the spec (SVID identity docs as X.509 **or** JWT, trust domains, a local Workload API). SPIRE is the reference implementation: a **SPIRE server** (CA + registration authority) plus a **per-node SPIRE agent** that attests workloads and issues/auto-rotates SVIDs over a local UDS. An entire control plane — two extra long-running daemons and an attestation/registration model.
- *Rust support (verified, decent)*: `rust-spiffe` (maxlambrecht) — layered crates, `X509Source`/`JwtSource` with automatic rotation off the Workload API, `spiffe-rustls` for rustls integration, `default = []` so features are opt-in. bytedance's `spire-workload-rs` is a second option. The Rust story is real and maintained; that is not the blocker.
- *Weight verdict*: **too heavy.** SPIRE solves fleet-scale problems this spike does not have. For **one edge authenticating to one server**, it is a control plane for a two-node graph: a second daemon to deploy, operate, secure, and rotate — against a product whose whole shape is a single binary.
- *What to borrow*: (a) trust-domain / delegating-authority model → the delegation recommendation (Q2); (b) short-lived auto-rotated SVIDs → short-lived-assertion recommendation (Q3); (c) the **JWT-SVID** *is* a signed `sub` assertion → anchor B. You capture ~80% of SPIFFE's value (asymmetric, channel-pinned, short-lived, delegating) from a pinned Ed25519 key + short-lived JWS with none of SPIRE's operational cost. Because the carrier is the same, the design is a **clean forward path**: adopting SPIRE's JWT-SVID later is a swap of key-source, not a redesign.

**Recommendation**: Do not adopt SPIRE for v1. Adopt its design vocabulary. Keep the anchor a JWT-shaped signed assertion so a future JWT-SVID drops in.

### Reverse-proxy header-injection hazard (external half)

**Answer**: When TLS terminates at a reverse proxy, the client cert is no longer visible to the in-process rustls verifier; identity must be forwarded in a header, which is a first-class injection surface unless handled exactly.

- *How identity gets forwarded*: nginx exposes `$ssl_client_verify` / `$ssl_client_s_dn` / the raw cert, typically set into a header like `X-Client-Cert`. Envoy uses XFCC (`x-forwarded-client-cert`). Backend reads that header as identity.
- *Pitfalls (Deutsche Telekom Security header-smuggling research + nginx docs)*:
  1. **Direct spoof** — any client can send `X-Client-Cert: <anything>`. If the proxy does not **unconditionally overwrite/strip** the header on *every* inbound request, the backend cannot distinguish proxy-set from client-set. Strip/reset at **root/server scope**, not only per-location.
  2. **Underscore/hyphen normalization differential** — a proxy that unsets `X-Client-Cert` may pass `X_Client_Cert`, which a backend framework normalizes back to the same name → smuggled. nginx drops underscore headers by default (`underscores_in_headers off`); Apache does not. Validate against the exact proxy+backend pair.
  3. **Path-parsing differential** — matrix params like `/auth/cert;foo=bar/x` parse differently proxy vs backend (Apache vs Tomcat), bypassing location-scoped rules → unset at root scope.
  4. **Header duplication/concatenation** into `X-Forwarded-*`.
- *Correct pattern*: (a) proxy performs mTLS and validates; (b) proxy **unconditionally strips all inbound instances** of the identity header; (c) proxy sets it from validated cert variables; (d) **the proxy→backend hop is itself authenticated** — mutual TLS or a shared secret header the client cannot know — so the backend trusts identity headers only on connections proven to originate from the proxy. Envoy XFCC with `sanitize_set`/`forward_only` on a private network is the canonical form.

**Design implication (favors anchor B)**: This hazard is **specific to mTLS (A) under a terminating proxy.** The edge-signed assertion (B) is **immune** — the signature is end-to-end from edge to server inside the payload; a terminating proxy cannot forge it without the edge private key and forwards nothing special. B survives reverse proxies, load balancers, and STDIO with no trusted-header dance. Decisive robustness argument for B as the portable v1 anchor, and a reason to treat A's proxy story as documented operator responsibility rather than something the server enforces.

## Recommendations Summary

- **Q1 (external)**: Smallest asymmetric, channel-pinned anchor = **(B) pinned Ed25519 public key verifying a short-lived edge-signed assertion** (`jsonwebtoken` EdDSA / `ed25519-dalek`); only anchor that spans HTTP and STDIO. **(A) mTLS** (`rustls::WebPkiClientVerifier`) is the smallest *channel* anchor for HTTP-direct but heavier and proxy-fragile — optional second tier. **(C) OS/process trust** is not asymmetric — acknowledge, don't rely on it as the anchor.
- **Q3**: Center the **pinned-key + short-lived-assertion** lifecycle — asymmetric, channel-pinned, rotatable via a two-key config overlap, "revocable via expiry," with **no CA, no JWKS endpoint, no revocation infra**. mTLS CA/leaf is heaviest; JWKS-pull mismatches the edge-dials-out topology.
- **Q2 (external)**: **Delegation (one edge root vouches for per-agent `sub`s)** is the smaller/safer first increment — one provisioning + one rotation lifecycle, matches OIDC/SPIFFE/SSH-CA precedent and the single-edge topology. Per-agent-at-server (N roots) is a full PKI and enterprise escalation; only upside is per-agent crypto revocation.
- **SPIFFE/SPIRE**: Too heavy for a single-binary server (control-plane daemon + node agents for a two-node graph). Rust support (`rust-spiffe`, `spiffe-rustls`) is real but the operational weight isn't justified. **Borrow the model**, keep a JWT-shaped carrier as the forward path.
- **Reverse-proxy hazard**: mTLS identity via header is injectable unless the proxy unconditionally strips + re-sets at root scope, normalization is validated, and the proxy→backend hop is itself authenticated. **Anchor B is immune** — strong point for choosing it.

**Relative sizing per anchor** (ecosystem view; internal track sizes against actual seams):
- **B — pinned-key short-lived assertion**: *handler/verifier-level*. Smallest. Both transports. Proxy-immune. → recommended v1.
- **A — mTLS client cert**: *transport-level*. Medium. HTTP only. Proxy-fragile. → optional hardening tier.
- **Per-agent PKI / hosted JWKS endpoint**: *architectural*. Large. → reject for v1.
- **SPIFFE/SPIRE**: *architectural + new daemon(s)*. Largest + standing ops. → reject for v1, borrow ideas.

## Unanswered Questions

- **Verifier placement** relative to `build_context_with_external_identity` and the 15 `require_cap` sites, and where TLS terminates today — owned by the internal track; not investigated here per dual-track split.
- **Exact channel-binding claim mechanism** (server-cert fingerprint vs a fresh per-connection nonce echoed by the server) — both viable; picking one needs the internal read of how the connection exposes the server cert / whether a challenge round-trip fits the handshake. Flag for synthesis/internal step.

## Out-of-Scope Discoveries

- **`spiffe-rustls` as a future integration seam** — bridges SPIFFE SVIDs into the rustls config the server already uses if workload identity is ever adopted. Forward-compat data point, not a new spike.
- **Envoy XFCC `sanitize_set`/`forward_only`** is the reference implementation of safe client-cert forwarding — cite verbatim in any future operator doc if mTLS-behind-proxy is ever supported. Doc concern, not a spike.

---

**Sources**: [rustls WebPkiClientVerifier](https://docs.rs/rustls/latest/rustls/server/struct.WebPkiClientVerifier.html) · [jsonwebtoken crate (10.4.0, 2026-05)](https://crates.io/crates/jsonwebtoken) · [jsonwebtoken jwk module](https://docs.rs/jsonwebtoken/latest/jsonwebtoken/jwk/index.html) · [rust-spiffe / spiffe-rustls](https://github.com/maxlambrecht/rust-spiffe) · [SPIFFE libraries](https://spiffe.io/docs/latest/deploying/libraries/) · [Telekom Security — smuggling HTTP headers through reverse proxies](https://github.security.telekom.com/2020/05/smuggling-http-headers-through-reverse-proxies.html) · [nginx mTLS / ssl_client_verify](https://docs.nginx.com/nginx-instance-manager/system-configuration/secure-traffic/)
