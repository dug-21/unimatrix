# infra-004 — Architect Retrospective Report

> MODE: retrospective. Feature: infra-004 (artifacts under product/test/infra-004/).
> PR #858 (MERGED), issue #859. Reviewed: ARCHITECTURE.md, ADR-001..004 (#5349-#5352),
> pseudocode OVERVIEW, gate-3a/3b/3c (incl. iter2/iter3), RISK-COVERAGE-REPORT,
> both investigator reports, #859 comments. Stewardship region reviewed: #5349-#5355.

## 0. Stewardship review of entries stored this cycle (#5349-#5355)

| Entry | Verdict | Action |
|-------|---------|--------|
| #5349 ADR-001 (warmup barrier) | Bound held; SIGNAL revised by implementation | **context_correct -> #5359** + Prerequisite edge |
| #5350 ADR-002 (tri-state exit-2/INFRA) | Validated by outcome (R-05, tristate 19/19); edges already correct (Prereq->#5192, Supports<-#5353) | Keep as-is |
| #5351 ADR-003 (blast-radius + sqlite3) | Validated (fail-closed table honored; sqlite3 self-contained) | Keep as-is |
| #5352 ADR-004 (cold-model proof + tag strategy) | **Strongly validated by outcome** — front-loading caught 2 latent defects | Keep as-is (outcome captured in new procedure #5360) |
| #5353 (static return-not-exit check) | Well-formed pattern, real teeth, good edges | Keep as-is |
| #5354 (marker-collision test: shadow recompute fn) | Well-formed pattern | Keep as-is |
| #5355 (marker PII-safety lesson) | **INCOMPLETE** — predated the two-filter discovery | **context_correct -> #5358** (extended to full two-filter contract) |

Structure/quality across the region is good (ADRs follow Context/Decision/Consequences; patterns are scoped with why+fix). Only #5355 was substantively incomplete; #5349 needed an outcome-correction note.

## 1. Patterns

**SKIPPED — folded into the corrected #5358 lesson.** The candidate "a content-probe gate
marker must satisfy every acceptance filter on EVERY served write surface it probes" is the
exact generalizable takeaway now carried by corrected #5358 (its closing paragraph states it
as a forward design principle). Storing a near-duplicate pattern alongside the lesson is the
over-storage the retro brief warns against. No new pattern stored. (#5353 and #5354 already
cover the reusable test-construction patterns from this cycle.)

## 2. Procedures

