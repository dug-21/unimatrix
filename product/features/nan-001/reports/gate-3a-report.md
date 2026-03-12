# Gate 3a Report: nan-001

> Gate: 3a (Design Review)
> Date: 2026-03-11
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 3 components match architecture decomposition; ADR decisions followed |
| Specification coverage | PASS | All FR/NFR/AC covered; no scope additions |
| Risk coverage | PASS | All 15 risks mapped to test scenarios; priorities reflected |
| Interface consistency | PASS | Shared types consistent; data flow coherent across components |
| Knowledge stewardship compliance | FAIL | Architect report missing `## Knowledge Stewardship` section |

## Detailed Findings

### 1. Architecture Alignment
**Status**: PASS
**Evidence**:

- **Component boundaries**: Architecture defines 3 components (CLI extension, export module, row serialization). Pseudocode OVERVIEW.md lists the same 3 components in the same files. Each pseudocode file maps to exactly one architecture component.
- **Interfaces match contracts**: Architecture Integration Surface defines `run_export(project_dir: Option<&Path>, output: Option<&Path>) -> Result<(), Box<dyn std::error::Error>>`. Pseudocode `export-module.md` uses this exact signature. CLI pseudocode passes `cli.project_dir.as_deref()` and `output.as_deref()` matching the `Option<&Path>` parameters.
- **Technology choices follow ADRs**:
  - ADR-001 (snapshot isolation): Pseudocode shows `BEGIN DEFERRED` before all reads, `COMMIT` after -- exactly as specified.
  - ADR-002 (explicit column mapping): Pseudocode uses `serde_json::Map<String, Value>` with explicit per-column `row.get` extraction by index -- no Rust struct intermediary.
  - ADR-003 (deterministic key ordering): Pseudocode OVERVIEW.md specifies enabling `preserve_order` feature on serde_json; row-serialization.md inserts `_table` first then columns in SQL declaration order.
- **Internal functions match**: Architecture lists 9 internal functions (write_header + 8 per-table). Pseudocode `export-module.md` defines the same 9 plus a `do_export` helper and a `write_row` helper (both are implementation details consistent with architecture intent).
- **Data flow**: Architecture's component interaction diagram matches the pseudocode OVERVIEW data flow exactly (CLI parse -> run_export -> ensure_data_directory -> Store::open -> lock_conn -> BEGIN DEFERRED -> write_header -> 8 table exports -> COMMIT -> flush).

### 2. Specification Coverage
**Status**: PASS
**Evidence**:

- **FR-01 (CLI subcommand)**: cli-extension.md adds `Export { output: Option<PathBuf> }` to Command enum (FR-01.1), sync dispatch (FR-01.6), respects --project-dir (FR-01.4). Error propagation via `Result` for non-zero exit (FR-01.7).
- **FR-02 (JSONL header)**: export-module.md `write_header` emits `_header:true`, `schema_version`, `exported_at`, `entry_count`, `format_version:1` -- all fields per FR-02.1.
- **FR-03 (table row format)**: row-serialization.md inserts `_table` as first key on every non-header row. Column keys match SQL column names 1:1 (FR-03.2). No extra keys (FR-03.3).
- **FR-04 (table emission order)**: export-module.md `do_export` calls table functions in the exact order: counters, entries, entry_tags, co_access, feature_entries, outcome_index, agent_registry, audit_log.
- **FR-05 (row ordering)**: Each per-table function in row-serialization.md uses the correct ORDER BY clause matching FR-05.1 through FR-05.8.
- **FR-06 (empty table handling)**: Not explicitly pseudocoded but inherently satisfied -- SQL queries on empty tables return zero rows, which produces zero lines per table.
- **FR-07 (transaction isolation)**: export-module.md wraps all reads in BEGIN DEFERRED / COMMIT.
- **FR-08 (excluded tables)**: Only the 8 specified tables have export functions; no excluded tables are queried.
- **FR-09 (implementation location)**: Pseudocode targets `crates/unimatrix-server/src/export.rs` with public `run_export`.
- **NFR-01 (performance)**: Streaming row-by-row writes with BufWriter; no full-table buffering (NFR-02 memory). Implementation note in row-serialization.md recommends `query` + `while let` for true streaming.
- **NFR-03 (determinism)**: Fixed table order, fixed row ordering, fixed key ordering via preserve_order.
- **NFR-04 (float precision)**: row-serialization.md uses `Number::from_f64(confidence).unwrap()` per serde_json/ryu.
- **NFR-05 (error handling)**: All errors propagate via `Box<dyn Error>` with `?` operator throughout.
- **NFR-06 (compatibility)**: Export module is self-contained. Cargo.toml change is `preserve_order` feature only.
- **NFR-07 (no new deps)**: OVERVIEW confirms only `preserve_order` feature addition to existing serde_json.
- **No scope additions**: Pseudocode implements exactly what is specified. No extra features, no extra tables, no extra fields.

### 3. Risk Coverage
**Status**: PASS
**Evidence**:

All 15 risks from RISK-TEST-STRATEGY.md are mapped to test scenarios in the test plans:

