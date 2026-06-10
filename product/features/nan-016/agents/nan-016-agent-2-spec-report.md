# nan-016-agent-2-spec — Report

## Artifact
- `product/features/nan-016/specification/SPECIFICATION.md`

## Summary
Authored Slice-A specification for the UDS dogfooding re-release capability. Verified all four named code surfaces (`lib/init.js`, `lib/merge-settings.js`, `lib/hook-client/config.js`, `package.json`) plus the C-04/C-9 gate scripts and existing init tests before writing requirements against them.

## Key requirements
- 15 functional requirements: copy-install (never npm link), complete frozen `files`-array tree, idempotent clean-replace, switchover via `mergeSettings`, rollback to Rust hook, effect-harness with daemon-absent case, code-freezing isolation proof, no live-settings mutation, runbook, init-path frozen.
- 8 NFRs with measurable targets (byte-identical re-installs, exit-0 daemon-absent, isolation content-hash invariance, zero live-repo writes, size/zero-deps gates green, container-rebuild durability via fixed dir).

## Acceptance criteria
- AC-01 copy-install idempotency; AC-02 switchover-by-effect (scratch fixture, asserts narrowed PreToolUse matcher + daemon-absent exit 0); AC-03 isolation framed as code-freezing NOT state separation (#4923 shared `{hash}` state); AC-04 runbook; AC-05 byte-identical init regression; AC-06 size + zero-deps gates. All six mapped to SCOPE AC-01..AC-06 with concrete verification methods.
- SR-04/05/06/07/08 directly addressed: effect tests are real (not string-diffs), matcher delta asserted, no live-settings touch, isolation correctly scoped, daemon-absent fail-open required.

## Open questions
- OQ-A: script location (repo-root `scripts/` vs `packages/unimatrix/scripts/`).
- OQ-B: pin one install mechanism (npm pack+extract vs npm install --prefix) with no host-mutating postinstall.
- OQ-C: effect-harness "re-fire hook" mechanics against installed path with scratch/absent UDS.
- OQ-D: AC-03 transiently edits tracked in-repo source — confirm clean restore is acceptable.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- no directly relevant entries (general packaging/binary-rename ADRs only); verified code surfaces directly.
