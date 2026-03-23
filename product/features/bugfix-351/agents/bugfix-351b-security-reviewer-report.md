# Security Review: bugfix-351b-security-reviewer

## Risk Level: low

## Summary

Wave-B introduces two performance-correctness fixes to the dead-knowledge pipeline: a
session-based two-step observation fetch in `background.rs` and an EXISTS dedup query in
`recurring_friction.rs`. Both use parameterized SQL; no new external trust boundaries are
opened; no injection vectors are present. The `source_domain` hardcoding finding from
wave-A (Finding 2) is resolved. The full-topic scan finding from wave-A (Finding 3) is
resolved. Wave-A Finding 1 (`extract_entry_ids` `#NNN` false-positive heuristic) remains
as documented — no change in wave-B. No blocking findings.

---

## Wave-A Finding Resolution Status

| Finding | Wave-A Verdict | Wave-B Status |
|---------|---------------|---------------|
| Finding 1: `#NNN` heuristic false positives in `extract_entry_ids` | Non-blocking, low | Still present, unchanged. Blast radius remains under-deprecation only. |
| Finding 2: Unbounded 5,000-row observation scan (inefficiency) | Non-blocking, low | **Resolved.** Two-step session-bounded fetch replaces the LIMIT 5000 scan. |
| Finding 3: Full-topic scan in `existing_entry_with_title` | Non-blocking, low | **Resolved.** EXISTS query with `status = 0` predicate replaces `query_by_topic` + Rust filter. |

---

## Findings

### Finding 1 (carry-forward): `extract_entry_ids` — `#NNN` false-positive heuristic

- **Severity**: low
- **Location**: `crates/unimatrix-observe/src/extraction/dead_knowledge.rs` — `extract_entry_ids`
- **Description**: Unchanged from wave-A review. The `#NNN` branch splits on `#` and parses
  the leading digit sequence as a u64. Any response snippet containing `#` followed by digits
  (markdown headings, GitHub PR references, code comments) produces extra IDs that are added
  to `recent_entry_ids`, protecting entries from deprecation even when they were not actually
  accessed. Failure mode is under-deprecation (entries survive that should be deprecated), not
  over-deprecation. No data loss possible from this path.
- **Recommendation**: Narrow the `#NNN` pattern to require a word boundary before `#`. Non-blocking;
  carry-forward from wave-A; no new risk in wave-B.
- **Blocking**: no

### Finding 4 (wave-B new): Dynamic SQL construction in `fetch_recent_observations_for_dead_knowledge` — confirmed safe

- **Severity**: low (informational)
- **Location**: `crates/unimatrix-server/src/background.rs` — `fetch_recent_observations_for_dead_knowledge`, Step B
- **Description**: The Step B query builds a SQL string with an IN-clause by formatting
  positional placeholders (`?1, ?2, ..., ?N`) where N equals the number of session IDs returned
  by Step A. The session IDs are then bound individually as parameters. This is the same
  pattern used in `load_observations_for_sessions` in `observations.rs`. The constructed SQL
  contains only `?N` placeholders — no session ID string is ever interpolated into the SQL
  text. All values flow through sqlx bind parameters.

  The maximum number of placeholders is bounded by `DEAD_KNOWLEDGE_SESSION_THRESHOLD = 20`,
  so the IN-clause will never exceed 20 parameters. SQLite has a default `SQLITE_LIMIT_VARIABLE_NUMBER`
  of 999; this is well within bounds.

  This is not an injection vector. It is noted here because dynamic SQL construction warrants
  explicit verification, which it passes.
- **Recommendation**: None required. The pattern is correct and bounded.
- **Blocking**: no

### Finding 5 (wave-B new): `source_domain` hardcoded to `"claude-code"` in `ObservationRecord` construction

- **Severity**: low (informational)
- **Location**: `crates/unimatrix-server/src/background.rs` — `fetch_recent_observations_for_dead_knowledge`, row mapping (line ~958)
- **Description**: The `ObservationRecord` struct populated from database rows has
  `source_domain` hardcoded to `"claude-code"`. The `observations` table does not store a
  `source_domain` column; it is a synthetic field used by the detection pipeline. The
  hardcoded value is consistent with the existing wave-A fetch code and with the project's
  current single-domain deployment (ADR-005 applies only to the source_domain guard at filter
  time, not to the value itself). The `detect_dead_knowledge_candidates` function re-applies
  the `source_domain == "claude-code"` filter internally, so all records will pass the filter
  regardless. No data confusion is possible in the single-domain case.

  This is a latent design concern (not a security issue): if a second source domain is ever
  added to the observations table, this function would incorrectly tag all fetched observations
  as `"claude-code"`. A proper fix would require a `source_domain` column in the table.
- **Recommendation**: File a follow-up issue to add `source_domain` to the observations table
  schema. Out of scope for this bugfix.
- **Blocking**: no

---

## OWASP Checklist (Wave-B Changes)

