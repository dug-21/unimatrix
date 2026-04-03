# Alignment Report: crt-043

> Reviewed: 2026-04-02
> Artifacts reviewed:
>   - product/features/crt-043/architecture/ARCHITECTURE.md
>   - product/features/crt-043/specification/SPECIFICATION.md
>   - product/features/crt-043/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Roadmap source: product/research/ass-040/ROADMAP.md
> Agent ID: crt-043-vision-guardian

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Signal plumbing directly enables Wave 1A adaptive intelligence goals |
| Milestone Fit | WARN | ASS-040 Group 5 describes GitHub fetch for goal text; SCOPE.md drops this in favour of MCP parameter — intentional but undocumented divergence from roadmap text |
| Scope Gaps | PASS | All SCOPE.md items (B, C, Item A rationale) covered in source docs |
| Scope Additions | WARN | ARCHITECTURE.md adds residual-race retry enhancement note; SPEC adds `decode_goal_embedding` paired helper (SR-02 mitigation not in SCOPE.md — additive, beneficial) |
| Architecture Consistency | PASS | Architecture resolves all SCOPE.md open questions; INSERT/UPDATE race resolution (Option 1) is internally consistent |
| Risk Completeness | PASS | All eight scope risks traced; two residual risks accepted with documented rationale |

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | None | — |
| Addition | `decode_goal_embedding` paired helper | SCOPE.md §Serialization Format documents the ADR rationale but does not explicitly require a paired decode helper as an AC. SPECIFICATION.md FR-B-05 and AC-14 mandate it. SCOPE-RISK-ASSESSMENT SR-02 recommends it. Addition is beneficial and traceable to a scope risk mitigation; no approval concern. |
| Addition | Residual-race retry enhancement note | ARCHITECTURE.md §INSERT/UPDATE Race Resolution documents an optional retry enhancement ("if residual race is unacceptable in future"). This is a documented future branch only — no code is added. Matches the deferred-branch pattern (#3742): acceptable as a documented deferral. |
| Simplification | Goal text source — GitHub fetch dropped (Item A) | ASS-040 Group 5 roadmap entry says "fetch GH issue title + body for the feature_cycle_id." SCOPE.md explicitly substitutes the `context_cycle(goal=...)` parameter for the GitHub source. Rationale documented: no external fetch, no new deps, simpler pipeline. This is a scope simplification relative to the roadmap — not relative to SCOPE.md. SCOPE.md is the user-approved contract; the simplification was made at scoping time. No approval gap. |

---

## Variances Requiring Approval

### WARN-1: ASS-040 Roadmap Group 5 goal-text source diverges from crt-043 scope

**What**: The ASS-040 roadmap (Group 5, row 2) specifies: "At `context_cycle` start: fetch GH issue title + body for the feature_cycle_id. Embed via existing pipeline." crt-043 SCOPE.md explicitly drops the GitHub fetch and instead uses the `goal` parameter passed directly to `context_cycle(type=start)`. The roadmap text has not been updated to reflect this decision.

**Why it matters**: The roadmap is a product-level artifact read by future feature authors. The current roadmap text describes a GitHub API call as part of the behavioral signal infrastructure. If a Group 6 or Group 7 author reads Group 5 expecting goal embeddings to have been produced from GH issue text, they will find embeddings produced from agent-supplied `goal` strings instead. The semantic content may differ — GH issue body is structured, agent-supplied goal is free-text — and Group 6 goal-clustering assumptions may not hold if authors expected the GH-fetch source.

**Recommendation**: Update ASS-040 ROADMAP.md Group 5, row 2 to match the approved approach: "At `context_cycle(type=start)`: embed the `goal` parameter text (agent-supplied; no GitHub fetch). No external dependencies." This is a roadmap maintenance action, not a scope change. The crt-043 scope and source documents are internally consistent.

---

### WARN-2: `decode_goal_embedding` visibility and module placement unresolved between architecture and spec

**What**: ARCHITECTURE.md §Component Breakdown offers two placement options for the serialization helpers: "`unimatrix-store/src/embedding.rs` or inline in `db.rs`". SPECIFICATION.md FR-B-05 and AC-14 mandate the helper exists and uses `bincode::serde::decode_from_slice` with `config::standard()`, but does not prescribe placement. The architecture leaves the decision to the delivery agent ("or inline in `db.rs`").

**Why it matters**: The helpers are declared `pub(crate)` — internal to `unimatrix-store`. Group 6 will need to call `decode_goal_embedding` from `unimatrix-server`. If the helper is `pub(crate)` and lives only in `unimatrix-store`, Group 6 must either make it `pub` or reach across crates in an unsupported way. This is a visibility scope question the delivery agent will need to resolve. If resolved incorrectly, Group 6 faces a breaking change.

**Recommendation**: Before delivery opens a PR, the delivery agent should confirm: (a) whether `decode_goal_embedding` needs to be `pub` (not `pub(crate)`) for cross-crate use by Group 6, or (b) whether Group 6 will consume goal embeddings exclusively through a store query method that decodes internally, keeping the helper crate-private. The architecture and spec should agree on this before implementation. This is a delivery-time decision but should not be deferred to Group 6 — it is part of the crt-043 API surface contract.

---

## Detailed Findings

### Vision Alignment

crt-043 is infrastructure plumbing for Wave 1A's adaptive intelligence pipeline. The product vision states:

> "The intelligence pipeline is the core of the platform. It is not a retrieval engine with additive boosts. It is a session-conditioned, self-improving relevance function: given what the agent knows, what they have been doing, and where they are in their workflow, surface the right knowledge — before they ask for it."

Goal embedding (`goal_embedding BLOB`) directly enables H1 (goal clustering) — the ability to retrieve knowledge from past cycles with similar goals. Phase capture (`phase TEXT` on observations) directly enables H3 (phase stratification) — the ability to score entries by their relevance to the current cycle phase. Both are prerequisites for Group 6 behavioral edge emission and Group 7 goal-conditioned briefing.

The vision's "self-sustaining loop" depends on cycle-close behavioral signal emission (Group 6), which depends on crt-043 existing. The feature is correctly positioned as infrastructure, not as a user-visible retrieval change. No retrieval path, search ranking, or MCP response format is touched.

crt-043 is fully aligned with vision intent.

### Milestone Fit

crt-043 targets ASS-040 Group 5. Per the roadmap dependency graph, Group 5 has no dependencies and can ship concurrently with Groups 2 and 3. Groups 2 and 3 are complete (crt-038, crt-039, crt-040, crt-041). Group 4 (PPR expander) is not yet shipped. Group 5 can ship now; there is no ordering constraint that blocks it.

The feature correctly delivers only Group 5 scope — it does not attempt Group 6 (behavioral edge emission) or Group 7 (goal-conditioned briefing), which are conditional on Group 5 first. No future milestone capabilities are prematurely built.

One discrepancy: the ASS-040 roadmap text for Group 5 describes the goal-text source as a GitHub issue fetch. crt-043 substitutes the MCP `context_cycle(goal=...)` parameter instead. The practical effect is a simpler, more reliable implementation (no external dependency, no network call). The scope simplification was made at scoping time with full documentation. However, the roadmap text remains misaligned with what was actually approved and is being built. See WARN-1 above.

### Architecture Review

The architecture document is thorough and makes sound decisions on all three open questions from SCOPE.md:

**INSERT/UPDATE race (SR-01):** Option 1 (fire embedding task from within `handle_cycle_event` after the INSERT spawn) is chosen and well-argued. The architectural rationale correctly identifies that Options 2 and 3 from SCOPE.md are architecturally unavailable — the MCP handler and UDS listener are independent paths with no shared triggering point. The residual race is acknowledged (the UPDATE may theoretically precede the INSERT under the multi-threaded tokio runtime) and accepted as cold-start-compatible degradation. This is the correct engineering judgment given that a silent NULL is the same outcome as the embed-service-unavailable path.

**Bincode serialization (ADR-001 reference):** The architecture mandates `bincode::serde::encode_to_vec` with `config::standard()` and requires paired encode/decode helpers as `pub(crate)`. The rationale — self-describing length prefix, model upgrade path, Group 6 precedent — is sound and aligns with the scope's documented ADR requirement.

**Migration atomicity (ADR-003 reference):** The architecture correctly notes both ADD COLUMN statements execute within the outer transaction opened by `migrate_if_needed()` — no additional `BEGIN`/`COMMIT` is needed because the caller already owns the transaction boundary. This directly addresses SR-04.

**One unresolved detail:** The helper visibility (`pub(crate)` vs `pub`) for cross-crate Group 6 use is not resolved in the architecture. See WARN-2.

The component interaction diagram accurately represents the data flows. The integration surface table is precise and matches the specification requirements. No architectural gaps found relative to SCOPE.md requirements.

### Specification Review

The specification covers all acceptance criteria from SCOPE.md and adds five additional AC entries (AC-13 for INSERT-before-UPDATE ordering verification, AC-14 for `decode_goal_embedding` presence) that address scope risks SR-01 and SR-02 respectively. These additions are appropriate — they close risk gaps that SCOPE.md flagged as recommendations.

FR-B-07 ("The embedding task MUST be spawned via `tokio::spawn` after the `context_cycle` tool response is composed") describes the fire-and-forget property correctly in intent, but the architecture resolves this differently: the embedding spawn fires from `handle_cycle_event` inside the UDS listener, not from the MCP handler after the response. The MCP handler calls the UDS dispatch as fire-and-forget, so the embedding spawns inside that fire-and-forget chain — the MCP response IS non-blocked. The functional requirement is satisfied, but the literal text of FR-B-07 implies the spawn is in the MCP handler. This is a documentation imprecision, not a functional gap, but could mislead a delivery agent reading the spec without the architecture. This does not rise to WARN level given the architecture document is unambiguous.

FR-M-02 ("Both ADD COLUMN statements MUST execute within a single `BEGIN`/`COMMIT` transaction") is slightly redundant with the architecture's clarification (no additional transaction needed — the caller's transaction is the boundary). No conflict, just wording imprecision.

