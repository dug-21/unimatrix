# Alignment Report: vnc-027

> Reviewed: 2026-06-08
> Artifacts reviewed:
>   - product/features/vnc-027/architecture/ARCHITECTURE.md (+ ADR-001..ADR-007, incl. amended ADR-004/ADR-006)
>   - product/features/vnc-027/specification/SPECIFICATION.md
>   - product/features/vnc-027/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md + goal entries #4671, #4673, #4677, #4678, #4710
> Scope source: product/features/vnc-027/SCOPE.md + SCOPE-RISK-ASSESSMENT.md (SR-01..SR-13)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly advances `personal-cloud` (#4710): single JS/TS edge language, F4 of the committed F1–F6 delivery path |
| Milestone Fit | PASS | F4a boundaries with vnc-030/F5/F6/crt-052 explicitly held in all three docs |
| Scope Gaps | PASS | All 4 SCOPE goals and AC-01..AC-10 traced to FRs/ACs; no gaps |
| Scope Additions | VARIANCE | Mechanical `accept: None` edits at `hook.rs` construction sites contradict the SCOPE non-goal "any change to it"; other additions (AC-11/AC-12, pruneOffsets wiring) are risk-sanctioned |
| Architecture Consistency | WARN | Spec FR-30/AC-10 still says "TaskCompleted and/or age-prune"; amended ADR-006 decided age-prune-only (TaskCompleted branch unreachable). Resolved at ADR level, not reflected in spec text |
| Risk Completeness | PASS | All SR-01..SR-13 traced; R-14..R-18 added; security surfaces (frame caps, accept allowlist, frozen-hook blast radius, F-02 gate) covered |

Counts: 4 PASS, 1 WARN, 1 VARIANCE, 0 FAIL.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | `hook.rs` mechanical edits | RISK-TEST-STRATEGY R-07 scenario 1: "the mechanical `accept: None` edits at hook.rs construction sites". SCOPE Non-Goals: "Rust `hook.rs` retirement or **any change to it** — F6 ... the Rust hook needs zero changes." See Variance 1 |
| Addition | AC-11 (wire additivity), AC-12 (HTTP regression guard) | Not in SCOPE's AC list, but explicitly recommended by SCOPE-RISK-ASSESSMENT SR-03/SR-08 — sanctioned, no approval needed |
| Addition | `pruneOffsets` wired live on every FNF spawn | Previously dead code; required to make ADR-006's age-prune-only decision real. Guarded by R-14 scenario 2 (fail-open, no sync-trio I/O). Within the FR-16 carry-item's intent |
| Simplification | AC-10 keying resolves to age-prune-only | Rationale (ADR-006, amended): `TaskCompleted` is registered nowhere — keying to it is unreachable; registering it would contradict the hook-set-reduction goal. SCOPE's "and/or" wording permits this. Explicit decision, pinned by unit test — not silent |
| Simplification | AC-03 "byte-identical" bounded by an accepted-divergence register | FR-21/FR-22: lone-surrogate (#4788) formally excepted; corpus = post-reduction event set only. Exactly what SR-04/SR-06 recommended ("enumerate accepted divergences ... do not leave the parity bar ambiguous") |
| Simplification | SCOPE OQ5 (worktree cwd dump) mechanism shifted | SCOPE assigned the stderr dump to the vnc-030 design session; architecture settled hash parity empirically instead (ADR-007: main/worktree/deep-subdir all hash to `0d62f3bf1bf46a0a`, matching the live daemon) and defers the literal dump to the dogfood soak. The consuming need (socket-path resolution, FR-15/AC-02 fixtures) is satisfied; informational only |

No scope gaps: SCOPE Goal 1 → FR-5..FR-16/AC-01/AC-02; Goal 2 → FR-17..FR-26/AC-03..AC-07; Goal 3 → FR-27..FR-29/AC-08; Goal 4 → FR-1..FR-4/AC-09 + FR-30..FR-31/AC-10. SCOPE OQ1–OQ4 all resolved per the uni-zero recommendations SCOPE recorded (ADR-002, ADR-001, ADR-004, FR-32). SCOPE constraints carried verbatim into spec §7.

## Variances Requiring Approval

1. **What**: The OQ2 server-side-preformatted design (ADR-001) adds `accept: Option<String>` to `ContextSearch`/`CompactPayload` in `wire.rs`, which forces mechanical `accept: None` edits at construction sites inside `hook.rs` — the file SCOPE's Non-Goals declares needs "zero changes" and the feature's own parity oracle.
   **Why it matters**: (a) SCOPE letter says "any change to it"; the SCOPE Constraints section says only "zero changes to `hook.rs`/`transport.rs` **behavior**" — the docs resolve the tension in favor of the behavior reading without flagging it to the human. (b) SCOPE-RISK Assumption 5 warns that "any concurrent Rust-side change invalidates goldens mid-feature" — these edits touch the oracle mid-feature, the exact condition the assumption guards.
   **Recommendation**: **Accept.** The edits are compiler-forced by the SCOPE-endorsed OQ2 resolution (server-side preformatting was uni-zero's recorded recommendation in SCOPE itself), are non-behavioral by construction (`skip_serializing_if`), and AC-11 proves it (all pre-existing Rust parity fixtures + ts-rs bindings pass byte-unchanged; R-07/R-08 scenarios include an end-to-end run of the real frozen binary). Record the acceptance on #680 so F6 inherits a clean "hook.rs behavior-frozen" claim.

No FAILs.

## Detailed Findings

### Vision Alignment

- **personal-cloud (#4710)** — direct hit. Goal success criteria state: "Single edge language — JS/TS hook client only ... The Rust hook.rs CLIENT path retires once TS+UDS reaches parity (committed in principle)" and the delivery path names "F4 vnc-027 TS UDS client". This feature is that step; SCOPE's problem statement ("contradicts the single-edge-language vision ... blocks F6") quotes the goal accurately. User memory confirms: JS/TS is the only edge language.
- **self-learning / proactive-delivery** — protected, not degraded. The hook-set reduction removes only a duplicate signal (ass-069 Q3: PreToolUse observation duplicates PostToolUse) and an event the server provably ignores (ADR-004, amended: SubagentStop is an all-None fallthrough at `listener.rs:2919`, now pinned by R-12's lifecycle test). Sync-injection events — the proactive-delivery surface — are explicitly untouched (FR-29, AC-08). The learning layer's data feed loses nothing.
- **Architectural principles**: #5 graceful degradation and #6 client-is-an-adapter are the spine of the design (fail-open NFR-3 on every new path; UDS transport as an adapter on the existing SendResult contract, ADR-002). #3 capability/auth: UDS peer-credential posture explicitly unchanged, noted for F6 (security table). #8 no secrets: NFR-3 covers stderr/breadcrumbs. #1/#2/#4/#7: N/A — no knowledge-entry, audit-schema, graph, or hot-path changes (justification: this is client/transport infrastructure).
- **Minor attribution note** (vision: "everything is attributed"): cross-transport replay splits session attribution (`http-{sid}` vs raw `{sid}`) — accepted, documented, pinned by R-10 scenario 2, and rare (requires a mid-project config flip). Not elevated; the alternative would be transport-aware queue state, which contradicts the queue's transport-agnostic design.
- **Goal entry staleness** (stewardship, not a variance): #4710's delivery path still describes F4 as "TS UDS client + attribution-heuristic demotion" — the 2026-06-08 split moved attribution to vnc-030 (F4b, #699). The split is a recorded human decision; the goal entry should be refreshed by uni-zero, not by this review.

### Milestone Fit

Boundaries are held with unusual discipline across all three docs: vnc-030 (attribution — SCOPE Non-Goal 1, spec §9.1), F5 (installer/UX — SR-09 resolved by ADR-004's "F5 owns any UX around the key"), F6 (hook.rs retirement — frozen-oracle constraint), crt-052 (distillation — spec §9.6). No future-milestone capability is built early; the one forward provision (the unreachable TaskCompleted delete branch, ADR-006 §3) is a single equality check with an explicit "drop it if it costs more" rule — acceptable. The pinned delivery order (vnc-027 → vnc-030 → crt-052) and the cross-feature AC-09 first-commit contract are honored (R-02, merge sequencing §).

### Architecture Review

ARCHITECTURE.md is consistent with the spec and the ADRs. The load-bearing decision (ADR-001 server-side preformatting) follows the SCOPE-recorded uni-zero recommendation and the vnc-025 ADR-005 shared-core precedent, and eliminates the largest size-budget risk (SR-02) — vision-over-convenience is satisfied, not violated. The parity bar (full transport parity, four enumerated accepted divergences) implements SR-04/SR-06's recommendations precisely — the ambiguity that drove vnc-026 rework is closed. Post-risk-review amendments verified current: ADR-004 now states SubagentStop server-side independence with code evidence (R-12 resolved); ADR-006 now corrects the frame-type claim (TaskCompleted → SessionClose frame) and decides age-prune-only (R-04 resolved).

### Specification Review

FRs are testable, ACs carry verification methods and FR bindings, SCOPE AC-01..AC-10 are present "verbatim in intent" with the two risk-sanctioned additions (AC-11/AC-12). §9 NOT-in-scope mirrors SCOPE Non-Goals plus the divergences the risk assessment forced into the open (lone-surrogate fix, mixed-client mitigation, event-set parity). One staleness item (the WARN): FR-30/AC-10 retain SCOPE's "TaskCompleted and/or age-prune" wording, while amended ADR-006 has since resolved it to age-prune-only with an unreachable-but-tested TaskCompleted branch. The "and/or" tolerates the decision, so this is not a contradiction — but delivery agents reading the spec alone could implement TaskCompleted-primary keying. Recommendation: delivery planning treats ADR-006 as authoritative for FR-30 (a one-line spec note would remove the ambiguity; not blocking). Spec OQ3 ("run the worktree-cwd stderr dump at design time") is answered by ARCHITECTURE OQ1: not capturable in a design session, made immaterial by the ADR-007 hash fixtures, live dump deferred to the soak — adequate.

### Risk Strategy Review

Complete traceability: every SR-01..SR-13 maps to an architecture risk or an explicit accepted-divergence/verified-bounds resolution. The strategy goes beyond the scope risks where it matters for the vision: R-08 (a `Text` frame to a non-`accept` caller would break every deployed frozen Rust hook — tested end-to-end with the real binary) protects the mixed-client migration path the personal-cloud goal depends on; R-01/R-16 (silent FNF loss, dogfood soak loss) protect the knowledge-capture feed that self-learning depends on, with concrete rollback triggers. Security table covers hostile length prefixes, the `accept` allowlist, the F-02 exact-equality gate, and correctly defers the UDS peer-cred posture to F6. The two "items requiring human/spec attention" at the end of the strategy are both closed by the ADR amendments (verified in this review); the spec-wording residue is the WARN above.

## Knowledge Stewardship
- Queried: /uni-query-patterns for vision alignment patterns — weak matches only (#2298 config semantic divergence, #3337 doc-header divergence misleading testers; the latter informed the architecture-vs-spec consistency check). No recurring vision-variance pattern applicable.
- Stored: nothing novel to store — the single variance (mechanical oracle edits vs "untouched" scope language) is specific to the frozen-oracle F4/F6 sequence and retires with hook.rs at F6; it does not generalize across features.
