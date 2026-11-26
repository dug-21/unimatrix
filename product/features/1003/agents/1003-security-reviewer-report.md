# Security Review: 1003-security-reviewer

## Risk Level: low

## Summary
Audit-only allocator change: `log_audit_event` now calls `counters::next_audit_event_id`, a single atomic upsert returning `max(counter, MAX(audit_log.event_id)) + 1`. No injection, access-control, dependency or secrets concerns. One non-blocking hardening note on unbounded imported event_ids.

## Findings

### F1 — format!-built SQL
- **Severity**: none (informational)
- **Location**: crates/unimatrix-store/src/counters.rs `next_audit_event_id`
- **Description**: SQL is assembled with `format!`, but only from module-private `const` table/column names; the counter name is bound (`?1`). No untrusted input reaches the string.
- **Recommendation**: None. Keep constants private.
- **Blocking**: no

### F2 — Imported extreme event_id can poison the allocator
- **Severity**: low
- **Location**: counters.rs `next_audit_event_id`; import/inserters.rs `insert_audit_log` (binds `r.event_id` unvalidated)
- **Description**: The allocator now trusts `MAX(audit_log.event_id)`. Import inserts event_ids from JSONL without range checks. A row with event_id = i64::MAX makes `MAX(...)+1` overflow (SQLite yields REAL), so the `i64` decode/subsequent inserts fail and every audit write errors permanently. Negative-only ids with an absent counter could yield a value <= 0 cast to u64. Previously, a large imported id only caused a far-future collision. Trust boundary is the operator-run local import CLI, so exploitation requires a crafted file supplied by the operator.
- **Recommendation**: Follow-up (not this PR): reject import audit_log rows with event_id <= 0 or above a sane bound; optionally guard `v <= 0` in `next_audit_event_id`.
- **Blocking**: no

### F3 — Concurrency
- **Severity**: none
- **Description**: Single-statement upsert with RETURNING on the transaction connection; first statement in the txn is a write, so SQLite takes the RESERVED lock before reading MAX. No TOCTOU (cf. bugfix-584). Rollback reverts both counter and row consistently. Concurrent test (16 writers, 2 write connections) passes locally.
- **Blocking**: no

## Blast Radius Assessment
Scope: every audit_log write (`log_audit_event`, including import `record_provenance`). Worst case of a subtle bug: audit writes fail (fire-and-forget path drops events — loss of audit trail / non-repudiation) or ids collide. Failure mode is an error returned, not silent corruption of knowledge entries; `next_entry_id` and other counters unaffected. `event_id` is the INTEGER PRIMARY KEY, so `MAX(event_id)` is an O(log n) lookup — no per-write table-scan DoS.

## Regression Risk
Low. Counter semantics unchanged (value = last issued id, same as `next_counter`). When counter >= MAX, behavior is identical to before. Ids may jump forward once when counter trails — intended. Other `INSERT INTO audit_log` sites outside tests: import inserter (explicit ids, restored before allocator use) and export tests only. Unrelated files in diff: DECISIONS.md / STALLS.md (process docs, no code). No new dependencies. No secrets.

Local verification: `cargo test -p unimatrix-store --lib audit` — 24 passed.

## PR Comments
- Posted 1 comment on PR #1029
- Blocking findings: no

Unimatrix entries consulted: #4396, #4397, #4398, #4405, #4406, #4408, #4604, #5840

## Knowledge Stewardship
- Stored: nothing novel to store -- F2 (unvalidated imported ids feeding MAX-based allocators) is a single occurrence; recorded in PR comment.
