# C-TS — `run_smoke_gate_tristate`

> File: `product/test/infra-001/scripts/release-gate-lib.sh`
> ADR-002 (#5350). **Additive** — `run_smoke_gate` (`:44`) stays byte-unchanged.
> Single source of truth for the exit-2/INFRA discrimination (D-1, NFR-3).

## Purpose

Discriminate the isolation gate's exit code 0/1/2/3/other so the release lane **blocks on RED**
without blocking on INFRA. The existing `run_smoke_gate` has no exit-2 case (INFRA would collapse
into `*) → return 1` and block every release on warmup/dependency noise). C-TS adds the missing
exit-2 branch as a new function — only the isolation lane calls it; the four existing blocking
lanes keep byte-identical behavior and zero exit-2 exposure (SR-08, R-07).

## New Function (add beside `run_smoke_gate`; do NOT modify `run_smoke_gate`)

```
FUNCTION run_smoke_gate_tristate(IMAGE, SMOKE_CMD...):
    image = $1 ; shift

    # --- Capture shape: EXACTLY the proven spine (R-05 PREREQUISITE, #5192/#4873) ---
    #   NO pipe between the smoke and $?  ·  return, never exit  ·  re-enable set -e after
    set +e
    out = "$(IMAGE="${image}" "$@" 2>&1)"      # no pipe to a downstream cmd here
    rc  = $?
    set -e
    echo "${out}"                              # diagnostic full-log echo on EVERY path (ADR-004 capture-first)

    CASE rc IN
        0)  : ;;                               # fall through to the anchored marker check below

        1)  echo "::error::isolation smoke FAILED (exit 1): genuine cross-tenant leak (RED) — blocking the release manifest."
            return 1 ;;                         # RED -> blocks (the DoD)

        2)  # INFRA: readiness/durability/dependency/pull not established this run.
            #   Non-blocking BUT VISIBLE — ::warning:: + the canonical greppable marker.
            echo "::warning::isolation smoke INFRA (exit 2): isolation could not be verified this run (warmup/durability/dependency/pull). Non-blocking but flagged."
            echo "[infra004-gate] INFRA — ISOLATION NOT VERIFIED THIS RUN"   # PINNED canonical literal (R-09, WARN #3337)
            return 0 ;;                         # INFRA -> does NOT block, visible

        3)  echo "::error::isolation smoke SKIPPED (exit 3): Docker-capable lane mis-provisioned — HARD failure."
            return 1 ;;                         # SKIP on a Docker-present lane -> blocks

        *)  echo "::error::isolation smoke exited unexpectedly (exit ${rc})."
            return 1 ;;                         # unknown -> blocks (never round to pass)
    ESAC

    # --- GREEN credited ONLY on the anchored RUNTIME run-marker (rc==0 AND marker) ---
    echo "${out}" | grep -qxE '\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*' \
        || { echo "::error::isolation smoke exited 0 but never printed ALL GATES PASSED — early-exit-0 (false-green)."; return 1; }
    return 0
```

### Pinned literals (must appear verbatim in code)

- Canonical INFRA marker: `[infra004-gate] INFRA — ISOLATION NOT VERIFIED THIS RUN` (em dash, exact).
- GREEN anchor: `grep -qxE '\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*'` — full-line `-x` anchor,
  matched against the **runtime** `log()`-prefixed line `[infra003-smoke] ALL GATES PASSED`, never
  a source literal (R-06). A forged marker embedded inside a longer line is NOT credited (the
  injection mitigation for the diagnostic echo, ARCH §6).

## State Machine — exit code → return code (truth table)

| `rc` | Marker present? | Annotation emitted | Returns | Manifest effect (once in `needs:`) |
|------|-----------------|--------------------|---------|------------------------------------|
| 0 | yes | — | 0 | passes |
| 0 | no | `::error::` early-exit-0 | 1 | **blocks** |
| 1 | (n/a) | `::error::` RED | 1 | **blocks** |
| 2 | (n/a) | `::warning::` + INFRA marker | 0 | non-blocking, visible |
| 3 | (n/a) | `::error::` mis-provisioned | 1 | **blocks** |
| other | (n/a) | `::error::` unexpected | 1 | **blocks** |

Only `rc==2` maps to a non-blocking `return 0`. No other path rounds a non-GREEN to a pass (R-05/R-08).

## Initialization Sequence

None — pure function in a sourced library. The lane sources `release-gate-lib.sh` and the stub
test sources the SAME bytes, so tested logic cannot diverge from shipped logic (NFR-3).
`run_smoke_gate_tristate` must remain invocable when **sourced**: it uses `return`, never `exit`,
and a caller stub-tests it by `export IMAGE; run_smoke_gate_tristate "$IMAGE" <stub>; unset IMAGE`
(a sourced function cannot be invoked via `env VAR=x fn`).

## Data Flow

- **Inputs:** `image` ($1), smoke argv ($@), and whatever the smoke prints to stdout+stderr.
- **Transformation:** invoke the smoke once with `IMAGE` exported, capture rc + combined output
  with no intervening pipe, echo the full log, branch on rc, then (rc==0 only) anchor-grep the
  runtime marker.
- **Outputs:** the full smoke log echoed to the job log; `::error::`/`::warning::` annotations; the
  canonical INFRA marker on rc==2; a return code (0 = pass/INFRA-visible, 1 = block).

## Error Handling

- A swallowed exit code is the cardinal defect (R-05): the capture must keep `rc` faithful — no
  pipe between the smoke and `$?`, `set -e` re-enabled only after the capture, `return` not `exit`.
- `run_smoke_gate` is NOT touched: its 0/3/4/1/* truth table must be byte-identical post-change
  (R-07); no existing lane emits exit 2 today, so the new branch adds no sibling exposure.
- The image pull-404 case is **inside the script** (it `infra_fail`s → exit 2), so C-TS maps it to
  non-blocking-visible INFRA — a deliberate divergence from `run_smoke_gate`'s exit-4-blocks
  (ADR-002/ADR-003 B); C-TS itself has no exit-4 case.

## Key Test Scenarios (hints — full plan in test-plan/)

1. **Full truth table via the REAL sourced lib (R-05 sc.3):** stub smoke exits a chosen code +
   prints a chosen marker → (0+marker)→0, (0,no-marker)→1, (1)→1, (2)→0, (3)→1, (other)→1.
2. **Capture-shape invariants (R-05 sc.1/2):** stub exits 1 → assert `rc==1` survives the capture
   and the function returns 1; assert no pipe between the smoke and `$?`; assert `return`, not `exit`.
3. **INFRA visibility (AC-08/AC-13, NFR-2):** rc==2 → assert `return 0` AND the log contains the
   `::warning::` line AND the exact canonical marker `[infra004-gate] INFRA — ISOLATION NOT
   VERIFIED THIS RUN`.
4. **Anchored marker (R-06):** rc==0 with the runtime-prefixed line credited; rc==0 with `ALL
   GATES PASSED` as a substring inside a longer line NOT credited (full-line `-x` holds).
5. **Sibling no-regression (R-07):** `git diff` shows `run_smoke_gate` byte-unchanged; re-run its
   0/3/4/1/* truth table → identical results.
6. **set-e-safe harness (R-14):** after sourcing the real lib (`set -euo pipefail`), the test does
   `set +e; set -uo pipefail` before driving the intentionally-RED row; the suite's final summary
   line prints (all rows ran).
