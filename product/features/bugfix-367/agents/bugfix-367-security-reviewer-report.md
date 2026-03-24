# Security Review: bugfix-367-security-reviewer

## Risk Level: low

## Summary

The diff consists of two constant changes and one test update in `background.rs`. No new logic, no new external inputs, no new SQL interpolation, no new dependencies. The primary security-relevant question is whether raising `DEAD_KNOWLEDGE_SESSION_THRESHOLD` from 20 to 1000 breaks the SQLite bind-parameter limit for the dynamic IN-clause in `fetch_recent_observations_for_dead_knowledge`. It does not: the bundled `libsqlite3-sys` compiles with `SQLITE_MAX_VARIABLE_NUMBER = 32766`, making a 1000-parameter IN-clause safe. All other OWASP concerns reviewed below are unchanged from the pre-fix state (not introduced by this diff).

## Findings

### Finding 1: Dynamic SQL IN-clause at 1000 bind parameters — within SQLite bundled limit
- **Severity**: low (informational, not a defect)
- **Location**: `crates/unimatrix-server/src/background.rs` lines 931–946 (`fetch_recent_observations_for_dead_knowledge`)
- **Description**: `DEAD_KNOWLEDGE_SESSION_THRESHOLD` now controls both the `LIMIT ?1` in Step A (capped at 1000 session IDs) and the number of `?N` bind parameters in the Step B IN-clause. The session IDs returned by Step A are database-internal values (fetched from the `observations` table `session_id` column) and are never user-supplied strings — they are bound as parameterised values, not interpolated into the SQL text. This is not an injection vector.

  The concern evaluated here is the SQLite bind-variable limit. The prior security review for bugfix-351 (finding 4) noted the IN-clause was bounded at 20 parameters and was safe against the common 999-parameter assumption. With this fix the bound rises to 1000. Verified: the bundled `libsqlite3-sys-0.30.1` compiles `sqlite3/sqlite3.c` with `#define SQLITE_MAX_VARIABLE_NUMBER 32766`. A 1000-parameter IN-clause is well within this limit.

  No SQL injection surface exists. No limit violation.
- **Recommendation**: None required. The bound is correct and safe. Optionally document the 32766 limit vs the 1000 constant in a comment near `DEAD_KNOWLEDGE_SESSION_THRESHOLD` for future maintainers.
- **Blocking**: no

### Finding 2: `extract_entry_ids` parses `response_snippet` from database rows — no injection surface
- **Severity**: low (informational, unchanged from pre-fix state)
- **Location**: `crates/unimatrix-observe/src/extraction/dead_knowledge.rs` lines 107–137
- **Description**: Entry IDs used for the "protected" set in `detect_dead_knowledge_candidates` are extracted by text-parsing `response_snippet` values stored in the `observations` table. These snippets originate from MCP tool responses captured by the hook system. The parsing uses pure string splitting and `parse::<u64>()` with no `format!` or string interpolation back into SQL. Extracted IDs are only used as a lookup set (`HashSet<u64>`) to filter the deprecation candidate list — they never flow back into a SQL query. Not an injection vector. This finding is not introduced by the diff; it is noted for completeness.
- **Recommendation**: None required.
- **Blocking**: no

### Finding 3: No new hardcoded secrets or credentials
- **Severity**: n/a
- **Location**: entire diff
- **Description**: Diff contains no API keys, tokens, passwords, or credentials. The only string constants are the SQL query text and the counters key `"dead_knowledge_migration_v1"` (unchanged). The `DEAD_KNOWLEDGE_MIGRATION_V1_KEY` string constant is a database row key, not a secret.
- **Blocking**: no

### Finding 4: No new dependencies introduced
- **Severity**: n/a
- **Location**: entire diff
- **Description**: No `Cargo.toml` changes in the diff. No new crates added. No dependency surface change.
- **Blocking**: no

### Finding 5: No new unsafe code or `.unwrap()` in production paths
- **Severity**: n/a
- **Location**: entire diff
- **Description**: Gate 3b report confirms zero new `unsafe` blocks and zero new `.unwrap()` calls in non-test code. The test change uses `DEAD_KNOWLEDGE_SESSION_THRESHOLD + 1` as an integer arithmetic expression — no overflow possible for a `usize` constant of 1000. Confirmed by reading the diff.
- **Blocking**: no

## Blast Radius Assessment

**Worst case if the fix has a subtle bug:**

The changed code runs in the background maintenance tick (`dead_knowledge_deprecation_pass`). The failure modes are:

1. **Over-deprecation** — If `detect_dead_knowledge_candidates` incorrectly returns a live entry as a candidate, that entry is marked `Deprecated`. Status changes are reversible (`context_correct` can restore `Active`). Data is not deleted. The cap at 50 per tick limits the blast radius to 50 entries per maintenance cycle.

2. **Under-deprecation** — If the function returns `None` (insufficient sessions guard), no entries are deprecated. The system continues to function normally; stale knowledge simply stays active longer. This is the benign failure mode.

3. **SQLite error on large IN-clause** — If the session ID count approaches or exceeds `SQLITE_MAX_VARIABLE_NUMBER` (32766 for the bundled build), `sqlx` would return an error. The error is handled by `return vec![]` (line 951), which causes the deprecation pass to skip that tick. No crash, no data corruption — the pass simply does nothing that tick.

4. **Memory pressure** — Fetching all observations for 1000 sessions could be a large result set on a high-activity instance. The function runs in `spawn_blocking` which won't block the tokio event loop, but excessive memory allocation in the blocking thread pool is possible if the observations table is very large. The cap at `DEAD_KNOWLEDGE_DEPRECATION_CAP = 50` applies to deprecation writes only, not to the observation fetch volume. This is a latent resource concern for future scaling, not a security issue.

**Conclusion**: Worst-case failure is non-data-destroying. The most severe realistic outcome is temporary over-deprecation of up to 50 active entries per tick, which is reversible.

## Regression Risk

The only regression risk is the test change: `insert_synthetic_sessions(&store, DEAD_KNOWLEDGE_SESSION_THRESHOLD + 1)` now inserts 1001 sessions in the test. This is a strictly correct approach — the test now verifies that the full fetch-and-detect pipeline works at the new threshold. The test runtime may increase due to 1001 database inserts vs 6, but this is a test performance concern, not a correctness concern.

No production paths were changed other than the two constant values. The session-based two-step query (`fetch_recent_observations_for_dead_knowledge`) was introduced in bugfix-351; this fix only changes the `LIMIT` parameter from 20 to 1000. All error handling paths are identical.

## PR Comments

- Posted 1 comment on PR #368 (see below)
- Blocking findings: no

## Knowledge Stewardship

- Attempted: /uni-store-lesson for "Bundled SQLite SQLITE_MAX_VARIABLE_NUMBER is 32766, not 999" — blocked: `uni-security-reviewer` lacks Write capability in Unimatrix (MCP error -32003). Lesson content is preserved in this report. A privileged agent can store it via context_store with topic: security, category: lesson-learned, tags: [sqlite, dynamic-sql, bind-parameters, security-review, libsqlite3-sys, bundled]. The lesson: when reviewing dynamic IN-clause SQL in Rust crates using the `bundled` feature of `libsqlite3-sys`, the effective bind-parameter limit is 32766 (not 999). The 999 assumption applies only to older or system-installed SQLite; always verify against the compiled constant in the cargo registry source.
