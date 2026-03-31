# Security Review: crt-036-security-reviewer

## Risk Level: low

## Summary

PR #463 implements a cycle-based activity GC replacing the previous 60-day wall-clock DELETE.
All SQL uses parameterized queries with no user-controlled input reaching GC query time.
The blast radius is bounded: GC only touches the five activity tables (observations, query_log,
injection_log, sessions, audit_log), protected tables are untouched, and both legacy DELETE sites
are fully removed. No new dependencies were introduced. No findings are blocking.

---

## Findings

### Finding 1: SQL injection surface — production GC queries
- **Severity**: low
- **Location**: `crates/unimatrix-store/src/retention.rs` lines 63–283
- **Description**: All production GC methods (`list_purgeable_cycles`, `gc_cycle_activity`,
  `gc_unattributed_activity`, `gc_audit_log`) use `sqlx::query(...).bind(...)` with typed
  bind parameters. No string interpolation of external input occurs. The `feature_cycle` values
  passed to `gc_cycle_activity` and subqueries originate from `cycle_review_index` (a prior
  internal write), not from any MCP request parameter at GC time. The `retention_days` and `k`
  values come from validated `RetentionConfig`, which aborts startup on out-of-range values.
  No SQL injection vector exists.
- **Recommendation**: No action required. Pattern is correct.
- **Blocking**: no

### Finding 2: Format! in test helper count_for_session / count_table (test code only)
- **Severity**: low (test code only — not production)
- **Location**: `crates/unimatrix-store/src/retention.rs` lines 385–402
- **Description**: The test helpers `count_for_session` and `count_table` build a SQL string
  with `format!("SELECT COUNT(*) FROM {table} ...")`. The `table` parameter is always a
  string literal supplied at every call site within the test module (e.g., `"observations"`,
  `"entries"`). This is test-only code behind `#[cfg(test)]` and never reachable in production.
  Not an injection risk in practice; no external input flows into these helpers.
- **Recommendation**: No action required for security. The pattern is acceptable in test code.
  Future developers should note the pattern and avoid replicating it in production store methods.
- **Blocking**: no

