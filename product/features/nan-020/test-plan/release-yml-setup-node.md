# Test Plan — `release.yml` pinned `setup-node@v4` on both smoke jobs

> Component: add a pinned `actions/setup-node@v4` (`node-version: '24'`) step to BOTH smoke jobs
> (smoke-amd64 / smoke-arm64), after `actions/checkout@v4` and before `run_smoke_gate`.
> Risk: R-04 (the FIRST line of defense; the script node-preflight is the backstop). AC: AC-03/05.
> Priors: #793 (pin-your-infra), ADR-002 amendment.

## Why

The nan-019 smoke jobs (`release.yml:406–446`) carry NO `setup-node` step and rely on incidental
runner-image `node`. Because nan-020 makes node-absence a HARD-fail (ADR-001), that incidental
reliance lets an unrelated runner-image change silently arm a release-blocker on a surface nan-020
never declared. Pinning node makes the dependency EXPLICIT; the script preflight becomes a safety
net for a provisioning regression, not the acquisition path (defense in depth).

## Test Vehicle

Static grep/YAML assertions against `.github/workflows/release.yml` (the verify-by-name discipline:
assert the step exists by name, #5180). No workflow execution needed — this is pre-merge-provable.
Add a small static-assertion block (in `release-gate-logic-test.sh` or a sibling parity test, mirroring
the existing `release-tag-parity-test.sh` static-grep convention — cumulative, no new framework).

## Assertions

- `test_setup_node_present_both_smoke_jobs`: assert a `uses: actions/setup-node@v4` step exists in
  BOTH the `smoke-amd64` and `smoke-arm64` jobs. Zero ⇒ RED at merge.
- `test_setup_node_version_pinned_24`: assert `node-version: '24'` (matching the `package-npm` job at
  `release.yml:215–218`). A drift/unpinned/missing version ⇒ RED.
- `test_setup_node_ordering`: assert the step is AFTER `actions/checkout@v4` and BEFORE the
  `run_smoke_gate` step in each smoke job (node must be provisioned before the gate runs).
- `test_node_absent_safety_net` (cross-ref `docker-http-posture-smoke.md` R-04 sc.1): assert the
  script's `command -v node` preflight still hard-fails (`fail()` exit 1, NOT exit 3) if provisioning
  slips — proving the pin and the preflight are complementary, not redundant. Per #5180 a missing
  prerequisite hard-fails, never greens.

## Pre-merge vs PENDING

- **PRE-MERGE-PROVABLE:** all four assertions (static YAML grep + the script preflight behavior).
  A pinned-step-missing or wrong-version condition is RED at merge — no tag push required.
- **PENDING-post-tag:** that the hosted runner actually provisions node 24 and the smoke job runs
  green end-to-end — confirmed on the tag run; never asserted pre-merge.

## Self-Check

- [x] Pinned `setup-node@v4` asserted present on BOTH smoke jobs by name (#5180 verify-by-name).
- [x] `node-version: '24'` parity with package-npm asserted.
- [x] Ordering (after checkout, before gate) asserted.
- [x] node-absent script backstop is a distinct hard-fail (defense in depth, not redundancy).
- [x] Static/pre-merge-provable; live provisioning labeled POST-TAG.
