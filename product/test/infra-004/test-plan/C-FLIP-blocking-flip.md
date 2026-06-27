# Test Plan — C-FLIP: Blocking Flip into `create-container-manifest.needs:`

> File under test: `.github/workflows/release.yml` (one-line edit to `create-container-manifest.needs:`, line 615).
> Test file: **NEW** `release-gate-isolation-lane-static-test.sh` (shared with C-LN — the
> `needs:`-graph assertions) + the C-TS forced-cell stub results.
> Risks: R-05/R-08 (the edge that realizes the DoD). ACs: AC-12, AC-13. Also AC-15 (diff scope).

## What the flip is
Add the isolation lane id to `create-container-manifest.needs:` (currently
`[smoke-amd64, smoke-arm64, embed-amd64, embed-arm64]`). Once present, a RED (exit 1) lane fails
the `needs:` edge and the manifest never assembles — **this is the line that realizes the DoD.**
INFRA (exit 2 → return 0) does NOT fail the edge but is visible.

## Static / graph assertion expectations

### AC-12 — lane ∈ manifest `needs:`; RED gates the edge (CRITICAL — R-05/R-08)
- `test_lane_in_manifest_needs` (`needs:`-graph assertion): parse `release.yml`; assert the
  isolation lane id is a member of `create-container-manifest.needs:`. This is the standing-gate
  → blocking-gate flip; its absence = DoD not realized.
- `test_red_gates_the_edge` (compositional): the C-TS `test_tristate_red_exit1_blocks` cell
  proves the lane **returns 1** on a forced RED; combined with the `needs:` membership above,
  this proves "RED blocks the manifest" **pre-merge, without a tag push** (ARCHITECTURE §7).
- `test_manifest_still_excludes_dispatch`: `create-container-manifest` keeps
  `if: github.event_name != 'workflow_dispatch'` (line 616) so the AC-11 dispatch run does not
  attempt the manifest.

### AC-13 — INFRA does NOT fail the manifest, stays visible (N4 preserved)
- `test_infra_does_not_gate_edge` (compositional): the C-TS
  `test_tristate_infra_exit2_nonblocking_visible` cell proves the lane **returns success** on
  forced INFRA AND emits `::warning::` + the canonical literal
  `[infra004-gate] INFRA — ISOLATION NOT VERIFIED THIS RUN`. A success return means the `needs:`
  edge is satisfied → manifest proceeds (non-blocking), while the marker keeps "enforcement went
  dark" visible.

### R-07/R-08 — sibling exposure unchanged by the flip
- `test_manifest_needs_only_appends`: the `needs:` edit **only appends** the new lane id; the
  four existing edges (`smoke-amd64`, `smoke-arm64`, `embed-amd64`, `embed-arm64`) are unchanged.

## AC-15 — diff scope (whole-feature gate, recorded here)
- `test_diff_scope` (`git diff --stat`): exactly `multi-tenant-isolation-smoke.sh`,
  `release-gate-lib.sh`, `.github/workflows/release.yml` (plus feature docs + the new/ extended
  test files); **zero `crates/` paths**; the smoke diff contains only the warmup barrier.

## CI-only (OVERVIEW §5)
The **first real execution of the blocking `needs:` edge on a tag** happens only post-merge
(R-10). Pre-merge proves the edge exists (graph) and that RED→return-1 / INFRA→return-0 (C-TS);
it does NOT prove tag-push resolution. A tag-path INFRA degrades to non-blocking (safe); the only
first-tag path that can block a healthy release is a harness-step failure, already exercised by
AC-11's dispatch run. Budget one post-merge tag round (C-10).

## Coverage requirement
The blocking edge is proven to exist by a `needs:`-graph parse; RED→block and INFRA→non-block are
proven compositionally via the C-TS forced cells; the edit is append-only; diff scope confined to
the three named files with no `crates/` change. Tag-push first-run is the explicitly budgeted
post-merge residual.