### Finding 3: audit_log_retention_days lower bound permits aggressive audit deletion
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/infra/config.rs` — `RetentionConfig::validate()`
- **Description**: The validated lower bound for `audit_log_retention_days` is 1 day. An operator
  setting this to 1 deletes almost all audit history on each tick, eliminating most of the
  accountability record. The RISK-TEST-STRATEGY documents this as a known operational trade-off.
  It is not a code bug. An operator who can write `config.toml` already has full filesystem access
  to the database. The risk is operational misconfiguration, not an attack vector.
- **Recommendation**: The existing inline documentation comment is sufficient. No code change
  needed. Consider a startup-time `tracing::warn!` if `audit_log_retention_days < 30` to alert
  operators of unusually short retention, but this is enhancement-level guidance.
- **Blocking**: no

### Finding 4: Accountability record erasure via GC — blast radius
- **Severity**: low
- **Location**: `crates/unimatrix-store/src/retention.rs` `gc_audit_log()` (line 267–284)
- **Description**: The GC has DELETE authority on `audit_log`. The `audit_log` table is an
  accountability record (all MCP operations). Once rows are purged they are unrecoverable.
  A misconfigured `audit_log_retention_days` (minimum 1, default 180) would permanently destroy
  audit history. This is the designed behavior — audit_log retention is explicitly time-based and
  documented as separate from the cycle-based policy. Validate() prevents `0`. The default of 180
  days is reasonable for compliance purposes.
- **Recommendation**: No code change. The lower bound of 1 day and the clear documentation
  are the correct controls given the operator-administered deployment model.
- **Blocking**: no

---

## OWASP Assessment

| OWASP Category | Verdict | Evidence |
|----------------|---------|---------|
| A01 Broken Access Control | PASS | GC runs only in background tick; no MCP request path; write access to cycle_review_index already requires Admin capability |
| A03 Injection (SQL) | PASS | All GC SQL uses parameterized bind parameters; no string interpolation in production paths |
| A04 Insecure Design | PASS | Architecture documents blast radius explicitly; rate cap (max_cycles_per_tick) prevents DoS from unbounded GC; per-cycle transactions prevent write pool stall |
| A05 Security Misconfiguration | PASS (with note) | validate() aborts startup on invalid config; however, audit_log_retention_days = 1 is valid and deletes nearly all audit history — acceptable operational trade-off |
| A08 Data Integrity | PASS | SR-05 (summary_json clobber) mitigated via struct update syntax; struct update passes all fields including summary_json; gate check record retained in scope |
| A09 Logging & Monitoring | PASS | Structured tracing at info/warn levels with field-level context; no sensitive data logged |
| Deserialization | PASS | No new deserialization of untrusted data; RetentionConfig deserialized from operator-controlled config.toml, validated at startup |
| Secrets | PASS | No hardcoded credentials, API keys, or tokens in the diff |
| New Dependencies | PASS | No new crate dependencies added |

---

## Blast Radius Assessment

**Worst case if gc_cycle_activity has a subtle bug:**

A bug permitting the GC to delete sessions for a cycle that has NOT yet been reviewed
(i.e., the crt-033 gate fails open) would permanently destroy raw signals for that cycle.
The retrospective pipeline could not reconstruct the cycle review. This is the R-05 risk.

Mitigations in place:
1. `list_purgeable_cycles` SQL uses NOT IN on `cycle_review_index` — only cycles with a
   review row can appear in the purgeable set. The gate is structural, not just runtime.
2. The per-cycle `get_cycle_review` gate check adds defense-in-depth: `Ok(None)` skips
   the cycle with a `tracing::warn!`.
3. Per-cycle transactions mean a crash mid-delete restores all rows on restart (SQLite
   rollback guarantee).

**Worst case if store_cycle_review fails (step 4c) after gc_cycle_activity succeeds:**

`raw_signals_available` stays 1 while data is gone. The retrospective record (`summary_json`)
is intact. A future diagnostic could detect and repair the stale flag. This is low-severity
and accepted in ADR-001.

**Protected tables:** `entries`, `GRAPH_EDGES`, `cycle_events`, `cycle_review_index` (rows
only updated, never deleted), and `observation_phase_metrics` are not touched by any GC method.
Test `test_gc_protected_tables_regression` and `test_gc_protected_tables_row_level` provide
row-level verification.

---

## Regression Risk

**Level: low**

1. **Legacy DELETE removal (R-01)**: Both 60-day DELETE sites confirmed absent from
   `status.rs` and `tools.rs`. The old code is not present anywhere in the diff.

2. **run_maintenance() signature change**: `retention_config: &RetentionConfig` added as
   a parameter. Both call sites in `maintenance_tick()` and the test harness in
   `bugfix_444_tests` were updated. No other callers exist in the production path.

3. **Background tick threading**: `retention_config: Arc<RetentionConfig>` is threaded through
   `spawn_background_tick` → `background_tick_loop` → `run_single_tick` → `maintenance_tick` →
   `run_maintenance`. Pattern is identical to the existing `inference_config` threading.

4. **cycle_review_index.rs change**: Only whitespace/formatting. No logic change.

5. **eval/profile/layer.rs change**: Only whitespace/formatting. No logic change.

6. **search.rs**: Diff is empty — no actual changes.

7. **Existing behavior preserved**: Step 6 (`gc_sessions`, 30-day time-based cascade) is
   unchanged. The new cycle-based GC and the existing session GC target disjoint populations.

---

## PR Comments
- Posted 1 comment on PR #463 (findings summary)
- Blocking findings: no

---

## Knowledge Stewardship
- Stored: nothing novel to store — the SQL parameterized query pattern, the per-cycle
  transaction design, and the validate()-at-startup pattern are already in Unimatrix
  (entries #2159, #2249, #3766). The audit_log minimum-retention concern is feature-specific
  and has not recurred across multiple PRs to warrant a generalizable lesson.
