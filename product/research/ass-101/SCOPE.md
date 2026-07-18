# ASS-101 — Root-of-trust establishment + secure edge binding

**Designation:** ass-101
**GitHub:** #955 · label `goal:integrity`, `research`
**Origin:** the trust-binding precondition ass-100 named but deliberately did not design (Q7). Downstream of #954.
**Session type:** research spike. Executes only after this SCOPE is complete (CLAUDE.md research rule).

---

## Framing — read this first

ass-100 established that **a trust-binding step is required and it's transport-dependent** — but not *how to establish it*. This spike answers the *how*.

The core insight, settled going in: **JWT is a carriage format, not the root of trust.** A signed token is only as trustworthy as the key that signed it and the server's ability to verify that key. Underneath any carrier, the load-bearing question is the same: **which key/cert does the server trust to speak for the edge?** ADR-007 *reserved* JWT-`sub` as the designed home, but the verifier is not built — nothing about JWT is settled, only earmarked.

This is a **design-research** spike: recommendation + sizing, **no build**. Its output feeds the same later decision ass-100 feeds.

**Posture (locked with the human):** whatever anchor this produces is **optional, behind the same `enabled` gate as ass-100's ceiling.** The **default is current behavior** — bearer-token channel auth, identity audit-only per ADR-008. This is a purely opt-in enterprise seam, not a posture change to OSS.

---

## The question

**What are the best ways to establish a root of trust that lets the Unimatrix server authenticate that an identity assertion genuinely comes from the trusted edge authority — and how cheaply does that extend to a Jurati (controller-first) client?**

Per candidate anchor **× per transport**, deliver a **recommendation + change-size estimate** and the seam it lands on. The mechanism decision (mTLS / signed-assertion / OS-trust, delegation vs per-agent, provisioning + rotation) is the deliverable.

---

## Design principles settled going in (verify, then build on)

1. **The anchor must be asymmetric and channel-pinned.** The edge holds a private key; the server holds only the public/cert half. This is why the shipped **static bearer token is disqualified as the anchor** — it's symmetric (server holds it too), so it cannot prove *the edge* spoke, and a leaked token is full impersonation. Bearer stays as coarse channel auth; it is not an identity root.
2. **Carrier ≠ anchor.** JWT-`sub` and mTLS both reduce to the same anchor ("which key/cert does the server trust"). Evaluate carriers, but size the *anchor + its lifecycle*, not the envelope.
3. **Ship coarse for OSS (ass-094 / avoid-overstating-defensive-structure).** Do not design an over-built PKI. Favor the smallest anchor that is asymmetric and channel-pinned. SPIFFE/SPIRE-class machinery is likely too heavy for a single-binary server — assess, don't assume.

---

## Candidate anchors to evaluate

| Anchor | Where it binds | Transport fit | Root of trust | Notes |
|---|---|---|---|---|
| **(A) mTLS client cert** | Channel (edge as peer) | HTTP | Pinned edge CA / leaf | Symmetric extension of the shipped fingerprint-pinned HTTPS (we already pin the *server*; this pins the *client*). Authenticates the whole connection as the trusted edge. |
| **(B) Edge-signed assertion** (JWT-`sub` or detached signature) | Inside the channel (per-agent) | HTTP **and** STDIO (portable) | Pinned edge public key (JWKS / pinned key) | Authenticates a per-agent `sub` within one authenticated connection. The transport-portable layer. |
| **(C) OS / process trust** | Channel (STDIO) | STDIO only | Process lineage / UDS peer creds / filesystem perms | No TLS on STDIO — the anchor differs fundamentally. |

**Transport-dependence is load-bearing:** the *channel anchor* is transport-specific (A for HTTP, C for STDIO); only the *signed-assertion layer* (B) is portable across both. A single mechanism likely will not cover both transports — the spike must say so explicitly rather than force one answer.

---

## Bounded questions

