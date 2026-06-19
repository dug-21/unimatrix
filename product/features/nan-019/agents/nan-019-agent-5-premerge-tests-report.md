# Agent Report — nan-019-agent-5-premerge-tests

## Scope
Create the two MUST-EXIST pre-merge HARD-gate shell tests (bash, not Rust) beside the
shipped smoke in `product/test/infra-001/scripts/`. Consume Wave 1's committed output
(`release-gate-lib.sh`, `release.yml`, smoke script) — never modify them.

## Files Created
- `/workspaces/unimatrix/product/test/infra-001/scripts/fixtures/stub-smoke.sh`
- `/workspaces/unimatrix/product/test/infra-001/scripts/release-gate-logic-test.sh`
- `/workspaces/unimatrix/product/test/infra-001/scripts/release-tag-parity-test.sh`

## Test 1 — Gate-logic truth table (T1 / R-01,R-02,R-03)
`source`s the SHIPPED `release-gate-lib.sh` and drives the real `run_smoke_gate` against
the controllable `fixtures/stub-smoke.sh` (no Docker, no network). 13/13 pass:
- Truth table {0, 1, 3, early-exit-0, 2, 139} × marker present/absent — only (0, marker
  present) is GREEN; every other cell RED with the SPECIFIC `::error::` substring asserted.
- R-03 anchoring: a mid-line substring of the marker does NOT satisfy `grep -qx` → RED
  (anti-spoof). Whole-line-anywhere marker is GREEN (the lib's documented behavior, pinned).
  Byte-identity cross-check reconstructs the smoke's RUNTIME `log()`-prefixed line and
  confirms the shipped grep pattern matches it.
- R-02 RC-survives-capture proven by EXECUTION of the exact `set +e; out="$(...)"; rc=$?`
  shape: exit 1 reads 1, exit 3 reads 3 (the #4873 setsid/pipe-swallow class). stderr is
  captured via `2>&1` (a `fail()` on stderr still reaches the grep and stays RED on exit 1).

## Test 2 — Tag-parity static assertion (T2 / FR-11 / R-09)
13/13 pass. Non-vacuous by construction — the two sides come from DIFFERENT sources:
- SMOKE side: `source`-ed shipped `resolve_image`.
- BUILD side: READs `release.yml`'s `docker/metadata-action` `tags:` patterns
  (`type=semver,pattern=v{{version}}-<arch>` + `type=raw,value=latest-<arch>`) and models
  the action semantics; the test first asserts those exact patterns are present in the YAML
  so a future YAML edit we don't mirror can't pass silently.
- Cases: push v1.2.3/v0.8.2 amd64+arm64 → `:vX.Y.Z-<arch>` (un-stripped v kept); dispatch →
  `:latest-<arch>`; per-arch suffix correct (no swap).
- Discrimination self-checks (prove non-vacuous): re-introduced `${...#v}` strip
  (`1.2.3-amd64` ≠ `v1.2.3-amd64`), swapped suffix, extra `v` — each confirmed to diverge.
- Mutation proof (out-of-band, against a `sed`-mutated COPY of the lib — real lib untouched):
  injecting `tag="${ref_name#v}"` yields `1.2.3-amd64` ≠ build `v1.2.3-amd64` → assertion
  would go RED. Also proved the gate-logic marker grep is load-bearing: neutering it in a lib
  copy flips the early-exit-0 cell to a false green, confirming the shipped grep is what makes
  that row RED.

## Results
- `release-gate-logic-test.sh`: 13 passed, 0 failed (exit 0).
- `release-tag-parity-test.sh`: 13 passed, 0 failed (exit 0).
- Both `source` the shipped `release-gate-lib.sh`; the parity test additionally READs
  `release.yml` — neither assertion is vacuous.

## Issues / Blockers
None. Did not run/modify the integration suites (Stage 3c). Did not modify
`release-gate-lib.sh`, `release.yml`, or the smoke script. Did not run any git commands
(Delivery Leader owns git).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern/decision, nan-019) — surfaced the
  gate-spine-as-lib pattern (#5192), ADR-001..004 (#5186/#5187/#5183/#5188), verify-by-name
  pattern #5180, and pre-merge-provable test-plan pattern #5189. Applied all.
- Stored: entry #5194 "Testing a sourceable shell gate lib: run the harness under set -uo
  pipefail only, never set -e" via /uni-store-pattern (covers the set -e harness-abort trap,
  RC-by-execution, non-vacuous two-source parity + sed-copy mutation proof, and the runtime
  vs source-line marker byte-identity gotcha).
