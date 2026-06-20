# Agent Report — nan-020-agent-1-pseudocode

## Deliverables (all under product/features/nan-020/pseudocode/)

- OVERVIEW.md — component interaction, shared contracts (exit table, blob format, isolated
  credstore path, terminal marker, canonical doc form), and the same-file coupling statement.
- docker-http-posture-smoke.md — Gates 5–7 append-only extension.
- hermeticity-sandbox.md — process-boundary sandbox lifecycle + REQUIRED negative control.
- release-yml-setup-node.md — pinned setup-node@v4 on both smoke jobs.
- docs-client-setup.md — rewrite structure + executable-claim classification.
- readme-bundle-example.md — exhaustive occurrence enumeration + convergence plan.
- uni-docs-remit.md — targeted agent-def edit plan.

## Components covered

6 components (matching the Component Map) + OVERVIEW.

## Same-file / ordering coupling Stage 3b MUST respect

- **CRITICAL same-file:** docker-http-posture-smoke.md AND hermeticity-sandbox.md edit the
  SAME file (docker-http-posture-smoke.sh). Route BOTH to ONE agent. The pre-merge negative-
  control STUB TEST (in hermeticity-sandbox.md Part 2) is a separate NEW test file and may be
  a distinct task, but its author must read the final script shape.
- setup-node (release.yml) and the script's node-absent hard-fail should land in the SAME PR
  (provisioning + enforcement halves) so the hard-fail is not armed before node is provisioned.
- Both doc components must use the identical canonical form `init --bundle <blob>` (no --slug)
  and identical "legacy" marking for --remote — shared contract OVERVIEW §H.
- uni-docs-remit.md is fully independent.

## Open questions / gaps found

- README has FOUR bundle-via-`--remote` occurrences (lines 123, 130, 585, 587), not the two
  OQ-B named. All enumerated in readme-bundle-example.md. Line 113 is the legacy
  `--remote <url> --token` form — mark legacy, do NOT converge. No design gap; just a more
  exhaustive enumeration than OQ-B anticipated.
- Gate 7 hook-fire invocation: the exact hook-client entry path + minimal event JSON is left
  as an implementation detail flagged in docker-http-posture-smoke.md — the load-bearing
  assertion is the per-slug store DELTA (the hook client is fail-open so its exit code is not
  authoritative for the 204; the HTTP-code message is best-effort where the client surfaces
  it). The implementer should confirm the hook-client entry path against the repo checkout.
- No other blocking gaps.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search — surfaced #5180 (self-skipping smoke must hard-fail
  on skip, keyed by distinct exit code — applied to node-absent hard-fail) and #5192 (extract
  the verify-by-name gate spine into a sourceable lib so YAML + pre-merge test share bytes —
  applied to the negative-control stub test sourcing release-gate-lib.sh). The HOME-isolation/
  negative-control search returned no nan-020-adjacent patterns; relied on ADR-005 + vnc-041
  AC-02/AC-06 references already in the source docs.
- Deviations from established patterns: none. The pseudocode is a faithful reuse of the
  nan-019 fail()/exit-1/marker contract (#5180/#5183/#5192) and the vnc-041 AC-06 negative-
  control shape — no new cross-feature pattern minted.
