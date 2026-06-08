# Agent Report — vnc-027-agent-3-size-gate

Component: size-gate (C-04). ADR-005. FR-1..FR-4, AC-09 (Critical, merges FIRST), R-02/R-03.

## Files modified
- `packages/unimatrix/test/check-hook-client-size.js` (REWRITE) — dual-limit gate + dependency-free state-machine comment stripper + embedded self-test.
- `packages/unimatrix/test/hook-client/size-gate.test.js` (NEW) — unit/behavior test suite per the component test plan.

## Commit
`ba338f08 impl(size-gate): C-04 dual-limit gate + state-machine stripper (#680)` on `feature/vnc-027`.
This is the literal FIRST vnc-027 commit touching the client tree (SR-02 / R-02). No `lib/hook-client/` byte changed.

## Implementation summary
- Two independent decimal limits over `lib/hook-client/**/*.js`: comment-stripped <= 100,000 B (PRIMARY) + raw <= 160,000 B (BACKSTOP). Either breach -> exit 1; both totals + per-file table always printed.
- `stripComments`: single-pass character state machine (CODE/SQ/DQ/TPL/LINE/BLOCK/REGEX/REGEX_CLASS). Removal-only — output is always a byte-subsequence of input. Template `${}` nesting tracked via a per-interpolation brace-depth stack; regex-vs-division via `regexCanOpen` (prev-significant-char punctuation set + keyword heuristic).
- `runSelfTest` runs the embedded `SELF_TEST_CORPUS` on EVERY invocation BEFORE measuring; failure fails the gate CLOSED (exit 1, no size table). Corpus covers `"// not a comment"`, template `${}`+backticks, regex with `//`/`/*` + char class, division-vs-regex, escapes.
- Stripper lives in `test/`; does NOT count against the client budget.
- Header documents the C-04 decision, both limits, and the cap-change-is-a-human-decision rule.
- `runGate`/`measureTree` expose DI seams (log/err/runSelfTest/measureTree/stripFn) so synthetic over-limit fixtures and the fail-closed path are unit-tested without touching the real ROOT.

## Tests
- New suite `test/hook-client/size-gate.test.js`: 21 tests, 21 pass (embedded self-test, string-literal safety, regex-vs-division, escapes, removal-only subsequence on real tree, both limits independently triggerable on disjoint synthetic fixtures, fail-closed ordering, backstop-under-forced-strip-bug, boundary-at-exactly-100000, empty tree, missing dir, header contract).
- Full hook-client suite (`node test/run-hook-client.js`): 451 tests, 450 pass, 0 fail, 1 pre-existing skip. No new failures.
- `node test/check-zero-deps.js`: PASS (no runtime deps; 13 modules built-ins/relative only).

## Confirm — current client passes the gate
`node test/check-hook-client-size.js` -> exit 0:
- comment-stripped total: 63,370 / 100,000
- raw total: 99,997 / 160,000  (matches the brief's 99,997)
Both well within budget; ~36 KB stripped headroom + ~60 KB raw headroom now available for F4a client growth.

## Issues / blockers
None. Did NOT run integration tests (Stage 3c owns those), per instruction.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search -- surfaced ADR-005 (#4806), lesson #4780 (vnc-026 100KB raw-limit rework driver), pattern #4786 (old 3-byte-headroom raw-limit pattern), and #4769 (ASCII-safe test strings).
- Stored: pattern #4820 (corrected/superseded #4786) "hook-client size gate is now dual-limit (stripped <=100KB primary + raw <=160KB backstop); measure against the stripped budget" via /uni-store-pattern (context_correct). Supersedes the obsolete single-raw-limit pattern.
