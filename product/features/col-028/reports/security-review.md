# Security Review: col-028-security-reviewer

## Risk Level: low

## Summary

The col-028 changes are additive, narrowly scoped, and follow established codebase patterns.
All SQL interactions use parameterised bindings with no string interpolation of user-supplied
data. The confirmed_entries HashSet is bounded by session GC (4-hour stale sweep plus explicit
drain). Phase values entering the database originate from an in-memory field that is populated
only through validated MCP paths or a pre-existing (not col-028) UDS path. No secrets, no
access-control changes, no new deserialization of untrusted data.

One pre-existing gap was noted: the UDS `next_phase` payload field bypasses
`validate_phase_field` before being stored in `SessionState.current_phase`. This is not
introduced by col-028 but the feature now propagates whatever is stored there into
`query_log.phase`. The impact is minimal (TEXT column, no downstream execution) and the
trust boundary for UDS events is localhost-only.

---

## Findings

### Finding 1: SQL injection — parameterised bindings verified (no issue)

- **Severity**: N/A (no finding)
- **Location**: `analytics.rs:482-498`, `query_log.rs:119-160`, `migration.rs:558-593`
- **Description**: All new SQL uses positional bind parameters (`?1` through `?9` in
  analytics INSERT; `?1` in scan WHERE clauses). The `phase TEXT` column value arrives as
  `Option<String>` through sqlx `.bind(phase)`, which serializes to SQL NULL or a quoted
  string literal — no interpolation occurs. The migration DDL (`ALTER TABLE`, `CREATE INDEX`,
  `UPDATE counters`) contains no user-supplied data whatsoever. The `pragma_table_info`
  pre-check uses a literal string for the table name. Zero SQL injection surface.
- **Recommendation**: None required.
- **Blocking**: No

### Finding 2: confirmed_entries memory growth — bounded by existing GC (no issue)

- **Severity**: N/A (no finding)
- **Location**: `session.rs:151`, `session.rs:278-284`
- **Description**: `confirmed_entries: HashSet<u64>` grows by at most one entry per
  `context_get` or single-ID `context_lookup` call. The session GC operates on two paths:
  (1) `drain_and_signal_session` on explicit session close removes the entire `SessionState`
  from the registry map, and (2) `sweep_stale_sessions` (called from the context_status
  tick and UDS session-close) removes sessions inactive for 4 hours. Both paths call
  `sessions.remove()` which drops the full `SessionState`, including `confirmed_entries`.
  In the worst case a session that calls `context_get` thousands of times in 4 hours
  accumulates that many `u64` entries (8 bytes each). A hypothetical 10 000-entry session
  costs ~80 KB — the same order of magnitude as `injection_history` (unbounded Vec, same GC
  path). No new memory risk beyond what already exists for injection_history. The HashSet
  deduplicates repeated fetches of the same entry, providing a natural ceiling equal to the
  corpus size.
- **Recommendation**: None required for this feature. If unbounded session memory becomes a
  concern in future, it applies equally to injection_history and should be addressed holistically.
- **Blocking**: No

### Finding 3: Phase value validation — MCP path fully validated; UDS path pre-existing gap (informational)

- **Severity**: low
- **Location**: `uds/listener.rs:2403-2418` (pre-existing); `infra/validation.rs:455-481`
- **Description**: Phase values stored in `SessionState.current_phase` come from two sources:
  (a) The MCP `context_cycle` tool handler, which calls `validate_cycle_params` → `validate_phase_field`.
  This function enforces: trimmed, lowercased, non-empty, ≤64 chars, ASCII alphanumeric + hyphen + underscore
  only. Phase values passing this gate are safe for TEXT column storage.
  (b) The UDS hook handler (`handle_cycle_event`), which reads `event.payload["next_phase"]`
  and calls `.map(|s| s.to_string())` with no validation before `set_current_phase`.
  This gap pre-exists col-028 (introduced in crt-025) and is not modified by this feature.
  col-028 now reads `current_phase` and propagates it to `query_log.phase` (TEXT column)
  and `UsageContext.current_phase` (downstream to `feature_entries.phase`). Since the database
  columns are TEXT with no further execution, injection of an arbitrary string via the UDS path
  would produce a long or non-standard phase value in the log rows — a data quality issue, not
  a security exploit. The UDS socket is only accessible to localhost processes; it is not
  reachable from the MCP client (external agent).
- **Recommendation**: Add length and character validation for `next_phase` in `handle_cycle_event`
  (matching `validate_phase_field` semantics) as a follow-up. This is not a col-028 blocker
  since the risk was accepted when crt-025 was merged and the trust boundary is localhost.
- **Blocking**: No (pre-existing, non-exploitable via external interface, data quality only)

### Finding 4: D-01 weight-0 guard — no bypass of legitimate access tracking (no issue)

