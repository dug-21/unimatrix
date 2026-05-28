# Scope Risk Assessment: nxs-013

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Removing `UNIMATRIX_CONFIG` ENV from Dockerfile changes the default config discovery path in-container | Low | Low | No external users exist — this is the initial correct design. docker-compose.yml comments should target new users setting up their first deployment |
| SR-02 | Distroless runtime prevents shell-based verification of config loading — only `docker inspect` and log output are available to confirm the ENV removal works | Low | Med | Spec should require log-based verification steps in acceptance criteria (AC-10 partially covers this) |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | PRODUCT-VISION.md W2-1 still describes a two-volume model (`unimatrix-data` + `unimatrix-shared`) that was never shipped — updating this risks scope creep into broader vision document revision | Med | Med | Constrain edits to W2-1 volume description only; do not revise other W2-1 content or adjacent sections |
| SR-04 | OQ-02 (whether WAVE2-ROADMAP.md is a historical document) is unresolved — editing it without a decision risks contradicting the document's intended purpose | Low | Med | Resolve OQ-02 before implementation; if historical, add a "corrected" annotation rather than rewriting |
| SR-05 | Three open questions (OQ-01 through OQ-03) remain in SCOPE.md — unresolved OQs risk rework if answers change the deliverable set | Med | Low | Architect/spec writer should resolve or explicitly defer each OQ before implementation begins |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | PR #636 provenance types (`ConfigLoadResult`, `ConfigProvenance`, `SourceStatus`) are the foundation — if log label changes in main.rs conflict with the structured provenance model, the 7 provenance tests may need updates despite AC-09 claiming "unmodified" | Med | Low | Verify that provenance tests assert on structured types (not log message strings) before assuming zero test changes |
| SR-07 | README is a single managed file (ADR-001/nan-005, 380 lines) updated by uni-docs agent — manual edits to the Configuration section risk merge conflicts with other in-flight README changes | Low | Low | Check for open PRs touching README before editing; prefer minimal, surgical edits |

## Assumptions

- **"Code already works correctly"** (SCOPE.md Rationale, line 85): Assumes `load_config` Step 2 path resolution inside the container produces a valid path when `UNIMATRIX_CONFIG` is unset. ADR-005 (#4573) confirms `HOME=/data` makes this work, but this assumption should be verified in a container build, not just code reading.
- **"Existing tests must pass unmodified"** (AC-09): Assumes provenance tests do not assert on exact log message strings that SR-06 identifies as changing. If they do, AC-09 is self-contradictory with AC-03.
- **PR #636 is stable** (Constraint, line 112): Assumes no follow-up changes to provenance types are in flight. Verified: merged 2025-05-25.

## Design Recommendations

- **(SR-06)**: Architect should confirm that provenance test assertions are structural (type-based), not string-based, before committing to "zero test changes." If string-based, budget for test label updates.
- **(SR-03, SR-04)**: Spec writer should define exact edit boundaries for PRODUCT-VISION.md and WAVE2-ROADMAP.md — line ranges, not "update the section."
- **(SR-05)**: Resolve OQ-01/OQ-02/OQ-03 during design, not during implementation.
