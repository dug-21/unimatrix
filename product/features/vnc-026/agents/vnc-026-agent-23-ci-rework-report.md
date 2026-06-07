# Agent Report: vnc-026-agent-23-ci-rework

Rework on feature/vnc-026 (PR #696), two human-directed items. Committed `46a3d149` (not pushed, per instructions).

## Files Modified

- `.github/workflows/ci.yml`
- `.gitattributes`

## Item 1 — CI job deletion

- Deleted `parity-drift` and `hook-client-layer2` jobs. Remaining jobs: `enforce-inference-sites`, `hook-client-matrix`, `hook-client-audit` (matrix and audit untouched except one stale comment fix).
- Rationale comment added in the workflow near the hook-client jobs: the committed 83-case corpus is the CI contract; oracle regen/drift and Layer 2 runs are local protocol steps at Gate 3c.
- Updated the stale `hook-client-matrix` comment that claimed Layer 2 "runs in its own job".
- `scripts/check-parity-drift.sh` left in place (local protocol step). Verified no CI reference remains (only the rationale comment mentions it by name).

## Item 2 — CRLF pin

Findings: the prior `packages/unimatrix/test/fixtures/parity/** -text` pattern DID match (check-attr showed `text: unset`) — the path-pattern-mismatch hypothesis was not confirmed locally. Git semantics: `eol` is ignored when `text` is unset, so the pin was changed to two rules:

```
packages/unimatrix/test/fixtures/parity/** text eol=lf
packages/unimatrix/test/fixtures/parity/**/*.bin -text
```

Corpus verified CR-free (all 280 tracked files, text and .bin) before choosing `text eol=lf` for the text fixtures — no renormalization noise (`git status` clean on fixtures after the change). Future goldens needing literal CR bytes must be `.bin`.

check-attr proof:

```
packages/unimatrix/test/fixtures/parity/alias-after-tool/expected-request.json: text: set
packages/unimatrix/test/fixtures/parity/alias-after-tool/expected-request.json: eol: lf
packages/unimatrix/test/fixtures/parity/stdout-briefing-content/expected-stdout.bin: text: unset
```

## Validation

- actionlint: not installed in this environment (skipped per "if available")
- YAML parse (python3 yaml): OK — jobs: enforce-inference-sites, hook-client-matrix, hook-client-audit
- `npm run test:hook-client`: 421 tests — 419 pass / 0 fail / 1 skip / 1 todo, exit 0 (the todo test `stdout-subagent-non-entries-fallback` is reported in the "failing tests" block but is todo-flagged and does not fail the run — under concurrent rework as expected)

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — no results relevant to gitattributes/CI job pruning (hits were gate-process/clippy lessons)
- Stored: entry #4782 "gitattributes pinning for byte-exact parity goldens: eol=lf is ignored when text is unset" (pattern, topic parity-corpus)
