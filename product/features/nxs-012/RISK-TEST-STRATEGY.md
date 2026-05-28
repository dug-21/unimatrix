# Risk-Based Test Strategy: nxs-012

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | graph_edges.weight NaN/Infinity corrupts JSONL — Number::from_f64 returns None, causing serialization panic or data loss | High | Med | High |
| R-02 | drop_all_data FK-cascade ordering wrong — observation_phase_metrics or observation_metrics left behind, producing stale aggregates after --force import | High | Med | High |
| R-03 | graph_edges duplicate (source_id, target_id, relation_type) in export file causes UNIQUE constraint violation on import — undetected data corruption | High | Low | Med |
| R-04 | format_version validation accepts unexpected values — v0 or v3+ import proceeds silently with missing or unknown table types | High | Low | Med |
| R-05 | observations.id or cycle_events.id collision on import — explicit INSERT with preserved IDs fails if target DB already has rows (non-force import) | Med | Med | Med |
| R-06 | CycleEventRow deserialization fails when goal_embedding field appears in a hand-edited or future export file | Med | Low | Low |
| R-07 | graph_edges with source_id/target_id referencing non-existent entries after import — dangling edge references | Med | Low | Low |
| R-08 | Export ordering non-determinism — graph_edges ORDER BY (source_id, target_id, relation_type) not enforced, round-trip diff fails | Med | Med | Med |
| R-09 | observations.input contains embedded newlines or unescaped JSON — JSONL line parsing breaks on import | Med | Med | Med |
| R-10 | graph_edges.metadata nullable TEXT with embedded JSON — null vs empty string mismatch through serialization round-trip | Med | Med | Med |
| R-11 | New ExportRow variants cause old binaries to panic on unknown _table tag instead of clean error — format_version guard is the only protection | Med | Low | Low |
| R-12 | record_provenance audit entry omits new table counts — import appears to have lost data in audit trail | Low | Med | Low |
| R-13 | print_summary missing new table lines — user sees no confirmation that new tables were imported | Low | Med | Low |
| R-14 | Transaction isolation gap — new export queries not inside the BEGIN DEFERRED snapshot, producing inconsistent cross-table state | High | Low | Med |
| R-15 | goal_embedding NULL post-import causes context_briefing crash instead of graceful degradation | High | Low | Med |
| R-16 | Export-side cascade incompleteness — one of the 5 entry-referencing exporters misses the skip_ids.contains() check, producing orphaned rows in the export file | High | Med | High |
| R-17 | Skip-set query runs outside DEFERRED snapshot — TOCTOU race between skip-set construction and table export passes | High | Med | High |
| R-18 | Default path regression — skip_quarantined=false allocates HashSet, performs status queries, or alters export behavior, violating AC-29 zero-change guarantee | High | Low | Med |
| R-19 | co_access dual-column check incomplete — only entry_id_a or entry_id_b checked against skip_ids, orphaned co_access row exported | High | Med | High |
| R-20 | graph_edges dual-column check incomplete — only source_id or target_id checked against skip_ids, orphaned edge exported | High | Med | High |
| R-21 | Non-entry-referencing tables incorrectly filtered — observations, cycle_events, counters, audit_log rows omitted by skip_ids check that should not exist | Med | Low | Low |
| R-22 | Export skip-count reporting incorrect or missing — user has no visibility into how many rows were filtered (AC-28) | Low | Med | Low |
| R-23 | --confirm safeguard bypass — --skip-quarantined proceeds without --confirm, producing a filtered export the user did not intend | High | Low | Med |
| R-24 | Export header missing skip_quarantined metadata — downstream consumers cannot distinguish filtered from full exports | Low | Med | Low |

## Risk-to-Scenario Mapping

### R-01: graph_edges.weight NaN/Infinity corrupts JSONL
**Severity**: High
**Likelihood**: Med
**Impact**: Export produces invalid JSON (NaN literal not valid in JSON spec), import fails to parse, or data silently lost. Historical evidence: patterns #4133 and #4533 document recurring NaN safety issues across the codebase.

