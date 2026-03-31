# Security Review: bugfix-458-security-reviewer

## Risk Level: low

## Summary

The diff contains a single-file SQL maintenance fix to `background.rs`. The change adds a `WHERE status != ?1` predicate to a parameterized DELETE and binds `Status::Quarantined as u8 as i64` — no user-controlled input reaches either the query or the parameter. No new dependencies, no unsafe code, no secrets. All OWASP concerns checked; none apply.

## Findings

### F-1: SQL Query Binding — Correct Pattern Confirmed
- **Severity**: informational (no issue)
- **Location**: `crates/unimatrix-server/src/background.rs:519–526`
- **Description**: The new predicate `WHERE status != ?1` is a parameterized query. The bound value `Status::Quarantined as u8 as i64` is a compile-time constant derived from a `#[repr(u8)]` enum. No runtime user input participates in either the query string or the bound parameter. This is the established codebase pattern (consistent with `services/status.rs:1023`). Previous security review of bugfix-444 (Unimatrix #3766) established that bare integer literals in SQL status filters would be blocking; this fix uses the typed bind pattern correctly.
- **Recommendation**: No action required.
- **Blocking**: no

### F-2: Test-Only `.expect()` in Test Helper
- **Severity**: low (test code only)
- **Location**: `crates/unimatrix-server/src/background.rs:2945`
- **Description**: `run_graph_edges_compaction` uses `.expect("compaction DELETE must succeed")`, which panics on failure. This is existing behavior in the test helper, not introduced by this diff — the test helper existed before this PR. Test panics are acceptable and expected in unit tests; the production path at line 519–548 correctly uses `match` with a logged non-fatal error path.
- **Recommendation**: No action required. Production error handling is correct.
- **Blocking**: no

### F-3: Test INSERT Uses Raw SQL With Full Column List
- **Severity**: informational (test code only)
- **Location**: `crates/unimatrix-server/src/background.rs:3107–3145, 3168–3205`
- **Description**: The two new tests insert quarantined entries via raw SQL INSERT with an explicit column list. Per Unimatrix lesson #3543 (compile-silent spec violation for nullable column additions), raw INSERT helpers can silently fail to bind new nullable columns when the schema evolves. Here the INSERT has 23 columns bound explicitly. This is a maintenance risk if columns are added to `entries` in future, but it is test-only, not a security risk, and consistent with the test pattern used in `insert_test_entry` elsewhere in the same file.
- **Recommendation**: No action required for this PR. Future feature agents adding `entries` columns should grep for raw INSERT helpers in tests.
- **Blocking**: no

## OWASP Assessment

| Concern | Verdict | Rationale |
|---------|---------|-----------|
| Injection (A03) | No risk | Parameterized query; no external input in SQL string or bind value |
| Broken access control (A01) | No risk | Maintenance tick is an internal background task; no trust boundary crossed |
| Security misconfiguration (A05) | No risk | No config changes introduced |
| Vulnerable components (A06) | No risk | No new dependencies added |
| Integrity failures (A08) | No risk | DELETE is bounded to quarantined entries; active/deprecated/proposed entries unaffected |
| Deserialization (A08) | No risk | No deserialization in the changed paths |
| Input validation (A03) | No risk | The status discriminant is a compile-time constant |

## Blast Radius Assessment

Worst case: the `status != ?1` predicate has a subtle off-by-one or type mismatch causing the DELETE to match either too few rows (orphaned edges accumulate — the pre-fix behavior, benign) or too many rows (active-entry edges deleted). The latter would corrupt the typed graph state for one tick cycle before being rebuilt on the next tick from the live `entries` table. The error is bounded: `build_typed_relation_graph` silently skips edges with missing endpoints, so a spurious delete produces a thinner graph for one tick, not a panic or data loss. Status values are compile-time verified via the `#[repr(u8)]` enum — the i64 cast is safe and consistent with the rest of the codebase.

The fix is non-fatal by design (lines 539–547): compaction failure is logged and the tick proceeds with the pre-compaction state. A silent regression here degrades graph quality but does not corrupt stored entries.

## Regression Risk

Low. The change narrows the DELETE to treat quarantined entries as "live" (not orphaned), which is the correct semantic. Active, deprecated, and proposed entries are unaffected — `status != 3` still includes `Active=0`, `Deprecated=1`, `Proposed=2`. The pre-existing test `test_background_tick_compacts_orphaned_graph_edges` still passes, confirming standard orphaned-edge deletion (target does not exist at all) is preserved. Two new tests specifically cover the quarantined-source and quarantined-target cases.

One edge case not tested: an entry with an unknown status value (outside 0–3) that somehow persists in the database. The `TryFrom<u8>` guard prevents this at write time; if it occurred anyway, `status != 3` would leave those edges in place — the same behavior as before the fix and benign.

## Dependency Safety

No new dependencies introduced. No `Cargo.toml` changes in the diff.

## Secrets

No hardcoded secrets, API keys, tokens, or credentials found in any changed file.

## PR Comments

- Posted 1 comment on PR #462 (see below)
- Blocking findings: no

## Knowledge Stewardship

- Stored: nothing novel to store — the codebase pattern of `.bind(Status::X as u8 as i64)` for SQL status filters was already captured by the fix agent in #3908. The lesson about the human reviewer upgrading SQL status literals to blocking (entry #3766) is already stored. No new anti-pattern observed.
