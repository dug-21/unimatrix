# Component: Gate-logic stub-smoke test (pre-merge, MUST exist — R-01/02/03)

> A pre-merge test artifact (NOT release.yml, NOT the smoke script). Extends the existing
> `infra-001` test infra cumulatively (location TBD by tester; C-12). It exercises the gate's
> exit-code `case` + run-marker grep against a **stub smoke** so the gate's spine is proven
> locally even though hosted execution is not (#4796). This MUST exist before merge (R-01).

## Purpose

Prove that the verify-by-name gate logic from `release-smoke-jobs.md` yields green ⟺
`(RC==0 AND marker present)` and red on every other combination, and that the RC survives the
capture (R-02, the #4873 class). Mitigates SR-01 / NFR-01.

## Design — extract the gate logic so it is testable

The gate body in the smoke job is shell. To unit-test it WITHOUT a Docker daemon or a real
release, drive the **exact** capture-and-branch shape against a stub:

- **Stub smoke:** a tiny script that prints a chosen fixture to stdout/stderr and `exit`s a
  chosen code. The test sets the gate's smoke-entrypoint to this stub (e.g. via the same
  `IMAGE=`-less invocation path, or by pointing the gate at the stub script path).
- **Gate-under-test:** the IDENTICAL `set +e; OUT="$(... 2>&1)"; RC=$?; set -e; case ...; grep`
  block from `release-smoke-jobs.md`. The test MUST run the real block, not a paraphrase — a
  paraphrased copy that drifts from the shipped block tests nothing (R-01).

```
runGate(stubExitCode, stubOutput) -> (jobResult, capturedDiagnostic):
    write stub that prints stubOutput (to stdout AND stderr) then `exit stubExitCode`
    invoke the gate block with that stub as the smoke entrypoint
    return the gate's final exit status + any ::error:: line emitted
```

## Truth table (the coverage requirement — only `(0, marker present)` is green)

| stubExitCode | output contains terminal marker? | Expected jobResult | Expected diagnostic |
|--------------|----------------------------------|--------------------|---------------------|
| 0 | yes (anchored terminal line) | **GREEN** | none |
| 0 | no (partial output, early-exit-0) | **RED** | "exited 0 but never printed ALL GATES PASSED" |
| 1 | no (`fail()` text) | **RED** | "smoke FAILED (exit 1)" |
| 3 | no (`SKIP: Docker not available`) | **RED** | "smoke SKIPPED (exit 3) ... mis-provisioned" |
| 2 (unexpected) | n/a | **RED** | "exited unexpectedly (exit 2)" |
| 139 / 137 / 124 (segfault/OOM/timeout) | n/a | **RED** | "exited unexpectedly" |

Each red row asserts the SPECIFIC `::error::` substring so a post-tag failure is diagnosable
without a re-run (R-01 scenario 2).

## Anchoring adversarial cases (R-03 — the grep must be line-anchored)

These MUST be RED (marker absent / not the terminal whole line):
- Marker as a substring of a longer line: `xx [783-smoke] ALL GATES PASSED yy` embedded mid-line
  (not a whole line) → `grep -qx` must NOT match → if RC==0, RED.
- Marker echoed earlier as a diagnostic/comment, then `exit 0` before the real end → the test's
  fixture places it non-terminally; gate still keys on whole-line match. (Note: `grep -qx`
  matches ANY whole line in the buffer, so the discriminating case is "appears only as a
  substring" vs "appears as its own line"; the early-exit-0 row covers "never printed at all".)
- Confirm the asserted marker string is byte-identical to the smoke's emitted literal
  `[783-smoke] ALL GATES PASSED` (and the `.*` tolerates the smoke's trailing prose).

## RC-survives-capture (R-02 — empirical, NOT by reading; the #4873 class)

Run the stub at `exit 1` and `exit 3` through the EXACT
`set +e; OUT="$(stub 2>&1)"; RC=$?; set -e` shape and assert:
- `RC == 1` when the stub exits 1; `RC == 3` when the stub exits 3.
- Verified by **execution** of the real capture shape — never by structurally reading the YAML.

Adversarial variants the test (or review) MUST reject as re-introducing R-02:
- smoke invoked in a pipe (`smoke | tee`) so `$?` reads `tee`;
- smoke under job `set -eo pipefail` with no local `set +e` guard;
- a YAML `if: ${{ success() }}` / `continue-on-error` that re-greens a non-zero step.

Confirm `2>&1` is captured so a `fail()` on stderr still reaches the marker grep.

## Error Handling / framework notes

- No new test framework — extend the existing `infra-001` shell-test surface (C-12). A bash
  test harness (assert helper + per-case function) is sufficient and matches the artifact.
- The test is fully local + deterministic (no Docker, no network): it is part of the pre-merge
  gate set, not the post-tag round-trip.

## Key Test Scenarios (this artifact IS the scenarios)

- All six truth-table rows above (R-01).
- The three anchoring adversarial cases (R-03).
- The two RC-survival empirical checks at exit 1 and exit 3 (R-02).
- Each red row's specific `::error::` substring present.

## Open Questions

- **Extraction mechanism (flag for tester):** whether to (a) `source`/invoke the shipped gate
  block directly from a shared snippet, or (b) keep the gate block inline in `release.yml` and
  copy it into the test. (a) is preferred (single source of truth — a drift can't pass), but if
  the gate stays inline in YAML, the test must assert byte-equality against the YAML block or be
  generated from it, so the tested logic cannot silently diverge from the shipped logic (R-01).
