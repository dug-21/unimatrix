# Scope Risk Assessment: vnc-034

Umbrella: personal-cloud multi-project serving (server + JS client + routing), 2 waves, 6 locked contracts C1–C6.
Historical evidence: #4869 (severable wave seam), #80 (ADR-004 path-hash isolation), #4321 (trust-boundary = fix-before-merge), #4274 (supply-chain checksum), #3756 (wave ordering).

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Promoting test-only `rcgen 0.13` cert-gen to production `load_or_generate_cert` may carry test-grade defaults (weak SAN set, short validity, non-0600 key) into the trust root. | High | Med | Architect specifies cert params (SAN list per C3, validity, key mode 0600) as production requirements, not inherited from the test helper. |
| SR-02 | C2 fingerprint is a wire contract computed independently on two stacks (Rust DER SHA-256 vs JS pin). Divergent DER serialization or hex casing silently breaks pinning at connect time. | High | Med | Architect defines a single oracle + cross-stack parity fixtures (per #4766 path-hash parity pattern); never hand-write the JS-side golden. |
| SR-03 | Pure-JS client < 250 KB with zero deps must pin a self-signed cert. Node TLS cert-pinning without a CA path is fiddly; a wrong abstraction could pull in a dependency or balloon size. | Med | Med | Spec the pinning mechanism (custom `checkServerIdentity` / fingerprint compare) and the size gate as a hard acceptance test in Wave 1. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | OSS↔enterprise seams (TlsConfig proxy, BearerValidator, slug→JWT claim) must stay additive. A Wave-1 shortcut (e.g. hardcoded TLS, slug baked into auth) forces enterprise re-architecture — a Non-Goals violation. | High | Med | Architect treats the seven ass-060 invariants as design constraints with explicit seam interfaces; spec writer adds acceptance criteria that the seam exists, not just the behavior. |
| SR-05 | OQ-C unresolved: Wave-1 single-project addressing (`/v1/tools` alias vs mandatory default slug) determines whether Wave 2 is purely additive or re-points live Wave-1 clients. Wrong call = client-breaking migration. | High | Med | Design must resolve OQ-C before locking C4 route shape; prefer the alias that makes Wave 2 additive (no client re-init). |
| SR-06 | The 1-client:1-project boundary (C5) is "permanent OSS," but its enforcement point is ambiguous (client config vs transport). If only client config prevents fan-out, a misconfigured client can mis-target. | High | Med | Spec must state where 1:1 is enforced: identity from transport (URL slug), agent has no payload field to name another project — unrepresentable, not merely rejected (C4 invariant 1). |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | C4 is built minimal in Wave 1, populated in Wave 2 — a classic deferred-seam trap. Per #4869, deferred seams break when later-wave routing is added outside the earlier-wave method, or a source-assertion/revert gate false-positives. | High | High | Architect designs `resolve_store` as a caller-owned interface with the Wave-2 router injected (Option/handle), Wave-1 default returns the one store; put per-slug hot-path routing inside the seam method (Principle #7 per-slug caches), not in a new edge. |
| SR-08 | C4 must reduce IDENTICALLY to the local-UDS path-hash store (ADR-004 #80). If the cloud slug path and the local path-hash path are two code paths, the "shared seam = proving ground" guarantee is lost and local parity (a non-negotiable constraint) silently regresses. | High | Med | Spec one seam, two resolvers (slug | path-hash); add a local-install regression test to the Wave-1 acceptance set, not deferred to Wave 2. |
| SR-09 | New trust boundaries (bundle parser C1, slug parser C4) accept untrusted operator/client input. Per #4321 a missing input-validation allowlist at a trust boundary is fix-before-merge, not cosmetic — slug path-injection (`../`, encoded separators) could escape `/data/.unimatrix/{slug}/`. | High | Med | Architect mandates a slug allowlist (charset + length) and bundle schema validation at the edge; flag both as security acceptance criteria for the spec. |
| SR-10 | C3 `UNIMATRIX_PUBLIC_URL` is one knob feeding three derived consumers (base-url, allowed_hosts, cert SAN). A single mis-derivation desyncs the cert SAN from the bundle URL → client connects but fingerprint/host mismatch. | Med | Med | Spec the single derivation function with all three consumers reading from it; add a test asserting bundle host ∈ cert SAN. |
| SR-11 | Distroless has no shell; first-boot provisioning (cert+token gen, /data writability) runs in the Rust binary. A bind-mount UID-65532 mismatch must fail loud-and-actionable, not silently or with a panic (`.unwrap()` forbidden). | Med | Med | Spec explicit fail-loud error paths for unwritable /data and missing creds; no `.unwrap()` in the provisioning path. |

## Assumptions

- **A1 (Goals §6, C5):** That a solo developer never legitimately needs one client across two projects — the entire 1:1 permanence rests on this. If false, the OSS boundary becomes a friction point, not a safety feature. (Vision goal #4934 supports it; flag as the load-bearing product bet.)
- **A2 (Constraints, ADR-004 #80):** That the local path-hash store and the cloud slug store are the *same* isolation mechanism. ADR-004's "moving a project changes its hash" must NOT leak into cloud, where the slug is operator-declared and path-independent (C5). If the path-hash assumption leaks, cloud project identity breaks on container moves/remounts.
- **A3 (Constraints):** That `rcgen 0.13`/`tokio-rustls 0.26` already present suffice with no new server crate. If pinning or DER-fingerprint needs a helper crate, the "no new crates" constraint and the size/dep posture are both stressed.
- **A4 (Wave Decomposition):** That Wave 1 against a single implicit project genuinely exercises the C4 seam Wave 2 depends on. If Wave 1's single-store path bypasses the seam (returns the store directly without routing through it), Wave 2 builds on unproven ground (#3756 — dependents must build on validated base).

## Design Recommendations

- **R-a (SR-07, SR-08):** Make `resolve_store` the single funnel from day one. Wave 1 routes the one store *through* the seam (not around it); local path-hash and cloud slug are two resolvers behind one interface. This is the highest-leverage design decision — it determines whether Wave 2 slots in or re-cuts (#4869, #3756).
- **R-b (SR-05, SR-04):** Resolve OQ-C toward the additive alias so Wave 1 clients survive Wave 2 unchanged; keep every enterprise seam (TlsConfig, BearerValidator, slug→claim) as an explicit interface so enterprise is additive (Non-Goals contract).
- **R-c (SR-02, SR-09):** Treat C2 fingerprint and the bundle/slug parsers as proof-grade trust boundaries — cross-stack parity fixtures for the fingerprint, allowlist validation for slug/bundle, both as fix-before-merge security criteria (#4321, #4766).