**Test Scenarios**:
1. Export a graph_edge with weight = f64::NAN — verify output contains fallback value (per ADR-003), not NaN literal
2. Export a graph_edge with weight = f64::INFINITY — verify fallback
3. Export a graph_edge with weight = f64::NEG_INFINITY — verify fallback
4. Export a graph_edge with weight = 0.7777777 — verify full f64 precision preserved (no f32 truncation)
5. Round-trip: insert NaN weight, export, import — verify imported weight is the fallback value

**Coverage Requirement**: All three non-finite f64 variants tested. Normal precision preserved.

### R-02: drop_all_data FK-cascade ordering wrong
**Severity**: High
**Likelihood**: Med
**Impact**: --force import leaves stale observation_metrics / observation_phase_metrics rows that produce incorrect phase affinity scores in context_briefing. User sees no error — data is silently wrong. Flagged as SR-07.

**Test Scenarios**:
1. Populate observation_phase_metrics, observation_metrics, observations, graph_edges, cycle_events. Run --force import. Verify all 5 tables empty before ingestion begins.
2. Populate only observation_phase_metrics + observation_metrics (no observations). Run --force import. Verify derived tables cleared even when parent observations is empty.
3. Verify DELETE ordering: observation_phase_metrics before observation_metrics (FK constraint would fail otherwise if PRAGMA foreign_keys ON).

**Coverage Requirement**: Both derived metric tables confirmed empty after drop_all_data, regardless of PRAGMA foreign_keys state.

### R-03: graph_edges duplicate natural key in export file
**Severity**: High
**Likelihood**: Low
**Impact**: Import fails mid-transaction with UNIQUE constraint violation. Data corruption in export file is surfaced rather than silently masked (intended behavior per FR-09).

**Test Scenarios**:
1. Craft export file with two graph_edges rows sharing (source_id=1, target_id=2, relation_type="Supports"). Import. Verify UNIQUE constraint error.
2. Verify the entire import transaction is rolled back (no partial state).

**Coverage Requirement**: Plain INSERT (not INSERT OR IGNORE) confirmed. Transaction rollback on constraint violation confirmed.

### R-04: format_version validation boundary
**Severity**: High
**Likelihood**: Low
**Impact**: Importing a future format (v3+) could produce silent data loss (unknown _table types skipped). Importing v0 could indicate a corrupted file.

**Test Scenarios**:
1. Import file with format_version: 0 — verify error with version number in message
2. Import file with format_version: 1 — verify success, 0 counts for new tables
3. Import file with format_version: 2 — verify success, all 11 table types processed
4. Import file with format_version: 3 — verify error with version number and supported range in message
5. Import file with format_version: 999 — verify error (boundary)

**Coverage Requirement**: All boundary values tested: 0 (reject), 1 (accept-legacy), 2 (accept-current), 3 (reject-future).

### R-05: Preserved ID collision on non-force import
**Severity**: Med
**Likelihood**: Med
**Impact**: Importing into a non-empty database without --force, explicit INSERT with preserved observations.id or cycle_events.id hits PRIMARY KEY constraint. Import fails mid-transaction.

**Test Scenarios**:
1. Populate observations with id=1. Import file containing observations id=1 without --force. Verify PRIMARY KEY constraint error.
2. Verify transaction rollback leaves original data intact.
3. Same scenario with --force: verify drop_all_data clears table first, import succeeds.

**Coverage Requirement**: Both force and non-force paths tested for ID collision.

### R-06: CycleEventRow deserialization with unexpected goal_embedding field
**Severity**: Med
**Likelihood**: Low
**Impact**: If a hand-edited export file includes a goal_embedding key in a cycle_events row, deserialization could fail because CycleEventRow has no such field. Depends on whether serde denies unknown fields.

**Test Scenarios**:
1. Deserialize a cycle_events JSON line that includes `"goal_embedding": null` — verify it either succeeds (ignored) or fails with a clear error.
2. Deserialize a cycle_events JSON line with no goal_embedding key — verify success.

**Coverage Requirement**: Confirm serde behavior for unknown fields on CycleEventRow.

### R-07: Dangling graph_edges after import
**Severity**: Med
**Likelihood**: Low
**Impact**: graph_edges.source_id or target_id references an entry ID that does not exist post-import. No FK constraint enforces this. Graph queries return edges pointing to nothing.

