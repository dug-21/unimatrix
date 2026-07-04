# Alignment Report: crt-057

> Reviewed: 2026-07-04 (REFRESHED for the ass-091 / #898 redesign; supersedes the boolean-era review)
> Artifacts reviewed:
>   - product/features/crt-057/architecture/ARCHITECTURE.md (+ ADR-001..ADR-006; ADR-001 load-bearing for residency/secrets)
>   - product/features/crt-057/specification/SPECIFICATION.md
>   - product/features/crt-057/RISK-TEST-STRATEGY.md
> Scope: product/features/crt-057/SCOPE.md (REWORKED by human 2026-07-04), SCOPE-RISK-ASSESSMENT.md (refreshed)
> Vision source: product/PRODUCT-VISION.md; goal #5219 (self-learning) + SL6 harvest; principle 8 / NG-1

## Redesign context (what changed since the prior review)

The prior LOCKED contract — a fused `include_transcript_candidates` boolean that both *emitted* candidates
and *triggered a purge* — is superseded. The reworked design (ass-091 ★ non-destructive note):

- `context_cycle_review` is **FULLY non-destructive** — no purge verb at all (not a flag, not a default).
  The eager review-purge is removed; reclamation is delegated entirely to the unchanged backstops
  (24h TTL / 64-cap / session-close).
- The transcript axis becomes a **read-only scoped retrieval** `transcript{ phase?, anchor?, match?, window? }`
  returning candidates + per-session `SessionLossInfo`; a no-match over a lossy/`Reconstructed` session is
  **INDETERMINATE**, never a silent false negative.
- Clock normalization is promoted to a first-class interface requirement.
- Retro causal synthesis is explicitly **agent-owned and out of scope** (NG-5).

Counts are re-baselined to the new design: **8 deliverables (D-1..D-8), 17 SCOPE ACs (AC-01..AC-17),
14 scope risks (SR-01..SR-14), 18 test risks (R-01..R-18 / 61 scenarios).**

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Principle 8 / NG-1 upheld and *strengthened* — disk posture unchanged, no candidate slot on the memoized report, content-free audits now at the backstop. Advances self-learning (#5219): scoped retrieval gives the retro agent targeted WHAT-transpired tools; D-6/D-7 wire the harvest into both protocols. Ownership boundary (Unimatrix serves planes, agent interprets) is squarely on-vision. |
| Milestone Fit | PASS | In-summary distillation deferred to ass-090 (NG-7), local inference deferred (NG-8, Q4 seam only). No future capability built early; R-18 report-body-invariance actively guards the NG-7 line. |
| Scope Gaps | PASS | Every SCOPE deliverable (D-1..D-8) and AC (AC-01..AC-17) is represented; SCOPE AC-01..AC-17 map 1:1 to SPEC AC-01..AC-17. |
| Scope Additions | WARN | Additions are all architect-delegated resolutions of SCOPE open questions (window default ±120 s/±3 blocks per OQ-2; `Window`/`r#match` shape per OQ-3; SPEC AC-18/AC-19 grounding OQ-2/NG-5). The `"summary"` alias DROP is the one breaking choice — verify no live caller before ship. |
| Architecture Consistency | PASS | ARCHITECTURE, ADR-001..006, SPECIFICATION, RISK-TEST-STRATEGY agree on the three orthogonal axes, file:line anchors, the ±120 s/±3-block window default, four-site fold-only gating, and both-protocols merge→close→retro. SPEC states "No conflicts against the architecture." |
| Risk Completeness | PASS | 14 SR risks → 18 R risks via a full traceability table; 61 scenarios. R-01 (silent false negative, the raison d'être) is the top risk; R-03 (persistence leak) is Critical with content-scans on all changed paths incl. reclamation-without-review. |

Counts: PASS 5, WARN 1, VARIANCE 0, FAIL 0.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | — | None. D-1 → ARCH §3/§4/§7, SPEC FR-6..FR-10 / AC-02; D-2 → ARCH §9, ADR-001(a), SPEC FR-11..FR-13 / AC-03; D-3 → ARCH §6, ADR-003, SPEC FR-14..FR-16 / AC-06,07; D-4 → ARCH §7.2, ADR-006, SPEC FR-17..FR-18 / AC-08; D-5 → ADR-001, SPEC FR-24 / AC-15; D-6 → ARCH §8, SPEC FR-19..FR-20 / AC-16; D-7 → ARCH §11, ADR-005, SPEC FR-21..FR-23 / AC-17; D-8 → full RISK-TEST-STRATEGY matrix (18 risks / 61 scenarios). |
| Addition | Window default ±120 000 ms / ±3 candidate blocks (SPEC FR-18 / AC-18, ADR-006 §7.3) | Resolves SCOPE OQ-2 ("window default sizing … not a blocker — a conservative default plus caller override"). Architect chose ±120 s/±3 blocks with over-inclusion as the safe direction. Within the delegated decision; caller-overridable; cap-bounded. |
| Addition | Ownership boundary as a NEGATIVE requirement (SPEC FR-25 / AC-19) | Formalizes SCOPE NG-5 as a testable negative (no GH-block synthesis, no applied-entry attribution, no rework↔cause join, no human-ledger). Directly grounded in SCOPE; not new scope — it fences scope out. |
| Addition | DROP of the `"summary"` render alias (SPEC FR-1 / AC-11, ADR-002) | SCOPE D-1 delegated "drop or fold" and calls the alias "dead". Architect chose DROP (breaking) over fold (non-breaking). In-scope but breaking — see Awareness item 2. |
| Simplification | `format` reduced to exactly `markdown\|json` | Rationale: removes a third divergent render path (SR-07); render-equivalence asserted (AC-11). Aligned with the render-only contract. |
| Simplification | Retro lifecycle (D-7) is *simpler* than the boolean era | Both review and cycle-close are now non-destructive, so merge→close→retro composes with no one-shot sequencing to guard (ARCH §11, ADR-005). The prior "buffers age out before a late retro" trade-off softens to ordinary TTL/cap aging, surfaced via loss propagation. |

## Variances Requiring Approval

**None requiring new human approval.** VARIANCE 0, FAIL 0.

Two items surfaced for awareness — both already resolved, not open variances:

1. **Memory-residency lengthening — now on EVERY path (human-ratified).**
   - **What**: Removing the eager review-purge means default / json / force / `transcript{}` all leave the
     buffer intact. Raw (possibly secret-bearing) bytes now reside in memory until a backstop reclaims,
     on every path — a larger posture movement than the boolean era, where a default review still purged.
     Worst case = up to 64 held buffers × per-buffer cap-bytes, for up to the 24h TTL (ADR-001 (b)).
   - **Why it matters**: Touches the *spirit* of principle 8 (secret-material handling), not its letter.
     Principle 8 governs databases; this is memory-only and NG-1 (never-persist-to-disk) is intact — the
     purge was never the disk barrier, so removing it does not weaken principle 8. Steady-state resident
     secret volume does rise.
   - **Disposition**: Bounded by the UNCHANGED cap/TTL/session-close backstops (behaviorally identical to
     "no review has run", which #4857 already budgets), no new persistence path, and explicitly ratified
     by the human in SCOPE "Accepted Residency Trade-off" and recorded in ADR-001 (b) via `context_correct`
     on #4742/#4857. No further approval needed; flagged so the posture change is visible and intentional.

2. **`"summary"` alias DROP is a breaking change.**
   - **What**: A live `format:"summary"` caller now receives `ERROR_INVALID_PARAMS` instead of a report.
   - **Why it matters**: Behavior break for an existing integration, if any exists.
   - **Recommendation**: Accept — SCOPE characterizes the alias as "dead" and R-12 sc.3 mandates a consumer
     sweep before ship. Delivery must run that sweep; if a live caller surfaces, reconsider fold-to-markdown
     (the non-breaking delegated option).

## Detailed Findings

### Vision Alignment

Principle 8 ("No secrets in any database") / NG-1 is the load-bearing constraint and is upheld and
strengthened, not weakened, by the redesign:
- **Disk posture unchanged** — ADR-001 (c), NG-1, NFR-2, AC-14: no buffer or candidate content reaches any
  SQL / file / log write on any changed path, including the read-only scoped-retrieval path and the
  now-longer-lived reclamation-without-review path.
- **No candidate slot on the persisted report** — the memoized `RetrospectiveReport` gains no candidate or
  loss field (#4850 anchor preserved); candidates + `SessionLossInfo` stay response-transient, attached
  out-of-band at assembly level (summary ⟂ Plane-B invariant, ARCH §2/§5).
- **Audit completeness preserved** (principle 2) — the content-free terminal audit relocates from "every
  review" to backstop reclamation only; ADR-001 (c) and SR-02 explicitly flag the derived caveat that
  "purge audit ⇒ a review occurred" no longer holds. Audit trail stays complete; its semantics are
  restated, not weakened.
- **Graph integrity under correction** (a #5219 success criterion) — the ADR amendment uses `context_correct`
  on #4742/#4857, not deprecate+store, preserving provenance (R-17, AC-15).
- **Graceful degradation** (principle 5) — aged/evicted/partial-buffer retrieval yields empty or
  `Reconstructed`-only candidates with loss surfaced, never a crash, never stale verbatim (NFR-8, AC-06, R-16).
- **In-memory hot path** (principle 7) — the common no-`transcript` path adds no new I/O or locks; the
  crt-055 content-opaque fold stays counter-only and idempotent across repeated non-purging reviews
  (NFR-4, R-14).

Self-learning (#5219 / SL6 harvest): the feature repairs the retrospective-harvest pipe
(`/uni-retro` → `context_cycle_review` → scoped `transcript{}` → agent-curated knowledge) that feeds SL6.
The redesign strictly improves the harvest surface: the retro agent gets *targeted* WHAT-transpired tools
(phase/anchor/match/window) instead of an all-or-nothing dump, retrieval is repeatable and non-destructive,
and honest loss propagation prevents a false-absence signal from poisoning attribution — aligned with the
goal's "learning pipeline resists poisoning" and "integrity-consistent under correction" criteria. D-6/D-7
wire `/uni-retro` into both protocols so the harvest runs consistently. crt-057 adds no new learning
dimension; it prevents silent starvation of the existing one and hands the agent a sharper instrument.

Ownership boundary: the design deliberately declines the richer three-source/attribution/human-ledger
synthesis proposed in the ass-091 headline (NG-5, FR-25) and holds Unimatrix to "serve honest planes; the
agent interprets." This is on-vision — the vision doc scopes Unimatrix as a knowledge engine, not an
orchestration/attribution engine, and keeps curation agent-driven. Resisting the headline's scope is a
milestone-discipline win, not a gap.

The residency window is the only genuine posture movement; it is memory-only, bounded, and human-ratified
(Awareness item 1).

### Milestone Fit

NG-7 holds the line cleanly: crt-057 defines *whether/how the buffer is read* (non-destructive summary +
honest scoped Plane-B retrieval); ass-090 (#896) explores *what more to distill INTO the summary*. The
source docs enforce this as a hard boundary — SPEC "NOT in Scope" marks any report-body enrichment a
variance, and R-18 sc.1 (report-body-invariance-under-buffer-state) is the strongest guard that no
transcript-derived signal enters the summary. NG-8 keeps Q4 local inference to a documented seam
(crt-056 `BackgroundJob` registry) with nothing built. The ordering dividend (crt-057 lands the fixed
contract first; ass-090 builds on it) is stated without pulling future work forward.

### Architecture Review

The three-axis decoupling rests on assumption A-1 (report is buffer-content-independent). ARCHITECTURE §5
re-verifies A-1 with file:line evidence in the worktree: `build_report` (`report.rs:15-53`) takes no
transcript argument; the sole non-test reader of buffer *content* is `take_transcripts_for_feature`
→ `distill_before_purge` (one caller); the other review-time reader is the content-opaque crt-055 fold.
With the purge removed, the four-site #4750 lockstep now gates ONLY the fold read (ADR-004), and the design
correctly notes source-assertion counting cannot see scope threading — pushing enforcement onto behavioral
memo-hit rows (R-07). Clock normalization is routed through a windowed (never exact) join with `byte_offset`
fallback for `ts:None` (ADR-006, §7). Orphan deletion of `purge_cycle_transcripts` + helpers and re-homing
the exhaustive `TranscriptRetention` match onto the backstops (anti-stub / C-5) are specified. Consistent
with SCOPE C-1..C-8 and SPEC CON-1..CON-8.

### Specification Review

SPEC is a full replacement of the boolean-era spec with an explicit retirement/renumbering block. It maps
every reworked SCOPE AC (AC-01..AC-17) 1:1 with a verification method, and adds AC-18 (window default,
grounding ADR-006/OQ-2) and AC-19 (ownership boundary negative, grounding NG-5) — both traceable, neither
inventing scope. FR-1..FR-25 and NFR-1..NFR-9 cover render/recompute/retrieval orthogonality, non-destructive
review, loss propagation, clock normalization, consumer/lifecycle reconciliation, and the negative ownership
requirement. Boolean-era FRs/ACs are explicitly retired, not silently dropped.

### Risk Strategy Review

Complete coverage: all 14 SR risks trace to R-risks in the Scope Risk Traceability table, and the register
is correctly re-derived around the new failure surface rather than ported from the destructive-path era
(the retired boolean-era rows are documented). The two highest-value concerns sit at the right altitude —
R-01 (silent false negative) demands a per-loss-condition matrix (`elided_bytes>0` / `has_holes` /
`Reconstructed` / `dropped_candidates>0` + OR-combination) proving INDETERMINATE per session, the feature's
raison d'être; R-03 (persistence leak) applies content-scans across all changed paths including the
least-tested reclamation-without-review path, keyed on synchronous state per #4879 (R-10). R-02/R-04
(consumer + two-protocol blast radius) are guarded end-to-end and per protocol, not at the server surface.
Secrets, graceful degradation, ReDoS surface of the caller-supplied `match` regex, and the NG-7 boundary
all have explicit guards.

## Knowledge Stewardship

- Queried: /uni-query-patterns (context_search, category=pattern) for vision alignment / scope-addition /
  milestone-discipline patterns — surfaced #3742 (optional future branch in architecture must match scope
  deferral — WARN pattern) and #2298 (config semantic divergence vs vision example). #3742 applies loosely:
  NG-7/NG-8 deferrals are future-branch boundaries held clean (no WARN warranted), and the OQ-2/OQ-3/"summary"
  resolutions are delegated decisions the architect closed within SCOPE's delegation — the only residual is
  the breaking `"summary"` DROP, captured in the Scope Additions WARN. Neither pattern triggered a finding
  beyond that WARN.
- Stored: nothing novel to store — the misalignment surface here is feature-specific (residency posture
  human-ratified in SCOPE + ADR-001; `"summary"` DROP is a delegated one-off; ownership-boundary restraint
  is intrinsic to this feature). No new pattern generalizes across 2+ features beyond the existing #3742.
  Note: the risk strategist flagged a candidate cross-feature pattern (a read-only-retrieval redesign that
  removes a destructive verb relocates risk from "did the destructive gate fire" to "is the negative result
  honest + is the now-sole backstop reclamation correct"); it awaits a 2nd-feature confirmation before it
  earns storage — deferred, not stored here.
