# Component: release.yml — pinned setup-node@v4 on both smoke jobs

## Purpose

Provision `node` explicitly + pinned on the two smoke jobs that run the doc-test, so the host
JS leg's hard-fail (Gate 6 `init --bundle`) is an INTENTIONAL safety net for a provisioning
regression, not a latent dependency on incidental runner-image node (NFR-10 / ADR-002 amended;
the #793 "pin your infra" discipline).

## Integration surface (verified, exact)

- Smoke jobs: `smoke-amd64` (`release.yml:406`, `runs-on: ubuntu-22.04`) and `smoke-arm64`
  (`release.yml:427`, `runs-on: ubuntu-22.04-arm`). Each has: `checkout@v4` → `Log in to
  GHCR` (docker/login-action@v3) → `run` step that sources `release-gate-lib.sh` and calls
  `run_smoke_gate`.
- Model to copy: the `package-npm` job already uses `actions/setup-node@v4` with
  `node-version: '24'` (`release.yml:215–218`). Match the version `24` — the node major that
  publishes the package and is therefore the operator's install surface.

## Edit plan (BOTH jobs, identical step)

Insert ONE step in each smoke job, **immediately after `- uses: actions/checkout@v4`** and
**before** the `Smoke the pushed … image` run step. (Placing it after checkout is fine;
placing it before or after the GHCR login is also fine — it only must precede `run_smoke_gate`.)

```yaml
      - name: Provision pinned Node for the documented init --bundle leg (nan-020)
        uses: actions/setup-node@v4
        with:
          node-version: '24'
```

Apply verbatim to BOTH `smoke-amd64` and `smoke-arm64`. Do NOT add `registry-url`/auth fields
(those are publish-only, from `package-npm`); the smoke leg only needs the runtime.

## Data flow / contract

- This step's SOLE job is to put a pinned node 24 on PATH before `run_smoke_gate` runs the
  extended script. It does not change exit-code handling, image resolution, or the gate call.
- Enforcement half lives in the script: the `command -v node` preflight in
  `docker-http-posture-smoke.md` hard-fails (exit 1) if node is somehow still absent —
  defense-in-depth, complementary not redundant (R-04 sc.2).

## Error handling

- If `setup-node` itself fails, the job fails at that step (standard Actions behavior) before
  the gate runs — correct (a provisioning failure must not green).
- If the step is later deleted/renamed, the script's node preflight catches it and hard-fails
  with `node not available — the documented init --bundle path cannot be exercised`. The two
  together make node-absence intentional, not silent.

## Key test scenarios (hints — inspection-based, R-04 sc.2)

- Inspect `release.yml`: BOTH smoke jobs contain a `setup-node@v4` step with
  `node-version: '24'`, positioned before the `run_smoke_gate` step.
- Version parity: the smoke `node-version` matches `package-npm`'s `'24'`.
- Defense-in-depth: the script's node preflight exists independently of the YAML step.
- (Post-tag-confirmable, accepted PENDING pre-merge: actual GH execution provisions node.)
