# Security Review: nan-001-security-reviewer

## Risk Level: Low

## Summary

Read-only JSONL export feature with no new external dependencies, no network exposure, and no deserialization of untrusted input. All SQL queries are static strings. The only non-trivial concern is the `preserve_order` feature flag on `serde_json` which changes `Map` backing from `BTreeMap` to `IndexMap` crate-wide, but this is acknowledged in ADR-003 and is semantically safe.

## Findings

### Finding 1: No output path validation (informational)
- **Severity**: Low
- **Location**: `crates/unimatrix-server/src/export.rs:44`
- **Description**: The `--output` path is passed directly to `File::create()`. Acceptable for a CLI tool where the user has filesystem access. Would need validation if ever exposed via MCP.
- **Recommendation**: Document that `run_export` trusts its caller.
- **Blocking**: No

### Finding 2: `preserve_order` feature has global scope
- **Severity**: Low
- **Location**: `crates/unimatrix-server/Cargo.toml:32`
- **Description**: Changes `serde_json::Map` from `BTreeMap` to `IndexMap` for the entire crate. Acknowledged in ADR-003.
- **Recommendation**: Confirmed acceptable.
- **Blocking**: No

### Finding 3: NaN confidence fallback to 0
- **Severity**: Low
- **Location**: `crates/unimatrix-server/src/export.rs:197`
- **Description**: NaN confidence silently becomes 0.0 in export. NaN should not occur in practice.
- **Recommendation**: Acceptable for v1.
- **Blocking**: No

### Finding 4: Transaction commit failure silently ignored
- **Severity**: Low
- **Location**: `crates/unimatrix-server/src/export.rs:57`
- **Description**: `let _ = conn.execute_batch("COMMIT")` discards the result. Correct for read-only DEFERRED transaction.
- **Recommendation**: Acceptable as-is.
- **Blocking**: No

### Finding 5: Audit log data included in export
- **Severity**: Low (informational)
- **Location**: `crates/unimatrix-server/src/export.rs:461-496`
- **Description**: Export includes session IDs and operational metadata. Appropriate for backup but worth noting in docs if files are shared externally.
- **Recommendation**: Document in user-facing docs.
- **Blocking**: No

## Blast Radius Assessment

Worst case: export produces JSONL with missing or corrupted columns, leading to data loss on nan-002 import. Source database is never modified (read-only operation with DEFERRED transaction). The `preserve_order` feature flag is the widest-reaching change, affecting all `serde_json::Map` usage in `unimatrix-server`.

## Regression Risk

Low. The change is additive (new module + new CLI subcommand). No existing code paths modified beyond cosmetic import reordering. The `preserve_order` feature flag is the only change with potential side effects on existing behavior, mitigated by running the full test suite.

## PR Comments
- Posted 1 comment on PR #210
- Blocking findings: No

## Knowledge Stewardship
- Stored: nothing novel to store -- standard CLI export pattern with no new security anti-patterns. Findings are specific to this PR (path validation reminder, feature flag scope) and not generalizable.
