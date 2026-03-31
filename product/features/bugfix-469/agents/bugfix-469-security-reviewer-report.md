# Security Review: bugfix-469-security-reviewer

## Risk Level: low

## Summary

This PR contains two independent fixes on the same branch: the primary fix for GH #469
(relaxing the `feature_cycle` attribution guard in `nli_detection_tick.rs`) and a
co-bundled fix for GH #468 (SQL ordering regression in `get_cycle_start_goal`). Both
changes are pure logic corrections to internal processing pipelines that operate on
already-stored data. No new external inputs, trust boundaries, or dependencies are
introduced. No blocking findings.

## Findings

### Finding 1 — Guard relaxation widens Informs candidate set (expected behavior)

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/nli_detection_tick.rs:769–774,
  798–802`
- **Description**: The fix removes a pre-filter that required `source_feature_cycle` to
  be non-empty, and relaxes two downstream guards so that entries with an empty
  `feature_cycle` (unknown provenance) are allowed through the Informs detection
  pipeline. This is the intended behavioral change. The security concern to verify is
  whether this opens an injection or spoofing path: could an attacker supply an empty
  `feature_cycle` string to manufacture cross-feature Informs edges they should not
  receive? Assessment: no. The `feature_cycle` value comes from `EntryRecord` in the
  SQLite store, not from a live external request. NLI edge inference is a background tick
  operating on at-rest data. An attacker who can write arbitrary entries to the store has
  already bypassed access control at the MCP layer (context_store tool). This guard does
  not form a security boundary.
- **Recommendation**: No change needed. Document in internal review that this guard is a
  semantic filter, not a trust boundary.
- **Blocking**: no

### Finding 2 — Both-empty path is newly reachable at Site 3

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/nli_detection_tick.rs:798–802`
- **Description**: After Sites 1 and 2 are relaxed, the composite guard
  (`apply_informs_composite_guard`) can now be reached with both
  `source_feature_cycle` and `target_feature_cycle` as empty strings. The old predicate
  `source != target` evaluated to `false` for two equal empty strings, which would have
  blocked this path. The new predicate correctly uses `is_empty() || is_empty() ||
  source != target` — when either side is empty the predicate passes. Verified against
  the logic: `"" != ""` is false, so the old guard would silently block all both-empty
  candidates; the new guard is logically correct. A new test
  (`test_apply_informs_composite_guard_both_empty_passes`) directly exercises this path.
- **Recommendation**: None — the fix is correct and the regression test is present.
- **Blocking**: no

### Finding 3 — SQL ORDER BY direction reversal in get_cycle_start_goal (co-bundled GH #468)

- **Severity**: low
- **Location**: `crates/unimatrix-store/src/db.rs:362–363`
- **Description**: The query changes from `ORDER BY timestamp DESC, seq DESC LIMIT 1`
  to `ORDER BY timestamp ASC, seq ASC LIMIT 1` and adds `AND goal IS NOT NULL` to the
  WHERE clause. Both changes are necessary and correct for the stated semantics
  (first-written-goal-wins). The query uses parameterized binding (`?1`) — no injection
  risk. The `cycle_id` parameter is bound through sqlx, not string-interpolated. The
  ORDER BY change is not exploitable from outside: it is a read-only internal lookup
  whose only effect is which of multiple `cycle_start` rows is returned to the caller.
  Both callers (`tools.rs:2018` and `listener.rs:578`) treat the result as an
  `Option<String>` and handle `None` gracefully (goal stays absent, session still
  registers). There is no access-control implication.
- **Recommendation**: None. The SQL change is safe and well-tested (T-V16-14 renamed,
  T-V16-15 added).
- **Blocking**: no

### Finding 4 — Co-bundled fix (GH #468) not in spawn scope

- **Severity**: informational
- **Location**: `crates/unimatrix-store/src/db.rs`,
  `crates/unimatrix-store/tests/migration_v15_to_v16.rs`,
  `product/features/bugfix-468/agents/468-agent-1-fix-report.md`
