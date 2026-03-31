# Security Review: crt-035-security-reviewer

## Risk Level: low

## Summary

crt-035 introduces bidirectional CoAccess edges via a tick helper refactor and a v18→v19
schema migration back-fill. The change set introduces no new external input surfaces: all
SQL parameters are derived from internal database rows, not from MCP tool parameters, user
input, or environment variables. All SQL uses parameterized queries with `.bind()`. No new
dependencies are introduced. No injection, path traversal, deserialization, or access-control
concerns were identified.

---

## Findings

### Finding 1: No new external input surface (informational)

- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/services/co_access_promotion_tick.rs:81-178`
- **Description**: `promote_one_direction` receives `source_id: i64`, `target_id: i64`, and
  `new_weight: f64` exclusively from the internal CO_ACCESS table read in Phase 2. These
  values are produced by a prior sqlx SELECT against an internal database table, not from
  any MCP tool parameter, agent-supplied string, file path, or network input. The trust
  boundary is the database itself, which is initialized and controlled by the server process.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 2: All SQL parameterized — no injection risk (informational)

- **Severity**: informational
- **Location**: `crates/unimatrix-store/src/migration.rs:647-669` and
  `crates/unimatrix-server/src/services/co_access_promotion_tick.rs:90-94, 116-120, 154-157`
- **Description**: Every SQL statement in the changed code uses either:
  - Parameterized bind variables (`?1`, `?2`, `.bind(value)`) for all variable fields, OR
  - Fully static string literals with no runtime interpolation (the v18→v19 back-fill INSERT
    OR IGNORE is entirely static — no user data flows into it).
  No `format!()`, `concat!()`, or string interpolation was used to build any SQL statement.
  Verified with grep scan: zero matches for dynamic SQL construction patterns in the changed
  files within this PR.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 3: Bare integer literal in migration.rs pre-existing — NOT introduced by this PR (informational)

- **Severity**: informational
- **Location**: `crates/unimatrix-store/src/migration.rs:199-200`
- **Description**: Line 199-200 contains `WHERE status = 3 AND pre_quarantine_status IS NULL`
  using a bare integer literal for a status discriminant. Per Unimatrix lesson #3766, the
  established convention is `.bind(Status::X as u8 as i64)` for SQL status filters.
  This line is NOT introduced by crt-035 — git diff confirms it does not appear in the PR
  diff. It is a pre-existing finding from an earlier migration block and is out of scope for
  this review. Noted for completeness only.
- **Recommendation**: Address in a separate cleanup PR.
- **Blocking**: no (pre-existing, out of scope for this PR)

### Finding 4: `bootstrap_only = 0` hardcoded on back-filled reverse edges — intentional and correct

- **Severity**: informational
- **Location**: `crates/unimatrix-store/src/migration.rs:658`, `co_access_promotion_tick.rs:94`
- **Description**: Reverse edges written by both the migration back-fill and the tick always
  carry `bootstrap_only = 0`. This means they are included in `build_typed_relation_graph`
  reads (which filter out `bootstrap_only = 1` rows). Per SR-SEC-02 in RISK-TEST-STRATEGY.md
  and ARCHITECTURE.md §Component 2, this is explicitly the intended behavior: reverse CoAccess
  edges should participate in live PPR traversal. There is no access-control concern — the
  `bootstrap_only` flag is an inclusion filter, not a privilege gate.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 5: `created_by` provenance copied from forward edge in back-fill (note for downstream analytics)

- **Severity**: low
- **Location**: `crates/unimatrix-store/src/migration.rs:657` (`g.created_by AS created_by`)
- **Description**: The back-fill SQL copies `created_by` from the forward edge into the
  reverse row (D1 design decision). This means reverse edges created by the back-fill will
  carry `created_by = 'bootstrap'` or `created_by = 'tick'` depending on the forward edge.
  Any future analytics or security audit query that counts `GRAPH_EDGES` rows by `created_by`
  to identify tick-managed or bootstrap-managed edges will return doubled counts post-migration
  without distinguishing direction. This is acknowledged in RISK-TEST-STRATEGY.md as IR-03.
  It is not exploitable and does not affect access control, but an auditor counting tick edges
  by `created_by` will see 2x the expected count.
- **Recommendation**: Document in IR-03 (already done). If future audit tooling counts edges
  by `created_by`, add a `source = 'co_access'` filter and expect even-numbered counts.
- **Blocking**: no

### Finding 6: No hardcoded secrets, API keys, or credentials

- **Severity**: informational
- **Location**: All modified Rust files
- **Description**: Grep scan across all modified source files found zero matches for secret,
  password, api_key, token, or credential patterns. Static string literals are limited to
  SQL keywords, table/column names, and enum variants ('CoAccess', 'tick', 'co_access').
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 7: No new dependencies introduced

- **Severity**: informational
- **Location**: `Cargo.toml` / workspace
- **Description**: The diff contains no Cargo.toml changes. No new crates are introduced.
  All SQL is executed via the existing sqlx dependency. No new deserialization surfaces, no
  new network clients, no new FFI. Dependency surface is unchanged from crt-034.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 8: No unsafe code introduced

- **Severity**: informational
- **Location**: All modified Rust files
- **Description**: No `unsafe` blocks appear in any of the changed production files.
  The new `promote_one_direction` helper is fully safe Rust using sqlx async. The migration
  SQL runs on a `&mut SqliteTransaction` obtained from the existing connection lifecycle
  already established before this change.
- **Recommendation**: No action required.
- **Blocking**: no

---

## OWASP Concern Assessment

| Concern | Assessment |
|---------|-----------|
| A1 — Injection (SQL) | Not present. All SQL uses `.bind()` parameterization or fully static literals. No format!() in SQL. |
| A1 — Injection (Path traversal) | Not present. No new file path operations introduced. `db_path` is server-controlled. |
| A2 — Broken Authentication | Not applicable. No authentication logic changed. |
| A3 — Sensitive Data Exposure | Not present. No new columns or tables expose PII. `weight` (f64) is non-sensitive. |
| A4 — XML External Entity | Not applicable. No XML/HTML parsing. |
| A5 — Broken Access Control | Not present. The tick runs as an internal background task with no capability gate bypass. Migration runs at startup under server identity. |
| A6 — Security Misconfiguration | Not present. `bootstrap_only = 0` is intentional (documented SR-SEC-02). No new config fields. |
| A7 — XSS | Not applicable. No web rendering. |
| A8 — Insecure Deserialization | Not present. The tick and migration process only internal i64/f64 values from SQLite. No new deserialization surfaces. |
| A9 — Vulnerable Components | Not present. No new dependencies. |
| A10 — Logging/Monitoring | Positive change. Log fields renamed from `inserted`/`updated`/`qualifying` to `promoted_pairs`/`edges_inserted`/`edges_updated` — more informative, no info leakage. |

---

## Blast Radius Assessment

**Worst case if promote_one_direction has a subtle bug:**

The worst-case failure mode is a write-path failure that logs at `warn!` and returns
`(false, false)`. The infallible contract means the tick does not abort on partial failure.
The failure is bounded to one direction of one pair per tick interval. The next tick will
attempt convergence. There is no silent data corruption path — a failed INSERT leaves the DB
in its prior state; a failed UPDATE leaves a stale weight. The weight staleness is bounded
by the `CO_ACCESS_WEIGHT_UPDATE_DELTA` floor (0.1).

**Worst case if the v18→v19 migration has a subtle bug:**

The migration runs inside the main transaction. Any SQL error causes a rollback to v18.
The server fails to start (migration error propagated). The database is not corrupted — it
remains at v18. Repeated open attempts retry the migration. This behavior is documented in
FM-01 and accepted as adequate per R-09 (Low/Low). There is no silent partial-migration path
because the schema_version counter is only updated after the back-fill succeeds.

**Information disclosure risk:** None. The migration and tick operate entirely on internal
graph topology data. No user content, no credentials, no MCP tool parameters flow through
either path.

---

## Regression Risk

**What existing functionality could break:**

1. **PPR search scores (positive change):** Existing PPR traversals will score higher after
   the back-fill, because previously invisible reverse CoAccess paths are now active.
   This is the intended behavior, not a regression. Scores will be different (higher for
   pairs where high-ID was seeded), not lower or corrupt.

2. **GRAPH_EDGES row count analytics:** Any query counting CoAccess rows will return 2x the
   pre-migration count. IR-03 documents this. Existing test `migration_v12_to_v13.rs` was
   updated to reflect the new count (2→4). No silent break — the test update was required
   and was made.

3. **Cycle detection:** Not affected. Cycle detection uses a Supersedes-only subgraph.
   Bidirectional CoAccess edges are excluded by design. The test
   `test_cycle_detection_on_supersedes_subgraph_only` continues to pass.

4. **Tests in `migration_v17_to_v18.rs`:** Correctly updated from exact `== 18` assertions
   to `>= 18` to remain valid across future schema bumps. This is a pattern change (#3803)
   applied correctly — prior migration tests become stale on each version bump; using `>= N`
   instead of `== N` is the established codebase pattern.

5. **`test_schema_version_is_14` in `sqlite_parity.rs`:** Updated from 18 to 19. Correct.

**Residual regression risk:** Low. The test suite ran 4152 tests with 0 failures (gate-3c
confirmed). The AC-12 PPR regression test directly guards the critical path.

---

## Gate-3b Mandatory Check Results (Independent Verification)

| Check | Result | Evidence |
|-------|--------|---------|
| GATE-3B-01: `"no duplicate"` grep | PASS | Zero matches in co_access_promotion_tick_tests.rs |
| GATE-3B-02: Odd count_co_access_edges values | PASS | All 13 assertion values are even: 2,2,6,2,2,2,0,0,6,10,2,2,2 |
| GATE-3B-03: EXPLAIN QUERY PLAN | PASS | Documented in migration_v18_to_v19.rs header: `SEARCH rev USING COVERING INDEX sqlite_autoindex_graph_edges_1` |
| GATE-3B-04: AC-12 uses SqlxStore::open | PASS | `test_reverse_coaccess_high_id_to_low_id_ppr_regression` calls `SqlxStore::open` at line 853 |

---

## PR Comments
- Posted 1 summary comment on PR #461
- Blocking findings: no

---

## Knowledge Stewardship

- nothing novel to store — the security findings here are all "no issue" confirmations of
  established patterns (parameterized SQL, infallible tick, migration transaction). Lesson
  #3766 (bare status integer literals) was checked but not triggered by this PR. No
  cross-feature anti-pattern was introduced that would benefit future agents.
