# Component: Hermeticity sandbox + REQUIRED negative control

> **SAME-FILE COUPLING:** the SANDBOX lifecycle below edits the SAME file as
> `docker-http-posture-smoke.md` (`docker-http-posture-smoke.sh`) and MUST be implemented by
> the SAME agent as one coherent append. The NEGATIVE CONTROL is a SEPARATE new pre-merge
> test file and may be a distinct task, but its author must read the final script shape.

## Purpose

Two responsibilities:
1. **Sandbox lifecycle** — establish a per-run, HOME-isolated, throwaway `--project-dir` at
   the PROCESS/SHELL boundary so Gates 6–7 measure the FRESH attach, never a prior run's
   `~/.unimatrix/<hash>/` credential residue (ADR-005 / FR-22 / R-07).
2. **Negative control** — a REQUIRED pre-merge test proving the hermeticity sentinel is
   non-vacuous: poison a stale cred + break the attach ⇒ Gate 7 MUST STILL FAIL
   (vnc-041 AC-06 / #5246 shape; AC-09).

## Why process-boundary (NOT in-process) — load-bearing constraint

Rust-2024 forbids `std::env::set_var("HOME", …)` (`unsafe`/forbidden); vnc-041 AC-02 DEFERRED
an in-process round-trip for exactly this. Therefore HOME is set in the SPAWNED CHILD's
environment by the shell (`HOME="$SANDBOX/home" node … init --bundle …`). The harness NEVER
mutates its own HOME. Any attempt at in-process HOME mutation is unsound and silently fails to
isolate — re-opening the false-green (R-07 coverage hazard). Do NOT do it.

## Part 1 — Sandbox lifecycle (in docker-http-posture-smoke.sh)

### 1a. Extend the EXISTING cleanup/trap (do NOT add a second trap)

The script already has:
```
cleanup() { if KEEP != 1: docker rm/volume rm; [ -n "$TMP" ] && rm -rf "$TMP"; }
trap cleanup EXIT
```
EXTEND `cleanup()` to also remove the sandbox (guard against unset for the early-exit case):
```
cleanup() {
    ... existing body unchanged ...
    [ -n "${SANDBOX:-}" ] && rm -rf "$SANDBOX"   # nan-020: hermetic sandbox teardown
}
```
Keep the single `trap cleanup EXIT`. Do not register a second trap (would clobber the first).

### 1b. Create the sandbox just before Gate 6 (clean-on-ENTRY + fresh dirs)

```
# nan-020 hermetic sandbox: per-run, HOME-isolated, throwaway project-dir (ADR-005).
SANDBOX="$(mktemp -d)"        # unique per run; cannot collide with a prior run's HOME
# Clean-on-ENTRY belt-and-braces (R-07 sc.2 / edge case "crashed prior run"): mktemp -d is
# already fresh, but explicitly recreate the subtree so a hypothetical reused path is pristine.
rm -rf "$SANDBOX/home" "$SANDBOX/proj"
mkdir -p "$SANDBOX/home" "$SANDBOX/proj"
log "hermetic sandbox at $SANDBOX (isolated HOME + throwaway --project-dir)"
```

> NOTE for the implementer: `mktemp -d` already yields a non-pre-existing root, so the
> credstore `$SANDBOX/home/.unimatrix/<hash>/remote.json` cannot inherit residue under normal
> runs. The explicit `rm -rf` + `mkdir -p` is the clean-on-entry guarantee ADR-005 requires
> for the crashed-prior-run case and makes the property visible/assertable.

### 1c. Gate 6/7 consume `$SANDBOX/home` (HOME) and `$SANDBOX/proj` (--project-dir)

Threaded into the child env exactly as shown in `docker-http-posture-smoke.md`. The SAME
isolated HOME is reused for the Gate 7 hook fire so the hook client resolves THIS run's
credstore, not the runner's real `~/.unimatrix/` (R-07 sc.1).

## Part 2 — REQUIRED pre-merge negative control (separate stub test)

Lives with the gate-logic stub tests (shares bytes with the YAML wrapper per #5189/#5192 —
the same `release-gate-lib.sh` is sourced; the script is driven against stubs). It is
PRE-MERGE-PROVABLE; classifying it PENDING is a gap (#5189).

### Shape (vnc-041 AC-06 / #5246 — "fires when it should AND not when it shouldn't")

```
TEST hermeticity_negative_control:
    # POISON: pre-seed a stale, valid-LOOKING credential where a NON-isolated run would read
    # it — the runner's REAL ~/.unimatrix/<hash>/remote.json (the residue surface, vnc-039).
    seed_stale_credential_at( "$REAL_HOME/.unimatrix/<hash>/remote.json" )

    # BREAK: point Gate 6 at a deliberately BROKEN attach — stub `init --bundle` to FAIL or
    # produce NO fresh credential in $SANDBOX/home (e.g. rc≠0, or a no-op that writes nothing).
    stub_init_bundle_to_fail_or_noop()

    run extended docker-http-posture-smoke.sh (driven via run_smoke_gate against stubs)

    # ASSERT STILL-RED: with isolation working, the stale cred is unreachable (wrong HOME),
    # so the broken attach yields no working credstore and Gate 7 cannot green.
    ASSERT gate result == FAIL (exit 1, NO terminal marker)
    # A harness WITHOUT HOME isolation would PASS here (stale cred satisfies observe) — that
    # pass is the vacuous false-green this control exists to catch (#4977).

TEST hermeticity_positive_twin:
    # Happy path: real fresh attach into the isolated sandbox is the ONLY green.
    no_poison(); real_attach_into_sandbox()
    ASSERT gate result == PASS (exit 0 + single terminal marker)
    ASSERT per-slug store delta > 0 attributable to THIS run's write (non-skip evidence)

TEST clean_on_entry:
    # Simulate a crashed-prior-run residue at the sandbox path; assert entry guard wipes it
    # so it cannot poison the new run (R-07 sc.2).
```

### Coverage discrimination (why both halves are required)

- Positive-only would pass even with a vacuous (residue-fed) sentinel.
- The negative control flips the only thing that could false-green (residue + broken attach)
  to RED, proving the green measures the FRESH attach, not residue (R-07 REQUIRED obligation).

## Error handling

A hermeticity miss surfaces through the EXISTING Gate 7 `fail()` exit 1 with the
`bundle-path observe did not land in per-slug store` (or `init --bundle failed`) message — no
new exit code (ADR-001). Sandbox creation failure (`mktemp` rc≠0) should `fail()` with a
clear message rather than proceed un-isolated.

## Key test scenarios (hints)

- Negative control (poison + break) ⇒ Gate 7 STILL FAILS — the load-bearing assertion.
- Positive twin ⇒ only the fresh attach greens; store delta > 0 from this run.
- Clean-on-entry ⇒ a planted residue at the sandbox path is wiped before Gate 6.
- Trap teardown ⇒ `$SANDBOX` is removed on exit (and on early `fail()` exit) — assert no leak.
- Process-boundary assertion ⇒ HOME is set on the child invocation, never via in-process
  mutation (grep the script: no `export HOME=` that outlives the child; HOME appears only as
  a per-command prefix on the `node` invocations).