| Risk | Priority | Test(s) | Coverage |
|------|----------|---------|----------|
| R-01 (column list divergence) | Critical | T-RS-01, T-RS-02, T-RS-03 | All 26 entry columns verified; PRAGMA cross-check; per-table key count |
| R-02 (f64 precision) | High | T-RS-04 | 5 edge-case values with bitwise round-trip |
| R-03 (JSON-in-TEXT double encoding) | Critical | T-RS-05 | All 4 JSON-in-TEXT columns tested with non-trivial content and nulls |
| R-04 (NULL encoding) | Critical | T-RS-06, T-RS-06b | All nullable columns tested; empty-string-vs-null distinction |
| R-05 (transaction isolation) | Critical | T-EM-01, T-EM-02 | Code review assertion + behavioral consistency test |
| R-06 (key ordering) | High | T-RS-07, T-EM-03 | Raw string key order verification + byte-identical repeated export |
| R-07 (excluded table leakage) | Medium | T-EM-04 | Excluded tables populated then verified absent from output |
| R-08 (row ordering) | High | T-EM-05 | Multi-table ordering with out-of-order inserts |
| R-09 (migration side-effect) | Medium | T-EM-06 | File modification time check after export |
| R-10 (partial output on error) | High | T-EM-07, T-CL-04 | Failing writer mock + non-writable path test |
| R-11 (preserve_order regression) | Medium | T-RS-08 | Full test suite regression check |
| R-12 (empty database) | Medium | T-EM-08 | Fresh database export validated |
| R-13 (unicode) | Medium | T-RS-09 | CJK, emoji, combining chars, newlines, JSON-special chars |
| R-14 (large integers) | Medium | T-RS-10 | i64::MAX counter, i32::MAX version, large timestamps |
| R-15 (--project-dir wiring) | Medium | T-CL-03 | Two-database discrimination test |

Risk priorities are reflected in test emphasis: Critical risks (R-01, R-03, R-04, R-05) have the most thorough test scenarios with multiple sub-assertions. High risks (R-02, R-06, R-08, R-10) have dedicated tests with edge-case coverage. Medium risks have at least one targeted test each.

The test plan OVERVIEW.md provides a complete risk-to-test mapping table and an acceptance-criteria-to-test mapping table covering all 18 ACs.

### 4. Interface Consistency
**Status**: PASS
**Evidence**:

- **Shared types**: OVERVIEW.md lists `serde_json::Map<String, Value>`, `rusqlite::Connection`, `rusqlite::Row` as shared types. All three pseudocode files use these consistently.
- **run_export signature**: cli-extension.md calls `run_export(cli.project_dir.as_deref(), output.as_deref())` which matches the `run_export(project_dir: Option<&Path>, output: Option<&Path>)` signature in export-module.md.
- **Per-table function signatures**: All 8 per-table functions plus `write_header` take `(conn: &Connection, writer: &mut impl Write)` consistently.
- **Write helper**: `write_row(map, writer)` takes `serde_json::Map<String, Value>` and `&mut impl Write` -- used consistently across all per-table functions in row-serialization.md.
- **Data flow coherent**: cli-extension passes `project_dir` and `output` to `run_export`. run_export resolves paths, opens store, acquires connection, passes `&Connection` and `&mut impl Write` to all per-table functions. No contradictions between component pseudocode files.
- **Module registration**: OVERVIEW.md notes `pub mod export;` in `lib.rs`, cli-extension.md uses `unimatrix_server::export::run_export` -- consistent.

### 5. Knowledge Stewardship Compliance
**Status**: FAIL
**Evidence**:

Design-phase agents and their stewardship status:

| Agent Report | Has `## Knowledge Stewardship`? | Content |
|-------------|------|---------|
| nan-001-agent-1-architect-report.md | NO | Has `## Unimatrix Knowledge Storage` instead -- does not conform to required format |
| nan-001-agent-1-pseudocode-report.md | YES | `Queried:` entries present (codebase pattern reads). No `Stored:` or `Declined:` entry. |
| nan-001-agent-2-testplan-report.md | YES | `Queried:` attempted but MCP unavailable. `Stored:` "nothing novel to store" with reason. |
| nan-001-agent-3-risk-report.md | YES | `Queried:` attempted but MCP unavailable. `Stored:` "nothing novel to store" with reason. |
| nan-001-agent-0-scope-risk-report.md | YES | `Queried:` not applicable. `Stored:` "nothing novel to store" with reason. |

**Issue**: The architect report (`nan-001-agent-1-architect-report.md`) is missing the required `## Knowledge Stewardship` section. It has a `## Unimatrix Knowledge Storage` section that discusses ADR storage but does not follow the required format with `Stored:` or `Declined:` entries. As an active-storage agent (architect), this report must have a conformant section.

**Secondary concern (WARN-level)**: The pseudocode report has a `Queried:` entry but no `Stored:` or "nothing novel to store" entry. As a read-only agent, the minimum requirement is `Queried:` entries, which are present. The missing `Stored:` line is not required for read-only agents per the gate check definition.

## Rework Required (REWORKABLE FAIL)

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| Missing `## Knowledge Stewardship` section in architect report | nan-001-agent-1-architect (or coordinator) | Add a `## Knowledge Stewardship` section to `product/features/nan-001/agents/nan-001-agent-1-architect-report.md` with conformant `Stored:` or `Declined:` entries. The existing `## Unimatrix Knowledge Storage` section can remain but does not substitute for the required format. The section should note that ADR storage was declined due to MCP tools being unavailable, e.g.: `Stored: Declined -- MCP tools unavailable in agent session; ADR files on disk for later ingestion` |