- **#5360** (NEW) — "Front-load cold-path proof of a blocking release gate via an in-feature
  byte-identical workflow_dispatch BEFORE flipping the needs: edge." topic: release-pipeline.
  The reusable how-to generalizing ADR-004 (#5352) beyond infra-004; carries the two-defect
  evidence as its proven rationale. Reusable for any future release-pipeline blocking gate.

## 3. ADR status

| ADR | Entry | Status | Reason |
|-----|-------|--------|--------|
| ADR-001 warmup barrier | #5349 -> **#5359** | **VALIDATED w/ correction note** | Bound (180s) held and is now measurement-corroborated (cold-ready ~5s, #767 ~70s floor). But the warmup SIGNAL changed observe-durability -> MCP context_store round trip, and OQ-WB-1 resolved affirmatively. Corrected (not superseded): decision substance unchanged; note records the surface change + resolved OQ. |
| ADR-002 tri-state INFRA | #5350 | **VALIDATED by outcome** | R-05 swallowed-exit-code proven; tristate 19/19; siblings byte-unchanged. No revision. |
| ADR-003 blast-radius + sqlite3 | #5351 | **VALIDATED** | Fail-closed table honored end-to-end; pull->INFRA divergence held; sqlite3 self-contained. No revision. |
| ADR-004 cold-model proof + tag strategy | #5352 | **STRONGLY VALIDATED by outcome** | The front-loaded AC-11 dispatch (GREEN, deterministic x2) caught two latent defects pre-merge that no off-Docker test surfaced — the standout decision of the cycle. No revision; generalized into procedure #5360. |

Note on ADR-001: handled as a context_correct NOTE, not a supersession — the decision stands,
only the probed surface was refined and an open question resolved. No human-approval reversal
was needed or performed.

## 4. Lessons

- **#5358** (corrected from #5355) — extended the PII-only marker lesson to the FULL two-filter
  contract: the marker must simultaneously satisfy the MCP-path PII content-scanner AND the
  observe-path looks_like_feature_id filter, which pull in OPPOSITE directions on digit runs;
  resolution is a short fixed all-digit token + letter-dominant base36 (both filters pass by
  construction), with symmetric fail-loud self-checks pinning BOTH server contracts (drift-proof
  Rust scanner anchor + bash feature-id oracle, exercised through the REAL default derivation).
- **#5361** (NEW) — "A readiness/warmup probe must exercise the actual dependency it claims to
  prove — observe-durability is not an embed-warmth signal." Generalizable beyond infra-004:
  trace the probe path to the dependency; prefer probing the same served surface the load-bearing
  ops use; a generous timeout cannot fix a wrong signal.
- **Capstone (front-loading caught two latent defects)** — captured as the proven rationale
  inside procedure #5360 rather than a separate lesson, to avoid redundancy with the procedure
  that operationalizes it.
- **Two REWORKABLE gates NOT stored.** Gate 3a iter1 (Knowledge Stewardship block not written
  to an artifact) and Gate 3c iter2 (coverage report not regenerated after a folded-in fix) are
  recurring PROCESS gaps / workflow choreography, not engineering lessons. Per project rules
  ("do not store workflow choreography"; the stewardship-block gap is a known recurrence) these
  are not stored as Unimatrix entries. Flagged below as retro findings instead.

## 5. Retrospective findings (hotspots, outliers)

- **Hotspots are a long-running-session artifact, not a quality signal.** All 5 warnings
  (file_breadth 82, cold_restart after 78m gap, mutation_spread 53, session gaps, Bash failed
  5x) cluster in the post-gate pr-review CI-debug window — the multi-round fix loop that found
  and fixed the two latent defects. Expected shape for a cold-path-debug session; no action.
- **adr_count 4 > threshold 3 (info)** — justified. The four ADRs are genuinely distinct
  decisions (warmup placement; tri-state INFRA; blast-radius/sqlite3; cold-model/tag strategy),
  each load-bearing and each validated. Not over-decomposed.
- **sleep_workarounds x6 (info)** — recommendation stands for future sessions: prefer
  run_in_background + TaskOutput over sleep-polling. Process nudge, not stored.
- **Process gaps worth a protocol nudge (not Unimatrix entries):**
  (a) Stage-3a agents must write the `## Knowledge Stewardship` block into a committed artifact —
  recurrence of a known gap; the gate caught it (REWORKABLE -> PASS) so the control works, but
  it keeps recurring. (b) Regenerate RISK-COVERAGE-REPORT after any folded-in fix before
  re-running Gate 3c (doc-currency lag). Both are choreography for the protocol/SM, not knowledge.
- **Transcript candidates: low value, did not over-weight.** Mostly scope-phase phase_gate/decision
  events, provenance "reconstructed", elided_bytes 0, no post-gate-window candidates. Relied on
  briefing + artifacts for the post-gate lessons, as instructed.
- **Standout outcome.** The cycle's central validated design bet — ADR-004's in-feature
  cold-model dispatch before trusting the blocking flip — is exactly what turned two
  release-blocking latent defects into pre-merge fixes. This is the durable takeaway and is now
  reusable via procedure #5360.

## 6. Relationship edges

**One edge asserted (HIGH bar met):**
- **#5359 (ADR-001 warmup barrier) --Prerequisite--> #5358 (two-filter marker contract).**
  Justification (traversal-necessary): the warmup barrier now probes via the MCP context_store
  leg, so its marker is governed by both server filters; an agent re-tuning the warmup
  marker/surface who does not read #5358 first will reintroduce the exact INFRA defect. Asserted
  at correction time (target #5358 already existed).

**Considered and NOT asserted (default-none, bar not met):**
- ADR-004 <-> ADR-001 / ADR-002: complementary but cross-referenced in prose; not must-read-first
  to avoid a wrong decision. Supports does not apply between two decisions.
- procedure #5360 --Supports--> ADR-004 #5352: discoverable by search; "connectedness", not a
  must-traverse-to-avoid-a-wrong-decision link.
- #5361 (readiness-probe lesson) <-> #5359: the feature-specific rationale is already inline in
  #5359's correction note; #5361 stands alone as the generalizable principle. No edge needed.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search ("infra-004", k=20) + context_get #5349/#5350/#5351/#5352/#5353/#5354/#5355 (markdown) -- reviewed the full #5349-#5355 stored region; #5355 was incomplete (predated the two-filter discovery), #5349's warmup signal was revised by implementation; ADR-002/003/004 validated by outcome.
- Stored: corrected #5355 -> #5358 (full two-filter marker contract); corrected #5349 -> #5359 (warmup-signal correction note + OQ-WB-1 resolved) with a Prerequisite edge -> #5358; entry #5360 (procedure: front-load cold-path proof before flipping a blocking gate); entry #5361 (lesson: a readiness probe must exercise the dependency it proves). Skipped a separate "per-surface filter" pattern (folded into #5358) and did not store the two REWORKABLE process gaps (workflow choreography, not knowledge).
