# Agent Report: bugfix-632-retro-architect

## Task
Retrospective architecture review of bugfix-632 (PR #634) — extract reusable knowledge from shipped config categories fix.

## Stewardship Actions

### Corrected Entries (4)
| Original | Replacement | Reason |
|----------|-------------|--------|
| #3721 | #4585 | Pattern updated: applies to add+remove (not just retire), config.rs no longer has duplicate const, count 5->7 |
| #4539 | #4586 | "goal" is now valid after bugfix-632; generalized to always verify against current INITIAL_CATEGORIES |
| #4266 | #4587 | ADR-002 nan-011 defaults table: categories updated from 5 to 7 entries |
| #4443 | #4588 | Added bugfix-632 as 8th data point (37 compile cycles from lockstep constant propagation) |

### Skipped (no action needed)
- #3715 (lesson-learned) — already deprecated
- #3002 ADR-005 (outcome retirement) — still accurate, outcome remains retired
- #3817 (serde/Default dual-site) — unrelated to categories lockstep, still valid
- #4333 (spec writers must read config.rs) — still valid, different scope than runtime divergence

## Patterns
| Action | ID | Title |
|--------|----|-------|
| corrected | #3721 -> #4585 | Changing INITIAL_CATEGORIES requires lockstep updates across 5 locations (4 independent sites) |

No new patterns stored. The corrected #4585 fully covers the lockstep constant pattern that was the root cause of bugfix-632.

## Procedures
None new or updated. Constant-propagation fix does not change any how-to procedure.

## ADR Status
| ADR | Status | Notes |
|-----|--------|-------|
| #3002 ADR-005 crt-025 (outcome retirement) | validated | Still accurate post-fix |
| #4266 -> #4587 ADR-002 nan-011 (config defaults) | corrected | Categories count 5->7 |

No ADRs flagged for supersession.

## Lessons
| Action | ID | Title |
|--------|----|-------|
| corrected | #4443 -> #4588 | Complete all struct/field changes before first compile (added bugfix-632 data point) |
| corrected | #4539 -> #4586 | Integration tests must use categories from current INITIAL_CATEGORIES allowlist |

No new lessons stored. The "incomplete fix" observation (config observability gap -> #635) is a normal backlog item, not a generalizable lesson about bugfix methodology.

## Retrospective Findings

### Hotspot-derived actions
- **compile_cycles (37 cycles)**: Root cause confirmed as iterative per-site edits across lockstep locations. Added to existing lesson #4588. The corrected pattern #4585 now explicitly lists all 5 lockstep sites to enable batch-editing.
- **cold_restart (106-min gap, 38 re-reads)**: Known session overhead. No new knowledge — existing patterns about file re-reading already captured.
- **search_via_bash (27.3%)**: Behavioral compliance issue, not architectural. No knowledge entry needed.
- **context_load (139KB before first write)**: Discovery-phase overhead for a config divergence bug. Not generalizable.

### Recommendation actions
- "Batch field additions before compiling" — already captured in #4588 with bugfix-632 added as data point.
- "Use run_in_background + TaskOutput instead of sleep polling" — operational advice, not architectural knowledge. Skipped.

### Outlier notes
- The 2-session split (discovery FAIL, fix FAIL, testing PASS) suggests the lockstep location list was not consulted during discovery. The corrected pattern #4585 should prevent recurrence by listing all sites explicitly.
