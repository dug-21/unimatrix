# Security Review: crt-033-security-reviewer

## Risk Level: low

## Summary

crt-033 introduces `cycle_review_index`, a SQLite memoization table for `context_cycle_review`
results. All new SQL uses parameterized queries via sqlx. No new external trust boundaries are
introduced. The sole caller-supplied string (`feature_cycle`) is validated by the existing
`validate_retrospective_params` guard before reaching any storage call. No hardcoded secrets
or new dependencies were added. Two informational findings are noted; neither is blocking.

---

## Findings

### Finding 1 — feature_cycle echo-back in error messages (informational)

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs` — error arms in the `force=true`
  purged-signals path (lines ~1440–1460, ~1480–1493)
- **Description**: The `feature_cycle` string supplied by the MCP caller is interpolated
  directly into `ErrorData` detail strings returned to the caller
  (e.g., `"No observation data found for feature '{feature_cycle}'"`,
  `"Stored cycle review for '{feature_cycle}' is corrupt"`). This is an MCP server where the
  caller already supplied the value, so the echo-back adds no information a caller could not
  already observe. The data originates from `validate_retrospective_params`, which enforces
  a 128-character length cap and rejects empty/whitespace-only values. No injection surface
  exists in an MCP error string.
- **Recommendation**: No action required. The pattern is consistent with other MCP error
  messages throughout tools.rs. The caller knows the value they sent.
- **Blocking**: no

### Finding 2 — NOT IN subquery over cycle_review_index in pending_cycle_reviews (informational)

- **Severity**: low
- **Location**: `crates/unimatrix-store/src/cycle_review_index.rs:160–165`
- **Description**: The `pending_cycle_reviews` query uses a correlated `NOT IN (SELECT
  feature_cycle FROM cycle_review_index)` subquery. In SQLite, `NOT IN` returns no rows when
  the subquery result set contains any NULL values. The `feature_cycle` column is `TEXT PRIMARY
  KEY` which SQLite implicitly enforces as `NOT NULL`, so the NULL-poisoning hazard is
  structurally excluded by the schema. Similarly `cycle_events.cycle_id` is declared
  `TEXT NOT NULL`. There is no actual vulnerability here; this is a defensive observation
  about a class of SQL gotcha that does not apply due to schema constraints.
- **Recommendation**: A `WHERE ce.cycle_id IS NOT NULL` guard or an `AND NOT EXISTS` form
  would make the NULL safety explicit without relying on DDL knowledge, but this is purely
  a style preference. The schema constraints are adequate. No code change required.
- **Blocking**: no

---

## OWASP Evaluation

| OWASP Category | Assessment |
|----------------|------------|
| A01 Broken Access Control | No change to access control or trust levels. `force` is a `bool`; cannot escalate privilege. |
| A02 Cryptographic Failures | No cryptographic material introduced. |
| A03 Injection (SQL) | All queries use sqlx parameterized binds (`?1`, `?2`, …). `feature_cycle` flows through `bind()` in every call site. No string interpolation in SQL. |
| A04 Insecure Design | Architecture reviewed. Defense-in-depth: 4MB ceiling enforced pre-DB-call; corrupted JSON falls through to recompute (ADR-003); pool starvation risk addressed by direct async await (ADR-001). |
| A05 Security Misconfiguration | No new config surface. No new server endpoints. No new trust level. |
| A06 Vulnerable Components | No new Cargo dependencies added. Cargo.toml files unchanged. |
| A07 Auth/Identity Failures | Identity chain unchanged. `force` field does not bypass identity validation (validation runs before step 2.5). |
| A08 Data Integrity Failures | `INSERT OR REPLACE` is safe for last-writer-wins. `SUMMARY_SCHEMA_VERSION` single-definition enforced (ADR-002, verified by grep). |
| A09 Logging/Monitoring | All error branches emit `tracing::warn!`. No sensitive data in log lines (only `feature_cycle` name and error description). |
| A10 SSRF | No network calls introduced. Purely internal SQLite operations. |

---

## Blast Radius Assessment

**If `store_cycle_review` silently writes a corrupt row**: the memoization hit path
deserializes via `serde_json::from_str`. A deserializtion failure falls through to full
recomputation (ADR-003 fallthrough), not a crash. Worst case: one extra recomputation per
corrupt cycle until `force=true` overwrites the row. No data corruption in other tables.

**If `pending_cycle_reviews` query returns wrong results**: operators see an inaccurate
backlog count in `context_status`. This is a display/observability issue only. No
write path is affected; no raw signals are modified.

**If the schema migration fails (v17 → v18)**: `CREATE TABLE IF NOT EXISTS` inside a
transaction rolls back cleanly. Database remains at v17. The next server start
re-attempts the migration. The fix path is deterministic.

**If `force=true` is passed by a low-trust agent with intent to flood writes**: each call
runs the full retrospective pipeline then writes one `INSERT OR REPLACE` row (1 per
`feature_cycle`). No unbounded write amplification. The 4MB ceiling on `summary_json`
prevents OOM from pathological payloads.

---

## Regression Risk

**Low overall.** The changes are additive:

- Schema migration is guarded with `if current_version < 18` and `CREATE TABLE IF NOT EXISTS`
  (idempotent).
- Step 2.5 (memoization check) and step 8a (memoization write) are new code paths inserted
  around the pre-existing full pipeline, which is fully bypassed only on a memoization hit.
- The `force` field defaults to `None` (treated as `false`); existing callers omitting `force`
  get identical behavior to pre-crt-033 (cache miss on first call, full computation).
- `StatusReport.pending_cycle_reviews` is a new additive field initialized to `vec![]`;
  existing JSON consumers will see a new `"pending_cycle_reviews": []` array but no
  previously-present field has been removed or renamed.

**Known regression surface**: a memoization hit suppresses full pipeline re-execution
on subsequent calls. This is intentional but means callers relying on "every call
recomputes" behavior must use `force=true`. This is documented behavior, not a
regression risk from a security perspective.

---

## Input Validation Assessment

`feature_cycle` is the only new caller-supplied string reaching storage. Validation path:

1. `validate_retrospective_params` — enforced before any handler step runs
2. `validate_string_field("feature_cycle", ..., MAX_FEATURE_CYCLE_LEN=128, false)` — length
   cap + empty/whitespace rejection
3. `bind(feature_cycle)` in all sqlx calls — parameterized, injection-safe

`force: Option<bool>` — boolean; no validation surface.

`k_window_cutoff: i64` in `pending_cycle_reviews` — computed server-side from
`SystemTime::now() - PENDING_REVIEWS_K_WINDOW_SECS`. Not caller-supplied; no injection surface.

---

## Dependency Safety

No new crates introduced. Both `Cargo.toml` files are unchanged on this branch. No new
transitive dependencies. `cargo audit` requirement applies to the base workspace and is
unaffected by this PR.

---

## Secrets Check

No hardcoded secrets, API keys, credentials, or tokens found in the diff. Constants
`SUMMARY_SCHEMA_VERSION = 1` and `SUMMARY_JSON_MAX_BYTES = 4 * 1024 * 1024` are
domain constants, not credentials.

---

## PR Comments

- Posted 1 comment on PR #454
- Blocking findings: no

---

## Knowledge Stewardship

- Stored: nothing novel to store — the NOT IN / NULL subquery gotcha is a well-known SQL
  pattern and the schema-level mitigation (NOT NULL primary key) is already how this project
  structures all other tables. No new generalizable anti-pattern visible.