**Test Scenarios**:
1. Export a DB with entries [1,2] and edge (1->2). Import. Verify edge is valid.
2. Craft export file with edge (1->999) where entry 999 does not exist. Import succeeds (no FK). Edge exists but target is dangling.

**Coverage Requirement**: Round-trip confirms entry IDs match edge references. Dangling edge case documented as accepted behavior.

### R-08: Export ordering non-determinism
**Severity**: Med
**Likelihood**: Med
**Impact**: Round-trip test (AC-15) fails because export order is not deterministic.

**Test Scenarios**:
1. Insert graph_edges in random order. Export. Verify output sorted by (source_id, target_id, relation_type).
2. Insert observations with non-sequential IDs. Export. Verify output sorted by id.
3. Insert cycle_events with non-sequential IDs. Export. Verify output sorted by id.
4. Full round-trip: export, import into fresh DB, re-export, byte-compare (excluding exported_at).

**Coverage Requirement**: Each table's ordering verified with deliberately unordered input data.

### R-09: observations.input with embedded newlines breaks JSONL
**Severity**: Med
**Likelihood**: Med
**Impact**: JSONL format requires one JSON object per line. If observations.input contains literal newlines, the serialized line could span multiple lines, breaking the line-by-line parser on import.

**Test Scenarios**:
1. Insert observation with input containing literal newline characters. Export. Verify the JSONL line is a single line (newlines escaped).
2. Import the exported file. Verify the observation.input is faithfully preserved.

**Coverage Requirement**: Round-trip preservation of newline-containing TEXT fields.

### R-10: graph_edges.metadata null vs empty string round-trip
**Severity**: Med
**Likelihood**: Med
**Impact**: SQL NULL metadata should serialize as JSON null and deserialize back as None. If nullable_text maps empty string to null or vice versa, metadata semantics change through round-trip. Pattern #1161 documents this contract.

**Test Scenarios**:
1. Export edge with metadata = NULL. Verify JSON null. Import. Verify SQL NULL.
2. Export edge with metadata = "" (empty string). Verify JSON "". Import. Verify empty string (not NULL).
3. Export edge with metadata = '{"nli_score": 0.8}'. Verify preserved. Import. Verify exact match.

**Coverage Requirement**: null, empty string, and populated string all round-trip correctly.

### R-11: Old binary encounters new ExportRow variants
**Severity**: Med
**Likelihood**: Low
**Impact**: Old binary (format_version 1 only) attempts to import a v2 file. Should reject at format_version validation, never reaching the unknown _table tag.

**Test Scenarios**:
1. Verify format_version 2 rejected by logic that only accepts v1 (simulate old binary behavior).
2. Verify serde error message if an unknown _table value reaches deserialization.

**Coverage Requirement**: Format version guard is the primary defense. Serde error is the fallback.

### R-12: record_provenance omits new table counts
**Severity**: Low
**Likelihood**: Med
**Impact**: Audit trail for import is incomplete — counts for graph_edges, observations, cycle_events missing from provenance detail string.

**Test Scenarios**:
1. Import v2 file. Query audit_log for import provenance event. Verify detail string includes counts for all 3 new tables.

**Coverage Requirement**: Provenance string includes all 11 table counts.

### R-13: print_summary missing new table lines
**Severity**: Low
**Likelihood**: Med
**Impact**: User sees import complete but no confirmation that new tables were imported.

**Test Scenarios**:
1. Import v2 file. Capture stderr. Verify lines for graph_edges, observations, cycle_events with correct counts.
2. Import v1 file. Verify 0 counts displayed for new tables.

**Coverage Requirement**: Summary output includes all 3 new tables for both v1 and v2 imports.

### R-14: Transaction isolation gap for new export queries
**Severity**: High
**Likelihood**: Low
**Impact**: If new export functions run outside the BEGIN DEFERRED transaction, concurrent writes could produce an inconsistent snapshot.

**Test Scenarios**:
1. Verify new export function calls are placed inside the existing do_export transaction scope (code review).
2. Integration test: export while a concurrent write modifies data — verify exported state is internally consistent.

**Coverage Requirement**: Code-level verification that all 11 export queries share the same transaction.

### R-15: NULL goal_embedding causes context_briefing crash
**Severity**: High
**Likelihood**: Low
**Impact**: After import, all cycle_events.goal_embedding values are NULL. If context_briefing dereferences the BLOB without null-checking, it panics.