| OWASP Category | Status | Notes |
|----------------|--------|-------|
| A01 Broken Access Control | Clear | No trust boundary changes; both functions are internal background operations |
| A02 Cryptographic Failures | N/A | No cryptographic operations in scope |
| A03 Injection | Clear | Step B SQL uses positional bind parameters only; `title` in EXISTS query is a statically-constructed format string, not user input; no shell or format string injection surface |
| A04 Insecure Design | Clear | Session-bounded fetch is strictly tighter than the previous LIMIT scan; EXISTS query adds a status filter that was previously missing |
| A05 Security Misconfiguration | Clear | No config changes; no new endpoints or transport paths |
| A06 Vulnerable Components | Note | `sqlx = "0.8"` added as a direct dependency of `unimatrix-observe`. This is the same version already used throughout the workspace — not a new version with an unknown CVE surface. No net new attack surface. |
| A08 Data Integrity Failures | Clear | Migration idempotency enforced by COUNTERS key; deprecation cap prevents write-pool saturation |
| A09 Logging Failures | Clear | Warnings log `error = %e` and `entry_id` only; no sensitive data in log fields |
| Hardcoded Secrets | Clear | No credentials, tokens, or keys found in changed files |

---

## Blast Radius Assessment

**Worst case for the two-step session fetch (Fix 2):**

Step A fetches session IDs using `MAX(id) DESC` ordering. If the `observations` table has no
index on `id` and a very large number of sessions, Step A degrades to a full aggregate scan.
However, `id` is the SQLite rowid (always indexed) so `MAX(id)` per group is efficient.
If Step A returns zero rows (empty observations table), the function returns `vec![]` and
`dead_knowledge_deprecation_pass` exits cleanly at the `observations.is_empty()` guard. No
deprecations occur. Safe failure.

If Step B's IN-clause contains session IDs that were deleted between Step A and Step B (a
race window on a multi-writer SQLite database), the query returns fewer rows. The detection
window shrinks. The pass may find fewer candidates or none. No spurious deprecations.

**Worst case for the EXISTS dedup guard (Fix 3):**

The EXISTS query is `WHERE topic = ?1 AND title = ?2 AND status = 0`. If the query erroneously
returns `true` (false positive), a valid friction proposal is suppressed for one tick. On the
next tick the dedup check runs again — if the erroneously-detected entry is absent, the
proposal proceeds. Self-correcting within one tick. No data loss.

If the query erroneously returns `false` (false negative), a duplicate entry is inserted. This
is the same behavior as the previous `Err(_) => false` fallback and is safe — the knowledge
base gains a redundant entry that a future dedup pass or manual review can remove.

**Cross-cut worst case:** Both functions are non-critical background maintenance paths. Failures
are logged at `warn` level and the tick proceeds. No entry is deprecated or inserted by either
function without an explicit `store.update_status` or `store.insert` call that can itself fail
safely.

---

## Regression Risk

**Low.** The wave-B changes are limited to three files:

1. `crates/unimatrix-server/src/background.rs` — `fetch_recent_observations_for_dead_knowledge`
   function signature change (removed `limit: i64` parameter). This function is not part of the
   public API; it is a private async function called only from `dead_knowledge_deprecation_pass`
   in the same file. The call site was updated in the same commit.

2. `crates/unimatrix-observe/src/extraction/recurring_friction.rs` — `existing_entry_with_title`
   function body replaced. The function signature is unchanged; callers are unaffected.
   The behavior change is correctness-only: deprecated entries no longer block re-proposals.

3. `crates/unimatrix-observe/Cargo.toml` — `sqlx = "0.8"` added as direct dependency.
   `sqlx` was already a transitive dependency at `0.8`; making it direct does not change the
   resolved version and does not affect any other crate.

All 11 wave-A tests and both wave-B tests pass. Clippy is clean for affected crates.
No extraction rule behavior visible outside the tick pipeline is changed.

---

## Input Validation Summary (Wave-B Focus)

| Input Surface | Validated? | Notes |
|--------------|------------|-------|
| Session IDs from Step A (returned by DB query) | Yes | Used only as bind parameters in Step B; never interpolated into SQL text |
| `title` in EXISTS query | Yes | Statically constructed format string `"Recurring friction: {rule_name}"` where `rule_name` comes from a `HashMap` key built from detection rule names — no external input |
| `DEAD_KNOWLEDGE_SESSION_THRESHOLD` (20) | Implicit | Compile-time constant; no external-input path |
| `status = 0` in EXISTS query | Yes | Hardcoded literal; correct per schema (`Status::Active = 0` in `schema.rs:11`) |
| Row column access via positional index | Partial | `row.get::<T, _>(N)` calls match the SELECT column order; a schema change would cause a runtime type error, not silent data corruption |

---

## PR Comments

- Posted 1 comment on PR #352 (wave-B findings summary, non-blocking).
- Blocking findings: no

---

## Knowledge Stewardship

- Nothing novel to store. The dynamic-SQL IN-clause pattern (Finding 4) is project-established
  and already documented. The `source_domain` hardcoding (Finding 5) is a latent schema
  limitation, not a recurring security anti-pattern. Finding 1 carry-forward is PR-specific.
  No generalizable new anti-pattern identified in this diff.
