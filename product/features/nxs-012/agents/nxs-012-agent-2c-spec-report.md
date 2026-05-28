# Agent Report: nxs-012-agent-2c-spec

## Task
Replace FR-18 through FR-25 (import-side quarantine filtering) with export-side functional requirements. Update NFRs, AC table, constraints, domain models, and user workflows.

## Changes Made

### FRs Replaced (FR-18 through FR-25 removed, FR-18 through FR-29 added)
- **FR-18**: `--skip-quarantined` CLI flag on export subcommand (was: import subcommand)
- **FR-19**: Skip-set construction via `SELECT id FROM entries WHERE status = 3` inside DEFERRED transaction (was: inline detection during ingest_rows)
- **FR-20**: Export-side filtering for entries (was: import-side entry skip)
- **FR-21**: Export-side filtering for entry_tags (was: import-side cascade skip)
- **FR-22**: Export-side filtering for feature_entries (was: import-side cascade skip)
- **FR-23**: Export-side filtering for co_access (was: import-side cascade skip)
- **FR-24**: Export-side filtering for graph_edges (was: import-side cascade skip)
- **FR-25**: observations and cycle_events NOT filtered (new -- explicit exclusion)
- **FR-26**: `--confirm` safeguard requirement (new)
- **FR-27**: Export summary reports skip counts (was FR-24 import summary)
- **FR-28**: Default export path unchanged (was FR-25 import default path)
- **FR-29**: Hash integrity preserved with --skip-quarantined (new)

### NFRs Updated
- **NFR-06**: Changed from "import file" to "database" as skip-set source; noted set is shared across table exporters
- **NFR-07**: Changed from "import time" to "export time"; references NFR-01 instead of NFR-02

### AC Table Updated
- AC-22 through AC-29: Rewritten for export-side semantics (emitted/not emitted vs inserted/not inserted)
- AC-30 added: --confirm safeguard (maps to FR-26)
- AC-31 added: Hash integrity (maps to FR-29)

### Constraints Updated
- Constraint 12: Replaced "entry-first ordering" (import-side) with "skip-set query inside snapshot transaction" (SR-02)
- Constraint 13: Replaced "skip filtering is import-only" with "consistent skip-set checking across 5 exporters" (SR-08)
- Constraint 14: Added "--confirm is a CLI flag, not interactive" (SR-09)

### Domain Models Updated
- "skip set" definition: changed from import-pass inline construction to pre-query inside DEFERRED transaction
- "quarantined" definition: changed from import-side filtering to export-side clean snapshot

### User Workflows Updated
- Export workflow: added `--skip-quarantined` and `--confirm` flags with full description
- Import workflow: removed `--skip-quarantined` flag, added "simple full-restore" clarification

## Key Decisions
1. FR numbering: FR-18 through FR-25 replaced with FR-18 through FR-29 (net +4 FRs) to cover new requirements (observations/cycle_events exclusion, --confirm safeguard, hash integrity, default path)
2. FR-25 explicitly states observations and cycle_events are NOT filtered -- this prevents implementers from incorrectly adding skip-set checks to those exporters
3. FR-27 specifies per-table skip counts (not just aggregate) for the export summary, providing better diagnostics

## Open Questions
1. SR-06 flags ADR-007 (#4614) as stale -- it describes import-side HashSet design. The architect should supersede or correct this ADR before designing the export-side implementation.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned 16 entries; #4614 (ADR-007 HashSet design) confirmed stale per SR-06; #4607 (feature scope) provided context; no other entries directly relevant to specification rewrite
