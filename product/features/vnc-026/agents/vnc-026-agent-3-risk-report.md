# Agent Report: vnc-026-agent-3-risk (architecture-risk mode)

## Deliverable
- `product/features/vnc-026/RISK-TEST-STRATEGY.md` — 20 risks (2 Critical, 6 High, 9 Medium, 3 Low), every risk mapped to test scenarios, all SR-01..SR-11 + A-1..A-4 traced.

## Risk Summary
- **Critical**: R-01 (build-request parity divergence), R-14 (cross-platform stdin — FR-01's `fs.readFileSync('/dev/stdin')` fails on Windows, fail-open hides total failure).
- **High**: R-02 corpus completeness, R-04 UTF-8 offset corruption, R-06 elision hole semantics vs F2 merge, R-09 silent config-resolution failure, R-10 breadcrumb accuracy, R-11 ownership-regex merge corruption.
- Three gate-blocking findings flagged for the leader: FR-01/ARCHITECTURE stdin discrepancy + OS-less CI matrix; ownership regex `\S*` vs spaced install paths; F2 hole-semantics assertion needs pinning with vnc-025 before Layer-2 gates.

## Knowledge Stewardship
- Queried: /uni-knowledge-search for lessons (gate rejections) and risk patterns — surfaced #4473 (warn+continue masks failure-path tests), #4321 (trust-boundary validation), #4452 (vacuous regression guards), #2984 (wrong expected values), #1203 (one-pass validation); all used as evidence to elevate R-09/R-10 likelihood and to add R-20 (drift-check vacuity).
- Stored: nothing novel to store — the recurring patterns informing this strategy (#4473, #4452) already exist; the Windows `/dev/stdin` trap is single-feature so far, below the 2+ feature bar.

---

# Update: ADR-008 pin (targeted, post-vnc-025 merge)

## Task
Pin R-06 against the merged vnc-025 server buffer per ADR-008 (end-anchored elision frame: declared `offset = file_len − bytes.length`); resolve stale gate notes; re-grade R-06.

## Sections Changed in RISK-TEST-STRATEGY.md
- **Header**: inputs now ADR-001..008.
- **Risk Register**: R-06 rewritten as "elision frame geometry regression", re-graded **High severity / Low likelihood / Medium priority**, moved to the Medium block; R-14 and R-17 descriptions de-staled (FR-01 fixed in spec; vnc-025 merged PR #692).
- **R-06 mapping**: scenario 2 asserts the end-anchored frame shape (frame ends exactly at `file_len`, uniform ADR-004 advance rule, NOT span start); added the four pinned Layer-2 helper assertions from ADR-008 — hole behind content at `(last_offset, file_len − bytes.length)`; `high_water == file_len`; `contiguous_tail` crosses the elision seam; no NUL bytes ever served (verified against merged `crates/unimatrix-server/src/infra/session_transcript.rs`). Re-grade rationale recorded inline.
- **R-14 scenario 1**: `/dev/stdin` vs fd-0 discrepancy marked resolved in spec; fd-0-on-Windows test obligation retained.
- **Traceability**: SR-04 row notes the ADR-008 pin.
- **Coverage Summary**: High 6→5, Medium 9→10 (counts now 2/5/10/3).
- **Gate notes**: note 1 RESOLVED (OS CI-matrix obligation stands under R-14); note 3 RESOLVED via ADR-008 with the four pinned assertions; note 2 (spaced-path regex) remains open.

## R-06 Re-grade Rationale
Severity stays High — a span-start regression silently yields a phantom unfillable hole and permanent PreCompact starvation (ADR-008 Context). Likelihood drops to Low — semantics frozen against merged code, single formula, deterministic Layer-2 assertions catch any regression. Priority Medium.

## Knowledge Stewardship (update)
- Queried: context_search "transcript buffer elision hole semantics contiguous tail vnc-025" — found #4758 (ADR-008, already stored), #4740 (vnc-025 ADR-002 server contract), #4739.
- Stored: nothing novel to store — the decision is already #4758; the geometry-regression risk is feature-specific and lives in the strategy document.
