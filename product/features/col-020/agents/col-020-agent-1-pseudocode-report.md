# Agent Report: col-020-agent-1-pseudocode

## Status: COMPLETE

## Files Produced

- `product/features/col-020/pseudocode/OVERVIEW.md`
- `product/features/col-020/pseudocode/types.md` (C2)
- `product/features/col-020/pseudocode/session_metrics.md` (C1)
- `product/features/col-020/pseudocode/store_api.md` (C4)
- `product/features/col-020/pseudocode/knowledge_reuse.md` (C3)
- `product/features/col-020/pseudocode/report_builder.md` (C5)
- `product/features/col-020/pseudocode/handler_integration.md` (C6)

## Components Covered

All 6 components from IMPLEMENTATION-BRIEF.md:
1. C1: session_metrics -- new file with 2 public functions, 3 internal helpers
2. C2: types -- 3 new structs, 5 new fields on RetrospectiveReport
3. C3: knowledge_reuse -- inline algorithm for handler, documented separately
4. C4: store_api -- 4 new Store methods across 4 files
5. C5: report_builder -- no signature change (post-build mutation pattern)
6. C6: handler_integration -- 7 new steps in context_retrospective handler

## Open Questions

1. **Attribution counting**: `scan_sessions_by_feature` returns only sessions with matching feature_cycle. The `discover_sessions_for_feature` method (ObservationSource trait) is used for total_session_count. Need to verify that discover_sessions_for_feature includes fallback-attributed sessions or if the handler's own observation loading path is the better source for total count. Pseudocode uses discover_sessions_for_feature for total, scan_sessions_by_feature for attributed -- consistent with architecture.

2. **Knowledge reuse "stored in session A" definition**: FR-02.1 says "stored or created in session A within the topic." Entries don't carry an origin session_id field. The pseudocode uses a cross-session appearance heuristic: an entry appearing in query_log/injection_log for 2+ distinct sessions counts as reused. This is a pragmatic approximation. If a stricter "origin session" determination is needed, entries would need a `created_in_session` field (not available today).

## Self-Check

- [x] Architecture output was read before writing any pseudocode
- [x] No invented interface names -- every name traced to architecture or codebase
- [x] Output is per-component (OVERVIEW.md + one file per component), not monolithic
- [x] Each component file includes function signatures, error handling, and test scenarios
- [x] No TODO, placeholder functions, or TBD sections -- gaps flagged as open questions
- [x] Shared types defined in OVERVIEW.md match usage in component files
- [x] All output files within `product/features/col-020/pseudocode/`
