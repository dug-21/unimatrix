# Security Review: col-024-security-reviewer

**PR**: #373
**Branch**: feature/col-024
**Reviewer agent**: col-024-security-reviewer
**Date**: 2026-03-24

---

## Risk Level: ADVISORY

## Summary

All five changed files were reviewed cold against the full diff and affected source. The SQL queries in `load_cycle_observations` use exclusively bound parameters — no SQL injection risk is present. Integer overflow is correctly guarded with `saturating_mul`. The `tracing-test` dev-dependency is a well-known crate with a pinned lockfile checksum and is confined to `[dev-dependencies]`, so it does not reach the production binary. One advisory finding requires attention before merge: `validate_retrospective_params` does not apply a length bound or control-character check on the `feature_cycle` string that flows into SQL `.bind()` calls and stored log messages, creating a DoS amplification path and a log-injection surface even though SQL semantics are safe. All other focus areas are clean.

---

## Findings

### Finding 1: feature_cycle has no length or character bound in validate_retrospective_params

- **Severity**: medium
- **Location**: `crates/unimatrix-server/src/infra/validation.rs:507-515`
- **Description**: `validate_retrospective_params` checks only that `feature_cycle.trim()` is non-empty. It does not call `validate_string_field` with `MAX_FEATURE_CYCLE_LEN` (128) or `check_control_chars`. The `feature_cycle` value flows unmodified to three SQL `.bind()` calls in `load_cycle_observations` (Step 0 `COUNT`, Step 1 `SELECT`, Step 2 per-window `SELECT`) and to two `tracing::debug!` log sites. Compare with `validate_store_params` (line 195-196), which does call `validate_string_field("feature_cycle", fc, MAX_FEATURE_CYCLE_LEN, false)` on the same field. The omission is a regression relative to the existing convention. By contrast, all SQL queries use `sqlx` bound parameters, so the risk is not SQL injection — it is (a) a potentially unbounded bind value propagated to every DB query for the lifetime of the `spawn_blocking_with_timeout` call, and (b) ANSI control characters written to structured log events from an MCP caller, a log-injection surface. A 64 KB `feature_cycle` string would also cause the debug log to emit a very large record.
- **Recommendation**: Add `validate_string_field("feature_cycle", &params.feature_cycle, MAX_FEATURE_CYCLE_LEN, false)?;` immediately after the empty-check in `validate_retrospective_params`. This mirrors the pattern already in `validate_store_params` and closes the inconsistency. The existing tests should be extended with a test for a 129-character `feature_cycle` and a control-character `feature_cycle`.
- **Blocking**: no — SQL is safe (parameterized throughout); the risk is log injection and DoS amplification. Advisory: fix before merge is recommended but the change is additive and low risk.

---

### Finding 2: session_ids in Step 2 / Step 3 of load_cycle_observations has no upper bound

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/observation.rs:391-434`
- **Description**: Step 2 accumulates DISTINCT `session_id` values from the `observations` table per time window into a `HashSet<String>`, then constructs a dynamically-sized SQL IN-clause with one `?N` placeholder per session. There is no cap. For a very large or long-running cycle with many distinct sessions (or adversarially many observations with `topic_signal` set to the target `cycle_id`), this produces a query with an arbitrarily large IN-clause. SQLite's default `SQLITE_LIMIT_VARIABLE_NUMBER` is 999. Exceeding it causes a runtime SQL error (`too many SQL variables`), which `map_err` converts to `ObserveError::Database`. This is a safe failure mode — the error propagates to the MCP caller — but the error surface and the `spawn_blocking_with_timeout` slot are consumed for every such call. The pre-col-024 `load_feature_observations` path has the same shape (lines 127-144) but was already present in main; it is noted for completeness. The new `load_cycle_observations` path introduces additional windows that compound this.
- **Recommendation**: Add a `MAX_SESSIONS_PER_CYCLE` constant (e.g. 500, well below SQLite's 999 variable limit) and return `ObserveError::Database` or a distinct error variant if `session_ids.len()` exceeds it before constructing the IN-clause. Alternatively, batch the IN-clause into groups of 500. This is a defensive depth-of-defense measure; it does not affect the current operational scale.
- **Blocking**: no — existing code in main has the same shape; the failure mode is an observable error, not silent data loss. Recommended follow-up.

---

### Finding 3: open-ended window (ADR-005) creates time-unbounded observation inclusion

- **Severity**: low (acknowledged design trade-off, documented in ADR-005)
- **Location**: `crates/unimatrix-server/src/services/observation.rs:374-380`
- **Description**: When a `cycle_start` event has no paired `cycle_stop`, the implementation uses `unix_now_secs()` as the window end. This means an abandoned (never-stopped) cycle will match ALL observations bearing the cycle's `topic_signal` up to the present moment, not just those at the time of the review call. If `topic_signal` enrichment writes any sessions under this cycle after the cycle was informally abandoned, they are included. This is a known correctness trade-off documented in ADR-005, not a security vulnerability. It is flagged here because an attacker who can issue hook events with a controlled `topic_signal` could inflate the observation set for any active-but-paused cycle.
- **Recommendation**: ADR-005 documents the known limitation. No action required unless an attacker model considers hook IPC an untrusted channel; if so, validate `topic_signal` at ingress against a registry of active cycles. Current UDS authentication (peer-UID Layer 2) limits this to same-UID processes. Accepted risk.
- **Blocking**: no.

---

### Finding 4: debug log in enrich_topic_signal emits session_id and feature strings

- **Severity**: low / informational
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:140-146`
- **Description**: The `tracing::debug!` call logs `session_id`, `extracted_signal`, and `registry_feature`. The `session_id` field is validated to `[a-zA-Z0-9-_]` max 128 chars (an opaque internal identifier); `extracted_signal` and `registry_feature` are feature-cycle identifiers (e.g., "col-024"). No name, email, credential, or user-generated free-text is logged. This is a debug-level event, suppressed by default. No PII concern.
- **Recommendation**: None. The log content is structurally safe. Future additions to `extracted_signal` or `registry_feature` should be reviewed if they ever carry free-text from external inputs.
- **Blocking**: no.

