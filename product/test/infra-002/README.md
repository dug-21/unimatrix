# infra-002 — cargo-test orphan-cleanup regression guard (#122)

Bug #122: every uni agent ran the bare convention `cargo test --workspace 2>&1 | tail -30`.
With no process group and no timeout, an interrupted Bash tool call signaled only the
pipeline leader; `cargo`/`rustc`/test-binary children reparented to PID 1 and survived,
holding `target/.cargo-lock` and test `.db` handles — hanging or false-failing later runs.

Tier A fix (this guard's subject): the canonical convention in
`.claude/rules/rust-workspace.md` was hardened to run in its own session/process group
with a hard ceiling, writing to a file instead of a live pipe:

```bash
# CARGO_TEST_TIMEOUT_SECS: hard ceiling so an interrupted run cannot orphan cargo children
setsid timeout "${CARGO_TEST_TIMEOUT_SECS:-600}" cargo test --workspace > /tmp/uni-test.$$.log 2>&1; rc=$?; tail -30 /tmp/uni-test.$$.log; rm -f /tmp/uni-test.$$.log; exit $rc
```

## What the guard does

`check-cargo-test-convention.sh` scans `.claude/` and exits non-zero if any agent,
protocol, or rule file invokes `cargo test` as the head of a bare pipe (`cargo test ... |`)
without `setsid`. This prevents the convention from silently reverting.

It is a standalone shell script — deliberately NOT a `cargo test --workspace` target — so
it can never be defeated by the orphan/lock bug it guards against.

## Run

```bash
# scan the live tree (CI / pre-PR gate)
bash product/test/infra-002/check-cargo-test-convention.sh

# prove the guard still works: flags the old bare form, passes the hardened form
bash product/test/infra-002/check-cargo-test-convention.sh --self-test
```

Exit codes: `0` clean, `1` violation(s) found, `2` usage/self-test failure.

## Scope

Tier A only. Tier B (hook-based reaping in `unimatrix-observe`) and Tier C (per-crate
routine testing default) are deferred to follow-up issues — see #122 discussion.
