# Agent Report: nan-001-agent-2-spec

## Agent
Specification Writer (nan-001-agent-2-spec)

## Status
COMPLETE

## Output
- `/workspaces/unimatrix-nan-001/product/features/nan-001/specification/SPECIFICATION.md`

## Key Decisions

1. **JSON key = SQL column name**: The format contract uses exact SQL column names as JSON keys. No renaming, no camelCase conversion. This eliminates mapping ambiguity (SR-01) and makes the contract trivially verifiable against the schema DDL.

2. **JSON-string-in-JSON columns exported as strings, not inlined**: Columns like `capabilities`, `allowed_topics`, `allowed_categories`, and `target_ids` contain JSON-encoded arrays stored as TEXT. The spec mandates these are exported as JSON string values (e.g., `"capabilities": "[1,2]"`), not parsed and inlined as JSON arrays. This prevents double-encoding ambiguity and ensures the importer stores the exact same string back into SQLite.

3. **Transaction isolation required (FR-07)**: Following SR-07 from the risk assessment, the spec mandates a single `BEGIN DEFERRED` transaction wrapping all 8 table reads. Without this, concurrent writes from the MCP server could cause rows from different tables to reflect different logical states.

4. **Determinism defined precisely (NFR-03)**: Determinism means byte-identical output when `exported_at` is held constant. Achieved through fixed table order, fixed row ORDER BY clauses, and ordered JSON key serialization (struct field order or BTreeMap, not HashMap). This addresses SR-05.

5. **Float precision deferred to serde_json defaults (NFR-04)**: serde_json serializes f64 with up to 17 significant digits, which is sufficient for lossless IEEE 754 round-trip. No custom formatting needed. SR-03 is addressed by testing round-trip equality for edge-case floats.

6. **26 columns on entries table**: Verified against the actual DDL in `crates/unimatrix-store/src/db.rs`. The `pre_quarantine_status` column (added in v8 migration, vnc-010) is nullable INTEGER -- correctly mapped as `number or null`.

7. **Column list hardcoding vs shared definition**: SR-02/SR-04 recommend deriving export column lists from a shared definition. The spec notes this as a desirable architectural property but does not mandate it for v1. The architect decides the implementation strategy.

## Open Questions

1. **Atomic output file writes**: The spec explicitly excludes temp-file-then-rename for `--output` mode. If the export fails mid-write, a partial file remains. Should the architect add atomic writes, or is caller-side cleanup acceptable for v1?

2. **`exported_at` clock source**: The spec says Unix timestamp in seconds. Should this use `SystemTime::now()` (wall clock) or is there a project-standard timestamp utility? The hook and server both use `SystemTime` so this is likely fine.

3. **Entry count vs total row count**: The header includes `entry_count` (entries table only). Should additional counts be added (e.g., `tag_count`, `agent_count`) for validation, or is `entry_count` sufficient for the header? The scope specifies only `entry_count`, so the spec follows that.

## Self-Check

- [x] SPECIFICATION.md covers all acceptance criteria from SCOPE.md (AC-01 through AC-18)
- [x] Every functional requirement is testable
- [x] Non-functional requirements include measurable targets where possible
- [x] Domain Models section defines key terms
- [x] NOT in scope section is explicit
- [x] Output file is in `product/features/nan-001/specification/` only
- [x] No placeholder or TBD sections
- [x] Knowledge Stewardship report block included

## Knowledge Stewardship
- Queried: /query-patterns for export format contract -- no query performed (read-only tier, spec decisions are feature-specific per agent definition)
