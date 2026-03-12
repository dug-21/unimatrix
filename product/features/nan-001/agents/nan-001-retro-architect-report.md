# nan-001 Retrospective Architect Report

Agent: nan-001-retro-architect
Mode: retrospective (post-ship knowledge extraction)

## 1. Pattern Extraction

### New Patterns Stored

| ID | Title | Rationale |
|----|-------|-----------|
| #1102 | Sync CLI Subcommand Pattern for unimatrix-server | No existing pattern covered the sync subcommand dispatch path (Command enum variant, sync match arm, peer module). Two validated instances (hook, export) confirm this is a stable pattern. |
| #1103 | Explicit SQL-to-JSONL Row Serialization Pattern | Distinct from #343 (redb JSON-Lines migration). This pattern covers SQLite-to-JSONL with explicit column mapping, type rules, preserve_order, and format versioning. Directly reusable by nan-002 (import). |

### Skipped Patterns

| Candidate | Reason |
|-----------|--------|
| `&mut impl Write` for testable I/O | Standard Rust idiom, not project-specific. Every Rust developer knows this. Storing it would be noise. |

## 2. Procedure Extraction

### New Procedures Stored

| ID | Title | Rationale |
|----|-------|-----------|
| #1104 | Procedure: Adding a Sync CLI Subcommand to unimatrix-server | No existing procedure. #323 covers adding a ServiceLayer service (async MCP tool), not a CLI subcommand. This 5-step procedure covers the full path from Command variant to module registration to testing. |

### Skipped Procedures

| Candidate | Reason |
|-----------|--------|
| preserve_order feature flag usage | Not a procedure -- it is a one-time Cargo.toml change documented in ADR-003 (#1099). No repeated steps. |

## 3. ADR Validation

All 3 ADRs were validated by successful implementation.

| ADR | ID | Status | Notes |
|-----|-----|--------|-------|
| ADR-001: Snapshot Isolation | #1097 | Validated | BEGIN DEFERRED at line 40, COMMIT at line 56. Cross-table consistency verified in integration tests. |
| ADR-002: Explicit Column Mapping | #1098 | Validated | All 8 per-table functions use serde_json::Map with explicit column insertion. No Rust struct intermediary. |
| ADR-003: Deterministic Key Ordering | #1099 | Validated | preserve_order enabled in Cargo.toml. No regressions in 2164 workspace tests. Determinism verified by byte-identical 3-run integration test. |

### Deviations

One minor deviation from pseudocode noted by gate 3b:

- **NaN fallback**: Implementation uses `Number::from_f64(confidence).unwrap_or(Number::from(0))` instead of pseudocode's `.unwrap()`. This silently maps NaN to 0 rather than panicking. Practical impact is nil (confidence is constrained to [0.0, 1.0]), but it is a correctness deviation -- a backup/export tool should arguably fail loudly on corrupt data rather than silently substituting. Not severe enough to warrant an ADR correction or lesson.

## 4. Lesson Extraction

### Skipped

| Candidate | Reason |
|-----------|--------|
| Gate 3a failed on wrong stewardship heading | Not a generalizable lesson. base-004 ADR-002 (#1005) already documents the structured block format. The nan-001 failure was expected first-enforcement friction -- the architect agent used `## Unimatrix Knowledge Storage` instead of the required `## Knowledge Stewardship`. This was fixed in minutes. The ADR and pattern (#1010) already exist. Storing a duplicate lesson adds noise. |

## 5. Retrospective Findings

1. **Clean execution**: nan-001 shipped with 1 reworkable gate failure (heading format) and 0 rework commits. All 3 ADRs validated on first implementation attempt. 49 tests (33 unit + 16 integration), all passing.

2. **MCP unavailability in worktrees**: Multiple delivery agents noted that the Unimatrix MCP server was not available in worktrees, preventing runtime pattern queries and storage during implementation. This is a known infrastructure limitation (agents noted it but were not blocked). No new knowledge to store -- the limitation is documented.

3. **nan-002 readiness**: The export format contract (ARCHITECTURE.md JSONL Format Contract section, ADR-002, and pattern #1103) provides a complete specification for the import side. nan-002 can consume the format_version 1 contract without ambiguity.

4. **File length boundary**: export.rs is 500 lines production + 899 lines test = 1399 total. The 500-line production code sits exactly at the conventional limit. For nan-002, if import logic is similarly sized, consider whether the module should be split (e.g., `import.rs` + `import/tables.rs`) or if the same inline-test pattern is acceptable.

## Knowledge Stewardship

- Queried: 6 searches across pattern/procedure/convention/lesson-learned categories. Reviewed entries #316, #320, #323, #343, #1005, #1010, #1097, #1098, #1099.
- Stored: #1102 (sync CLI subcommand pattern), #1103 (SQL-to-JSONL serialization pattern), #1104 (subcommand procedure).
- Declined: `impl Write` testable I/O (standard Rust idiom, not project-specific). Gate heading lesson (already covered by #1005 and #1010).
