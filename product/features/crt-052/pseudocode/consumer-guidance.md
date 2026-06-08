# C10 — Consumer Guidance

**Target source:** `.claude/skills/uni-retro` + the cycle-review protocol step (docs/guidance only)
**Wave:** A — **NO code, NO reference to `transcript_hold.rs`.**
**ADRs:** ADR-007 (provenance weighting). **AC:** AC-13. **FR:** FR-15.
**Sequencing:** independent doc work; can land any time in Wave A.

## Purpose

Instruct the cycle-review consumer (the retrospective agent invoking `context_cycle_review`) how to
extract the `transcript_candidates` section into `context_store` with feature attribution. Rules SELECT
server-side; the AGENT EXTRACTS all semantics (Constraint 6). This is guidance, not code — no pseudocode
functions, only the content the skill/protocol step must contain.

## Required Guidance Content (AC-13 review checklist)

The uni-retro skill (and the protocol cycle-review step) MUST instruct the agent to:

1. **Four-family extraction.** For each `TranscriptCandidate`, re-classify (the `family_hints` are
   ADVISORY, not authoritative) into the four target families and store via `context_store` with
   feature attribution:
   - **Decision** → ADR (`/uni-store-adr`).
   - **Rework** → pattern / lesson as appropriate.
   - **Lesson** → `/uni-store-lesson`.
   - **PhaseGate** → procedure / gate-narrative as appropriate.
   The agent decides the family; the server's hint is a starting point only.

2. **Provenance weighting (ADR-007).** Read `SessionLossInfo`. Weight `Reconstructed`-provenance
   candidates DIFFERENTLY (lower trust — 0.81 fidelity floor, DEC-weakest). Note when high
   `elided_bytes` likely lost EARLY decisions (ass-070 Q5: elision clips the head, the highest-value
   DEC family) and temper decision-family extraction for elided/reconstructed sessions.

3. **The ass-070 Q8 folds:**
   - Join warning-level hotspots to TIMESTAMP-ADJACENT candidates for rework-why narratives.
   - Treat gate-failure narratives as UNITS (do not fragment a gate failure across stores).
   - Build a human-intervention ledger from USER-block content.
   - Narrate phase transitions from phase/gate-family candidates.

4. **Call-time-vs-cached note (OQ-4 / AC-05 — explicit).** Candidates reflect the buffer content
   present AT CALL TIME, NOT the memoized `RetrospectiveReport`. On a memoization HIT, candidates are
   distilled fresh and MAY DIFFER from the cached report. The agent treats candidates as call-time
   content and does not assume they match the cached metrics.

5. **Cap-drop awareness (AC-08).** Read `SessionLossInfo.dropped_candidates`: if a session/cycle had
   candidates truncated by the volume caps, note that the narrative may be incomplete for that session.

## Dependency Posture (AC-13 / NFR-6)

The guidance review also confirms (CI/protocol checklist): `cargo audit` passes; no new heavyweight
runtime dependency was introduced (regex-class only). This is a review-gate item the guidance step
references, not content the agent acts on.

## Data Flow

- **Input (to the agent):** the `transcript_candidates` response section (candidates + loss).
- **Output:** `context_store` writes (ADRs/lessons/patterns/procedures) with feature attribution — the
  ONLY path distillation output reaches the knowledge base (AC-09, two-pipe boundary preserved).

## Error Handling

N/A (documentation). The guidance instructs the agent to proceed normally when the section is ABSENT
(no transcripts / nothing to report — AC-04): cycle review still produces its standard report.

## Key Test Scenarios (verification = documentation review checklist, AC-13)

- Four-family extraction instructions present.
- Q8 folds present (warning-hotspot join, gate-failure units, human-intervention ledger, phase narration).
- Call-time-vs-cached note present and explicit.
- Provenance-weighting + cap-drop awareness instructions present.
- Feature-attributed `context_store` usage shown.
- `cargo audit` / dependency-diff review-gate items referenced.