- **Description**: This PR includes changes for two distinct GitHub issues (#469 and
  #468) on the same branch. The security reviewer was spawned for #469. The #468 changes
  are present in the diff and have been reviewed above. Both changes are in separate files
  with no interaction. There is no security concern from the co-bundling itself, but it
  is noted for traceability: the gate report only covers #469, and the #468 agent report
  is present but there is no separate #468 gate report visible in the diff. This is an
  observation, not a blocking finding.
- **Recommendation**: Confirm a gate report exists for bugfix-468 (not required from this
  reviewer's scope, noted for completeness).
- **Blocking**: no

## OWASP Assessment

| Concern | Status | Notes |
|---------|--------|-------|
| A03 Injection | Clear | SQL uses parameterized binding (`?1`). No string interpolation of user data into queries. No shell command execution. |
| A01 Broken Access Control | Clear | Guard changes are in a background tick on at-rest data, not an authorization enforcement path. MCP-layer access control is unchanged. |
| A05 Security Misconfiguration | Clear | No configuration changes. Inference config values come from the existing `InferenceConfig` struct, unchanged. |
| A08 Data Integrity Failures | Clear | SQL ordering change improves data integrity (prevents NULL shadowing). NLI guard relaxation is a semantic change with test coverage. |
| A09 Deserialization | N/A | No new deserialization of untrusted input. NLI scores are computed internally from already-validated embeddings. |
| Input Validation | Clear | No new external inputs introduced. `feature_cycle` values originate from the store, not from live request parameters. |
| Secrets | Clear | No hardcoded secrets, API keys, or credentials in the diff. |
| New Dependencies | Clear | No changes to Cargo.toml or Cargo.lock. |

## Blast Radius Assessment

**nli_detection_tick.rs guard relaxation (GH #469)**

Worst case: the relaxed guard allows previously-suppressed entry pairs to reach NLI
inference. If the NLI model assigns a spurious `neutral > 0.5` score to a pair that
should not form an Informs edge, a false-positive Informs edge is written to the graph.
This affects search re-ranking (0.15*confidence boost from co-access) but does not cause
data loss, privilege escalation, or information disclosure. The failure mode is a
degraded relevance ranking for queries that traverse the graph — observable and
reversible via edge correction. Impact is bounded to the Informs graph inference path.

**get_cycle_start_goal ordering change (GH #468)**

Worst case: if the new `ASC` ordering returns the wrong goal in a future edge case not
covered by current tests (e.g., seq collision with non-monotone timestamps), the
`context_cycle_review` tool returns a stale first-written goal rather than a later
correction. This is the exact failure mode that was present before the bug was introduced
— the semantic regression risk is equivalent to the pre-fix state. Both callers degrade
gracefully to `None` on error. Impact: a user sees a stale goal in cycle review output.
No data corruption, no privilege issue.

## Regression Risk

**Low.** The changes affect two isolated paths:

1. Background inference tick — no synchronous user-facing path. Regressions surface as
   missing or extra graph edges in later ticks, not panics or errors. The four new unit
   tests exercise all three guard sites under the new semantics. The existing test
   `test_phase8b_no_informs_when_same_feature_cycle` confirms the intra-feature block
   still functions (non-regression of the intended cross-feature protection).

2. `get_cycle_start_goal` read path — one query, two call sites, both handle `None`
   gracefully. The test suite has six tests for this function including the bug
   reproduction test (T-V16-15). No schema change; no migration.

The gate report records 4262 unit tests, 22/22 smoke, 13/13 contradiction, 41/41
lifecycle passing. No new xfail markers introduced.

## PR Comments

- Posted 1 comment on PR #470 (assessment summary)
- Blocking findings: no

## Knowledge Stewardship

- nothing novel to store — entry #3957 already captures the cross-feature guard
  conflation pattern in full. Entry #604 on permissive safety guards confirms the fix
  approach is consistent with established validation patterns. No generalizable new
  anti-pattern identified across this review.
