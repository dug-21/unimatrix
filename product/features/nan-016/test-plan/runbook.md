# Test Plan — `product/features/nan-016/RUNBOOK.md` + cross-cutting regression gates

> The runbook is a documentation artifact; its "tests" are content checks (file exists,
> contains the five FR-14 items) plus a cross-check that its rollback steps actually reproduce
> the Rust command form (proven by the dogfood-effect rollback test). This file also owns the
> **cross-cutting regression gates** (R-14 / AC-05 / AC-06) that protect the frozen surfaces.
> Covers AC-04 (documentation half) and AC-05 / AC-06.

## Part 1 — Runbook content verification (AC-04, R-06 cross-check)

The runbook documents five FR-14 items; verified by file-existence + content grep (no `.sh`
source diff).

- `runbook_exists` — `product/features/nan-016/RUNBOOK.md` exists.
- `runbook_documents_promotion-is-rerun-install-and-f6-reset`
  - Assert: documents promotion = re-run `dogfood-install.sh` = the F6 soak-reset point (a).
- `runbook_documents_rollback-reverts-to-rust-hook`
  - Assert: documents rollback = revert hooks to the Rust `hook.rs` / `target/release/unimatrix`
    command form via `dogfood-switchover.sh rollback` (b).
- `runbook_documents_flip-deferred-to-no-active-feature-window`
  - Assert: states the live flip is DEFERRED to a no-active-feature window and that nan-016 does
    NOT execute it and does NOT start the F6 soak clock (c).
- `runbook_documents_pretooluse-matcher-narrowing-as-intended-delta`  ← R-09 cross-check
  - Assert: documents the PreToolUse matcher-narrowing (`"*"` →
    `context_cycle|mcp__unimatrix__context_cycle`) as an INTENDED behavioral delta the operator
    will see at flip time (d). This is the operator-facing half of R-09 (the harness asserts the
    delta against the imported constant; the runbook must warn about it).
- `runbook_documents_daemon-assumed-running-fail-open`
  - Assert: documents the daemon is assumed running, nan-016 does no daemon lifecycle, and hooks
    fail-open (exit 0) if the daemon is absent (e).
- `runbook_rollback-steps_cross-checked-by-effect`
  - Cross-check: the rollback steps the runbook describes match what the dogfood-effect rollback
    test proves by effect (exact Rust form via the shipped legacy arm — R-06). The runbook must
    not describe a bespoke revert that diverges from the tested mechanism.

**Coverage Requirement (AC-04):** all five FR-14 items present; rollback documentation matches
the effect-tested mechanism (no divergence).

## Part 2 — Cross-cutting regression gates (R-14 / AC-05 / AC-06)

nan-016 adds NO `lib/` bytes (C-8). These gates prove the frozen surfaces are untouched. Run in
Stage 3c (see OVERVIEW Integration Harness Plan sections B–D).

### AC-05 — Init local path byte-identical (R-14)

- `regression_existing-init-suites-green`
  - Run `node --test` over `test/init.test.js`, `test/init-integration.test.js`,
    `test/merge-settings.test.js`, `test/init-remote.test.js`. Assert all green.
- `regression_no-diff-to-frozen-runtime-surfaces`
  - Assert via `git diff`: zero behavioral change to `lib/init.js`, `lib/merge-settings.js`,
    `lib/hook-client/config.js`, `package.json` runtime behavior.

### AC-06 — Size + zero-deps gates (R-14 / NFR-5 / NFR-6 / C-9)

- `gate_hook-client-size-exit-0`
  - Run `node packages/unimatrix/test/check-hook-client-size.js`. Assert exit 0
    (stripped ≤ 100 KB / raw ≤ 160 KB over `lib/hook-client/**/*.js`).
- `gate_zero-deps-exit-0`
  - Run `node packages/unimatrix/test/check-zero-deps.js`. Assert exit 0 (shipped JS remains
    dependency-free).

**Coverage Requirement (R-14):** existing init/merge suites green; size + zero-deps gates exit 0;
no diff to frozen runtime surfaces. Since nan-016's only `packages/unimatrix/` addition is the
test file, the size and zero-deps gates are pure regression guards — a failure indicates an
inadvertent frozen-surface edit and MUST be fixed in this feature (not deferred).

## Notes

- The runbook content checks are deterministic string/section presence checks — keep them robust
  to wording (assert on stable phrases / section headings, not exact sentences) to avoid brittle
  doc-coupling.
- These gates have NO pytest/Rust surface; do not invoke `cargo test` or infra-001 for nan-016.