---

### Finding 5: tracing-test dev-dependency supply chain

- **Severity**: informational
- **Location**: `crates/unimatrix-server/Cargo.toml:57`, `Cargo.lock`
- **Description**: `tracing-test = "0.2"` resolves to `0.2.6` (checksum `19a4c448db514d4f24c5ddb9f73f2ee71bfb24c526cf0c570ba142d1119e0051`). It is placed in `[dev-dependencies]` only, so it is excluded from the production binary and the installed `unimatrix` executable. `tracing-test` is a well-established crate in the Rust tracing ecosystem. The two dependencies it introduces (`tracing-core`, `tracing-subscriber`, `tracing-test-macro`) are all already present in the workspace from production dependencies. No new transitive production code is introduced. No supply chain concern.
- **Recommendation**: None.
- **Blocking**: no.

---

## SQL Injection Assessment (Focus Area 1)

All SQL queries in `load_cycle_observations` use `sqlx` bound parameters exclusively:
- Step 0: `WHERE cycle_id = ?1` with `.bind(&cycle_id)` — clean.
- Step 1: `WHERE cycle_id = ?1 ORDER BY timestamp ASC` with `.bind(&cycle_id)` — clean.
- Step 2: `WHERE topic_signal = ?1 AND ts_millis >= ?2 AND ts_millis <= ?3` — all three values bound, `cycle_id` is `?1` — clean.
- Step 3: dynamic IN-clause uses positional markers `?3..?N` with values bound in order — clean. `cycle_id` is not in this query; only session IDs (which passed through DISTINCT from a prior query) are bound.

The `format!` macro is used only to build the IN-clause skeleton (`?3`, `?4`, ...) — user-controlled input is never interpolated into the SQL string. This pattern is consistent with the pre-existing `load_feature_observations` implementation.

---

## Integer Overflow Assessment (Focus Area 3)

`cycle_ts_to_obs_millis` at observation.rs:495-497 uses `ts_secs.saturating_mul(1000)`. For the current epoch (approximately 1.74 × 10^12 seconds), multiplication by 1000 yields ~1.74 × 10^15, well within i64::MAX (9.22 × 10^18). The saturating behavior is correct for adversarially large values (i64::MAX → i64::MAX, not panic). The helper is called in a single location in the loop body and once for the open-ended window. No overflow path exists.

---

## Blast Radius Assessment

The `load_cycle_observations` method is called from a single `spawn_blocking_with_timeout` closure inside `context_cycle_review`. The three-path fallback is designed so:

- `Err` from `load_cycle_observations` propagates via `?` directly to the MCP error response. It does NOT activate the legacy fallback paths. This is the correct behavior documented in FM-01 — a SQL failure is not silently masked.
- `Ok(vec![])` activates the legacy `load_feature_observations` fallback (Path 2), then `load_unattributed_sessions` (Path 3). This path was present before col-024 and is unchanged.

Worst case if `load_cycle_observations` has a subtle bug:
1. Returns `Ok(vec![])` incorrectly: the tool falls back to legacy paths, producing a stale or content-attributed report. Visible in the output (report says "cached" or uses legacy attribution). No data corruption, no privilege escalation.
2. Returns `Err` on every call: `context_cycle_review` always fails with an observable MCP error. No data corruption; legacy paths are bypassed. Recoverable by fixing the bug.
3. Over-includes observations (open-ended window, ADR-005): report contains extra observations, inflating metrics. Confidentiality: observation records from other sessions with matching `topic_signal` may appear in the report. This is scoped to the MCP caller who already has `Read` capability.

No silent data corruption path was found. The failure mode is always observable.

---

## Regression Risk

The four `enrich_topic_signal` call sites in listener.rs replace a bare `obs.topic_signal = obs.topic_signal` (or equivalent `None` pass-through) with a Mutex read on `session_registry`. The Mutex uses `unwrap_or_else(|e| e.into_inner())` poison recovery (confirmed at session.rs:206), so a poisoned lock does not panic — it returns the last-written state. Lock starvation is not a concern: the lock is held for a single `HashMap::get` and clone (microseconds), and the batch path in `RecordEvents` holds the lock once per event in a serial `.map()` over the batch. There is no recursion or re-entrant lock acquisition.

The new test coverage (`T-CCR-01` through `T-CCR-04`) validates the three-path fallback logic including error propagation, which is the highest-risk regression path.

The `tracing-test` dev-dependency does not affect the production binary.

---

## Checklist

- [x] Full git diff was read
- [x] Root cause analysis (agent-3-risk-report) read from disk
- [x] All affected source files read in full (not just diff hunks)
- [x] OWASP concerns evaluated for each changed file
- [x] Blast radius assessed — worst case scenario named
- [x] Input validation checked at system boundaries
- [x] No hardcoded secrets in the diff
- [x] Findings posted as PR comments via gh CLI
- [x] Risk level accurately reflects findings
- [x] Report written to the correct agent report path

---

## Knowledge Stewardship

- Stored: nothing novel to store — the `validate_retrospective_params` missing length bound is a specific finding for this PR, not a generalizable pattern. The existing conventions around `validate_string_field` usage are already expressed in the codebase. If this pattern recurs in a second feature, store via /uni-store-lesson with topic "security / validation gap for MCP-facing string params".
