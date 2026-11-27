# 1060 Retro — Architect Report

Agent: 1060-retro-architect (uni-architect), retrospective mode. Observation data: cycle key `bugfix-1060`. Provenance is reconstructed, so candidates were weighted lower.

## Entries
| Action | ID | Title | Reason |
|--------|----|-------|--------|
| Corrected | #5267 → #5978 | Release-only gates fail at the tag (first-time gates in sequence; green gates drift on unrelated changes): dry-run release.yml pre-tag | This bug is a recurrence #5267 predicted. Added Mode B (green gates drift after an unrelated layout change) and the workflow_dispatch dry-run before tagging. |
| New, then corrected | #5975 → #5979 | Read-back gates counting ~/.unimatrix subdirs break on new siblings; select by marker file (remote.json) | Marker-file selection was not covered elsewhere. Trimmed to that lesson so it does not duplicate #5978. |
| New | #5977 | Harness daemon fixtures: passive stderr readiness, no probe in parity harnesses, inside try/finally (pattern) | No existing readiness-fixture entry. Combines the stderr signal, parity (#5293) and try/finally (security F1). |
| Skipped | #5922 | gh issue view --comments fails (Projects-classic GraphQL) | F-02 Bash failures are already covered by this entry. |
| Skipped | — | F-01: 11 coordinator respawns | These are SM resumes via SendMessage, which the protocol requires. Not a real respawn problem, so no lesson. |
| Validated | #5126 | vnc-039 ADR-003 credstore at <projectHash>/remote.json | Holds. The fix depends on it as the marker. No amendment. |
| Validated | #5293 | nan-021 ADR-003 MetricVector comparison contract | Holds. It is the basis for rejecting the probe call. No change. |

No ADRs created or superseded. No edges asserted.

## Knowledge Stewardship
- Queried: context_cycle_review bugfix-1060; context_search "1060", release-gate drift (#5267, #5268, #5188), dir-scan selection (#4972), readiness fixtures (none), gh CLI failures (#5922); context_get #5267, #5126, #5293
- Stored: #5977 (pattern), #5979 (lesson-learned, via #5975); corrected #5267 → #5978

Unimatrix entries consulted: #5267, #5126, #5293, #5188, #4972, #4531, #5922, #5268, #5266