The Domain Models section is comprehensive. The NOT in Scope section is precise and matches SCOPE.md non-goals exactly.

### Risk Strategy Review

The risk-test strategy is well-structured and traceable. All eight scope risks (SR-01 through SR-08) are resolved in the Scope Risk Traceability table. All 13 architecture risks are covered with concrete test scenarios.

The non-negotiable test list is correctly identified and aligns with the highest-severity risks:
- Round-trip encode/decode test (R-02, AC-14) — addresses the silent-garbage-float risk
- Real v20 database migration test (R-05, FR-M-04) — addresses partial-migration risk
- All-four-write-sites phase test (R-03, AC-09/AC-10) — addresses the highest-likelihood integration failure
- Embed-service-unavailable path (R-10, AC-04a)
- Empty/absent goal no-spawn path (R-09, AC-04b)
- Migration idempotency (R-06, AC-11)

One additional observation: R-13 (composite index deferred) assigns the delivery agent a mandatory written decision requirement (FR-C-07). The risk strategy correctly records that deferring this to Group 6 is not acceptable. This is a process gate, not a test coverage gap.

The security section is appropriately scoped: no new network surface, no SQL injection surface (all user input goes through ONNX embedding or parameterized SQL), blast radius limited to internal rayon pool. No security gaps found.

The edge cases section covers the whitespace-only goal ambiguity (behavior unspecified — delivery agent must decide). This is the only unresolved delivery-time question in the risk strategy and is appropriately flagged.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found #3742 (optional future branch in architecture must match scope intent), #2298 (config key semantic divergence), #3158 (deferred scope resolution leaves AC references live). Entry #3742 directly informed the residual-race retry enhancement assessment (deferred branch: acceptable). Entry #3158 informed the FR-B-07 wording imprecision observation.
- Stored: nothing novel to store — the roadmap-text-divergence pattern (WARN-1) is feature-specific: crt-043 explicitly simplified the Group 5 goal-text source at scoping time, not a recurring architect behavior that generalizes across features. The decode helper visibility gap (WARN-2) is a one-time consequence of the `pub(crate)` decision and does not generalize beyond Group 5/6 cross-crate patterns already covered by existing conventions.