**Test Scenarios**:
1. Set all cycle_events.goal_embedding to NULL. Call context_briefing. Verify it returns valid results (neutral affinity, no panic).
2. Verify goal-cluster scoring code path handles Option::None for goal_embedding.

**Coverage Requirement**: context_briefing operates correctly with 100% NULL goal_embeddings.

### R-16: Export-side cascade incompleteness — missed skip_ids check in one of 5 exporters
**Severity**: High
**Likelihood**: Med
**Impact**: One of the 5 entry-referencing table exporters (`export_entries`, `export_entry_tags`, `export_feature_entries`, `export_co_access`, `export_graph_edges`) omits the `skip_ids.contains()` check. The export file contains orphaned rows that reference quarantined entry IDs which were excluded from the entries section. Import succeeds (no FK constraint), but the imported database has dangling references — graph traversal, co-access scoring, and tag queries silently produce incorrect results. Pattern #3910 documents this exact class of bug: inconsistent filtering across passes on the same reference table produces ghost records that survive indefinitely.

**Test Scenarios**:
1. Populate DB with 2 quarantined entries, each having entry_tags, feature_entries, co_access, and graph_edges rows. Export with `--skip-quarantined --confirm`. Verify zero rows in the export file reference any quarantined entry ID across all 5 affected table types.
2. Verify non-quarantined entries and all their dependents are fully preserved (no over-filtering).
3. Round-trip: export with `--skip-quarantined --confirm`, import into fresh DB, query all 5 tables — confirm no ID from the original quarantine set appears anywhere.
4. Code-level audit: confirm all 5 affected export functions accept `skip_ids: &HashSet<i64>` and invoke `skip_ids.contains()` on the correct column(s).

**Coverage Requirement**: All 5 affected table types verified for both skip and pass-through behavior in a single integrated test. This is the critical test for the entire --skip-quarantined feature.

### R-17: Skip-set query runs outside DEFERRED snapshot — TOCTOU race
**Severity**: High
**Likelihood**: Med
**Impact**: The `SELECT id FROM entries WHERE status = 3` query that builds the skip set executes outside the `BEGIN DEFERRED` snapshot transaction. A concurrent operation quarantines an entry between the skip-set query and the `export_entries` pass. The skip set contains the entry ID, so entries omits it — but dependent tables were read under a different snapshot where the entry was still active, so their rows reference the now-skipped entry. The export file has orphaned dependents. ADR-008 explicitly requires the skip-set query inside the DEFERRED transaction (SR-02).

**Test Scenarios**:
1. Code-level verification: confirm the `SELECT id FROM entries WHERE status = 3` query executes after `BEGIN DEFERRED` and before any `export_*` call within `do_export`.
2. Verify the skip-set HashSet is built from the same `&SqlitePool` / connection as all table export queries.
3. Integration test: export with `--skip-quarantined --confirm` on a DB with quarantined entries — verify the export file is internally consistent (no orphaned references).

**Coverage Requirement**: Transaction ordering verified at code level. Integration test confirms consistency.

### R-18: Default path regression — skip_quarantined=false alters export behavior
**Severity**: High
**Likelihood**: Low
**Impact**: When `--skip-quarantined` is absent, the export must produce identical output to pre-feature behavior (AC-29). If the implementation unconditionally runs the `SELECT id FROM entries WHERE status = 3` query, or adds `skip_ids.contains()` checks that alter row emission for an empty set in an unexpected way, the default path changes. ADR-008 specifies: empty HashSet, `contains()` is O(1) no-op.

**Test Scenarios**:
1. Export a DB with quarantined entries WITHOUT `--skip-quarantined`. Verify all entries (including status=3) are present in the export file.
2. Verify all dependent rows for quarantined entries are present in the export file.
3. Compare byte-for-byte (excluding exported_at) with an export from the pre-feature code path if feasible, or verify row counts match `SELECT COUNT(*)` for each table.

**Coverage Requirement**: Default path produces identical output — quarantined entries and all dependents exported.

### R-19: co_access dual-column check — only one side checked against skip_ids
**Severity**: High
**Likelihood**: Med
**Impact**: co_access rows have `entry_id_a` and `entry_id_b`. If only one column is checked against skip_ids, a co_access row where the unchecked side references a quarantined entry is exported. The exported file contains an orphaned co_access pair. Lesson #4536 documents how status guard correctness is invisible without explicit tests for each column variant.

