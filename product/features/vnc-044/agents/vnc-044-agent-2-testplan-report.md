# Agent Report — vnc-044-agent-2-testplan (Stage 3a, Test Plan Design)

## Deliverables

| File | Component | Primary risks |
|------|-----------|---------------|
| `product/features/vnc-044/test-plan/OVERVIEW.md` | strategy, R→test map, integration harness plan | all |
| `product/features/vnc-044/test-plan/verbosity.md` | `response/verbosity.rs` primitives | R-01, R-02, R-10, R-12 |
| `product/features/vnc-044/test-plan/graph_read_projection.md` | `graph_read_projection.rs` | R-07, R-03 (proj side), R-10, R-14 |
| `product/features/vnc-044/test-plan/graph_read.md` | `graph_read.rs` resolver + seam | R-04, R-05, R-08, R-03, R-09, R-06, R-13 |
| `product/features/vnc-044/test-plan/tools.md` | `tools.rs` description | R-11 (doc gate), R-13 |

Component plans map 1:1 to the pseudocode/component files in IMPLEMENTATION-BRIEF.md Component Map.

## Risk Coverage Mapping (all 14 risks placed)

- **Critical:** R-01 (verbosity.md unit boundary table + no-panic property), R-02 (verbosity.md byte-compare incl. 257-floors-to-256 trap), R-03 (graph_read.md 5-mode integration + graph_read_projection.md per-envelope metadata), R-04 (graph_read.md golden byte-equality + full-arm-not-projection structural guard).
- **High:** R-05 (graph_read.md markdown ×7 modes, pre-dispatch), R-06 (graph_read.md static `--no-run`/grep/regression gates), R-07 (graph_read_projection.md present-AND-absent key sets, node + edge).
- **Med:** R-08 (resolver alias/conflict table, order pinned), R-09 (accept-and-ignore ×2 edge modes + bogus reject), R-10 (empty/tags/confidence), R-11 (tools.md doc/expectation gate — explicitly NOT tested as a defect).
- **Low:** R-12 (single-source `256` grep), R-13 (substring discipline throughout), R-14 (file placement/line budget review).

Every AC (AC-02..AC-09, AC-03b) traced to at least one named test in the component plans.

## Integration Suite Plan

- **Gate:** `smoke` (add 1 summary smoke) + `test_tools.py` (axis behavior, markdown ×7, alias/conflict, accept-and-ignore) + `test_lifecycle.py` (5-mode default-summary + envelope-metadata + full golden). `test_protocol.py`/`test_get_edges.py` regression + edge field-set. Not run for behavior: security/contradiction/confidence/volume (traversal & scoring unchanged, NFR-6).
- **Required harness extension (cumulative):** add `detail: str | None = None` to `harness/client.py:746 context_graph(...)` + its arg-marshalling — mirrors existing `format` handling at `:771`. All new integration tests depend on this one change. Do NOT scaffold a parallel client.
- **Parsing gotcha:** use the brace-depth JSON extractor (pattern #4469) if any graph mode appends a non-JSON trailer.

## Open Questions (for Stage 3b/3c)

1. **Golden capture method (R-04/AC-04).** A true pre-vnc-044 byte-for-byte golden requires capturing `detail=full` output from the pre-change binary. If the harness can't checkout/build the old binary, the plan specifies a documented fallback (complete `EntryRecord` key set present + byte-stable across runs). Tester must state which was used in RISK-COVERAGE-REPORT.md.
2. **`parse_detail` case policy — RESOLVED (Gate 3a).** Ratified policy is case-INSENSITIVE accept, mirroring `response/mod.rs::parse_format`'s `f.to_lowercase().as_str()`. verbosity.md corrected: `"Summary"`/`"SUMMARY"`/`"Full"`/`"FULL"` → accepted; only a genuinely-unknown value (`"brief"`) → `ERROR_INVALID_PARAMS`. Named tests `test_parse_detail_case_insensitive` + `test_parse_detail_unknown_rejected`.
3. **AC-06 size threshold.** No absolute KB is asserted (fixture-dependent); the plan asserts the summary/full ratio for the same query. If a concrete #913-scale fixture (75 nodes) is built, a threshold can be set then.
4. **`graph_read.rs` line budget (R-14, architect OQ-1).** Non-blocking; if the resolver push crosses 500 lines, relocate `resolve_graph_output` per C-7. Watch at Gate 3b.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` (task: vnc-044 test plan design) + `context_search(topic:testing)` + `context_get` on #4469, #5449. Findings: #5509 (ADR-001 two-axis contract), #5449 (vnc-043 twin-literal byte-equality description guard `test_graph_tool_attr_description_matches_const` #869 — drove the tools.md test plan), #4469 (infra-001 brace-depth JSON extractor for JSON+trailer responses — drove the harness parsing note), #4502/#4503/#4490 (GraphParams layout-lock permits `Option<T>` additions; `graph_read.rs` line budget), #2928 (backward-compat snapshot + static-grep gate pattern — informed the golden/full-arm guards).
- Stored: nothing novel at plan-design stage — all applied patterns were reused, not discovered. A golden-payload harness technique may be worth storing at Stage 3c if one is built (flagged for the executing tester).
