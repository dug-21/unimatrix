# Security Review: crt-047-security-reviewer

## Risk Level: low

## Summary

crt-047 adds curation health metrics (correction/deprecation counts, rolling baseline,
sigma comparison) to `context_cycle_review` and `context_status`. The feature is
read-only at all new write-path SQL boundaries. All SQL uses sqlx parameterized binds
with no string interpolation. NaN and divide-by-zero guards are explicit and correct.
The two-step upsert correctly preserves `first_computed_at`. No new dependencies, no
hardcoded secrets, no unsafe code in the new or modified files.

---

## Findings

### Finding 1: `feature_cycle` parameter is MCP-user-controlled but trusted implicitly

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs:273` (`RetrospectiveParams.feature_cycle`)
- **Description**: `feature_cycle: String` is deserialized directly from MCP caller input with no
  length limit or format validation before being bound into SQL. The SQL bind (`?1`) is
  parameterized — there is no injection risk. However, an arbitrarily long or pathological
  `feature_cycle` value (e.g., 1MB string) will be stored in `cycle_review_index.feature_cycle`
  and re-read into every `get_curation_baseline_window()` row result. This is a pre-existing
  condition, not introduced by crt-047; crt-047 does not worsen it. The `summary_json` 4MB
  ceiling (NFR-03) is enforced; there is no ceiling on the `feature_cycle` key string itself.
- **Recommendation**: Add a length cap (e.g., 256 chars) on `RetrospectiveParams.feature_cycle`
  at MCP parameter validation time. This is an existing gap, not a crt-047 regression.
- **Blocking**: no

### Finding 2: `write_pool_server()` used for read-only queries in `compute_curation_snapshot()`

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/curation_health.rs:131`
- **Description**: `compute_curation_snapshot()` uses `write_pool_server()` (the serialized
  single-connection write pool) for three read-only SELECT queries against ENTRIES. The
  comment documents this as intentional: `read_pool()` is `pub(crate)` in `unimatrix-store`
  and not cross-crate accessible (entry #3028). This creates a pool contention risk — the
  write pool is held during three sequential SQL queries before `store_cycle_review()` acquires
  the write connection. In practice `write_pool_server` has at most 2 connections; under
  sustained concurrent `context_cycle_review` calls the read phase could contend with the
  write phase on connection acquisition.
  
  The write pool is NOT held across both phases; `compute_curation_snapshot()` uses `.fetch_one()`
  which releases the connection immediately after each query. The ARCHITECTURE.md I-01 ordering
  requirement (read before write) is preserved. The risk is latency under concurrency, not
  a security concern in the strict OWASP sense. Classified as low because the server is
  single-tenant MCP.
- **Recommendation**: If `read_pool()` visibility is ever relaxed to `pub` in `unimatrix-store`,
  migrate `compute_curation_snapshot()` to use it to free the write pool for writes. The
  workaround is documented but worth resolving in a future cleanup.
- **Blocking**: no

### Finding 3: `orphan_deprecations` query over full timestamp range when `cycle_start_ts = 0`

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/curation_health.rs:205-220`
- **Description**: When no `cycle_start` event exists for the requested cycle,
  `cycle_start_ts` falls back to `0`. The orphan query becomes
  `WHERE updated_at >= 0 AND updated_at <= review_ts` — effectively scanning all deprecated
  entries in the entire database history. The result over-counts orphans (documented, EC-02)
  but the computation is non-fatal. The caller logs a warning when `cycle_start_ts = 0`
  (verified in tools.rs diff). This is a correctness risk (inflated metric), not an injection
  or disclosure risk. There is no panic path.
- **Recommendation**: The behavior is documented. Consider adding a `has_valid_window: bool`
  annotation to `CurationSnapshot` so callers can signal the over-count condition in output,
  rather than only logging it internally.
- **Blocking**: no

### Finding 4: Information disclosure via `corrections_system` field

- **Severity**: low (informational)
- **Location**: `crates/unimatrix-observe/src/types.rs` — `CurationSnapshot.corrections_system`
- **Description**: `corrections_system` is serialized into `context_cycle_review` and
  `context_status` output, surfacing the rate of system/cortical-implant-initiated corrections.
  This reveals internal write volume to any caller with sufficient permissions to call these
  tools. Both tools are **Admin-only** (pre-existing access control), so the audience is
  already operators with full system visibility. Not a blocking concern.
- **Recommendation**: None required. Document in operator guide that `corrections_system`
  reflects internal automation volume if sensitive operational patterns are a concern.
- **Blocking**: no

---

## OWASP Assessment

| OWASP Category | Status |
|----------------|--------|
| A03 Injection | CLEAR — all SQL uses sqlx parameterized binds; zero string interpolation in added SQL |
| A01 Broken Access Control | CLEAR — new curation fields exposed only via existing Admin-only tools |
| A05 Security Misconfiguration | CLEAR — no new configuration surface; migration uses DDL-only changes |
| A08 Data Integrity Failures | CLEAR — `first_computed_at` preservation verified by two-step upsert; 4MB ceiling enforced |
| A06 Vulnerable Components | CLEAR — no new dependencies added |
| A04 Insecure Design (NaN/float) | CLEAR — explicit zero-denominator and zero-stddev guards at every division site |
| Deserialization | CLEAR — new types derive standard serde; no custom deserializers |
| Secrets | CLEAR — no hardcoded credentials, tokens, or API keys anywhere in the diff |
| `unsafe` code | CLEAR — no `unsafe` in new or modified service/store files |

---

## Blast Radius Assessment

The worst-case subtle bug scenarios:

1. **`first_computed_at` preservation fails silently**: If the two-step upsert had a
   regression (e.g., wrong branch taken), historical `force=true` calls would update
   `first_computed_at` to the current time, perturbing the baseline window ordering.
   Effect: incorrect σ baseline, false anomaly flags. NOT a data corruption or privilege
   escalation risk. The test `test_store_cycle_review_preserves_first_computed_at_on_overwrite`
   covers this path.

2. **NaN propagates into stored JSON**: If any zero-division guard were bypassed,
   `corrections_total_sigma` or `orphan_ratio_sigma` could be NaN or Inf. SQLite stores
   the NaN as a float literal; JSON serialization of NaN in Rust with serde_json produces
   `null` (not a crash). The downstream formatter would show `null` sigma — odd but not
   harmful. Multiple unit tests assert `!is_nan()` explicitly.

3. **Migration fails mid-run**: Outer transaction atomicity (ADR-004) means the schema
   stays at v23. On next `Store::open()` the `pragma_table_info` pre-checks skip already-added
   columns and complete idempotently. No data loss path.

4. **`compute_curation_snapshot()` SQL error**: Treated as non-fatal; `curation_health`
   block absent from response. No abort, no panic, no information leak beyond the warning log.

Maximum blast radius: incorrect curation health metrics in `context_cycle_review` and
`context_status` output. No data loss, no privilege escalation, no silent data corruption.

---

## Regression Risk

- **Schema cascade**: `sqlite_parity.rs`, `migration_v22_to_v23.rs`, and `server.rs` tests
  were all updated. The v22→v23 test was correctly relaxed from `== 23` to `>= 23`. The
  `test_schema_version_is_14` function name is now misleading (asserts version 24) but
  this is cosmetic — function was already misnamed before this PR.

- **`CycleReviewRecord` struct expansion**: All pre-existing test fixtures that construct
  this struct directly were updated to add `..Default::default()` or explicit zero values
  for the new fields. The diff shows this was done consistently across `status.rs` tests,
  `retention.rs` tests, and the tools test module. No missed construction sites detected.

- **`RetrospectiveReport.curation_health`**: Added as `Option<CurationHealthBlock>` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]`. Existing stored JSON
  without this field deserializes to `None` via `serde(default)` — backward compatible.

- **`StatusReport.curation_health`**: Same pattern — `Option` with `skip_serializing_if`.
  Callers that don't consume this field are unaffected.

- **`SUMMARY_SCHEMA_VERSION` bump (1 → 2)**: All historical `cycle_review_index` rows will
  show the stale-record advisory on `force=false` calls after deployment. This is a
  designed behavior, documented in ARCHITECTURE.md. Operators unaware of this may
  interpret the advisory as a system error — documented risk R-12.

Overall regression risk: **low**. The feature is additive (new columns, new optional fields,
new module) with no modifications to existing write paths. The schema migration is guarded
by pre-checks and outer transaction atomicity.

---

## PR Comments

- Posted 1 comment on PR #534 (security assessment summary).
- Blocking findings: no.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the `write_pool_server()` cross-crate read workaround
  is already documented in entry #3028. The parameterized SQL pattern is a project-wide
  convention (entry #358). The NaN guard pattern is documented in entries #4133 and #3525.
  No new generalizable anti-patterns emerged from this review.