**Test Scenarios**:
1. co_access row where entry_id_a is quarantined, entry_id_b is not — verify omitted from export.
2. co_access row where entry_id_b is quarantined, entry_id_a is not — verify omitted from export.
3. co_access row where both are quarantined — verify omitted.
4. co_access row where neither is quarantined — verify present in export.

**Coverage Requirement**: All 4 combinations tested explicitly. This is the FR-23 verification matrix.

### R-20: graph_edges dual-column check — only one side checked against skip_ids
**Severity**: High
**Likelihood**: Med
**Impact**: graph_edges rows have `source_id` and `target_id`. If only `source_id` is checked, an edge pointing TO a quarantined entry is exported. Graph traversal in the imported DB follows edges to non-existent entries, producing empty results or panics in downstream algorithms (PPR, typed relation graph).

**Test Scenarios**:
1. graph_edges row where source_id is quarantined, target_id is not — verify omitted from export.
2. graph_edges row where target_id is quarantined, source_id is not — verify omitted from export.
3. graph_edges row where both are quarantined — verify omitted.
4. graph_edges row where neither is quarantined — verify present in export.

**Coverage Requirement**: All 4 combinations tested explicitly. This is the FR-24 verification matrix.

### R-21: Non-entry-referencing tables incorrectly filtered by skip_ids
**Severity**: Med
**Likelihood**: Low
**Impact**: Table exporters that do NOT reference entry IDs (counters, outcome_index, agent_registry, audit_log, observations, cycle_events) should be unaffected by `--skip-quarantined`. If a developer mistakenly passes skip_ids to these exporters or adds a `contains()` check, rows are incorrectly omitted. audit_log is especially critical: ADR-008 states "audit_log rows are NOT filtered even when their metadata references quarantined entry IDs."

**Test Scenarios**:
1. Export with `--skip-quarantined --confirm`. Verify counters, outcome_index, agent_registry, audit_log row counts match `SELECT COUNT(*)` from the source DB.
2. Verify observations and cycle_events row counts match the source DB exactly.
3. Verify an audit_log row whose detail text mentions a quarantined entry ID is still exported.

**Coverage Requirement**: All 6 unaffected table types confirmed at full count.

### R-22: Export skip-count reporting incorrect or missing
**Severity**: Low
**Likelihood**: Med
**Impact**: Export with `--skip-quarantined` completes but the stderr summary does not report how many entries and dependent rows were omitted (AC-28). User has no visibility into the filtering that occurred.

**Test Scenarios**:
1. Export with `--skip-quarantined --confirm`. Capture stderr. Verify a line reporting skipped entry count.
2. Verify per-table skipped dependent row counts are reported.
3. Export WITHOUT `--skip-quarantined`. Verify no skip-related lines in summary output.

**Coverage Requirement**: Skip counts present when flag active, absent when flag inactive.

### R-23: --confirm safeguard bypass — --skip-quarantined proceeds without --confirm
**Severity**: High
**Likelihood**: Low
**Impact**: `--skip-quarantined` runs without `--confirm`, producing a filtered export the user did not explicitly acknowledge. The export appears to be a full backup but is missing quarantined entries and dependents. If the user later restores from this export without the original database, data is permanently lost. ADR-009 requires immediate abort with a clear error message.

**Test Scenarios**:
1. Run export with `--skip-quarantined` but no `--confirm`. Verify non-zero exit code and error message mentioning `--confirm`.
2. Verify no output file is created (or is empty) — the abort happens before any DB access.
3. Run with `--skip-quarantined --confirm`. Verify export proceeds normally.
4. Run with `--confirm` alone (no `--skip-quarantined`). Verify `--confirm` is silently ignored — export produces a full (unfiltered) output.

**Coverage Requirement**: All 4 flag combinations tested: neither, skip-only (error), both (filtered export), confirm-only (full export).

### R-24: Export header missing skip_quarantined metadata
**Severity**: Low
**Likelihood**: Med
**Impact**: The export header does not include `skip_quarantined: true` when the flag is active. Downstream consumers (human inspection, automated audits) cannot distinguish a filtered export from a full export. The architecture specifies this metadata in the header.

