# Security Review: bugfix-528-security-reviewer

## Risk Level: low

## Summary

The change is a surgical SQL filter flip in the co_access promotion tick: four JOIN conditions change from denylist (`status != Quarantined`) to allowlist (`status = Active`), and the bound parameter changes from `Status::Quarantined` to `Status::Active`. All OWASP-relevant categories were evaluated and no security concerns were identified. The change is logically correct, minimal, and consistent with the existing compaction DELETE which already uses the same allowlist pattern.

## Findings

### Finding 1 — SQL Parameterization (Injection)
- **Severity**: low (informational — no issue found)
- **Location**: `crates/unimatrix-server/src/services/co_access_promotion_tick.rs:232-251`
- **Description**: All SQL parameters are bound via sqlx positional placeholders (`?1`, `?2`, `?3`). No string interpolation or format! macros are used. The status value is bound as `Status::Active as u8 as i64` — a compile-time constant, not derived from any external input. SQL injection risk is zero.
- **Recommendation**: None — existing parameterization is correct.
- **Blocking**: no

### Finding 2 — Status Numeric Encoding Correctness
- **Severity**: low (informational — no issue found)
- **Location**: `crates/unimatrix-store/src/schema.rs:10-15` vs `co_access_promotion_tick.rs:251`
- **Description**: Verified the `Status` enum directly: `Active = 0`, `Deprecated = 1`, `Proposed = 2`, `Quarantined = 3`. The bind `Status::Active as u8 as i64` resolves to `0i64` at compile time. The compaction DELETE in `background.rs:518` uses the identical binding pattern `Status::Active as u8 as i64`. Both sides of the compaction/promotion symmetry now use the same encoding, eliminating the oscillation root cause.
- **Recommendation**: None — encoding is correct and consistent.
- **Blocking**: no

### Finding 3 — Blast Radius of Allowlist Flip
- **Severity**: low
- **Location**: `co_access_promotion_tick.rs:239-244`
- **Description**: Worst case if the fix has a subtle bug: the tick silently promotes nothing (if `Status::Active` had the wrong value, `status = 0` would match no rows, `qualifying_count == 0`, tick returns early after logging). The failure mode is data starvation (no new graph edges), not data corruption. The existing SR-05 early-tick warn! at `current_tick < 5` would surface this signal loss within five ticks. Existing tests (35 co_access_promotion tests) cover the zero-qualifying-pairs early-return path.
- **Recommendation**: None — failure mode is safe and observable.
- **Blocking**: no

### Finding 4 — typed_graph.rs Comment-Only Change
- **Severity**: low (informational — no issue found)
- **Location**: `crates/unimatrix-server/src/services/typed_graph.rs:95-109`
- **Description**: The only behavioral change in typed_graph.rs is a comment expansion. The filter line (`filter(|e| e.status != Status::Quarantined)`) is unchanged. No logic was altered. The comment explicitly warns future maintainers not to filter deprecated nodes, which is the correct defensive documentation pattern.
- **Recommendation**: None.
- **Blocking**: no

### Finding 5 — No New Dependencies
- **Severity**: low (informational — no issue found)
- **Description**: Diff introduces no new crate dependencies, no new `Cargo.toml` entries, and no version bumps. No CVE exposure introduced.
- **Recommendation**: None.
- **Blocking**: no

### Finding 6 — No Hardcoded Secrets or Credentials
- **Severity**: low (informational — no issue found)
- **Description**: Full diff contains no tokens, API keys, passwords, or credential literals. The only literals introduced are SQL strings and comments.
- **Recommendation**: None.
- **Blocking**: no

### Finding 7 — Access Control and Trust Boundaries
- **Severity**: low (informational — no issue found)
- **Description**: The promotion tick runs as a background server-internal process. It reads from `co_access` (server-populated) and writes to `graph_edges` (server-populated). No external input crosses a trust boundary in this code path. The status filter change does not alter who can call the tick or what data callers can supply.
- **Recommendation**: None.
- **Blocking**: no

### Finding 8 — Input Validation at System Boundaries
- **Severity**: low (informational — no issue found)
- **Description**: No new inputs from external sources (MCP tool params, file paths, user data) are introduced by this change. The `?3` bind is a compile-time enum variant, not a runtime value from any external caller. No validation gap exists.
- **Recommendation**: None.
- **Blocking**: no

## Blast Radius Assessment

Worst case if the fix has a subtle bug:

1. **Wrong status value bound**: If `Status::Active as u8 as i64` produced the wrong integer (it resolves to `0` — verified in schema.rs), the JOIN would match no entries, `qualifying_count == 0`, and the tick would return early. The SR-05 early-tick warn! fires within 5 ticks. Graph edges stagnate but are not corrupted. Recoverable on next correct deploy.

2. **Subquery filter missed on one side**: If ea2 or eb2 used the wrong operator, max_count would be inflated, producing deflated weights on promoted edges (the original bug). The new `test_deprecated_endpoint_pair_not_promoted` test directly catches this: it asserts `weight == 1.0`; a missed subquery filter yields `0.5`. This regression cannot silently land.

3. **typed_graph.rs breakage**: Since typed_graph.rs has no logic change (comment only), breakage is impossible.

Overall blast radius is bounded: no data is deleted, no access control is widened, and the failure mode is stagnant graph edges (weight data starvation), not corruption or privilege escalation.

## Regression Risk

**Low.** The change symmetrizes the promotion tick with the compaction DELETE (which already used the allowlist since bugfix-471). All 35 existing co_access_promotion tests pass. The 2769 unimatrix-server unit tests and full smoke/lifecycle integration suites pass per the verifier report. The only behavioral change is that Deprecated and Proposed endpoint pairs are now excluded from promotion (they were already excluded from graph_edges by compaction DELETE, so oscillation stops).

The documented pre-existing xfails/xpasses are unrelated to this change (tick timing, MCP injection, backward compat tests).

No existing functionality could break: the allowlist is strictly narrower than any realistic active-plus-deprecated data set in ways that were already expected to be excluded by the compaction side.

## PR Comments

- Posted 1 comment on PR #530
- Blocking findings: no

## Knowledge Stewardship

- Stored: nothing novel to store — entry #4162 already captures the generalized lesson (allowlist on ALL passes of the promotion tick SELECT, including subquery aliases ea2/eb2, with test design requiring the deprecated pair to have a higher count than the active pair). The security reviewer adds no new pattern beyond what entry #4162 documents. Reason: the fix is exactly the pattern already recorded; no new anti-pattern surface.
