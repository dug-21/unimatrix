# Agent Report: vnc-018-synthesizer-b

**Agent**: vnc-018-synthesizer-b (Unimatrix Synthesizer)
**Task**: Regenerate IMPLEMENTATION-BRIEF.md and ACCEPTANCE-MAP.md after design review amendment pass

## Deliverables

- `product/features/vnc-018/IMPLEMENTATION-BRIEF.md` — overwritten
- `product/features/vnc-018/ACCEPTANCE-MAP.md` — overwritten

## All 6 Amendment Findings Reflected

1. **`current` mode CTE `AND e.status = 'Active'` filter** — Added as hard constraint #5 in the Constraints section. Current mode SQL block is reproduced verbatim in the brief with the filter explicitly present and explained. R-20 (Critical) and the orphaned deprecated entry error path are called out. AC-06 updated to note the status filter; AC-06b added to the acceptance map.

2. **BFS visited set keyed by `node_id` only** — Resolved Decisions table entry added. Constraint #7 added. AC-11a added to acceptance map: unit test asserting a node reachable at two depths appears exactly once at depth=1.

3. **`validate_no_unsupported_params` rejects `resolve_supersessions=Some(true)` on chain mode** — Resolved Decisions table updated. Constraint #4 added: check belongs in the centralized function, not `handle_chain`. AC-15c verification detail updated to emphasize the unit test targets `validate_no_unsupported_params` directly.

4. **AC-05a added: `current` mode on non-existent ID returns error** — AC-05a present in acceptance map with intentional asymmetry note, pairing instruction, and comment mandate. Constraint #6 added to brief emphasizing both AC-04 and AC-05a must be present as a matched pair. R-21 reflected.

5. **Validation ordering corrected** — Constraint #3 now reads: capability check → parameter validation → mode dispatch. Function signature comment for `handle_graph` documents "Capability check runs in tools.rs BEFORE handle_graph is called." ADR-003 resolved decisions row updated to remove the incorrect "before capability check" language.

6. **R-07 resolved via ADR-008** — ADR-008 added to Source Document Links table. Resolved Decisions table has a new row for `node_index_for` accessor. Component map adds `unimatrix-engine graph.rs` as a component. Files-to-Modify adds `unimatrix-engine/src/graph.rs`. Constraint #16 states this is a delivery-time implementation task, not a future-phase decision. The old "node_index visibility (must be resolved at Stage 3a)" constraint section has been removed.

## Additional Changes

- R-20 and R-21 as new risks are reflected in constraints (items #5, #6) and alignment status section.
- Alignment status section lists all 6 amendment findings explicitly by number.
- AC-06b (orphaned deprecated terminal returns error) added to acceptance map.
- AC-15c verification detail tightened to require both unit test on the function and MCP integration path test.
- AC-15b verification detail expanded to include the unrecognized-mode-with-field test (R-04 scenario).

## Status

COMPLETE — no remaining inconsistencies found.
