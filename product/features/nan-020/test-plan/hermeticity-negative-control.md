# Test Plan — Hermeticity Sandbox + REQUIRED Negative Control

> Component: process-boundary `HOME`/credstore + throwaway `--project-dir` isolation for Gates 6–7,
> proven non-vacuous by a negative control. Risk: **R-07 (CRITICAL)**. AC: **AC-09**.
> Priors: #4977 (vacuous-pass / assert-non-skip), vnc-041 **AC-06** (negative control shape),
> vnc-041 **AC-02** (Rust-2024 forbids in-process HOME mutation → isolate at the process boundary).
>
> **This is the single most load-bearing test in nan-020.** PENDING-without-proof here IS a gap
> (ADR-005, AC-09, #5189). A hermeticity assertion that is itself unproven is the vacuous false-green
> it claims to prevent.

## Why this exists

Gate 6 writes a REAL credential to the HOME-keyed credstore (`~/.unimatrix/<hash>/remote.json`,
vnc-039) and a `--project-dir` tree. A PRIOR run's leftover credential can satisfy Gate 7's observe
round-trip **without a fresh attach** — so a broken `init --bundle` would still green. A doc-test
that false-greens off residue reproduces the EXACT #768 blind spot nan-020 exists to close
(self-defeating). Hermeticity is therefore a PROOF OBLIGATION, not best-effort cleanup.

## Test Vehicle

Same stub-driven harness as `docker-http-posture-smoke.md` (extend `release-gate-logic-test.sh`).
The negative control is a special-case row of the Gate 5–7 truth table: **poisoned credstore +
broken attach**. The stubs (`stub-init-bundle.sh`, observe/store stub) and a writable
fake-`HOME`/credstore layout under a temp dir make it pre-merge-provable without Docker.

---

## R-07 sc.1–2 — Isolation + clean-on-entry (structural assertions)

- `test_gate6_runs_under_isolated_home`: assert Gate 6 invokes the child with
  `HOME="$SANDBOX/home" node … init --bundle "$BUNDLE" --project-dir "$SANDBOX/proj"`, where
  `SANDBOX="$(mktemp -d)"` per run. The credstore therefore resolves to
  `$SANDBOX/home/.unimatrix/<hash>/remote.json` — a path that **cannot pre-exist this run**. Static
  assertion on the smoke source; the env is set on the **child**, never via in-process mutation.
- `test_no_inprocess_home_mutation` (vnc-041 AC-02 hazard): assert the harness does NOT attempt to
  set/reset `HOME` within a single live process (no Rust `set_var`, no exported-then-reset HOME in
  the parent shell that would race). Isolation is at the process/shell boundary by construction.
  Grep-assert the smoke threads HOME into the child invocation only.
- `test_sandbox_clean_on_entry`: assert the sandbox/credstore/`--project-dir` are cleaned **on
  ENTRY** (plus `trap` on exit), so a crashed prior run cannot poison the next. Drive a pre-existing
  `$SANDBOX`-shaped dir and assert it is reset before Gate 6 writes.
- `test_gate7_reuses_same_isolated_env`: assert the Gate 7 hook fire reuses the SAME isolated
  `HOME`/`--project-dir`, so the hook client reads THIS run's credstore, never the runner's real
  `~/.unimatrix`.

---

## R-07 sc.3 — REQUIRED NEGATIVE CONTROL (load-bearing; the green is vacuous without it)

`test_hermeticity_negative_control_still_red` (vnc-041 AC-06 shape):

1. **Poison:** pre-seed a stale, valid-looking credential at the location a **non-isolated** run
   would read it — i.e. the runner's real `~/.unimatrix/<hash>/remote.json` (use a redirected fake
   HOME for the *poison* placement so the test never touches the developer's real home, but place it
   where a harness that FAILED to isolate would pick it up).
2. **Break:** point Gate 6 at a deliberately BROKEN attach (stub-init-bundle simulates a no-op /
   wrong-or-dead endpoint — it does NOT write a fresh credential into `$SANDBOX/home`).
3. **Assert STILL RED:** Gate 7 MUST FAIL (`fail()` exit 1, `bundle-path observe did not land in
   per-slug store` / attach-broken). The store delta from THIS run is 0 because no fresh write landed.

**Discrimination proof (the whole point):** also assert that a harness WITHOUT HOME isolation WOULD
PASS this scenario — i.e. wire the test so that, with isolation disabled, the poisoned cred satisfies
observe and Gate 7 greens. That green is exactly the vacuous false-green the control catches. The
isolated harness must turn that same scenario RED. **A test that passes with a pre-seeded cred + a
broken attach is a vacuous sentinel and FAILS this risk's coverage requirement.**

`test_hermeticity_positive_twin`: the ONLY green is the fresh attach into the sandbox — stub-init
writes a fresh credential into `$SANDBOX/home`, observe 204, store delta>0 attributable to THIS
run's write. Green here + red in the negative control together prove the gate measures the fresh
attach, not residue.

---

## R-07 sc.4 — Non-skip / fresh-write evidence (#4977)

- `test_fresh_write_delta_positive`: assert Gate 7 passes only on a **delta>0** in the per-slug
  store (`du -s` over the store dir, WAL-robust, reusing nan-019 `store_size`) — NOT on an absolute
  count and NOT on mere exit 0. A delta of 0 (silent no-op attach) is a fail.
- `test_delta_not_attributable_to_preexisting`: a delta caused by a pre-existing credential (no
  fresh write this run) must NOT count — this is the negative-control row above, asserted from the
  "evidence the attach ran THIS run" angle (the pinned `Ping` succeeded + the NEW write landed).

---

## Un-retryable assertion discipline (#5189 — REQUIRED before merge)

The store-grew assertion carries **no `|| retry`**. Before merge:

- `test_store_grew_non_flaky` — run the positive twin ≥**5 times** against the stub; assert it is
  green every time (deterministic, no flakiness).
- The **negative control above IS the discrimination proof** — it must actually flip the assertion
  red. A tolerance band that hides flakiness also hides the real defect (#5189).

## Pre-merge vs PENDING

- **PRE-MERGE-PROVABLE (REQUIRED — gap if PENDING):** all of the above, against the stub
  broken-attach + poisoned-cred condition. ADR-005 + AC-09 are explicit: classifying the negative
  control PENDING IS a gap (#5189).
- **PENDING-post-tag:** the LIVE hermetic round-trip (real container `init --bundle` writing a real
  credential into the real `$SANDBOX/home`, real observe 204, real store delta) — confirmed on the
  hosted runner, phrased "configured + verified locally; GH execution confirmed post-tag."

## Self-Check

- [x] Isolation is structural + on the process/shell boundary (no in-process HOME mutation — vnc-041 AC-02).
- [x] Clean-on-entry (not just exit) asserted.
- [x] REQUIRED negative control: poisoned cred + broken attach → Gate 7 STILL RED; vacuous-pass
      explicitly shown to be what a non-isolated harness would do (vnc-041 AC-06).
- [x] Positive twin: fresh-attach-into-sandbox is the ONLY green; delta>0 attributable to this run.
- [x] Un-retryable store-grew proven non-flaky (≥5) AND discriminating (negative control flips it).
- [x] Classified REQUIRED pre-merge; PENDING IS a gap. Live round-trip labeled POST-TAG only.