- **Severity**: N/A (no finding)
- **Location**: `services/usage.rs:316-326`
- **Description**: The guard `if ctx.access_weight == 0 { return; }` fires only for
  `AccessSource::Briefing` calls. The `access_weight` field is set by the handler, not by the
  MCP caller (there is no user-supplied weight parameter in the briefing tool schema).
  `context_search` (weight=1), `context_lookup` (weight=2), and `context_get` (weight=2) all
  set non-zero weights and do NOT call `record_briefing_usage` — they dispatch via
  `AccessSource::McpTool` to a different code path. The guard can only be reached when
  `record_access` dispatches to `record_briefing_usage`, which only happens for
  `AccessSource::Briefing`. A caller cannot inject `access_weight=0` through the MCP interface
  to suppress tracking on a non-briefing path. The guard correctly prevents the dedup slot from
  being consumed by an offer-only event, which is the stated intent.
- **Recommendation**: None required.
- **Blocking**: No

### Finding 5: Positional column binding alignment — verified correct, type-safe (no issue)

- **Severity**: N/A (no finding)
- **Location**: `analytics.rs:482-498`, `query_log.rs:164-191`
- **Description**: The INSERT column list is `(session_id, query_text, ts, result_count,
  result_entry_ids, similarity_scores, retrieval_mode, source, phase)` with bindings
  `?1` through `?9` chained in the same order. The SELECT column list in both
  `scan_query_log_by_sessions` and `scan_query_log_by_session` appends `phase` as the last
  column (index 9 in zero-indexed `row.try_get`). The deserializer reads index 9 with
  `try_get::<Option<String>, _>(9)`. The types align: INSERT binds `Option<String>` → SQLite
  TEXT/NULL; SELECT reads `Option<String>` ← SQLite TEXT/NULL. The prior `source` column is
  at index 8 as `String` (not nullable, consistent with the non-optional Rust type). No
  mismatch between column order, index, and Rust type.
- **Recommendation**: None required.
- **Blocking**: No

### Finding 6: last_activity_at not updated by record_confirmed_entry (informational, not a defect)

- **Severity**: N/A (no finding)
- **Location**: `session.rs:278-284`
- **Description**: `record_confirmed_entry` does not update `last_activity_at`. This is
  consistent with `record_category_store` (the stated pattern), which also does not update it.
  Both are secondary metadata-capture methods that execute only after the primary MCP handler
  has already returned, so `last_activity_at` is guaranteed to have been updated by the
  preceding `record_injection` or similar call. No staleness-detection gap.
- **Recommendation**: None required.
- **Blocking**: No

---

## Blast Radius Assessment

**Worst case if the fix has a subtle regression**:

- analytics INSERT bind count mismatch (e.g., if `phase` bind is accidentally omitted):
  sqlx would raise a runtime error from the analytics drain goroutine; the error surfaces
  as a tracing warning but does NOT crash the server (fire-and-forget pattern). Query log
  rows would fail to write. Search functionality unaffected.

- `row_to_query_log` index-9 read on old schema (pre-migration row): SQLite returns NULL
  for out-of-schema columns on old databases only if the column was added via ALTER TABLE.
  After v16→v17 migration, all rows return NULL for phase until a new row is written.
  `try_get::<Option<String>, _>(9)` handles NULL correctly. No silent data corruption.

- `current_phase_for_session` returns stale phase if called after a `set_current_phase` race:
  The C-01 constraint (phase snapshot before first `.await`) ensures atomicity within one
  handler invocation. A race between the phase snapshot and a concurrent `cycle_stop` event
  is possible but produces a phase value that was valid nanoseconds earlier — a known and
  accepted condition per ADR-002.

- Weight change for `context_get` (1→2) and `context_briefing` (1→0) affects confidence
  scoring for existing entries. This is intentional. The worst case is a temporary re-ranking
  of entries in sessions where briefing was previously counted as access. Since the dedup
  system means each (agent, entry) pair is counted once, the delta is bounded: entries that
  got one briefing access count in the past now get zero, and context_get entries go from
  one unit to two. No entries are deleted or corrupted.

---

## Regression Risk

**Moderate-to-low.** The changes touch the query_log write path (analytics drain), the session
state struct (struct literal updates in tests), and four MCP handler call sites. The highest
regression risk items are:

1. The `QueryLogRecord::new` signature change (phase as final arg): any call site that passes
   positional arguments without the new `phase` argument will fail to compile. The implementation
   brief lists all call sites updated (UDS compile-fix, test helpers, knowledge_reuse.rs).
   Compile-time detection makes this low-risk.

2. The access_weight changes (get: 1→2, briefing: 1→0): these are not backward-compatible for
   any downstream code that asserts specific `access_count` values. The test suite for D-01
   covers the briefing path. The change in `context_get` weight has no test dedicated to the
   exact value in isolation — it is covered by the existing usage service integration path.

3. The confirmed_entries field addition: every `SessionState` struct literal in tests must
   include `confirmed_entries: HashSet::new()`. Pattern #3180 is referenced in the brief. If
   any test helper was missed, the build fails with "missing field" (compile-time catch).

---

## PR Comments

No open PR was found for col-028 at review time. Findings were documented in this report only.
A PR comment thread should be opened on the PR once it is created.

- Posted 0 PR comments (no PR found)
- Blocking findings: No

---

## Knowledge Stewardship

Nothing novel to store — the UDS `next_phase` validation gap is a pre-existing condition noted
in the crt-025 era. The pattern of parameterised bindings for new TEXT columns (with
pragma_table_info idempotency pre-check) is already well-established in this codebase and does
not need a new lesson entry.
