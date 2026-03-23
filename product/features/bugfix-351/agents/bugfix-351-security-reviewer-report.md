# Security Review: bugfix-351-security-reviewer

## Risk Level: low

## Summary

The fix removes a self-replicating noise loop (`DeadKnowledgeRule` as an additive extraction rule) and replaces it with a direct deprecation pass in the background tick. All SQL operations use parameterized queries; no new external trust boundaries are opened; no new dependencies are introduced. Two low-severity observations are noted — neither is blocking.

---

## Findings

### Finding 1: `extract_entry_ids` — `#NNN` heuristic can produce false positives from snippet content

- **Severity**: low
- **Location**: `crates/unimatrix-observe/src/extraction/dead_knowledge.rs:123-133`
- **Description**: The `#NNN` branch splits on `#` and parses the leading digit sequence as a u64. Any response snippet containing `#` followed by digits — markdown headings (`## 5 results`), GitHub PR references (`#123`), code comments — will produce IDs that are added to `recent_entry_ids`. This inflates the "recently seen" set, causing entries with those numeric IDs to be _protected_ from deprecation even when they were not actually accessed. The consequence is false negatives (entries that should be deprecated survive), not false positives (entries that should not be deprecated are incorrectly deprecated). This is an incorrect-but-safe failure mode: the worst case is under-deprecation, not accidental data loss.
- **Recommendation**: Narrow the `#NNN` pattern to require that the `#` appear immediately after whitespace or at the start of the string (avoid false matches from markdown headers). Non-blocking for this fix since the blast radius is under-deprecation, not data corruption.
- **Blocking**: no

### Finding 2: `source_domain` hardcoded to `"claude-code"` in `fetch_recent_observations_for_dead_knowledge`

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/background.rs:915`
- **Description**: Observations are fetched from the DB without a `WHERE source_domain = 'claude-code'` filter in the SQL query. The `detect_dead_knowledge_candidates` function applies the domain filter in Rust after fetching. This means up to 5,000 rows are loaded regardless of domain, then the domain guard filters them. For a system with multiple source domains this is unnecessary IO, but it does not affect correctness because the domain guard is present and mandatory per ADR-005. The concern is efficiency, not security.
- **Recommendation**: Add `WHERE hook IN ('PreToolUse','PostToolUse')` or a domain-equivalent filter to the SQL query to reduce row count. Not a security issue.
- **Blocking**: no

### Finding 3: `existing_entry_with_title` — full topic scan for dedup check

- **Severity**: low
- **Location**: `crates/unimatrix-observe/src/extraction/recurring_friction.rs:153-174`
- **Description**: The dedup guard calls `store.query_by_topic("process-improvement")` and then does a linear title scan in Rust. This loads all `process-improvement` entries into memory on every evaluation call. As the knowledge base grows this will get slower, but there is no security risk: the topic string is a hardcoded literal (not user-controlled), and the store function uses parameterized queries. Failure mode is `false` (safe default — allows the proposal), so store errors do not suppress legitimate proposals.
- **Recommendation**: Long-term, add a `find_by_topic_and_title` store method to avoid the full-topic fetch. Not a security concern.
- **Blocking**: no

---

## OWASP Checklist

| OWASP Category | Status | Notes |
|----------------|--------|-------|
| A01 Broken Access Control | Clear | No trust boundary changes; deprecation pass is an internal background operation only |
| A02 Cryptographic Failures | N/A | No cryptographic operations in scope |
| A03 Injection | Clear | All SQL uses parameterized queries (`?1` placeholders). `extract_entry_ids` parses only digit sequences — no SQL or command construction from snippet content |
| A04 Insecure Design | Clear | Direct deprecation replaces additive insert; one-shot migration correctly gated by COUNTERS key |
| A05 Security Misconfiguration | Clear | No config changes; no new endpoints or transport paths |
| A06 Vulnerable Components | Clear | No Cargo.toml or Cargo.lock changes — zero new dependencies |
| A08 Data Integrity Failures | Clear | Migration idempotency is enforced by a COUNTERS key read before executing; tests verify this |
| A09 Logging Failures | Clear | Deprecation events logged at `debug` level; migration start/finish at `info`; errors at `warn`. No sensitive data in log fields |
| Hardcoded Secrets | Clear | No credentials, tokens, or keys found in changed files |

---

## Blast Radius Assessment

**Worst case if `dead_knowledge_deprecation_pass` has a subtle bug:**

The pass calls `store.update_status(entry_id, Status::Deprecated)` for entries that the detector identifies as stale. If `detect_dead_knowledge_candidates` returns an incorrect candidate list (e.g., via `extract_entry_ids` false positives in the `#NNN` path), entries could be protected from deprecation that should be deprecated (under-deprecation) — but no legitimate active entry can be incorrectly deprecated via this path because the filter requires `access_count > 0` and absence from recent session snippets.

The inverse — spurious deprecation of an entry that _was_ recently accessed — requires a false positive in `extract_entry_ids` to cause an ID to be _omitted_ from `recent_entry_ids`. The `#NNN` heuristic can only _add_ extra IDs to that set (protecting entries from deprecation), never remove valid ones. So the blast radius of a bug in ID extraction is one-directional and safe: under-deprecation, not over-deprecation.

**Worst case for the one-shot migration:**

If the COUNTERS key write fails after partial deprecations, the migration will retry on the next tick. Re-running is safe: already-Deprecated entries are either skipped by the `Active` filter or `update_status` on an already-Deprecated row is idempotent (status unchanged). This is a correct safe-retry design.

**Cap-at-50 design:**

The deprecation cap of 50 per tick prevents write-pool saturation. If a system has 1000 stale entries, they are deprecated over 20 ticks (~5 hours at 15-min intervals). This is intentional and safe.

---

## Regression Risk

**Low.** The changes are additive to `maintenance_tick` (new Steps 11 and 12 appended after the existing 10 steps) and subtractive in the extraction pipeline (removing one rule from 5 → 4). The removal of `DeadKnowledgeRule` from `default_extraction_rules` is the only behavior change visible to existing callers of the extraction pipeline. All tests for the remaining 4 rules continue to pass.

The `RecurringFrictionRule` change adds a store query that was not present before. If the query blocks longer than expected (e.g., pool exhaustion), the extraction tick will be delayed — but `block_in_place` inside `existing_entry_with_title` prevents blocking the Tokio runtime thread. The fallback `false` on error means proposals proceed in degraded state, which is the correct fail-open choice for a non-critical dedup check.

---

## Input Validation Summary

| Input Surface | Validated? | Notes |
|--------------|------------|-------|
| `observations` slice | Yes | `source_domain == "claude-code"` filter applied as first operation per ADR-005 |
| `window` parameter | Implicit | Caller passes constant `5`; no external-input path |
| `response_snippet` from DB | Partial | Capped at 500 chars at insertion; legacy path has no cap (pre-existing) |
| SQL parameters in new queries | Yes | All use `?N` bind parameters |
| `rule_name` in `remediation_for_rule` | Yes | Match arm with safe fallback default; no format string injection possible |
| COUNTERS key string | Yes | Hardcoded constant `"dead_knowledge_migration_v1"` |

---

## PR Comments

- Posted 1 comment on PR #352 (findings summary, non-blocking).
- Blocking findings: no

---

## Knowledge Stewardship

- Nothing novel to store — the `extract_entry_ids` `#NNN` heuristic false-positive risk is PR-specific, not a generalizable anti-pattern beyond this module. No recurring cross-PR security anti-pattern identified in this diff.
