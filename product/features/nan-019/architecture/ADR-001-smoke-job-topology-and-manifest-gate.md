## ADR-001: Per-Arch Smoke Jobs Gate the Manifest, Not the Per-Arch Pushes (OQ-1 mechanics)

### Context

The release pipeline builds and pushes two per-arch container images
(`build-container-x64` on `ubuntu-22.04`, `build-container-arm64` on
`ubuntu-22.04-arm`) and then assembles them into a multi-arch manifest
(`create-container-manifest`, `needs: [build-container-x64, build-container-arm64]`).
Operators pull the manifest tag (`ghcr.io/<owner>/unimatrix:v<version>` /
`:latest`), not the per-arch intermediates. (The manifest job resolves
`version=${GITHUB_REF_NAME}` UN-stripped, so its tag KEEPS the `v`: tag `v0.8.3`
→ `:v0.8.3`; the per-arch sources it assembles are `:v0.8.3-amd64` / `-arm64`.)

nan-019 must wire `docker-http-posture-smoke.sh` so that **no released
multi-arch artifact is ever un-smoked** (Goal 1/2, N5) while preserving
**ADR-004 container-CI independence** (#4572): the binary/npm branch must gain
no `needs` edge to/from the container/smoke branch (SR-08). OQ-1 is
architect-owned; the human-recorded preference is to gate the manifest. The
open mechanical question is exactly *where* the `needs` edges land so the block
falls on the released artifact and nowhere else.

Two placements were considered:

- **Gate each per-arch push** (block `build-container-x64` / `build-container-arm64`
  on their smoke). Rejected: the smoke needs the image to already be **pushed**
  to GHCR (ADR-002 / SR-02 — test the pushed bytes), so the smoke cannot precede
  its own push; and per-arch pushes are independent leaves with no shared gate
  point, so this cannot express "release only if BOTH arches pass."
- **Gate the manifest** (block `create-container-manifest` on both smokes).
  Chosen.

### Decision

Insert two smoke jobs between the per-arch pushes and the manifest, forming this
container-branch subgraph (binary/npm branch unchanged and unreferenced):

```
build-container-x64  ──▶ smoke-amd64 ─┐
(ubuntu-22.04)           (ubuntu-22.04)│
                                       ├─▶ create-container-manifest
build-container-arm64 ─▶ smoke-arm64 ─┘     (needs: [smoke-amd64, smoke-arm64])
(ubuntu-22.04-arm)       (ubuntu-22.04-arm)
```

Concretely:

1. `smoke-amd64` runs on `ubuntu-22.04`, `needs: [build-container-x64]`.
2. `smoke-arm64` runs on `ubuntu-22.04-arm`, `needs: [build-container-arm64]`.
3. `create-container-manifest` changes from `needs: [build-container-x64,
   build-container-arm64]` to **`needs: [smoke-amd64, smoke-arm64]`**, and gains
   **`if: github.event_name != 'workflow_dispatch'`** (dispatch carve-out above).
   (The build jobs remain transitive prerequisites through the smokes, so the
   manifest still cannot run before both builds; the explicit edge moves to the
   smokes.)

Each smoke `needs:` only its **own-arch** build (one push, one smoke — no
cross-arch coupling), and the manifest gates on **both** smokes (BOTH arches
must pass — honors the SCOPE HARD RULE that arch coverage is named, never
silently capped, SR-07). The per-arch `:v<version>-amd64` / `-arm64` tags
(metadata-action keeps the `v`) are briefly public before their smokes run; this
is accepted (SR-09, OQ-1/OQ-2) because the manifest operators pull is never
created until both smokes pass.

**Dispatch carve-out:** on `workflow_dispatch` the build jobs push only
`:latest-<arch>` (semver yields nothing off-tag), so the manifest's
`version=${GITHUB_REF_NAME}` = `<branch>` would assemble `:<branch>` from
`:<branch>-<arch>` sources that never existed → a guaranteed manifest failure
that reds an otherwise-clean dry-run. The manifest therefore also carries
`if: github.event_name != 'workflow_dispatch'` (see ADR-004): on dispatch only
the smoke-* statuses are the meaningful signal; on a `v*` push the manifest runs
and is gated on both smokes as above.

No `needs` edge touches `build-linux-*`, `package-npm`, or `create-release`.
ADR-004 holds: a smoke or ARM64-runner failure blocks **only** the manifest, and
the binary/npm release proceeds independently (AC-04).

### Consequences

- Easier: A single gate point (`create-container-manifest`) cleanly expresses
  "release the manifest only if both per-arch first-run paths passed." No
  released artifact is ever un-smoked (N5). ADR-004 independence is structurally
  guaranteed — the gate lives entirely on the container branch.
- Easier: BOTH-arch coverage is explicit in the `needs:` list, so a future
  silent drop to amd64-only would be a visible YAML edit, not a buried default.
- Harder: The per-arch intermediate tags are briefly public before they are
  validated (SR-09) — accepted, documented, and bounded (operators pull the
  gated manifest).
- Harder: The container release path is now longer by one job per arch (a boot +
  poll + write, not a rebuild — ADR-002 keeps it to image pull, not build). The
  manifest step waits on the slower of the two smokes (arm64 cold model load,
  SR-03). Binary/npm latency is unaffected.
- Related: ADR-002 (pushed-bytes contract — why each smoke `needs:` its push),
  ADR-003 (verify-by-name — what makes a smoke job pass), ADR-004 (#4572 —
  the independence invariant this topology preserves).
