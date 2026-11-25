# vnc-050-retro-architect report

## Results
- Patterns: NEW #5833 (stdio bridge EOF drain). Skipped: one-off slug guard, token table and cap ceiling.
- Procedures: none. The npm test selection fix (4ae10bd6) is already covered by #5781.
- ADRs: #5814 through #5829 (ADR-001..006, with ADR-004 corrected to #5826 and ADR-006 to #5829) validated by delivery and gates. None flagged for supersession.
- Lessons: NEW #5832 (CI matrix: Node 18 IPv6 localhost, Windows USERPROFILE, vacuous negative TLS tests). Extends #5128. Mid-cycle #5827, #5828 and #5830 kept. #5831 auto-generated retrospective entry.
- Findings: coordinator_respawns 55 (12.1 sigma outlier). bash_for_search 681 (2.1 sigma). Unimatrix queries skipped because MCP tools were deferred, which recurs #3500. Fix is a protocol/spawn-prompt change (ToolSearch preload); human action, not stored.
- Edges: none (bar not met).

## Knowledge Stewardship
- Queried: context_search vnc-050 / CI-matrix / deferred-tools; context_cycle_review
- Stored: #5832 lesson, #5833 pattern
