# Alignment Report: crt-052

> Reviewed: 2026-06-08
> Artifacts reviewed:
>   - product/features/crt-052/architecture/ARCHITECTURE.md
>   - product/features/crt-052/specification/SPECIFICATION.md
>   - product/features/crt-052/RISK-TEST-STRATEGY.md
> Scope source: product/features/crt-052/SCOPE.md
> Scope-risk source: product/features/crt-052/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md
> Strategic goal: goal:self-learning (#4677), confirmed on GH issue #689

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Advances self-learning (curation feedstock); honors hash-chain, secrets, graceful-degradation, single-binary, in-memory principles |
| Milestone Fit | PASS | Next-up in the pinned OSS-cloud sequence (vnc-027 → vnc-030 → crt-052); builds only on shipped interfaces; #700 deferred |
| Scope Gaps | PASS | All 8 Goals, 13 ACs, 14 Constraints traced into ARCH + SPEC + RISK; no SCOPE item dropped |
| Scope Additions | WARN | Two new config knobs (`transcript_hold_max_sessions`, `transcript_hold_ttl_secs`) and Wave A/B staging are SCOPE-derived but not named in SCOPE; both justified, no human approval needed |
| Architecture Consistency | PASS | Component/type/seam names consistent across ARCH ↔ SPEC ↔ RISK; ADR index maps cleanly to ACs and SRs |
| Risk Completeness | PASS | All 9 SRs traced to architecture risks; secrets, untrusted-input, and held-buffer integrity all covered as merge gates |

Status counts: PASS 5, WARN 1, VARIANCE 0, FAIL 0.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | (none) | Every SCOPE Goal (G1–G8), AC (AC-01–13), and Constraint (C1–C14) is carried into the three source docs. |
| Addition | `transcript_hold_max_sessions` / `transcript_hold_ttl_secs` config knobs | ARCH C9, SPEC NFR-4. Not enumerated in SCOPE, but are the concrete mechanism SCOPE Goal 8 + SR-01 demand ("explicit held-session count cap and an independent stale-sweep TTL"). Derived, not net-new scope. |
| Addition | Wave A / Wave B delivery staging + rollback boundary (ADR-009) | ARCH §1, RISK R-11. A delivery-structuring decision, not a feature surface. Keeps Wave A shippable with empty buffers; reduces risk. No new capability beyond SCOPE. |
| Simplification | `topic_source` as soft ordering preference only, never a filter | SCOPE OQ-1/SR-06 → SPEC FR-9, ARCH ADR-006. Rationale: hardening to a filter would drop legitimately-attributed sessions; SCOPE explicitly bounds this. Correctly held. |
| Simplification | Reconstruction fallback is a fidelity floor (0.81), not parity | SCOPE Goal 5 → SPEC FR-8. Rationale documented (decisions live in prose observations never carry); provenance labeling mandatory. Aligned with SCOPE intent. |

## Variances Requiring Approval

None. No VARIANCE or FAIL findings. The single WARN (config-knob/staging additions) is documented and SCOPE-derived; it is noted for human awareness, not approval.

## Detailed Findings

### Vision Alignment — PASS

crt-052 advances **self-learning intelligence** (#4677): its sole purpose is to recover the 65% of hand-labeled session value (28/43 items, ass-070) that is currently destroyed at transcript purge, and route it into the knowledge base as agent-curated ADRs/lessons/patterns. This is the vision's core thesis made operational — "knowledge curation [as] a first-class activity in the workflow itself … decisions get attributed, lessons get captured, patterns get refined" (PRODUCT-VISION.md §Vision). The GH label `goal:self-learning` is confirmed.

The design honors the non-negotiable architectural principles:

- **No secrets in any database (#8, #4721).** The defining design constraint. Candidates are response-transient and attached at assembly level *outside* the memoized `RetrospectiveReport` (ARCH ADR-004; SPEC FR-7; RISK R-04, a merge gate). The crt-033 synchronous persist to `cycle_review_index` is correctly identified as the trap and gated against. Metadata-only `Debug` on snapshot types (R-19) closes the log/panic leak surface. Strongly aligned.
- **Graceful degradation (#5).** Reconstruction fallback (FR-8) gives a defined, labeled-degraded fallback when buffers are empty/holed — "absent or failed = previous behavior, not broken behavior." Wave A degrades cleanly to fallback with no Option B (ARCH §1, R-11).
- **In-memory hot path (#7) / single-binary, zero-infra (#6).** No new infrastructure; no wire change (NFR-7); held buffers live in `Arc<Mutex<_>>` server-side only. The server "selects, never extracts" (no generation capability — Constraint 6) respects the engine's ONNX-only posture.
- **No generation server-side** keeps the rules-select / agent-extracts boundary, which is exactly the "explicit curation" half of the vision's two-surface model. Distilled knowledge reaches the KB only via the agent's `context_store` writes (AC-09).

Unimatrix is "not an orchestration engine" — crt-052 stays within the knowledge-engine boundary: it harvests narrative for curation, it does not coordinate or schedule agents.

### Milestone Fit — PASS

crt-052 is the next-up feature in the pinned OSS-cloud finalization sequence (vnc-027 #680 MERGED → vnc-030 #699 MERGED → crt-052). Both predecessors have shipped, so crt-052 builds only on *delivered* interfaces: the vnc-025 transcript buffer + named seam (#4742), vnc-030 contractual attribution + close/sweep precedence (#4819), crt-033 memoization (#3793). No future-milestone capability is built ahead of need — the downstream #700 (MARKER recovery) is explicitly out of scope; crt-052 ships only the *consumable seam* shaped so #700 can reuse it (Constraint 4). That is correct milestone discipline: designing the seam for the known next consumer without implementing the consumer.

One judgment call worth noting (not a variance): Goal 8 / Option B (the held-buffer state machine) is non-trivial new machinery. The architecture justifies it as binding — without it, per-turn drain (#4799) starves the primary path and every multi-turn review degrades to the 0.81 fallback. This is a remedy for a real, verified starvation, not gold-plating; it is appropriately walled into Wave B behind a rollback boundary. Accepted as in-scope per SCOPE OQ-1 (binding human decision, #689).

### Architecture Review — PASS

The integration-surface table (ARCH §4) is declared binding and is internally consistent with SPEC domain models and RISK scenarios: `take_transcripts_for_feature`, `TranscriptSnapshot`/`SessionTranscriptSnapshot`, `select_candidates`, `reconstruct_from_observations`, the four-return helper, and `transcript_hold.rs` all appear with matching signatures across the three docs. The ADR index (ARCH §5) maps every ADR to specific ACs and SRs with no orphans. The 500-line file discipline (Constraint 10) is respected — new logic lands in new focused modules with thin wiring into the over-length `tools.rs`/`session.rs`/`listener.rs`.

Minor naming note (not a variance): the snapshot return type is `TranscriptSnapshot` in ARCH §4 and `SessionTranscriptSnapshot` in SPEC §Domain Models. The field sets are compatible (bytes + elided_bytes + hole_info + high_water + base_offset). Flagging for the delivery team to pin one name; no design conflict.

### Specification Review — PASS

SPEC traces every SCOPE AC by ID (AC-01–13 all present) and adds two supplementary verification criteria (AC-V-SEAM, AC-V-FUZZ) that harden the #700 single-reader invariant and the untrusted-input boundary — both SCOPE-implied, neither expands scope. FR-1…FR-15 each cite their SCOPE Goal/Constraint/SR. The "NOT in Scope" section reproduces SCOPE's Non-Goals faithfully, including the load-bearing exclusions (server-side LLM extraction, multi-provider parsing, sidechain transcripts, wire changes, `topic_source` as hard filter, #700 marker recovery).

The four genuinely-open spec questions (held-buffer cap/TTL defaults, per-cycle aggregate cap default + truncation order, `byte_offset` logical-vs-array semantics, audit no-consumer survey) are all design-tuning decisions, not scope questions — correctly surfaced for the delivery phase rather than reopening scope.

### Risk Strategy Review — PASS

All nine scope risks (SR-01…SR-09) trace to architecture risks (RISK §Scope Risk Traceability) with concrete ADR resolutions. The three risks the vision's principles care most about are merge-gated:

- **Secrets breach (SR-07 → R-04/R-19):** content-leak grep/log/SQL gate + structural absence from the memoized struct + re-review-of-stored-record test. Directly enforces architectural principle #8.
- **Untrusted-input DoS (SR-09 → R-10):** AC-V-FUZZ skip-with-count, handler never panics. Buffer content is client-disk JSONL — correctly treated as an attack surface.
- **Held-buffer integrity / mis-attribution (SR-01/SR-02 → R-01/R-02):** fail-loud re-adoption keyed to `feature_cycle` (cites #981), independent cap + TTL bounding memory without relying on cycle review. Mis-scope is correctly framed as a KB *integrity* attack, not mere data loss — consistent with the vision's "trustworthy, consistent" knowledge promise.

The AC-11 `continuity_simulated_lifecycle` test is correctly identified as the *only* pre-merge proof of the primary path before dogfooding switchover, and made a hard merge gate. The one tracked coverage gap (ADR-009 no-consumer audit survey not yet performed) is named as a prerequisite to landing the Wave B audit move — appropriately surfaced, not hidden.

## Knowledge Stewardship
- Queried: /uni-query-patterns (context_search topic `vision`) for vision alignment patterns -- surfaced #2298 (config-key semantic divergence) and #3337 (architecture-diagram header divergence from spec); neither applies to crt-052. The only crt-052 echo of #3337 is the benign `TranscriptSnapshot`/`SessionTranscriptSnapshot` naming nit flagged above, which does not rise to a tester-assertion divergence.
- Stored: nothing novel to store -- crt-052 produced a clean PASS with no recurring cross-feature misalignment pattern. The findings are feature-specific (config-knob derivation, one naming nit) and do not generalize into a new vision pattern. The relevant traps (secrets-persist via memoization #3793, four-return gating #4750) are already captured.
