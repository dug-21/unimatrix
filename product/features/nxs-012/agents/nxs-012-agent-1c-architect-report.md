# Agent Report: nxs-012-agent-1c-architect

## Task
Supersede ADR-007 (import-side skip-quarantined) with export-side design. Add ADR-008 (export-side filter) and ADR-009 (--confirm safeguard). Update ARCHITECTURE.md.

## Completed

1. **ADR-007 superseded**: File updated with SUPERSEDED notice. Unimatrix entry #4614 deprecated with reason referencing ADR-008.

2. **ADR-008 written**: Export-side skip-quarantined filter via pre-query HashSet. Stored as Unimatrix #4615. Key design: `SELECT id FROM entries WHERE status = 3` inside BEGIN DEFERRED transaction, before any export_* call. HashSet passed to 5 affected table exporters. Empty HashSet when flag is off (zero overhead on default path).

3. **ADR-009 written**: --confirm safeguard. Stored as Unimatrix #4616. Key design: --skip-quarantined without --confirm aborts with clear error message. No interactive prompts (consistent with nan-002 ADR-003 precedent). --confirm without --skip-quarantined is silently ignored.

4. **ARCHITECTURE.md updated**: Replaced C5 (import-side) with export-side design. Updated component interactions diagram, technology decisions table, integration surface table. Import signatures unchanged (no skip_quarantined parameter on import). Export signatures gain skip_ids, skip_quarantined, and confirm parameters.

## Files Modified
- `product/features/nxs-012/architecture/ARCHITECTURE.md` (updated)
- `product/features/nxs-012/architecture/ADR-007-skip-quarantined-cascade-design.md` (superseded)
- `product/features/nxs-012/architecture/ADR-008-export-side-skip-quarantined.md` (new)
- `product/features/nxs-012/architecture/ADR-009-confirm-safeguard-for-skip-quarantined.md` (new)

## Unimatrix Actions
- Deprecated: #4614 (ADR-007, import-side design)
- Stored: #4615 (ADR-008, export-side design)
- Stored: #4616 (ADR-009, --confirm safeguard)

## Key Design Decisions
- Skip-set query runs INSIDE the BEGIN DEFERRED snapshot transaction (SR-02)
- 5 table exporters receive skip_ids; 6 are unaffected (SR-08)
- Import is unchanged -- no skip_quarantined parameter, no conditional inserts
- --confirm is a CLI flag, not interactive prompt (SR-09, nan-002 ADR-003 precedent)
- Export header includes skip_quarantined metadata when active

## Open Questions
None.
