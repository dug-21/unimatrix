# nan-019 Agent Report — Pseudocode (nan-019-agent-1-pseudocode)

## Deliverables
- product/features/nan-019/pseudocode/OVERVIEW.md
- product/features/nan-019/pseudocode/release-smoke-jobs.md (smoke-amd64 + smoke-arm64, shared/parameterized)
- product/features/nan-019/pseudocode/create-container-manifest.md (needs/if rewire)
- product/features/nan-019/pseudocode/docker-http-posture-smoke.md (AC-05 grew-assertion)
- product/features/nan-019/pseudocode/test-gate-logic-stub-smoke.md (pre-merge truth-table test)
- product/features/nan-019/pseudocode/test-tag-parity.md (pre-merge static parity test)

## Components covered
Per Component Map: smoke-amd64, smoke-arm64 (one shared file, neither dropped — NFR-06),
create-container-manifest rewire, docker-http-posture-smoke AC-05, plus the two pre-merge test artifacts.

## Load-bearing contracts honored VERBATIM
- Run-marker capture pattern (set +e / RC=$? / set -e / case 0|1|3|* / anchored grep
  `grep -qx '\[783-smoke\] ALL GATES PASSED.*'`) — ADR-003, pinned.
- Un-stripped push tag: `VERSION="${GITHUB_REF_NAME}"` ⇒ `:v<version>-<arch>`; NEVER `${...#v}`.
  Verified against release.yml ground truth: manifest line 410 uses `version=${GITHUB_REF_NAME}`
  and line 421–423 consumes `:${VERSION}-<arch>` un-stripped — the smoke now matches byte-for-byte.
- Dispatch tag `:latest-<arch>`; manifest `needs: [smoke-amd64, smoke-arm64]` +
  `if: github.event_name != 'workflow_dispatch'`.
- AC-05 WAL-robust grew-signal via `du -s` over the store dir through the read-only `vol()` busybox
  sidecar; marker stays LAST; no silent retry anywhere.
- Single-file editing-surface note for release.yml is in OVERVIEW (serialize on one Stage-3b agent).

## Open questions / gaps flagged (none blocking)
- OQ-B/NFR-07: arm64 cold-boot 90s deadline may be tight (#767); widening is in-scope rework if
  dispatch/tag run shows margin pressure — not changed in this bounded edit.
- OQ-C/R-04: `du -s` store-dir signal must be validated monotone over >=5 local runs; fall back to
  explicit unimatrix.db + -wal + -shm byte sum if du block-rounding masks a single-write delta.
- Gate-logic test extraction: prefer single-source-of-truth for the gate block (avoid a drifting
  copy); if it stays inline in YAML, assert byte-equality / generate the test from it.
- Tag-parity build side must be derived from a DIFFERENT source than the smoke side (ideally read
  release.yml `tags:` patterns) so the assertion is not vacuously true.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (decision/nan-019 + pattern) -- surfaced ADR-001..005
  (#5186/#5187/#5185/#5188; note #5184 referenced in spec as the dispatch/tag-resolution ADR) and
  pattern #5180 (verify-by-name / skip-is-failure / run-marker — this gate's spine). Sufficient;
  full ADR text also read from architecture/ARCHITECTURE.md + brief.
- Deviations from established patterns: none. Pseudocode implements pattern #5180 and ADR-001..005
  as written; tag resolution follows the corrected un-stripped contract (the spec already flagged
  the stored #5184 strip as the defect to correct — that is an ADR-correction task for the owning
  agent, not a pseudocode deviation).
