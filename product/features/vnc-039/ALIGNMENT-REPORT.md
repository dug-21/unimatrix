# Alignment Report: vnc-039

> Reviewed: 2026-06-18
> Artifacts reviewed:
>   - product/features/vnc-039/architecture/ARCHITECTURE.md
>   - product/features/vnc-039/specification/SPECIFICATION.md
>   - product/features/vnc-039/RISK-TEST-STRATEGY.md
> Scope source: product/features/vnc-039/SCOPE.md
> Scope risk: product/features/vnc-039/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md
> Strategic goal: #4946 (`personal-cloud` — Individual developer-friendly deployment)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly restores the missing on-demand `context_*` half of goal #4946's two-surface HTTPS contract. |
| Milestone Fit | PASS | Vinculum (MCP/connectivity) phase; client-only, builds on shipped vnc-038 `v:2` bundle. No future-milestone over-build. |
| Scope Gaps | PASS | All 11 ACs and both scopes are addressed across architecture + spec + risk strategy. |
| Scope Additions | WARN | Two source-doc items extend beyond SCOPE.md's headline goals — both are documented and in-blast-radius, but worth human awareness (see below). |
| Architecture Consistency | PASS | Architecture, spec (FR/AC), and risk strategy (R-01..R-16) trace consistently to the same ADRs and constraints. |
| Risk Completeness | PASS | Trust-boundary, false-green-stub, and schema-mismatch risks (the vnc-034 lineage) are covered with named, non-negotiable ACs. |

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | No SCOPE.md item is missing from the source docs. AC-01..AC-11 each map to FRs and risk scenarios. |
| Addition | Universal creds relocation on the **legacy** path | SCOPE.md frames Scope B around the bundle/remote-attach credential. Architecture §6 extends the out-of-tree store write to the **legacy** `--remote` credential too (with `fingerprint: null`, hook client stays unpinned). Defensible (no in-tree creds for ANY remote path) and consistent with the goal, but it touches the legacy path's credential behavior, which SCOPE's Non-Goals say to "keep as-is." Documented in architecture §6 and risk R-15. |
| Addition | Latent unpinned-observe bug fix (schema mismatch) | The hook client today never reads `fingerprint`, so the file-mode observe path runs unpinned / falls back to UDS. Scope B reconciles this (FR-23, ADR-004, R-06). SCOPE.md explicitly folds this into Scope B's blast radius ("fixing it is in the blast radius of Scope B, not a new goal") — so this is an **approved-in-scope** addition, listed here for traceability, not as an unapproved expansion. |
| Simplification | Scope A live validation deferred | Live end-to-end validation is gated on #774; stub/local validation only. Rationale: #774 host-allowlist 403s every remote MCP request. Documented as a sequencing dependency with explicit `not-validated-live` caveats on every Scope-A AC (SR-04, R-03). Acceptable. |

## Variances Requiring Approval

None rise to VARIANCE or FAIL. One WARN is flagged for human awareness:

