# infra-002 — cargo-test orphan-cleanup regression guard (#122)

Bug #122: every uni agent ran the bare convention `cargo test --workspace 2>&1 | tail -30`.
With no process group and no timeout, an interrupted Bash tool call signaled only the
pipeline leader; `cargo`/`rustc`/test-binary children reparented to PID 1 and survived,
holding `target/.cargo-lock` and test `.db` handles — hanging or false-failing later runs.

Tier A fix (this guard's subject): the canonical convention in
`.claude/rules/rust-workspace.md` was hardened to run in its own session/process group
with a hard ceiling, writing to a file instead of a live pipe:

```bash
# CARGO_TEST_TIMEOUT_SECS: hard ceiling so an interrupted run cannot orphan cargo children.
# `setsid -w` is REQUIRED: without -w, setsid returns its own fork status (0) instead of
# the inner command's exit code, producing false-green gates (GH#709). rc=124 = killed at ceiling.
log="$(mktemp -t uni-test.XXXXXX.log)"; setsid -w timeout "${CARGO_TEST_TIMEOUT_SECS:-600}" cargo test --workspace > "$log" 2>&1; rc=$?; tail -30 "$log"; rm -f "$log"; exit $rc
```

Follow-up defect (GH#709): the first hardened form shipped `setsid` **without** `-w`.
Bare `setsid` forks and returns the *fork's* exit status (always 0), so `rc=$?`
captured 0 even when `cargo test` failed or was killed at the ceiling — a silent
false-green that inverted the very gate-integrity guarantee #122 exists to protect.
The corrected form uses `setsid -w`, which waits for the child and propagates its
real exit code.

## What the guard does

`check-cargo-test-convention.sh` scans `.claude/` and exits non-zero if any agent,
protocol, or rule file:

- invokes `cargo test` as the head of a bare pipe (`cargo test ... |`) without
  `setsid` (CLASS 1, #122 — no process group), OR
- invokes `cargo test` via `setsid` **without** the `-w` flag (CLASS 2, GH#709 —
  false-green exit code).

This prevents the convention from silently reverting to either defective form.

It is a standalone shell script — deliberately NOT a `cargo test --workspace` target — so
it can never be defeated by the orphan/lock bug it guards against.

## Run

```bash
# scan the live tree (CI / pre-PR gate)
bash product/test/infra-002/check-cargo-test-convention.sh

# prove the guard still works: flags the bare-pipe form AND the setsid-without-w
# form, passes the hardened `setsid -w` form
bash product/test/infra-002/check-cargo-test-convention.sh --self-test
```

Exit codes: `0` clean, `1` violation(s) found, `2` usage/self-test failure.

## Scope

Tier A only. Tier B (hook-based reaping in `unimatrix-observe`) and Tier C (per-crate
routine testing default) are deferred to follow-up issues — see #122 discussion.

---

# infra-002 full-workspace LINK smoke — regression guard for bug #878 (link-step OOM)

Bug #878: `cargo test --workspace` OOM-killed `ld` at the **link** step — cumulative
N-parallel-link RSS summed past the memory ceiling (swap exhausted). It was salvaged three
times without exercising the real thing: `-j1` (#750), then `--lib` (#873/#877, which
**skips integration-test links entirely**), then the #878 fix. Nothing exercised the
full-workspace link, so a re-regression stayed invisible until a human ran the full suite.

The #878 fix is config-only: `[profile.dev]` debug-info reduction in root `Cargo.toml`
(`debug = "line-tables-only"` + `split-debuginfo = "unpacked"`) plus an empirically-derived
`[build] jobs` cap in `.cargo/config.toml`.

`check-workspace-link-smoke.sh` runs `cargo test --workspace --no-run` (link only, no test
execution) at the repo's **configured parallelism** and FAILS if the link does not complete,
distinguishing an OOM (the #878 mode) from an ordinary compile/link error.

**Why the configured `jobs` cap and not default `-j nproc`:** MEASURED post-fix, peak `ld`
RSS is ~1112 MB per heavy server link and default `-j nproc(10)` STILL OOMs (≈11 GB demand
vs ~9.4 GB avail, swap=0). Safety comes from the jobs cap, not from making 10-way links fit,
so a default-`-j` smoke is guaranteed-RED and un-gateable. It is also **not** `-j1` (a single
link always fits and would neuter the guard). At the cap it sums the real operational load
(e.g. 6 × 1112 ≈ 6.7 GB), and still TRIPS when (a) cumulative growth pushes cap × per-link
past the ceiling, or (b) the `[profile.dev]` levers are removed (per-link reverts to
~1842 MB → cap × 1842 ≈ 11 GB → OOM). So it also guards the fix's *presence* without a
separate profile-presence assertion, and gates unsafe `jobs`-cap raises.

## Run

```bash
# link-only smoke at configured parallelism (pre-PR / Rust protocol test gate)
bash product/test/infra-002/check-workspace-link-smoke.sh

# prove the OOM-detection logic without provoking a real OOM
bash product/test/infra-002/check-workspace-link-smoke.sh --self-test
```

Exit codes: `0` link holds · `1` link failed (OOM or other) · `2` usage/self-test failure ·
`3` self-skipped (no cargo). Tunable: `LINK_SMOKE_TIMEOUT_SECS` (default 1200).
