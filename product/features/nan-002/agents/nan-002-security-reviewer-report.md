# Security Review: nan-002-security-reviewer

## Risk Level: low

## Summary

The nan-002 Knowledge Import implementation is well-structured with strong security fundamentals. All SQL operations use parameterized queries via rusqlite `params![]`, preventing SQL injection. The input surface (JSONL file) is handled with line-by-line parsing, serde typed deserialization, and proper error propagation. One medium-severity design concern exists around `drop_all_data` executing outside the import transaction, creating a window where data loss is irrecoverable if the subsequent import fails.

## Findings

### Finding 1: drop_all_data executes outside transaction boundary
- **Severity**: medium
- **Location**: crates/unimatrix-server/src/import/mod.rs (run_import function, force-drop block)
- **Description**: When `--force` is specified, `drop_all_data()` is called before `BEGIN IMMEDIATE`. If the import then fails (e.g., header validation, JSONL parse error, FK violation), the original data has already been permanently deleted with no rollback possible. The architecture (ADR-003) accepts this risk for the `--force` flag, but the current code structure amplifies it -- the drop could be moved inside the transaction to make it atomic with the import.
- **Recommendation**: Move `drop_all_data()` to execute after `BEGIN IMMEDIATE` but before `ingest_rows()`, so that both the drop and the import are covered by the same transaction. If the import fails, everything rolls back including the drop.
- **Blocking**: no

### Finding 2: SQL injection prevention -- PASS
- **Severity**: informational
- **Location**: crates/unimatrix-server/src/import/inserters.rs (all 8 insert functions)
- **Description**: All INSERT statements use `params![]` macro with positional parameters (?1, ?2, etc.). No string interpolation in production SQL. The one `format!()` call for SQL is in test code (`test_all_eight_tables_restored`) with hardcoded table names, which is acceptable.
- **Recommendation**: None needed.
- **Blocking**: no

### Finding 3: Input file path -- no traversal risk
- **Severity**: informational
- **Location**: crates/unimatrix-server/src/main.rs (Import CLI args)
- **Description**: The `--input` path is only used with `File::open()` for reading. No path components from JSONL content are used as file paths. No path traversal risk.
- **Recommendation**: None needed.
- **Blocking**: no

### Finding 4: No secrets or credentials in diff
- **Severity**: informational
- **Location**: entire diff
- **Description**: No hardcoded API keys, tokens, passwords, or credentials found in any of the 7,473 added lines.
- **Recommendation**: None needed.
- **Blocking**: no

### Finding 5: Deserialization bounded by line-by-line reading
- **Severity**: low
- **Location**: crates/unimatrix-server/src/import/mod.rs (ingest_rows)
- **Description**: JSONL is read line-by-line via BufReader, which bounds per-line memory allocation. serde_json rejects NaN/Infinity by default. However, there is no validation that `entry_count` in the header matches the actual number of entries ingested. A malicious file with `entry_count: 5` but millions of lines would be processed in full. The blast radius is disk exhaustion and long runtime, not data corruption.
- **Recommendation**: Consider adding a check that compares the actual ingested entry count against `header.entry_count` as a post-ingestion sanity check. Low priority since the import is a local CLI tool processing user-provided files.
- **Blocking**: no

### Finding 6: No new external dependencies
- **Severity**: informational
- **Location**: Cargo.toml (unchanged in diff)
- **Description**: The import module uses only existing workspace crates (unimatrix-store, unimatrix-embed, unimatrix-vector, unimatrix-engine) and existing external dependencies (serde, serde_json, clap, rusqlite). No new dependency supply chain risk.
- **Recommendation**: None needed.
- **Blocking**: no

## Blast Radius Assessment

The worst-case scenario involves `--force` on a production database where the import subsequently fails after `drop_all_data()` but before `COMMIT`. In this case, all existing knowledge base data is lost. The user would need a prior export to recover. This is partially mitigated by the `--force` flag being explicitly opt-in and by the stderr warning. Moving the drop inside the transaction would eliminate this risk entirely.

For non-force imports, failure modes are safe: the transaction rolls back, leaving the database unchanged. Post-commit failures (ONNX model unavailability, embedding errors) leave the database intact but without a vector index -- the database remains queryable by ID but not by semantic search, which is an acceptable degraded state per ADR-004.

## Regression Risk

Low. The changes are purely additive:
- Three new modules (`format.rs`, `import/mod.rs`, `import/inserters.rs`, `embed_reconstruct.rs`)
- Three lines added to `lib.rs` (module registration)
- One new CLI variant in `main.rs`

No existing code was modified beyond registration. The new modules cannot affect existing MCP server behavior, export behavior, or hook behavior unless explicitly invoked via the `import` subcommand.

## PR Comments
- Posted 1 comment on PR #218
- Blocking findings: no

## Knowledge Stewardship
- Nothing novel to store -- the anti-patterns checked (SQL injection, path traversal, deserialization, transaction safety) are already well-understood. The one finding (drop outside transaction) is specific to this PR's design choice rather than a generalizable anti-pattern.
