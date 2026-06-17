# Scope Risk Assessment: vnc-038

Revises the vnc-034 deployment contract. Product-level risks surfaced before architecture/spec. Evidence cited from Unimatrix entries.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | **Dumb-client / server-composed-bundle bet.** Moving ALL route grammar server-side and shipping finished URLs in the bundle is the spine. If any client path still appends/derives (init.js:303-308, transport-http.js:84), #766's bug class re-opens. The bet only pays if every client-side compose site is eliminated, not just the observe one. | High | Med | Architect: enumerate ALL client path-composition sites as a closed set; spec an invariant test (client posts bundle URLs verbatim, byte-for-byte). |
| SR-02 | **v:2 bundle parity break.** ADR-001 strict exact-key guard (#4954): Rust sole encoder, JS decodes. A naive field add breaks decode unless both sides + corpus move atomically. Known mechanics traps: corpus uses hex not base64, oracle fns are pub(crate) and need re-export for tests (#4956). | High | Med | Architect: treat v:2 as one atomic dual-side change; spec must pin the parity corpus update and reuse the existing codec corpus, not scaffold new. |
| SR-03 | **Ceremonial-funnel trap on observe (#4974).** Objective 5 moves observe onto the per-request funnel. vnc-034 already shipped this funnel ceremonially once (`let _store` discard + parallel adapter) — green at N=1, unproven at N=2. Observe re-routing risks repeating it. | High | Med | Architect: make the resolved handle the SOLE observe route (no boot-bound fallback, no parallel path). Spec the proof at N=2, not N=1. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | **Hard-cutover blast radius.** Removing DefaultResolver + `/v1/tools→Default` alias + `_=>Default` arm (RD-5) touches the central route grammar (seam.rs:184-192) and reserved-slug set (config.rs:2483, `tools`/`observe`). Over-reach beyond the served-project model could break local UDS (AC-10) or the per-request MCP seam. | High | Med | Architect: scope the cutover to cloud/container served-project only; reconcile local UDS path-hash addressing under the unified resolver explicitly (AC-10 is the guardrail). |
| SR-05 | **Reserved-slug coupling.** `RESERVED_SLUGS=["v1","health","observe","tools"]` is downstream of the route-grammar decision. Changing the default alias and adding `/v1/{slug}/observe` silently shifts what `tools`/`observe` must reserve; a missed update lets a registerable slug shadow a route. | Med | Med | Spec: make reserved-slug re-derivation an explicit AC tied to the new route grammar; test slug registration against every reserved name. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | **#735 collision (coordination).** First-boot/router work is in flight on #735 on the SAME surface (router.rs, main.rs boot path, first-boot provisioning). Concurrent edits risk merge conflicts and contradictory route-grammar assumptions. | Med | High | Leader: sequence vnc-038 after #735 lands, or pin a shared branch point and coordinate the seam owner. Do not design in isolation from #735's router changes. |
| SR-07 | **register→restart routing-intent write.** `register <slug>` must write `[[projects]]` (projects.rs:334) read once at boot (main.rs:1004, #5079). Distroless has no shell — provisioning is Rust-binary only. A malformed/partial config write or non-idempotent re-register could corrupt routing or genesis-clobber an existing store (hash chain is sacred). | High | Med | Architect: spec atomic config write + re-attach (open, never genesis) on existing slug, per projects.rs State B precedent. |

## Assumptions

- **(Problem Statement / RD-1)** No existing cloud/container users → no served store to migrate. If any deployed instance holds a default path-hash served store, the hard cut (AC-09) loses data. Validate "zero existing served users" before cutover.
- **(Goal 2 / RD-4)** Restart-to-apply is acceptable for this single-dev, not-always-on deployment. If the deployment is expected always-on/multi-tenant, restart-to-apply is a regression, not a simplification.
- **(Non-Goals / RD-6)** #767 (embedding-model RW-mount) is NOT a dependency. If first boot cannot download the model, the dogfood still fails despite vnc-038 being correct — the end-to-end "one command" claim depends on #767 shipping in parallel.
- **(Constraints, AC-10)** Local UDS keeps path-hash identity under the unified resolver without a manual slug. Assumes the resolver can address a single store identity-free; if not, AC-10 forces a special-case arm that contradicts RD-5's "single = N=1, no special case."

## Design Recommendations

1. **SR-01/SR-03:** Make the dumb-client invariant and single-funnel testable at N=2, not N=1. The ceremonial-seam history (#4974) is direct precedent — do not accept a green N=1 observe test as proof of per-slug isolation.
2. **SR-02:** Treat v:2 as an atomic Rust+JS+corpus change; reuse existing parity-corpus infrastructure (#4956) — hex encoding, re-exported oracle fns.
3. **SR-04/SR-05:** Bound the hard cutover to the served-project model; make reserved-slug re-derivation and local-UDS reconciliation explicit ACs so the central route-grammar change does not silently break adjacent paths.
4. **SR-06:** Coordinate sequencing with #735 before design starts — same router/boot surface.
