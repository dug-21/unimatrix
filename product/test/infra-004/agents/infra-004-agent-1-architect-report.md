# Agent Report — infra-004-agent-1-architect

## Task
Architecture for infra-004 — convert the infra-003 cross-tenant isolation smoke into a
blocking release gate (DoD: a cross-tenant leak cannot ship a release). Four deliverables:
warmup barrier, standing lane, in-feature cold-model GREEN, blocking flip.

## Artifacts
- product/test/infra-004/architecture/ARCHITECTURE.md
- product/test/infra-004/architecture/ADR-001-warmup-barrier-placement-and-bound.md (Unimatrix #5349)
- product/test/infra-004/architecture/ADR-002-exit2-infra-tristate-handling.md (Unimatrix #5350)
- product/test/infra-004/architecture/ADR-003-blocking-flip-blast-radius-containment.md (Unimatrix #5351)
- product/test/infra-004/architecture/ADR-004-cold-model-proof-and-post-merge-tag-strategy.md (Unimatrix #5352)

## Key Decisions
- **ADR-001:** Warmup barrier slots after `assert_routes_live`, before `run_isolation_matrix`;
  reuses `write_then_barrier` with a throwaway marker (no new mechanism, auto stub-seam
  compatible); bound = `WARMUP_DEADLINE_SECS` default 180s (#767 `READY_TIMEOUT_SECS`, validated
  under real cold HF download). SR-01 answered: the barrier covers only model-load (store
  liveness/registration is established earlier), exactly #767's measured profile. SR-02:
  classify-as-INFRA, no pre-pull.
- **ADR-002:** New **additive** `run_smoke_gate_tristate` in `release-gate-lib.sh`;
  `run_smoke_gate` untouched (SR-08 contained). Map: 0+marker→pass, 1→block, 2→`::warning::` +
  greppable marker + non-block, 3→hard-fail, other→block. Pull-failure is script-exit-2 (INFRA),
  not exit-4 — documented divergence.
- **ADR-003:** Only script-exit-2 is non-blocking; all harness-step failures + other exits
  fail-closed/block (matches the four existing blocking lanes' precedent). Self-contained
  sqlite3 provisioning (no hard #849 dependency). SR-04 deliverable = the explicit failure-source
  → blocking-outcome table in ARCHITECTURE §5.
- **ADR-004:** AC-11 cold-model GREEN via feature-branch `workflow_dispatch` (byte-identical to
  `main`) gates the flip; proves warmup+verdict+harness, NOT tag-push resolution. Single-PR flip
  with a budgeted post-merge tag round (#5267); safe because INFRA-never-blocks degrades a
  never-green-on-tag failure to visible-vacuous. Diagnostic-capture-first. SR-06 rebase
  constraint + SR-09 early GHCR-write verification + two-step fallback retained.

## Open Questions (for human / spec)
- SR-07: accept chronic-INFRA as documented human-vigilance risk for infra-004 (with grep-based
  escalation as a follow-up), given the marker is deliberately stable/greppable?
- SR-06: spec must require the AC-11 dispatch to build from a feature-branch tip == `main` HEAD.
- SR-09: verify dispatch-from-branch can push `:latest-amd64` early (low risk; nan-021 precedent).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned the reusable shapes applied here:
  #5347 (bidirectional N×M isolation smoke), #5192 (extract verify-by-name gate spine into a
  sourceable lib — no-pipe capture / return-not-exit / anchored marker), #5183 (ADR-003
  verify-by-name contract), #5180 (self-skip→hard-fail), #5258 (stub-drive appended gates),
  #5267 (never-green-on-tag lesson), #5184 (dispatch-vs-tag resolution).
- Stored: entry #5349 "ADR-001 Warmup Barrier", #5350 "ADR-002 Exit-2/INFRA Tri-State" (with
  Prerequisite edge → #5192), #5351 "ADR-003 Blast-Radius Containment", #5352 "ADR-004
  Cold-Model Proof + Post-Merge Tag Strategy" — all via context_store (category: decision,
  topic: infra-004).
