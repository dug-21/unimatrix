# Security Review: 198-security-reviewer

## Risk Level: low

## Summary
The fix addresses three feature_cycle attribution gaps with minimal, well-scoped changes across 3 files. Input from untrusted event payloads is properly sanitized via the existing `sanitize_metadata_field` function. No new dependencies, no SQL injection vectors, no secrets, no unsafe code. The fire-and-forget persistence pattern is consistent with existing codebase conventions.

## Findings

### Finding 1: Input Sanitization Applied Correctly
- **Severity**: informational
- **Location**: crates/unimatrix-server/src/uds/listener.rs:598-618
- **Description**: The `feature_cycle` field is extracted from `event.payload` (untrusted JSON from external hook clients) and passed through `sanitize_metadata_field()` before use. This function strips non-ASCII, control characters, and truncates to 128 chars. The sanitized value is used for both in-memory session state and SQL persistence.
- **Recommendation**: None -- correctly handled.
- **Blocking**: no

### Finding 2: Division Safety in Eager Attribution
- **Severity**: low
- **Location**: crates/unimatrix-server/src/infra/session.rs:256
- **Description**: `check_eager_attribution` computes `leader_tally.count as f64 / total_count as f64`. Division by zero is prevented by the `is_empty()` guard on line 244. If `topic_signals` is non-empty, `total_count` must be >= 1. Safe.
- **Recommendation**: None -- the guard is sufficient.
- **Blocking**: no

### Finding 3: Majority Vote Duplication
- **Severity**: low
- **Location**: crates/unimatrix-server/src/infra/session.rs:390-430 (majority_vote_internal)
- **Description**: The gate report notes this is "same algorithm as listener.rs `majority_vote`." Code duplication of vote logic means a future fix to one copy could miss the other. This is a maintainability concern, not a security concern.
- **Recommendation**: Consider extracting shared majority vote logic into a common function in a future cleanup.
- **Blocking**: no

### Finding 4: Fire-and-Forget Persistence Pattern
- **Severity**: low
- **Location**: listener.rs:607-612, listener.rs:636-648, status.rs:701-703
- **Description**: Feature_cycle persistence uses `spawn_blocking_fire_and_forget` (listener.rs) and `tokio::task::spawn_blocking` with `let _ =` (status.rs). If the write fails, only a `tracing::warn!` is emitted. This is consistent with the existing fire-and-forget pattern used throughout the codebase for session data, but means feature_cycle attribution can silently fail to persist to SQLite while the in-memory state shows it as resolved.
- **Recommendation**: Acceptable for the current use case (feature attribution is informational, not access-control). No change needed.
- **Blocking**: no

### Finding 5: No New SQL Injection Surface
- **Severity**: informational
- **Location**: crates/unimatrix-server/src/uds/listener.rs:2031-2039
- **Description**: `update_session_feature_cycle` delegates to `store.update_session()` which uses parameterized SQL queries (`?1` placeholders). The feature_cycle value is never interpolated into SQL strings.
- **Recommendation**: None.
- **Blocking**: no

## Blast Radius Assessment

**Worst case**: If `check_eager_attribution` has a subtle threshold bug (e.g., fires too eagerly with incorrect share calculation), sessions would get attributed to the wrong feature_cycle prematurely. The `set_feature_if_absent` guard means once set, the feature cannot be overridden -- so an incorrect early attribution would stick for the session lifetime. Impact: incorrect feature attribution in outcome tracking and retrospective analysis. This is a data quality issue, not a security or availability issue.

**Failure mode**: Safe. All new code paths either return `None`/`false` or write informational metadata. No panics in production paths (all `.unwrap()` calls are in test code only). Poison recovery via `unwrap_or_else(|e| e.into_inner())` is used consistently.

## Regression Risk

- **sweep_stale_sessions return type changed**: From `Vec<(String, SignalOutput)>` to `Vec<SweepResult>`. All callers updated (listener.rs and status.rs). Existing tests updated to use `.session_id` instead of `.0`. Low regression risk -- the compiler would catch any missed callsite.
- **Existing majority_vote in listener.rs unchanged**: The new `majority_vote_internal` in session.rs is additive. The existing SessionClose path in listener.rs still uses its own `majority_vote`. No regression to existing close-time behavior.
- **Formatting changes**: Large portions of the listener.rs and status.rs diffs are rustfmt reformatting (line wrapping, import reordering). These are semantically neutral.

## PR Comments
- Posted 1 comment on PR #200
- Blocking findings: no
