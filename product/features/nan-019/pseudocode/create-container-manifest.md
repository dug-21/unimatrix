# Component: `create-container-manifest` rewire (release.yml)

> **Same file as `release-smoke-jobs.md`: `.github/workflows/release.yml`.** Serialize both
> on one Stage-3b agent (OVERVIEW single-file note). This component is a TWO-LINE-class edit
> to an existing job — the manifest assembly body is UNCHANGED.

## Purpose

Move the release gate onto the multi-arch manifest — the tag operators actually pull — so no
released artifact is ever un-smoked, and green-skip the manifest on dispatch dry-runs so only
the `smoke-*` statuses carry signal. (FR-06/FR-10; AC-02/04/08; C-05/C-14; ADR-001/ADR-004)

## The edit (only two lines change; body untouched)

```
job create-container-manifest:
  needs: [smoke-amd64, smoke-arm64]                     # WAS [build-container-x64, build-container-arm64]
  if: github.event_name != 'workflow_dispatch'         # ADD — dispatch green-skip
  runs-on: ubuntu-latest
  steps:
    - GHCR login (docker/login-action@v3)               # UNCHANGED
    - Extract version: echo "version=${GITHUB_REF_NAME}" >> "$GITHUB_OUTPUT"   # UNCHANGED (un-stripped)
    - docker buildx imagetools create ...               # UNCHANGED (assembles :v<version> + :latest)
```

## Why each line is shaped this way

- **`needs: [smoke-amd64, smoke-arm64]`** — the builds stay in the graph **transitively**:
  `smoke-amd64 needs build-container-x64`, `smoke-arm64 needs build-container-arm64`. Re-listing
  the builds here is redundant and is OMITTED (FR-06). Single gate point: BOTH arches must pass
  before the manifest assembles (R-08). Both names must be present and spelled identically to the
  job keys in `release-smoke-jobs.md` — a missing/misspelled name silently un-gates an arch.
- **`if: github.event_name != 'workflow_dispatch'`** — on a dispatch dry-run the build pushes
  only `:latest-<arch>` and `GITHUB_REF_NAME` is a branch, so the body's
  `:${VERSION}-<arch>` ⇒ `:<branch>-<arch>` sources never existed → `imagetools create` would 404
  and red the job on a false signal. Skipping the job makes the two `smoke-*` statuses the only
  meaningful dispatch signal; the skipped manifest is a **green-skip**, not a false-red. On a
  `v*` push the condition is true, so the job runs and stays smoke-gated. (FR-10/AC-08/NFR-11)
- **Body unchanged** — `version=${GITHUB_REF_NAME}` (un-stripped) and `:${VERSION}-<arch>` are the
  parity anchor: the smoke jobs resolve the SAME un-stripped form (R-09). Touching the body is
  out of scope and would risk drifting parity.

## ADR-004 invariant (must hold after the edit — R-06)

- After the rewire, trace the full `needs:` graph and confirm:
  - No `smoke-*` job name appears in any `build-linux-*` / `package-npm` / `create-release` `needs`.
  - No binary/npm job name appears in any `smoke-*` `needs`.
  - `create-release` still `needs: package-npm` only.
  - Single manifest block point; smoke jobs depend ONLY on container-branch jobs.
- This closed-set invariant is what a future `needs:` edit must not violate; the tag-parity /
  needs-graph test (`test-tag-parity.md` / gate-logic test) pins it.

## Data Flow

- **In:** `smoke-amd64` + `smoke-arm64` job conclusions; `GITHUB_REF_NAME`; pushed per-arch tags.
- **Out (push only):** multi-arch index `:v<version>` and `:latest` pushed to GHCR.
- **Out (dispatch):** job skipped; no manifest pushed; no release object created.

## Error Handling / Propagation

| Condition | Manifest job behavior |
|-----------|-----------------------|
| Either smoke red (push) | `needs` unsatisfied → manifest **skipped** (not run) → no release. (AC-02) |
| Both smokes green (push) | runs; assembles + pushes `:v<version>` and `:latest`. |
| `workflow_dispatch` | `if` false → **green-skip**; not a false-red. (AC-08) |
| Per-arch source tag absent (push) | `imagetools create` fails → job red (backstop; primary catch is the smoke pull). |

## Key Test Scenarios (hints)

- Config: `create-container-manifest.needs` includes BOTH `smoke-amd64` and `smoke-arm64` (R-08).
- Config: `if: github.event_name != 'workflow_dispatch'` present (R-08/AC-08).
- Config: `needs`-graph trace shows zero cross-branch edge; single block point (R-06/AC-04).
- Reasoned/behavioral: forcing a smoke red leaves `package-npm` + `create-release`
  reachable/unaffected; manifest skipped (R-06).
- Post-tag (AC-07): both smokes green → manifest publishes. Dispatch run → manifest skipped,
  both smokes green (AC-08).

## Open Questions

None. Both decisions (gate point; dispatch gating) are DECIDED in ADR-001/ADR-004.
