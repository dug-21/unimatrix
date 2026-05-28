# Agent Report: nxs-012-agent-3b-risk

## Task
Supplement RISK-TEST-STRATEGY.md with risks and test scenarios for the `--skip-quarantined` import flag (SCOPE Goal #9, AC-22 through AC-29, ADR-007).

## Artifacts Modified
- `product/features/nxs-012/RISK-TEST-STRATEGY.md` — added 9 risks (R-16 through R-24), 27 test scenarios, integration risks, edge cases, failure modes, and scope risk traceability for the skip-quarantined feature.

## Risk Summary
- **9 new risks** added (R-16 through R-24)
- **27 new test scenarios**
- By priority: 5 High, 2 Medium, 2 Low
- Total file now: 24 risks, 64 scenarios

## Top Risks for Delivery Attention

1. **R-16 (High)**: Cascade incompleteness — every dependent row type must have its own `quarantined_ids.contains()` check. Missing one creates silent dangling references. Pattern #3910 documents this exact failure class.

2. **R-19 / R-20 (High)**: Dual-column checks on co_access and graph_edges — both sides of each relationship must be checked. Checking only one column is a likely implementation oversight that lets quarantined-referencing rows through. Lesson #4536 shows status guard correctness is invisible without per-column variant tests.

3. **R-23 (Med)**: Hash validation interaction — `--skip-quarantined` reduces imported row count, which likely causes hash mismatch with export footer. Must resolve whether `--skip-hash-validation` is required when using `--skip-quarantined`, or whether hash computation runs post-filter.

## Open Questions
1. Does hash validation compare against the full export file or the imported subset? This determines whether `--skip-quarantined` requires `--skip-hash-validation`.
2. Does `reconstruct_embeddings` iterate the database or the export file? If the export file, it will attempt reconstruction for skipped entries.

## Knowledge Stewardship
- Queried: /uni-knowledge-search for "lesson-learned failures gate rejection import filter" -- found #3910 (multi-pass status filter consistency) and #4536 (status guard invisible without per-column tests), both directly informed R-16, R-19, R-20
- Queried: /uni-knowledge-search for "risk pattern HashSet filter skip cascade" -- no directly applicable patterns found
- Queried: /uni-knowledge-search for "import CLI flag boolean default path" -- found #3817 (dual-site config defaults), informed R-18 default path concern
- Stored: nothing novel to store -- the dual-column check risk and cascade completeness risk are well-covered by existing patterns #3910 and #4536