**WARN-1 — Legacy-path credential behavior is touched despite a "don't extend legacy" Non-Goal.**
1. **What**: Architecture §6 applies the universal out-of-tree store write to the legacy `--remote`/`--token` credential (writing `fingerprint: null`, keeping the hook client unpinned for legacy). SCOPE.md's Non-Goals and Constraints repeatedly assert legacy is "kept as-is, not extended."
2. **Why it matters**: Touches the principle of milestone/legacy discipline. The change is benign (preserves today's unpinned-legacy observe behavior) and arguably required for consistency (the relocation must be universal or an in-tree leak survives on legacy), but it is a behavior touch on a path SCOPE said to leave alone.
3. **Recommendation**: **Accept.** The relocation is coherent only if universal — leaving legacy creds in-tree would re-open the exact leak the feature closes. The architecture correctly keeps legacy *unpinned* (no functional extension of legacy), and R-15 tests the `fingerprint: null` posture. Confirm the human is comfortable that "no extension of legacy MCP" is preserved (it is — no bridge is wired for legacy) while the credential *relocation* is universal.

## Detailed Findings

### Vision Alignment
Goal #4946's intent and success criteria explicitly commit to the two-surface contract: *"Multi-LLM connect identically via HTTPS with full intelligence-pipeline fidelity… retrieve knowledge on demand (search, lookup, get)."* The vision doc's narrative reinforces this: *"agents retrieve knowledge on demand (search, lookup, get), and Unimatrix delivers it proactively"* (PRODUCT-VISION.md:9). vnc-039's stated problem is that remote attach delivers only the proactive (`observe_url`) half and silently drops the on-demand `context_*` half — a structural gap against the goal. Restoring it is squarely on-vision, not a convenience addition.

Architectural-principle checks:
- **Principle 3 (capability checks at the service layer regardless of transport)** — Respected. The bridge is a transport adapter; it forwards the bearer and posts `mcp_url` verbatim. Identity/capability resolution stays server-side. The bridge does not become a second authorization point.
- **Principle 6 (single binary server; client is an adapter)** — Strongly honored. ADR-001/ADR-002 keep the bridge a thin pure-JS adapter; the Rust binary remains the server. The "house pattern" stdio bridge (entry #1897) is the established adapter shape. No infrastructure added.
- **Principle 8 (no secrets in any database)** — Adjacent and reinforced. This feature concerns a creds *file*, not a DB, but the spirit (no secret where it can leak) is advanced: the token leaves the repo working tree entirely (Scope B). Cleartext-at-rest is explicitly accepted by decision (NFR-05) — consistent with prior guidance to manage at-rest secrecy as a documented human-risk rather than over-build keychain machinery (memory: avoid-overstating-defensive-structure).
- **Dumb-client invariant (vnc-038 spine)** — Carried onto the new surface intact (AC-05, FR-06): the bridge composes no path, derives no slug, posts `mcp_url` verbatim. Slug stays server-authoritative payload, not a client-derived key.

Single-edge-language constraint (memory: unimatrix-edge-language-single-jsts) is honored — the bridge is pure Node stdlib; the BUILD/DIY zero-dep decision is grounded in research spike ass-080 (#777), not merely inherited posture.

### Milestone Fit
Vinculum (`vnc`) phase — MCP server & connectivity — is the correct home for restoring an MCP surface over HTTPS. The feature is client-only and builds on already-shipped vnc-038 (`v:2` bundle #5081, per-slug route). No future-milestone capability is pre-built: enterprise concerns (OS keychain, at-rest encryption, RBAC) are explicitly deferred as Non-Goals, consistent with goal #4946's "enterprise extends, never re-architects" and the documented guidance against over-building defensive structure. The #774 dependency is a same-arc server fix, not a milestone leap.

### Architecture Review
The architecture cleanly decomposes into five components (C1–C5) with a crisp Scope-A/Scope-B boundary: Scope B owns the credential store (write + both reads); Scope A owns the bridge and consumes the store. This enforces SR-05 independence (Scope B lands first, no #774 dependency). The five ADRs (ADR-001 trust contract, ADR-002 entrypoint, ADR-003 `projectHash` key, ADR-004 canonical schema, ADR-005 boundary/sequencing) each resolve a named scope risk. Notably:
- ADR-003 resolves the SR-08 two-key trap by fixing the store key to `projectHash` (one shared derivation → write-key and read-keys cannot disagree), and correctly relegates slug to payload inside `mcp_url`. This is the highest-leverage integration decision and the architecture locks it as a constraint rather than leaving it open. Sound.
- ADR-004 reconciles the pre-existing write/read schema mismatch rather than porting it, fixing the latent unpinned-observe bug — correctly scoped as in-blast-radius (matches SCOPE Constraint and SR-07).
- ADR-001's per-socket re-pin contract (§3.2) correctly identifies the divergence from the single-shot observe path (persistent connection, fail-loud vs fail-open) — this is the subtle, high-value insight that prevents a token leak on a second socket.

Architecture's three open questions for the human (§11) are genuinely design-detail, not scope — consistent with SCOPE's Open Questions framing.

### Specification Review
The spec traces every FR (FR-01..FR-27) to one or more ACs, and every AC (AC-01..AC-11) carries a verification method and a validation tier ([stub/local] vs [no-cloud]). The spec correctly preserves SCOPE's binding decisions (bundle-only cloud MCP, zero-dep by decision, NFR-06 no-token-to-logs) and does not introduce capability beyond SCOPE. The `not-validated-live (#774)` caveat is explicitly attached to Scope-A ACs (AC validation tier legend + cross-cutting caveat), honoring lesson #4796 (never assert an unrun AC as fact). FR-26 correctly leaves the exact key/path to architecture while fixing the one-key invariant — clean spec/architecture handoff. No scope addition originates in the spec.

### Risk Strategy Review
The risk strategy is anchored in the directly-relevant prior failure (lesson #4970, vnc-034 F1 — trust-boundary dead-pin false-green) and elevates the trust-boundary and false-green-stub risks to Critical with **named, non-negotiable** ACs (live good/wrong-pin handshake proving the token never crosses on mismatch; per-socket re-pin; `pinnedFp`-populated regression; stub framing provably derived from a captured rmcp response). This covers exactly the architectural principles that matter for this feature: secret hygiene (Principle 8 spirit) and the cert-pin trust model. The fresh-context security-review requirement even on green gates is the correct second-order response to the #4970 lesson (same suite, same blind spot). The hybrid flip-bar (SR-03/OQ-1) is pre-thresholded as a delivery checkpoint, not left to vibe. Risk coverage is complete and proportionate; no vision-relevant risk is skipped.

## Knowledge Stewardship
- Queried: /uni-query-patterns and context_search for vision alignment / scope-addition / over-build patterns -- found #2298, #3337 (alignment/spec-divergence patterns, not directly applicable here); no recurring "architect adds scope on tightly-constrained spec" pattern surfaced. The relevant cross-feature lesson is #4970 (trust-boundary false-green), already woven into the risk strategy.
- Stored: nothing novel to store -- the variances here are feature-specific (legacy-path creds relocation; an in-blast-radius latent-bug fix). Neither generalizes into a recurring vision-misalignment pattern across multiple features yet. The "don't over-build defensive structure" guidance is already captured in user memory and is being honored, not violated, here.
