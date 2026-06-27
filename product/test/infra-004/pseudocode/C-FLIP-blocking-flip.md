# C-FLIP — Blocking Flip

> File: `.github/workflows/release.yml`
> ADR-003 (#5351). The single blocking edge — kept a separate component so the flip
> is reviewable in isolation. Lands ONLY after the AC-11 cold-model GREEN proof.

## Purpose

Add the standing isolation lane (`multi-tenant-isolation-amd64`, C-LN) to
`create-container-manifest.needs:` so a **RED** verdict fails the lane job, leaves the `needs:`
edge unmet, and the release manifest never assembles — realizing the DoD: a cross-tenant leak
cannot ship a release.

## The Edit (one line)

Current (`release.yml:615`):

```
create-container-manifest:
    needs: [smoke-amd64, smoke-arm64, embed-amd64, embed-arm64]
```

After C-FLIP:

```
create-container-manifest:
    needs: [smoke-amd64, smoke-arm64, embed-amd64, embed-arm64, multi-tenant-isolation-amd64]
```

Nothing else in `create-container-manifest` changes. Its existing
`if: github.event_name != 'workflow_dispatch'` guard stays — the manifest assembles only on tag
push; the lane still runs (and is visible) on dispatch via C-LN.

## Precondition (gating, do NOT land C-FLIP early)

- **AC-11 cold-model fresh-build GREEN** must be demonstrated first (C-LN run via
  `workflow_dispatch` on the rebased feature branch, real cold HF download path, evidence
  recorded). C-FLIP is the last of the four deliverables; landing it before AC-11 GREEN reintroduces
  the never-green-on-tag risk on a blocking edge (R-10).

## State Machine — manifest assembly post-FLIP

```
[smoke-amd64, smoke-arm64, embed-amd64, embed-arm64, multi-tenant-isolation-amd64]
        │ ALL succeed (isolation lane: C-TS returned 0 → GREEN or INFRA-visible)
        ▼
   create-container-manifest runs → release proceeds
        │ isolation lane FAILS (C-TS returned 1 → RED / early-exit-0 / SKIP / unexpected)
        ▼
   needs: edge unmet → manifest does NOT assemble → leak cannot ship (DoD)
        │ isolation lane INFRA (C-TS returned 0 + ::warning:: + canonical marker)
        ▼
   needs: edge MET → manifest assembles, but isolation flagged not-verified (visible-vacuous, safe mode)
```

## Data Flow

- **Input:** the C-LN job's success/failure status.
- **Transformation:** GitHub Actions evaluates the `needs:` edge — any `needs:` job failure
  short-circuits `create-container-manifest`.
- **Output:** manifest assembles (release ships) or does not (release blocked).

## Error Handling / Blast-Radius

C-FLIP changes nothing about classification — it only activates the edge. The fail-closed contract
is C-LN's (ARCH §5): only the script's exit-2 (mapped by C-TS to `return 0`) is non-blocking; every
harness-step failure now blocks all releases (fail-closed), identical exposure to the four existing
blocking lanes plus one sqlite3 step. AC-11's dispatch run already exercised that entire harness
before this edge went live, so a harness break is caught pre-flip.

## Key Test Scenarios (hints — full plan in test-plan/)

1. **`needs:`-graph assertion (AC-12):** static YAML parse — `multi-tenant-isolation-amd64 ∈
   create-container-manifest.needs:`.
2. **RED-blocks (AC-12, pre-merge):** C-TS forced-RED stub returns 1 → the lane job exits 1 → the
   `needs:` edge would be unmet → manifest would not assemble (proven via the stub + graph
   assertion, no tag push).
3. **INFRA-does-not-block (AC-13):** C-TS forced-INFRA → lane returns 0 (edge met) → manifest
   assembles, with the `::warning::` + canonical marker visible (N4 preserved).
4. **Diff confinement (AC-15):** the only `release.yml` change in C-FLIP is the single `needs:`
   list addition; no other job edits.
