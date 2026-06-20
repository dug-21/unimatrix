# nan-020 Agent 3 (mechanism) — Report

Scope: the ENTIRE doc-test mechanism — Gates 5–7 extension of `docker-http-posture-smoke.sh`,
the hermeticity sandbox + REQUIRED negative control, the pinned `setup-node@v4` on both smoke
jobs, and the stub-driven gate-logic tests. No Rust/JS app code.

## Files modified / created

Modified:
- `product/test/infra-001/scripts/docker-http-posture-smoke.sh` — appended Gates 5–7 as a
  single sourceable `bundle_attach_gates()` function (env-injectable seam + hermetic sandbox);
  extended `cleanup()` to teardown `$SANDBOX`; added a sourced-guard before the Docker
  preflight. Gates 1–4, the `IMAGE=` arm, exit-3 preflight, `store_size()`, and the single
  terminal marker are UNCHANGED and un-reordered. (358 lines)
- `.github/workflows/release.yml` — added a pinned `actions/setup-node@v4` (`node-version: '24'`)
  step to BOTH `smoke-amd64` and `smoke-arm64`, after `checkout@v4`, before `run_smoke_gate`.

Created (test infra — permitted additions, NOT new smoke scripts):
- `product/test/infra-001/scripts/release-gate-bundle-logic-test.sh` — dynamic stub-driven
  truth table + R-02/R-04/R-05 + the REQUIRED hermeticity negative control + positive twin +
  non-flaky(>=5) + run_smoke_gate marker-suppression integration. (353 lines)
- `product/test/infra-001/scripts/release-gate-bundle-static-test.sh` — static source/YAML-grep
  assertions: repo-checkout client, process-boundary HOME isolation, clean-on-entry + trap
  teardown, append-only ordering, single terminal marker, `release-gate-lib.sh` byte-unchanged
  (sha256 vs HEAD), setup-node present/pinned/ordered on both jobs, no-new-smoke-script. (202)
- `fixtures/stub-client-bundle.sh`, `fixtures/stub-init-bundle.sh`, `fixtures/stub-hook-fire.sh`,
  `fixtures/stub-store-size.sh` — controllable stand-ins for the three new external commands +
  the Gate-7 store sampler.

(Split the gate-logic test into dynamic + static files to keep each under the 500-line limit.)

UNCHANGED (asserted): `product/test/infra-001/scripts/release-gate-lib.sh` — byte-identical to
HEAD (sha256 match, verified by `git diff --quiet`).

## Tests (stub gate-logic, local, no Docker/node/network)

- `release-gate-bundle-logic-test.sh`: 19 passed, 0 failed
- `release-gate-bundle-static-test.sh`: 12 passed, 0 failed
- `release-gate-logic-test.sh` (nan-019, regression): 14 passed, 0 failed
- Total new nan-020 coverage: 31 assertions, all green.

Key proofs landed pre-merge (per ADR-005 / R-07 — PENDING would be a gap):
- Hermeticity NEGATIVE CONTROL: poison stale cred at a fake real-HOME + broken attach => Gate 7
  STILL RED (`bundle-path observe did not land in per-slug store`).
- Discrimination twin: a non-isolated harness (HOME at the poisoned home) WOULD green —
  proving the control flips the only thing that could false-green.
- Positive twin is the ONLY green; store-grew proven non-flaky over 5 runs.
- Every ADR-001 new-failure mode => exit 1 with its distinct message; marker suppressed on
  failure; `run_smoke_gate` byte-unchanged.

NOT run (correctly deferred to Stage 3c / post-tag): the live container round-trip.

## Issues / blockers

None.

- `release-gate-lib.sh` is BYTE-UNCHANGED (confirmed).
- Hook-client entry path used: `packages/unimatrix/lib/hook-client/index.js` (per-spawn entry
  `node .../index.js <EVENT>`, event on stdin; confirmed against the checkout). It is fail-open
  (always exit 0, zero stdout on its own paths), so per the pseudocode the per-slug store
  `du`-delta is the LOAD-BEARING 204 assertion and `observe_code` is best-effort (empty on the
  real path; only the stub surfaces an HTTP code). Bundle emit uses the verified
  `--project-dir /data client-bundle "$SLUG"`; consume uses
  `node packages/unimatrix/bin/unimatrix.js init --bundle "$BUNDLE" --project-dir "$SANDBOX/proj"`
  (no `--slug`).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern + decision) -- surfaced #5192 (sourceable
  gate spine), #5189 (pre-merge-provable shell gate), #5180 (skip-is-failure), ADR-005 (#5257).
  Applied all: sourced-lib single-source-of-truth, RC-survives-by-execution, anchored marker,
  process-boundary HOME isolation, REQUIRED-not-PENDING negative control.
- Stored: entry #5258 "Stub-drive appended Docker-smoke gates + prove HOME hermeticity via
  negative control (no Docker/node)" via /uni-store-pattern; linked Supports -> #5192.
