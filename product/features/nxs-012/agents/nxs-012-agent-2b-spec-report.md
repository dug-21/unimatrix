# Agent Report: nxs-012-agent-2b-spec

## Task
Supplement SPECIFICATION.md with coverage for `--skip-quarantined` import CLI flag (Goal #9, AC-22 through AC-29).

## Changes Made

Updated `/workspaces/unimatrix/product/features/nxs-012/specification/SPECIFICATION.md`:

### New Functional Requirements (FR-18 through FR-25)
- **FR-18**: `--skip-quarantined` CLI flag definition (boolean, default false)
- **FR-19**: Entry skip set construction — `HashSet<i64>` built from entry rows with status=3
- **FR-20**: Cascade skip for `entry_tags` rows (check `entry_id` against skip set)
- **FR-21**: Cascade skip for `feature_entries` rows (check `entry_id` against skip set)
- **FR-22**: Cascade skip for `co_access` rows (check both `entry_a` and `entry_b`)
- **FR-23**: Cascade skip for `graph_edges` rows (check both `source_id` and `target_id`)
- **FR-24**: Skip count reporting in import summary (skipped entries + skipped dependent rows)
- **FR-25**: No behavioral change when flag is absent

### New Acceptance Criteria (AC-22 through AC-29)
All 8 ACs from SCOPE.md mapped to FRs with verification methods.

### New Non-Functional Requirements
- **NFR-06**: Skip set memory overhead (under 100KB for typical databases)
- **NFR-07**: Skip filtering performance (O(1) lookup, negligible overhead)

### New Constraints
- **Constraint 12**: Skip set requires entry-first ordering (depends on existing export ordering invariant)
- **Constraint 13**: Skip filtering is import-only (export remains lossless)

### Domain Model Updates
- Added **skip set** definition
- Added **quarantined (status=3)** definition

### User Workflows Update
- Updated CLI Import usage to show `--skip-quarantined` flag
- Added paragraph describing skip set construction and cascade behavior

### NOT in Scope Update
- Removed "New CLI flags or subcommands" bullet (now contradicted by `--skip-quarantined`)

## Key Decisions
1. FR-24 reports aggregate skipped dependent rows rather than per-table skip counts — keeps the summary concise while still communicating the impact
2. FR-22 and FR-23 explicitly require both-side checks for symmetric/directed relationships — prevents dangling references
3. Constraint 12 documents the ordering dependency rather than adding explicit enforcement — the existing export ordering already guarantees entry-first

## Open Questions
None. All requirements are fully derivable from SCOPE.md Goal #9 and AC-22 through AC-29.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned 17 entries; relevant: ADR for quarantine threshold (#2716), quarantine status storage pattern (#600), entry restore obligations (#3764). No new patterns to store.
