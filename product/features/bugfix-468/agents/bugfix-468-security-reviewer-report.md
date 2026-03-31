# Security Review: bugfix-468-security-reviewer

## Risk Level: low

## Summary

The fix is a two-line SQL query change in `SqlxStore::get_cycle_start_goal` (db.rs) — adding `AND goal IS NOT NULL` and reversing the `ORDER BY` direction from DESC to ASC. No new dependencies, no new external inputs, no access control changes. All inputs are parameterised via sqlx bind parameters. The change is minimal and surgical, with no unrelated code touched.

## Findings

### Finding 1 — SQL injection: not present
- **Severity**: informational
- **Location**: `crates/unimatrix-store/src/db.rs:362`
- **Description**: `cycle_id` is the only user-supplied parameter. It flows into the query exclusively via sqlx's `?1` positional bind — never via string interpolation. The query body itself is a static string literal. No injection surface was introduced or widened by this change.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 2 — Input validation at trust boundaries: confirmed present
- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/uds/listener.rs:554-555`, `listener.rs:742`, `listener.rs:826`, `listener.rs:2280`
- **Description**: The `cycle_id` value reaching `get_cycle_start_goal` comes from two call sites. The UDS path (`listener.rs:578`) passes `clean_feature`, which has been run through `sanitize_metadata_field` (strips non-printable ASCII, truncates to 128 chars) before use. The MCP tool path (`tools.rs:2018`) passes `feature_cycle` sourced from `params.feature_cycle` — this is an MCP JSON parameter deserialized from the rmcp layer, not directly from raw sockets. Neither call site was modified by this fix.
- **Recommendation**: No action required for this fix. The pre-existing sanitisation coverage is consistent.
- **Blocking**: no

### Finding 3 — Ordering reversal: semantic correctness, not a security concern
- **Severity**: informational
- **Location**: `crates/unimatrix-store/src/db.rs:363`
- **Description**: Changing `ORDER BY timestamp DESC, seq DESC` to `ASC, ASC` changes which row wins when duplicates exist. The `AND goal IS NOT NULL` filter means only rows with a non-NULL goal are candidates. The `LIMIT 1` cap on ASC ordering means the earliest non-NULL goal is returned. This is the correct "first-written-goal-wins" semantic. There is no security implication — the goal string is display-only data, not an access control predicate.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 4 — `Option::flatten()` on query result: sound
- **Severity**: informational
- **Location**: `crates/unimatrix-store/src/db.rs:377`
- **Description**: The `fetch_optional` return is `Option<Option<String>>`. With the `AND goal IS NOT NULL` filter in place, a matched row will never carry a NULL goal — `Some(None)` cannot occur from this query. The `flatten()` call therefore always maps `Some(Some(s))` to `Some(s)` and `None` (no row) to `None`. The pre-existing comment (lines 371-376) is still accurate in the general sense, and the code is correct and safe. No information is leaked.
- **Recommendation**: No action required. The comment could note that `Some(None)` is now unreachable post-filter, but this is cosmetic.
- **Blocking**: no

### Finding 5 — No hardcoded secrets
- **Severity**: informational
- **Location**: all changed files
- **Description**: No passwords, tokens, API keys, or credentials appear anywhere in the diff.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 6 — No new dependencies
- **Severity**: informational
- **Location**: Cargo.toml / Cargo.lock (unchanged)
- **Description**: The diff touches only `crates/unimatrix-store/src/db.rs` and `crates/unimatrix-store/tests/migration_v15_to_v16.rs`. No dependency changes. No CVE exposure introduced.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 7 — No unsafe code
- **Severity**: informational
- **Location**: all changed files
- **Description**: No `unsafe` blocks in any changed file, consistent with the gate report.
- **Recommendation**: No action required.
- **Blocking**: no

## Blast Radius Assessment

The changed function is `get_cycle_start_goal`, a read-only query called from two places:

1. `uds/listener.rs:578` — goal resume on session start. If the query had a subtle bug and returned `None` erroneously, the session would start without a resumed goal. The failure mode is `unwrap_or_else(|e| { warn; None })` — safe degradation, no panic, no data corruption.

2. `mcp/tools.rs:2018` — goal field in `context_cycle_review`. If the query returned `None` erroneously, `report.goal` remains `None` and `report.cycle_type` remains `None`. The failure mode is a warn log and continued execution — the review still returns, minus the goal field. No data is corrupted or lost.

Worst case of a subtle regression in this fix: the `context_cycle_review` response omits the `goal` field for affected cycles. That is observable and recoverable. No write path is touched. No access control decisions depend on the returned goal value.

## Regression Risk

Low. The fix narrows the query result set (filters out NULL rows) and reverses sort order to select the earliest non-NULL row instead of the latest row regardless of NULL. Existing callers are unchanged. The pre-existing tests T-V16-10 through T-V16-13 continue to exercise the no-null, single-row, and unknown-cycle-id cases. The two new tests (T-V16-14 renamed, T-V16-15 new) directly cover the multi-row and NULL-shadowing scenarios.

The only regression risk is the ASC ordering change affecting a caller that previously relied on "latest goal wins" semantics. Review of both call sites shows neither has any logic that depends on which of multiple cycle_start goals is returned — both use the value as a display field. The change in ordering cannot trigger a security-relevant regression.

## PR Comments

- Posted 1 comment on PR #472.
- Blocking findings: no.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` for SQL injection / input validation / trust boundaries. Results (#3569, #1265, #1039, #1276, #1544) confirmed existing patterns for validation at entry points — no new pattern introduced by this fix.
- Stored: nothing novel to store — the sqlx parameterised-bind pattern and sanitize_metadata_field coverage are pre-existing conventions, not introduced here.
