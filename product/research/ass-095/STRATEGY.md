# ass-095 — Strategic consult (Q1/Q2), uni-zero

Follows FINDINGS.md. Grounds the empirical result in the product vision + active roadmap
(#898 ass-091 cycle-review redesign, #941 cross-run analytics, self-learning goal #5518).
No Unimatrix writes (research + uni-zero do not store non-goal knowledge).

## Q1 — What we can make materially better
1. **`SignatureFailureRule`** (write-block class, 53 events / 23 sessions) — proven build; runs
   on Plane A durable observations, needs no Plane B transcript. → **#948**.
2. **Unscoped `SignatureScanner`** ships the precision-0.67 approach today (counts prose that
   discusses errors). Live defect. → **#949**.
3. **Retrospective → `lesson-learned` pollution** (190 circular engine-output nodes). Hardening;
   any KB-signal consumer should default-filter `source:retrospective`. Not yet filed.
4. **Inert `PhaseDurationOutlierRule`** (emits nothing in-engine). Cleanup. → folded into #948.

## Q2 — What we're not looking for today
- **A. Failure events as the self-learning negative signal.** Self-learning's north-star is blocked
  on SL-METRIC — "no untainted signal of what's bad." Failure events are behavioral ground truth.
  Today they die in a human-facing retro. Highest-value, most likely to sprawl — spike only after
  the SL-METRIC keystone conversation. Not filed.
- **B. Deterministic = rear-view mirror.** A signature bank catches only already-named classes
  (write-block was findable only because #5465 named it). The genuinely-unnamed class is NOT a
  signature problem — that is the earned role for inference (the #898 Q4 lane). Evidence, not a build.
- **C. Observation store is an unqueryable analytics corpus.** Write-block spanned 20+ cycles yet is
  invisible to every current tool (only per-cycle access exists). Empirically validates #941's
  premise; the beyond-#941 idea is a failure taxonomy over time as a methodology-quality metric.
- **D. RETRACTED (2026-07-10) — the "learning loop must not eat its own ground truth" framing was
  backwards.** It labeled the success state as a cost: an error trending to zero is *elimination of the
  bad issue* (the objective), not a lost signal. And discovery of the next unnamed class does NOT depend
  on human lesson-authoring — the failure signal is behavioral (ass-095 measured 273/639 failures matching
  no curated signature; that unmatched residual is the discovery beacon). Automating known-class detection
  frees human attention toward the residual rather than starving anything. **What survives, on its own
  merit:** don't auto-source signatures from `lesson-learned` — 1.1% literal density + circular retro
  output — use a curated catalog. That is a data-hygiene point, not a principle. Not added to
  PRODUCT-VISION.md.

## Actions taken
- Created **#948** feat(context_cycle_review): deterministic detection enhancements (ass-095).
- Created **#949** bug(context_cycle_review): SignatureScanner counts prose, not events.
- #898 / #941 — **not touched** (human: ass-091 research already run; enhancements aggregate in #948).
- #938 — **not touched** pending human decision on a pointer comment.
- Principle D — **not** promoted to PRODUCT-VISION.md (human unconvinced).
