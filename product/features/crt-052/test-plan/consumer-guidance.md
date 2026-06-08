# Test Plan — C10 Consumer Guidance

**Component**: `.claude/skills/uni-retro` + the cycle-review protocol step (documentation /
guidance, not Rust). **ADRs**: ADR-009 (consumer guidance), ADR-007 (loss/provenance weighting).
**Wave**: A. **Verification**: manual documentation-review checklist + shell (`cargo audit`).
This is the AC-13 home.

## AC-13 — Consumer guidance + dependency posture (manual + shell)

### Documentation review checklist (manual — Stage 3c verifies presence)
The uni-retro skill / cycle-review protocol step MUST be updated to include:
- [ ] The **four marker families** (Decision, Rework, Lesson, PhaseGate) and that hints are ADVISORY
  (rules select, the agent extracts — Constraint 6).
- [ ] **Q8 folds** — how the transcript candidates fold into the retro's Q8 step.
- [ ] **Call-time-vs-cached note** — on a memoization hit, candidates are distilled FRESH from
  call-time buffer content and MAY differ from the cached report (AC-05, OQ-4).
- [ ] **Feature-attributed `context_store`** — the agent stores extracted knowledge via `context_store`
  scoped to the reviewed feature; distillation reaches the KB only via the agent, never server-side.
- [ ] **Provenance weighting** (ADR-007) — guidance to weight `Reconstructed` candidates differently
  (0.81 fidelity floor) and to note when early-decision elision is likely (`elided_bytes > 0`,
  ass-070 Q5: elision clips the highest-value DEC family).
- [ ] **Loss visibility** — how to read the `loss` section (per-session `elided_bytes`, `has_holes`,
  provenance) and the per-session / per-cycle cap dropped-counts.

### Shell / dependency posture
- `cargo audit` passes (AC-13, NFR-6).
- Dependency-diff review confirms **regex-class crate only** — no new heavyweight runtime dependency
  added (the marker set uses a regex-class crate, not an LLM/NLP runtime).

## Verification Method (Stage 3c)
- Manual: walk the checklist against the updated `uni-retro` skill / protocol step; record each item
  present or missing in the RISK-COVERAGE-REPORT AC-13 row.
- Shell: run `cargo audit`; capture pass/fail. Run the dependency-diff (e.g. `git diff` on
  `Cargo.toml`/`Cargo.lock`) and confirm only a regex-class crate was added.

## Assertions Summary (concrete)
- The retro guidance names all four families, the Q8 folds, the call-time-vs-cached note, the
  feature-attributed `context_store` flow, and provenance/loss weighting.
- `cargo audit` clean; dependency diff = regex-class only.
