# Security Review: bugfix-279-security-reviewer

## Risk Level: low

## Summary

The fix introduces a compile-time constant `EXTRACTION_BATCH_SIZE: i64 = 1000` and a private synchronous helper `fetch_observation_batch()` that replaces an inline SQL query hardcoded to `LIMIT 10000`. The change is minimal, scoped to a single internal background processing function, introduces no new input surfaces, and preserves all existing error propagation paths. No OWASP concerns are raised by this diff.

## Findings

### Finding 1: No SQL Injection Risk from LIMIT Parameterization
- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/background.rs:882,892`
- **Description**: The LIMIT clause was changed from a hardcoded integer literal (`LIMIT 10000`) to a bind parameter (`LIMIT ?2`). Both approaches are safe. The implementation correctly uses `rusqlite::params![watermark as i64, EXTRACTION_BATCH_SIZE]`, which goes through the prepared statement parameter interface — no format string SQL construction, no user-controlled input in the query.
- **Recommendation**: No action required. The implementation chose the cleaner approach over the `format!()` alternative the investigator noted.
- **Blocking**: no

### Finding 2: Integer Cast watermark as i64
- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/background.rs:892`
- **Description**: The `watermark: u64` parameter is cast to `i64` for the rusqlite bind. This was pre-existing behavior in the original code. A watermark value exceeding i64::MAX would require more than 2^63 rows in the observations table, which is not a realistic threat. The function is private and the watermark value is controlled entirely by `ctx.last_watermark` within the extraction pipeline.
- **Recommendation**: No action required. The cast is safe for any realistically reachable watermark value.
- **Blocking**: no

### Finding 3: Deserialization of input Field (Pre-existing)
- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/background.rs:929`
- **Description**: `serde_json::from_str(&s).ok()` is used to parse the `input` column for non-SubagentStart hooks. This is pre-existing behavior not introduced by this fix. Malformed JSON silently becomes `None`, which is a safe failure mode. No new deserialization surface is introduced by this PR.
- **Recommendation**: Pre-existing. Out of scope for this fix.
- **Blocking**: no

### Finding 4: No Hardcoded Secrets
- **Severity**: informational
- **Location**: entire diff
- **Description**: No credentials, API keys, tokens, or secrets appear anywhere in the diff. The only new constant is a batch size integer.
- **Recommendation**: None.
- **Blocking**: no

### Finding 5: Uncommitted xfail Marker (Gate WARN — Resolved)
- **Severity**: informational
- **Location**: `product/test/infra-001/suites/test_adaptation.py`
- **Description**: The gate report flagged the xfail marker as uncommitted (WARN). Verification via `git log main..HEAD` confirms commit `3625dc8` ("test(availability): cap extraction tick batch and xfail pre-existing adaptation test (#279)") includes `test_adaptation.py`. The gate warning is resolved; the marker is committed.
- **Recommendation**: No action required.
- **Blocking**: no

## Blast Radius Assessment

The change is confined entirely to `extraction_tick()` in `background.rs`. No store layer, vector index, extraction rules, or MCP handler dispatch paths are modified.

Worst case if the fix has a subtle bug: the watermark advances incorrectly.
- **Watermark advances too far**: observations skipped — silent loss of derived knowledge entries (not raw data). Detectable via `entries_rejected_total` counter going unexpectedly low. Safe failure mode.
- **Watermark stalls at 0**: all observations re-fetched every tick — duplicate extraction proposals, caught by the quality gate's near-duplicate check. Server continues to function; backlog never clears. Observable via metrics.
- **Neither failure** causes data corruption, privilege escalation, denial of service, or information disclosure.

## Regression Risk

Low. The only behavioral change is that observation backlogs exceeding 1000 rows are distributed across multiple ticks rather than processed in one pass. Extraction rules that require co-occurring observations from the same session (e.g., `DeadKnowledgeRule` needing 5 sessions) may see partial session coverage if observations straddle a batch boundary. The investigator assessed this as an acceptable eventual-consistency trade-off at the 15-minute tick interval.

Availability suite (6/6 pass, including `test_concurrent_ops_during_tick` and `test_read_ops_not_blocked_by_tick`) directly confirms no availability regression. Full workspace: 2533 tests passed, 0 failed.

## Dependency Safety

No new dependencies introduced. The only crate touched is `unimatrix-server` with existing dependencies (`rusqlite`, `serde_json`, `tokio`).

## PR Comments
- Posted 1 comment on PR #290 with full OWASP assessment, blast radius analysis, and verdict
- Self-approve attempted but blocked (author cannot approve own PR — expected in solo workflow)
- Blocking findings: no

## Knowledge Stewardship
- nothing novel to store — this fix is a straightforward batch size refactor. The generalizable lesson (named EXTRACTION_BATCH_SIZE constant + bind parameter pattern) was already stored as entry #1736 by the investigator agent (279-investigator). No new anti-pattern or security finding emerged that warrants a separate lesson-learned entry.
