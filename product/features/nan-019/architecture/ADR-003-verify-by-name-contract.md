## ADR-003: Verify-By-Name Gate Contract — Exit-Code Discrimination Plus Positive Run-Marker

### Context

This is the feature's load-bearing risk (SR-01, High) and its reason to exist.
The smoke self-skips with `exit 3` when Docker is absent. A naive wiring
(`run: bash ...smoke.sh`) lets `set -e`/pipefail-driven job success treat any
non-failing exit as green — and worse, a future *early* `exit 0` (a bug that
returns success before reaching the behavioral assertions) would also pass.
Either re-creates the false-green class (#4796/#4970): a job exits 0 while the
behavioral assertions never ran. Hosted `ubuntu-*` runners normally ship Docker
(so `exit 3` is rare in practice), but the guard exists precisely to refuse
passing a mis-provisioned/self-hosted lane green — it must not depend on the
runner always having Docker.

The smoke exposes a clean, distinct contract (verified in the script):

| Exit | Meaning | stdout marker |
|------|---------|---------------|
| `0` | ran to the end AND all behavioral gates passed | terminal line `ALL GATES PASSED` is printed |
| `1` | ran but a behavioral gate failed (`fail()`) | a `FAIL:` line on stderr; **no** `ALL GATES PASSED` |
| `3` | self-skipped, Docker absent (preflight) | `SKIP: Docker not available`; **no** `ALL GATES PASSED` |

Crucially, `ALL GATES PASSED` is the script's **last** statement (line 142),
emitted only after Gate 1 (HTTP-on), Gate 2 (per-slug 204), Gate 3 (per-slug DB
present), and AC-05 (grew-assertion, ADR-006) have all passed. So the marker is a
true positive run-proof: it cannot print on `1` or `3`, nor on any early `exit 0`.

### Decision

The smoke step in each job interprets the smoke as a **verify-by-name** gate
requiring BOTH an exit-code check and a positive run-marker — `set -e` must NOT
short-circuit the capture, and YAML `if:`/`continue-on-error` must NOT swallow
the code. The job step:

1. Disables fail-fast around the smoke so the exit code is captured, not consumed:
   run with the step shell's `set +e` (or capture `$?` immediately), e.g.

   ```bash
   set +e
   OUT="$(IMAGE=ghcr.io/<owner>/unimatrix:<tag>-<arch> \
          bash product/test/infra-001/scripts/docker-http-posture-smoke.sh 2>&1)"
   RC=$?
   set -e
   echo "$OUT"          # surface full smoke output in the job log
   ```

   (`2>&1` so the run-marker — printed by `log()` to stdout — and `fail()`/`SKIP`
   diagnostics on stderr are all captured for the grep and the log.)

2. Branches on the **distinct** exit code — only `0` may continue; `1` and `3`
   each fail the job loudly with a code-specific diagnostic:

   ```bash
   case "$RC" in
     0) : ;;  # fall through to the run-marker assertion
     3) echo "::error::smoke SKIPPED (exit 3): Docker-capable lane is mis-provisioned. This is a HARD failure, not a deferred step (SR-01)."; exit 1 ;;
     1) echo "::error::smoke FAILED (exit 1): a behavioral gate did not pass — shipped image first-run path is broken."; exit 1 ;;
     *) echo "::error::smoke exited unexpectedly (exit $RC)."; exit 1 ;;
   esac
   ```

3. Asserts the **positive run-marker** — green is granted only if the terminal
   line was emitted (defends against a future early-`exit 0`):

   ```bash
   echo "$OUT" | grep -qx '\[783-smoke\] ALL GATES PASSED.*' \
     || { echo "::error::smoke exited 0 but never printed 'ALL GATES PASSED' — early-exit-0, gates did NOT run to completion (SR-01)."; exit 1; }
   echo "smoke ran to completion and all behavioral gates passed."
   ```

   The grep is anchored to the script's own prefix (`[783-smoke] ALL GATES
   PASSED`) so an attacker-/echo-injected substring in unrelated output cannot
   forge the marker; matching the line, not a bare substring.

4. **No silent retry** (OQ-6, SR-04): the smoke is invoked exactly once. No
   `|| retry`, no loop, no `continue-on-error: true`. A flaky deployability gate
   is itself a signal; retrying would launder the signal away.

A job is green **iff** `RC == 0` AND the anchored `ALL GATES PASSED` line was
captured. Any other state fails the job (and thus, via ADR-001, blocks the
manifest).

### Consequences

- Easier: A green smoke job provably means "the smoke ran to its terminal line
  and every behavioral assertion passed" — the verify-by-name promise. Closes the
  #4796/#4970 false-green class for this gate.
- Easier: The two checks are independent defenses — exit-code catches `1`/`3`;
  the run-marker catches a future early-`exit 0` the exit code alone would miss.
- Harder: The wiring is more than a one-line `run:`; it must explicitly manage
  `set -e` so the exit code survives. Getting this wrong (e.g. letting pipefail
  in a pipe swallow `$?`) silently reintroduces the risk — so the capture pattern
  is pinned here and in the Integration Surface, not left to the implementer.
- Harder: The gate is coupled to the literal marker string `ALL GATES PASSED`.
  If a future smoke edit changes that line, this assertion breaks loudly (job
  fails) rather than silently — acceptable, and the marker is named in the
  Integration Surface as a stable contract the smoke must keep emitting.
- Related: ADR-001 (a failed gate blocks the manifest), ADR-006 (AC-05 extends
  the assertions that must pass *before* the marker prints), pattern #5180 (the
  general self-skip→hard-fail rule this ADR instantiates).