**Test Scenarios**:
1. Export with `--skip-quarantined --confirm`. Parse header. Verify `skip_quarantined: true` (or equivalent) is present.
2. Export without `--skip-quarantined`. Parse header. Verify `skip_quarantined` is absent or false.

**Coverage Requirement**: Header metadata reflects the actual filtering state.

## Integration Risks

1. **Export-import contract mismatch** — Export serializes via serde_json::Map (manual construction), import deserializes via serde tagged enum. A field name typo in export that doesn't match the struct field name causes silent field loss on import. Mitigated by round-trip test (AC-15), but each field must be individually verified.

2. **drop_all_data ordering vs. new PRAGMA foreign_keys behavior** — ADR-001 chose explicit ordering over CASCADE reliance. If a future schema change adds FK constraints to graph_edges or cycle_events, the DELETE ordering must be updated. Current architecture has no FK on these tables — implicit assumption.

3. **Import ordering dependency** — graph_edges reference entry IDs. Entries must be imported before graph_edges. The architecture places new tables after existing 8 in the JSONL stream, so entries are always first. A hand-edited file with graph_edges before entries produces semantically invalid edges (no FK catches it).

4. **Transaction duration with large observation sets** — 50K observations within a single BEGIN IMMEDIATE transaction extends lock hold time. NFR-02 targets 30 seconds. If observations.input contains large JSON payloads, the transaction could exceed this target.

5. **skip_ids threading through do_export signature** — `do_export` gains `skip_ids: &HashSet<i64>`. All 5 affected exporters gain this parameter. If a new table exporter is added in the future that references entry IDs but does not receive `skip_ids`, it silently produces orphaned rows. The architecture must document this obligation for future table additions (ADR-008 consequences section does this).

6. **--confirm flag ignored when --skip-quarantined is absent** — ADR-009 specifies `--confirm` without `--skip-quarantined` is silently ignored. If future features add their own confirmation semantics to `--confirm`, the shared flag creates ambiguity.

## Edge Cases

1. **Empty tables** — All 3 new tables empty. Export produces zero lines for each. Import succeeds with 0 counts. (AC-21)
2. **Very large observations.input** — Single observation row with multi-MB input field. Export/import must handle without truncation.
3. **graph_edges with all nullable fields NULL** — metadata is the only nullable column. Edge with metadata=NULL must round-trip correctly.
4. **cycle_events with all nullable fields NULL** — phase, outcome, next_phase, goal all NULL. Schema-valid. Must round-trip.
5. **Maximum i64 values** — source_id, target_id, id fields at i64::MAX. JSON serialization must not lose precision.
6. **Unicode in TEXT fields** — observations.tool, graph_edges.metadata, cycle_events.goal containing non-ASCII characters. JSONL must preserve UTF-8.
7. **graph_edges weight = 0.0** — Valid weight. Must not be confused with NaN fallback. Exported as 0.0, not replaced.
8. **format_version 1 file with unexpected graph_edges rows** — v1 file that contains graph_edges _table lines. Import should process them (match arm exists regardless of version).
9. **Zero quarantined entries with --skip-quarantined** — Flag active but no entries have status=3. Skip set is empty. All rows exported. Skip counts are 0.
10. **All entries quarantined with --skip-quarantined** — Every entry has status=3. No entries exported. All dependent rows skipped. Only counters, audit_log, observations, cycle_events exported.
11. **co_access self-referencing row** — entry_id_a == entry_id_b and the entry is quarantined. Trivially caught by either-side check.
12. **graph_edges self-loop** — source_id == target_id and the entry is quarantined. Trivially caught by either-side check.

## Security Risks

1. **Path traversal in import file** — The import file path is user-supplied. The import pipeline reads via standard file I/O. No path traversal risk beyond what the OS filesystem allows (import does not construct paths from file content).

2. **SQL injection via export data** — All inserters use parameterized queries (sqlx bind parameters). Even if observations.input or graph_edges.metadata contain SQL injection payloads, they are bound as parameters. Risk: negligible.

3. **Denial of service via large import file** — Import reads line-by-line within a single transaction. A maliciously crafted file with millions of rows could exhaust memory or hold the write lock indefinitely. Blast radius: limited to the local CLI user (no network exposure). No mitigation required for local CLI tool.