1. **Anchor recommendation × transport.** For HTTP: mTLS client cert (A) vs edge-signed assertion (B) vs both (mTLS channel + JWT-`sub` per-agent inside it). For STDIO: does OS/process trust (C) suffice, or is (B) still required? Recommend per transport, with sizing.
2. **Delegation vs per-agent root (carries Q8 from ass-100).** Does the server trust the edge as a **delegating authority** — one root, the edge vouches for the per-agent identities it stamps — or do **per-agent credentials terminate at the server** (N roots, issuance + rotation lifecycle)? How does each change *what the root of trust has to prove*? Size both; state which is the smaller/safer first increment.
3. **Provisioning & rotation — where the cost lives.** How does the edge obtain its key/cert, how does the server learn to trust it, and how does it rotate without downtime? This is the primary cost driver — center it, don't footnote it. Sketch the lifecycle for the recommended anchor.
4. **Extend to Jurati (controller-first) — the smallest change.** Jurati ships one edge process (the controller-proxy) holding the pinned HTTPS session and stamping `_attested_identity`. What is the minimal binding that turns that one process into an authenticated delegating authority, so its per-agent stamps become trusted delegated claims? Confirm whether this collapses Q8 to delegation for v1.
5. **Verifier placement.** Where in the server does the anchor get verified, relative to the `build_context_with_external_identity` seam and the 15 `require_cap` chokepoints ass-100 mapped? Does verification happen at transport termination, at identity resolution, or both? (Note the reverse-proxy hazard: if TLS terminates upstream, the client-cert identity must be forwarded trustworthily — a header-injection surface.)
6. **Gate shape (consistency with ass-100).** Confirm the anchor rides the same `enabled` gate; OFF ⇒ bearer-only, audit-only, unchanged. Fail-closed vs fail-open on a verification failure.

---

## Approach

Design-research: ecosystem/prior-art evaluation + a feasibility read of the Unimatrix server's existing TLS/transport seams. May combine an external-landscape pass (mTLS, JWKS, SPIFFE/SPIRE, short-lived signed assertions) with a codebase read of where TLS terminates and where identity currently resolves. **No implementation** — the output is a recommended anchor + provisioning/rotation sketch + sizing, not code.

---

## Deliverables

- **Recommended v1 anchor** (per transport) with a **change-size estimate** (`handler-only / schema+handler / transport / architectural`) and the seam it lands on.
- **Provisioning + rotation lifecycle sketch** for the recommendation — the primary cost driver, made concrete.
- **Transport matrix** — HTTP vs STDIO, showing where a single mechanism does and does not cover both.
- **Delegation-vs-per-agent root** implications (Q8 resolution input): which model each anchor supports, and the smaller/safer first increment.
- **The Jurati extension** — the minimal binding to turn the controller-proxy into an authenticated delegating authority.
- **Explicit non-recommendation-to-act framing** — options + sizing, feeding the later decision alongside ass-100 and other Jurati input.

---

## Constraints & non-goals

- **Optional, behind the `enabled` gate; default = current behavior** (bearer channel auth, identity audit-only). Purely opt-in enterprise seam; no OSS-default change.
- **No build.** Recommendation + sizing only.
- **HTTP is primary; STDIO is assessed** specifically for where the anchor diverges (it is not a full STDIO implementation — that transport remains a separate track).
- **Ship coarse for OSS** — recommend the smallest asymmetric, channel-pinned anchor; do not design an over-built PKI.
- **Feeds a later decision** — does not itself settle the mechanism or amend any ADR.

---

## Relationship to ass-100 (#954)

- **ass-100** sizes the *ceiling* (can the server resolve a principal and enforce) **assuming trust-binding is solved**, and names trust-binding as the gating precondition.
- **ass-101 (this spike)** designs that trust-binding — the root of trust and edge binding.
- Together they inform one decision: whether, and how, to activate config-gated enforcement on an edge-minted identity. Neither builds; neither amends ADR-008.

---

## Prior art / seams to verify

- The **shipped** fingerprint-pinned HTTPS + single bearer token (the mirror-image of mTLS; the anchor to extend, not replace).
- ADR-007 — `external_identity` upgrade path; JWT-`sub` *reserved* (verifier not built — confirmed by ass-100).
- ADR-008 (vnc-045) — audit-only default posture this spike preserves.
- `build_context_with_external_identity` (`server.rs`), the 15 `require_cap` sites, `TrustLevel` — the enforcement targets ass-100 mapped (verifier placement, Q5).
- Ecosystem: mTLS, JWKS/pinned-key verification, SPIFFE/SPIRE (workload identity + rotation — assess weight), short-lived signed assertions.
- ass-094 lesson — "architect for enterprise, ship coarse for OSS."
- Jurati ASS-009 FINDINGS — controller-first (Mode B/P2): the single edge process this binding must authenticate.

---

*Downstream of Jurati ASS-009 / Unimatrix ass-100 (#954). Server-side trust-establishment owned by Unimatrix. No build.*
