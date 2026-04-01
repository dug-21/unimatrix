# Security Review: bugfix-476-security-reviewer

## Risk Level: low

## Summary

The diff is a minimal, well-scoped SQL fix that adds quarantine-status JOIN
filters to the `co_access_promotion_tick` batch SELECT. All SQL parameters are
bound via sqlx positional placeholders — no string interpolation, no injection
surface. No new dependencies, no unsafe code, no secrets. The deferred
write-time quarantine guard (GH #477) is an acknowledged but non-blocking gap.

---

## Findings

### Finding 1: SQL parameter binding — no injection risk
- **Severity**: informational
- **Location**: `co_access_promotion_tick.rs:225-244`
- **Description**: Three bound parameters (`?1` = min count, `?2` = cap,
  `?3` = quarantine status). All three use sqlx `.bind()` with typed Rust
  values (`i64`). The quarantine constant is derived from
  `Status::Quarantined as u8 as i64 = 3`, which matches the established
  codebase pattern used in `status.rs:1026`, `background.rs:530`,
  `background.rs:2950`, and `server.rs:1559`. No string interpolation
  anywhere in the changed code.
- **Recommendation**: No action required. Pattern is consistent and safe.
- **Blocking**: no

### Finding 2: INNER JOIN as access control gate
- **Severity**: informational
- **Location**: `co_access_promotion_tick.rs:232-238`
- **Description**: The fix uses INNER JOINs (`JOIN entries ea ON ...
  AND ea.status != ?3`) as the quarantine gate. INNER JOIN semantics are
  correct here: a co_access row whose endpoint id has no matching `entries`
  row (FK miss) or whose endpoint has `status = 3` will be excluded from the
  result set. This is the same pattern used in `typed_graph.rs:101`
  (`.filter(|e| e.status != Status::Quarantined)`). The SQL correctly excludes
  both "endpoint is quarantined" and "endpoint row is missing" cases.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 3: Subquery join also filtered — weight-inflation attack surface closed
- **Severity**: low (potential integrity issue, not a security vulnerability per se)
- **Location**: `co_access_promotion_tick.rs:230-234`
- **Description**: The scalar subquery computing `max_count` also carries the
  quarantine JOINs. Without this, a quarantined entry with an artificially
  inflated co_access count could silently depress edge weights for all active
  pairs — a subtle data integrity issue with blast radius across the entire
  graph edge weight normalization. The fix correctly addresses both the outer
  SELECT and the subquery. Test
  `test_quarantine_mixed_batch_only_active_pairs_promoted_with_correct_weight`
  directly exercises this path (weight 1.0 vs. 0.5).
- **Recommendation**: No action required. Already fixed and tested.
- **Blocking**: no

### Finding 4: Write-time quarantine gap (deferred GH #477)
- **Severity**: low
- **Location**: `analytics.rs:392-398`
- **Description**: Co_access events are written to the `co_access` table
  regardless of the endpoint's quarantine status. The clarifying comment
  correctly states that the tick-side JOIN is the authoritative gate. However,
  accumulated co_access signal for quarantined entries continues to grow in
  the `co_access` table. If an entry is later un-quarantined (restored to
  Active), it would immediately be eligible for promotion with a potentially
  high accumulated count, bypassing the normal slow-build signal accumulation
  that other entries go through.
  This is a data-integrity concern, not a security vulnerability: the
  promotion filter is correctly applied at tick time, and there is no path
  by which a quarantined entry's graph edges are promoted while it remains
  quarantined. The concern is the post-restoration burst promotion case.
  GH #477 tracks the write-time guard as defense-in-depth.
- **Recommendation**: Confirm GH #477 is open and acknowledged. No blocking
  action required for this PR — the tick-side filter is the correct primary
  gate. Consider documenting the "restoration burst" risk in the GH #477
  description if not already present.
- **Blocking**: no

### Finding 5: No input validation concerns — internal tick, no external input
- **Severity**: informational
- **Location**: entire diff
- **Description**: `run_co_access_promotion_tick` is an internal background
  tick called from the server scheduler. It receives only a `&Store` reference
  and an `&InferenceConfig` struct — both are internal, trusted inputs.
  There is no MCP tool surface, no user-supplied parameters, and no
  deserialization of external data in the changed code. OWASP injection,
  broken access control, and deserialization concerns do not apply here.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 6: No hardcoded secrets
- **Severity**: informational
- **Location**: entire diff
- **Description**: Grep confirms no secrets, tokens, API keys, or credentials
  in any changed file.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 7: Test-only `unwrap()` usage
- **Severity**: informational
- **Location**: `co_access_promotion_tick_tests.rs` (multiple lines)
- **Description**: All `unwrap()` calls appear in test helper functions
  (`seed_entry`, `seed_co_access`, `seed_graph_edge`, `count_co_access_edges`,
  `fetch_co_access_edge`) and in `#[tokio::test]` bodies. This is consistent
  with project conventions (`.unwrap()` banned in non-test code, acceptable
  in tests). The production path in `co_access_promotion_tick.rs` uses only
  `match` + `tracing::warn!` + early return — no `unwrap()` anywhere.
- **Recommendation**: No action required.
- **Blocking**: no

---

## Blast Radius Assessment

**Worst case if the fix has a subtle bug:**

The changed code is on the batch SELECT path. Two realistic failure modes:

1. **?3 bind misaligned or value wrong**: If `Status::Quarantined as u8 as i64`
   evaluated to a different integer (it cannot — `#[repr(u8)]` with explicit
   discriminant `= 3` makes this a compile-time constant), all entries would
   be excluded (nothing would match `status != 3` if `3` is wrong), and
   `qualifying_count == 0` would be silently reached every tick. No graph edges
   would be promoted. Failure mode: safe (no edges written, not data corruption).
   The comment in the source correctly notes this as the "missing bind" failure
   mode.

2. **JOIN logic wrong (wrong column, wrong alias)**: If an alias error caused
   the JOIN to filter the wrong column, quarantined entries could still be
   promoted. This is the pre-fix bug being re-introduced. The three regression
   tests would catch this.

3. **Subquery JOIN missing**: If the subquery filter were accidentally removed,
   `max_count` normalization would be corrupted for batches containing
   quarantined high-count pairs. Weights would be artificially low. This would
   affect all simultaneously-promoted pairs' edge weights.
   `test_quarantine_mixed_batch_only_active_pairs_promoted_with_correct_weight`
   directly catches this.

Overall blast radius is low-to-medium: the worst realistic outcome is that
quarantined entries accumulate graph edges (pre-fix state) or that no edges
are promoted (zero-qualifying-count path). Neither path causes data corruption
of existing entries, privilege escalation, or information disclosure.

---

## Regression Risk

**Low.** The fix adds INNER JOIN filters to a SELECT query that previously had
no JOIN against `entries`. The JOINs can only reduce the result set — they
cannot add rows that were not returned before. Existing active-entry promotion
behavior is preserved provided both endpoint entries exist in the `entries`
table with non-Quarantined status.

Pre-existing tests (4264 unit, 22 smoke, 41 lifecycle) all pass. The updated
`seed_co_access` helper now calls `seed_entry` to ensure entry rows exist
before inserting co_access pairs — this is a required change because the new
INNER JOIN would drop pairs with missing entry rows. This change correctly
maintains backward compatibility for all prior tests (INSERT OR IGNORE means
existing test seeds are unaffected).

**One identified regression risk to monitor**: any test that seeds co_access
pairs without seeding corresponding entry rows will now return 0 qualifying
pairs rather than the pair being eligible for promotion. The `seed_co_access`
update mitigates this for all callers in the test file.

---

## OWASP Checklist

| Concern | Assessment |
|---------|-----------|
| A01 Broken Access Control | Not applicable — internal background tick, no user-facing access control change |
| A02 Cryptographic Failures | Not applicable — no cryptography involved |
| A03 Injection | Not applicable — all SQL parameters bound via sqlx positional placeholders, no string interpolation |
| A04 Insecure Design | Not applicable — fix closes a data integrity gap, does not introduce new attack surface |
| A05 Security Misconfiguration | Not applicable — no configuration changes |
| A06 Vulnerable Components | Not applicable — no new dependencies introduced |
| A07 Auth/Identity Failures | Not applicable — no authentication or identity logic changed |
| A08 Data Integrity Failures | Partially applicable (write-time gap, GH #477) — tick-time filter is authoritative; write-time guard is deferred defense-in-depth |
| A09 Logging Failures | Not applicable — logging unchanged |
| A10 SSRF | Not applicable — no network calls |

---

## PR Comments

Posted via gh CLI (see below). No blocking findings.

---

## Knowledge Stewardship

- nothing novel to store — the "INNER JOIN entries for quarantine status
  exclusion" pattern is already documented in Unimatrix entries #3980 and
  #3981 (stored during this bugfix cycle). No recurring anti-pattern observed
  across multiple PRs that would warrant a new lesson-learned entry.