4. **Untrusted JSONL content** — Import deserializes arbitrary JSON from the file. serde_json is memory-safe. Unexpected field values (negative IDs, empty strings for NOT NULL columns) are caught by SQLite constraints at INSERT time.

5. **--skip-quarantined as data exfiltration filter** — A malicious actor with CLI access could use `--skip-quarantined --confirm` to produce an export that omits specific entries (by first quarantining them). Blast radius: same as DELETE access — requires write access to the database. No additional attack surface beyond existing `context_quarantine` permissions.

## Failure Modes

1. **Export failure mid-stream** — If an export query fails, the transaction rolls back. The output file may contain a partial JSONL stream. The header's row hashes will not match on import, causing rejection. Expected behavior: clean rejection on import.

2. **Import failure mid-transaction** — If an INSERT fails (constraint violation, disk full), the BEGIN IMMEDIATE transaction rolls back. No partial state committed. Expected behavior: error message, database unchanged.

3. **Import of v2 file by old binary** — Old binary rejects format_version 2 at header validation. Clear error message. No data modification attempted.

4. **Import with --force on a database with active connections** — BEGIN IMMEDIATE acquires a write lock. If another connection holds a read lock, the import waits (or times out with SQLITE_BUSY). Expected behavior: sqlx timeout, error propagated.

5. **Missing table in export** — v2 file missing observations rows (hand-edited). Import succeeds with 0 observation count. No error — pipeline does not validate that v2 files contain all 11 types. By design (v1 compatibility works the same way).

6. **--skip-quarantined without --confirm** — Export aborts immediately with a clear error message (ADR-009). No output file created. No database access performed. Expected behavior: non-zero exit code, actionable error.

7. **--skip-quarantined with empty database** — Skip-set query returns 0 IDs. Export proceeds normally, producing a file identical to a non-filtered export. No error.

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (NaN weight corrupts JSONL) | R-01 | ADR-003: Number::from_f64 with fallback. Semantically correct default weight. |
| SR-02 (Skip-set query must run inside DEFERRED snapshot — TOCTOU) | R-17 | ADR-008: skip-set query executes inside the existing BEGIN DEFERRED transaction, before any export_* call. Same snapshot as all table reads. |
| SR-03 (goal_embedding NULL degradation) | R-15 | ADR-004: Exclude from SELECT. context_briefing graceful degradation path verified. |
| SR-04 (v2 unreadable by old binaries) | R-11 | ADR-002: format_version guard rejects v2 at header. Documented one-way compatibility. |
| SR-05 (Non-exact export confusion with --skip-quarantined) | R-23, R-24 | ADR-009: --confirm safeguard prevents accidental use. Export header includes skip_quarantined metadata for identification. |
| SR-06 (ADR-007 stale — superseded by ADR-008) | — | Resolved. ADR-007 marked SUPERSEDED. ADR-008 defines export-side design. No residual risk. |
| SR-07 (FK cascade in drop_all_data) | R-02 | ADR-001: Explicit DELETE ordering. observation_phase_metrics before observation_metrics before observations. |
| SR-08 (5 exporters must consistently check skip set) | R-16, R-19, R-20 | ADR-008: all 5 affected exporters receive `skip_ids: &HashSet<i64>`. R-16 tests the full cascade. R-19/R-20 test dual-column checks specifically. |
| SR-09 (--confirm must be CLI flag, not interactive) | R-23 | ADR-009: --confirm is a clap boolean flag. No interactive stdin prompt. Matches nan-002 ADR-003 precedent. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical (High S x High L) | 0 | 0 scenarios |
| High (High S x Med L, or High S x Low L) | 10 (R-01, R-02, R-14, R-15, R-16, R-17, R-18, R-19, R-20, R-23) | 33 scenarios |
| Medium (Med S x Med L, High S x Low L overlap counted above) | 7 (R-03, R-04, R-05, R-08, R-09, R-10, R-11) | 18 scenarios |
| Low (Low S, or Med S x Low L) | 7 (R-06, R-07, R-12, R-13, R-21, R-22, R-24) | 14 scenarios |
| **Total** | **24** | **65 scenarios** |
