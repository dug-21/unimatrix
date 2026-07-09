# Security Review: vnc-047-security-reviewer

PR #943 · branch `feature/vnc-047` · GH #940 · fresh-context review.

## Risk Level: low

## Summary
Additive, opaque `cycle_tags` feature riding the existing col-025 goal path. All SQL is
parameterized, markdown output is escaped, the migration is purely additive/idempotent, and
concurrency is race-safe under `BEGIN IMMEDIATE` + 10s `busy_timeout`. No new trust boundary,
no secrets, no untrusted deserialization surface. No blocking findings.

## Findings

### F1 — SQL injection on the tag path: DEFENDED (verify passed)
- **Severity**: n/a (verification of the load-bearing control)
- **Location**: `unimatrix-store/src/db.rs` `insert_cycle_start_with_tags`, `get_cycle_tags`
- **Description**: Because value-opacity forbids input validation, parameterization is the sole
  SQLi defense. Confirmed: every statement uses `?1`/`?2` binds (`bind(cycle_id)`, `bind(tag)`,
  `bind(feature_cycle)`). DDL is static string literals. No `format!`/string interpolation on any
  SQL path. `ON CONFLICT(feature_cycle, tag) DO NOTHING` uses column names, not values. No `LIKE`
  / `like_escape` on the write path (correct — no namespace query ships).
- **Blocking**: no

### F2 — Markdown injection via attacker-controlled tags: DEFENDED
- **Severity**: n/a
- **Location**: `unimatrix-server/src/mcp/response/retrospective.rs` `render_tags_section`
- **Description**: Tags are opaque, caller-controllable strings surfaced into review markdown.
  `escape_md_text` is applied per tag (test `test_render_tags_escapes_metacharacters` covers a
  leading-`#` heading-injection attempt). Downstream-renderer safety, not validation — opacity
  preserved.
- **Blocking**: no

### F3 — Unbounded tag count/size (DoS)
- **Severity**: low (accepted risk, documented)
- **Location**: `hook.rs` extraction (no cap) → `db.rs::insert_cycle_start_with_tags` insert loop
- **Description**: Value-opacity means no length/count cap (contrast `MAX_GOAL_BYTES` on goal).
  The entire submitted set is inserted row-by-row inside ONE `BEGIN IMMEDIATE` transaction that
  holds SQLite's single global write lock for the loop's duration. A caller supplying a very large
  tag array therefore stalls ALL other DB writers (they wait up to the 10s `busy_timeout`, then
  fail). This is a lock-starvation angle on the documented DoS risk, sharper than pure disk growth.
- **Mitigating factors**: the write route is the local UDS hook path (Read/Search/SessionWrite;
  NOT `Capability::Write`, NOT `Capability::Admin`) — a trusted local client, not an external MCP
  caller. Fire-and-forget: a failed/slow write only `tracing::warn`s. Accepted under the opacity
  contract per RISK-TEST-STRATEGY "Security Risks / DoS".
- **Recommendation**: none required for merge. When the deferred cycle-tag mutation home (ADR-006)
  is built, consider a soft count cap there. Non-blocking.
- **Blocking**: no

### F4 — Authorization posture (no new gap; documentation nuance)
- **Severity**: low (informational)
- **Location**: `uds/mod.rs` `UDS_CAPABILITIES`; `listener.rs` `handle_cycle_event` step-5 spawn
- **Description**: The strategy states a "single `Capability::Write` gate," but persistence
  actually rides the UDS hook path, whose `UDS_CAPABILITIES` is `{Read, Search, SessionWrite}` and
  explicitly excludes `Write`/`Admin`. Tags inherit the EXACT gate of the existing col-025 goal /
  `cycle_start` write — no new authorization surface. `agent_id` is carried but never used to
  authorize or scope the write (audit-only, as designed). Any client that could already write a
  `cycle_start`/goal can now also write tags — proportionate, not an escalation.
- **Recommendation**: none blocking. Reconcile the "Capability::Write" wording in the strategy doc
  with the actual UDS gate at retro to avoid future confusion.
- **Blocking**: no

### F5 — Concurrency / TOCTOU on the whole-set-once guard: DEFENDED
- **Severity**: n/a
- **Location**: `db.rs::insert_cycle_start_with_tags`
- **Description**: `BEGIN IMMEDIATE` on a single dedicated connection acquired from `write_pool`
  takes the write lock BEFORE the `EXISTS` guard, so two concurrent same-cycle starts serialize;
  the loser observes the frozen set and no-ops. `busy_timeout=10000` (db.rs:159) means contention
  waits rather than immediately erroring. Race-safe as designed (R-15).
- **Blocking**: no

### F6 — Schema migration v30→v31 data integrity: SAFE
- **Severity**: n/a
- **Location**: `migration.rs` (`if current_version < 31`), `db.rs::create_tables_if_needed`
- **Description**: Purely additive `CREATE TABLE/INDEX IF NOT EXISTS` — no `ALTER`, no data
  transformation, no back-fill. Idempotent on re-run and safe on a populated v30 DB (no existing
  rows touched). Single version stamp at txn end; intermediate v30 stamp preserved for ordering.
  Fresh-create and migration DDL are byte-parallel (guards drift). No upgrade regression risk.
- **Blocking**: no

### F7 — Deserialization / malformed input: safe-degrading
- **Severity**: n/a
- **Location**: `hook.rs` tag extraction; `listener.rs` payload read; `types.rs` `tags` field
- **Description**: Malformed `tags` (non-array, non-string elements, object, all-blank) degrades to
  no-tags with no panic (tested at both hook and listener). `RetrospectiveReport.tags` uses
  `#[serde(default)]`, so v5 blobs backward-read to `[]`. No untrusted binary deserialization, no
  eval, no code path interprets tag content.
- **Blocking**: no

### F8 — Secrets: none
- No hardcoded credentials, tokens, or keys in the diff.

## Blast Radius Assessment
Worst case if a subtle bug exists: confined to `cycle_tags` rows for a single `feature_cycle` and
their display in `context_cycle_review`. Opaque storage means no downstream code interprets tag
content — no injection/eval/deserialization amplification. A read failure degrades review to empty
tags (never fails the review). A write failure is fire-and-forget (`tracing::warn`, no caller
impact). The only cross-cutting effect is the shared SQLite write lock during a large tag insert
(F3), bounded by `busy_timeout` and the trusted local write route.

## Regression Risk
Low. `insert_cycle_event` signature and its 15 call sites are unchanged; only Start-with-tags is
routed to the new primitive (tested both arms + goal-still-persists). The two version cascades
(schema v31, summary v6) each have discrete path coverage + pinned tests. GC protection-by-omission
is regression-tested across both DELETE surfaces with positive controls. No existing behavior
altered for tagless starts or non-start events.

## PR Comments
- Posted 1 review comment on PR #943 (comment, not request-changes).
- Blocking findings: no.

## Knowledge Stewardship
- Stored: nothing novel to store — findings are PR-specific (F3/F4 are accepted, documented risks
  for this feature, not recurring anti-patterns); the parameterized-bind and markdown-escape
  controls are already codebase idioms. Per "bugs-are-GH-issues-not-lessons," no lesson filed.
