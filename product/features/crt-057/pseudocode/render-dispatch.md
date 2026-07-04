# Component: Render dispatch — drop `"summary"` arm ×4

File: `unimatrix-server/src/mcp/tools.rs`. The four `"markdown" | "summary"` render arms are at
`:2532`, `:3359`, `:4268`, `:4324` (grep-confirmed exact strings).

## Purpose

`format` accepts EXACTLY `"markdown"` (default) and `"json"`. The dead `"summary"` alias is DROPPED
(not folded) → falls to the existing `ERROR_INVALID_PARAMS` arm at all four loci (ADR-002 / FR-1 /
CON-5). No third render path survives.

## The edit (identical at all four loci)

```
# BEFORE (each of :2532, :3359, :4268, :4324):
match format:
    "markdown" | "summary" => <markdown render>
    "json"                 => <json render>
    other                  => ERROR_INVALID_PARAMS("Unknown format '{other}'. Valid values: \"markdown\", \"json\".")

# AFTER:
match format:
    "markdown"             => <markdown render>       # "summary" removed from this arm
    "json"                 => <json render>
    other                  => ERROR_INVALID_PARAMS("Unknown format '{other}'. Valid values: \"markdown\", \"json\".")
```

Only the pattern `"markdown" | "summary"` → `"markdown"` changes at each site. The error arm and its
message already exist (`:2537`, `:3415`, `:4300`, `:4381`) — `"summary"` now routes there. Do NOT change
the message; it already reads `Valid values: "markdown", "json".` (CON-5 exact string).

Two of the four sites are inside render helper fns (`:4268`, `:4324`) and two are inline in the handler
(`:2532` in the cached-metrics dispatch helper, `:3359` full-pipeline); the edit is textually identical.

## Doc-comment sweep

- `tools.rs:426` doc-comment (`Response format: "summary", "markdown", "json".`) is on a DIFFERENT struct
  (`EnrollParams`), NOT `RetrospectiveParams` — leave it unless it actually governs review format
  (it does not). Update the `RetrospectiveParams.format` doc (retrospective-params.md) and the
  `context_cycle_review` tool description (consumer-reconciliation.md) to list exactly `markdown | json`.

## Render equivalence (FR-2 / R-12)

`"markdown"` and `"json"` produce identical report CONTENT, differing only in serialization. Neither
retrieves candidates nor purges — both are non-`transcript` paths unless `transcript` is also supplied
(orthogonal axes).

## Consumer sweep (R-12 sc.3 — delivery pre-flight)

DROP is breaking for any live `format:"summary"` caller. Sweep the reconciled consumers
(consumer-reconciliation.md) + a repo grep for `format:"summary"` / `"format":"summary"`; flag any live
caller to the delivery leader. If one surfaces, reconsider fold-to-markdown (the non-breaking option) —
otherwise proceed with the DROP.

## Error handling

- `"summary"` and any unknown value → `ERROR_INVALID_PARAMS` with the exact valid-values message; no
  partial render.

## Key test scenarios

- `format:"summary"` → `ERROR_INVALID_PARAMS` with exact message at ALL four loci; no `"summary"` arm
  survives (R-12 sc.2, AC-11).
- `markdown` vs `json` same cycle → semantic content equality, buffer intact after both, no candidates on
  either (R-12 sc.1).
- Existing tests referencing `format:"summary"` as a valid alias (e.g. `tools.rs:5764`) updated to expect
  the error path.
