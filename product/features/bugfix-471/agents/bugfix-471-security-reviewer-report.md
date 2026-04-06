# Security Review: bugfix-471-security-reviewer

## Risk Level: medium

## Summary

The core compaction fix is correct: switching `status != Quarantined` to `status = Active` in the GRAPH_EDGES DELETE properly closes the deprecated-endpoint gap. However, a medium-severity finding exists: the co_access promotion tick (`co_access_promotion_tick.rs`) still uses the old denylist form `status != Quarantined` and is not covered by deprecated-endpoint tests. This creates an asymmetric predicate between compaction (allowlist) and promotion (denylist) — exactly the oscillation pattern documented in Unimatrix entry #3978. Deprecated-endpoint co_access pairs will be filtered by compaction each tick then re-promoted by the promotion tick on the next tick. The diff does not introduce security vulnerabilities, injection risks, or secrets.

## Findings

### Finding 1: Promotion tick uses denylist form — deprecated endpoints will re-promote after compaction deletes them
- **Severity**: medium
- **Location**: `crates/unimatrix-server/src/services/co_access_promotion_tick.rs:232-237,244`
- **Description**: The compaction DELETE (just fixed by this PR) now uses the allowlist `status = Active`. The promotion tick SELECT uses `status != ?3` bound to `Status::Quarantined`. This denylist form passes Deprecated entries (status=1) through: a co_access pair where one endpoint is Deprecated will be excluded from GRAPH_EDGES by compaction, then re-inserted by the promotion tick on the same or next tick. This is not a new regression introduced by this PR — it is a pre-existing asymmetry. However, this PR explicitly fixes compaction to use the allowlist form and its gate-3c report characterises the fix as resolving deprecated-endpoint edge accumulation. With the asymmetric promotion tick still live, deprecated-endpoint edges will continue to accumulate in GRAPH_EDGES via the promotion path, making the fix incomplete at runtime. Unimatrix entry #3978 documents this exact oscillation pattern from bugfix-476: "compaction correctly deletes...promotion immediately re-inserts them." The promotion tick comment (line 216-220) states it "excludes quarantined-endpoint pairs" but makes no mention of deprecated endpoints.
- **Recommendation**: The promotion tick SELECT should be updated to use `status = Active` (allowlist) on both endpoints and in the max_count subquery, replacing `status != ?3` / `Status::Quarantined`. This would be consistent with the compaction fix and with entry #4156's stated rule: "Every compaction or prune DELETE that references `entries.id` must use `WHERE status = Active` (allowlist)." The promotion tick SELECT is the mirror-image operation. Alternatively, if the intent is to allow deprecated-endpoint promotion (e.g., for supersedes-chain traversal reasons — see typed_graph.rs:101 comment), this must be explicitly documented and the compaction fix must be re-evaluated for consistency.
- **Blocking**: No — this is a pre-existing condition that was present before this PR landed. The PR does not make it worse. However, it is materially relevant to the stated goal of the fix and should be tracked.

### Finding 2: typed_graph.rs rebuild uses denylist for quarantined but explicitly includes deprecated — undocumented interaction with compaction
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/typed_graph.rs:99-102`
- **Description**: `TypedGraphState::rebuild` includes Deprecated entries in the graph (by design, for supersedes-chain traversal). The compaction now deletes GRAPH_EDGES where either endpoint is not Active. After compaction, the rebuild will see no edges for deprecated endpoints even though deprecated entry nodes are included. This is likely correct (edges should be gone, nodes retained for traversal), but the interaction is not commented in the rebuild code. No security impact — this is a correctness/clarity note.
- **Recommendation**: Add a comment in typed_graph.rs near line 101 noting that compaction removes GRAPH_EDGES for deprecated endpoints so the rebuild will not see such edges regardless of node inclusion.
- **Blocking**: No.

### Finding 3: No test coverage for deprecated endpoint in promotion tick
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/co_access_promotion_tick_tests.rs`
- **Description**: Unimatrix entry #4156 explicitly states: "For every compaction DELETE, write tests for: (a) endpoint fully absent, (b) endpoint quarantined, (c) endpoint deprecated, (d) endpoint active." The new tests in background.rs cover cases (a)-(d) for compaction. But `co_access_promotion_tick_tests.rs` has no test for a deprecated-endpoint co_access pair. Given that the promotion tick uses the denylist form (Finding 1), a deprecated endpoint will be promoted. A test asserting that deprecated-endpoint pairs are NOT promoted would fail against the current code — surfacing the asymmetry.
- **Recommendation**: Add a test in `co_access_promotion_tick_tests.rs` verifying that a co_access pair with a Deprecated endpoint is NOT promoted into GRAPH_EDGES. This would either (a) pass after the promotion tick is fixed to use the allowlist, or (b) serve as a regression sentinel until that follow-up is done.
- **Blocking**: No.

## OWASP Checklist

| Concern | Assessment |
|---------|-----------|
| SQL Injection | Not present. All SQL uses parameterized queries with `.bind()`. The changed SQL strings contain no user-controlled interpolation. |
| Broken access control | Not applicable to this change. No trust boundary or permission check was modified. |
| Security misconfiguration | Not applicable. No configuration values changed. |
| Deserialization | Not applicable. Change is pure SQL logic. |
| Input validation | Not applicable. The changed code operates on internal status enum values, not external input. |
| Hardcoded secrets | None found. |
| Vulnerable dependencies | No new dependencies introduced. |
| Injection (command/path) | Not present. No shell commands or file path operations. |

## Blast Radius Assessment

**Worst case if the compaction SQL has a subtle bug:**

- If `status = Active` accidentally deleted too broadly (e.g., SQLite type coercion caused `Active as u8 as i64 = 0` to match nothing), compaction would delete ALL GRAPH_EDGES every tick. TypedGraphState rebuild would start from an empty graph each tick. PPR scores would collapse to zero; graph-based re-ranking would be blind. This would be immediately visible in integration tests and in production metrics (`edges_inserted` exploding each tick).
- If `status = Active` accidentally protected too broadly, the original bug would persist. Deprecated-endpoint edges would survive. The existing new tests would catch this.
- Neither scenario involves data corruption, privilege escalation, or information disclosure. Failure mode is behavioral (graph scoring quality), not security.

**Actual code path of the change:**
The DELETE runs inside `run_single_tick` inside `{...}` block, wrapped in `match compaction_result` with a non-fatal error path. A SQL error logs at `error!` and proceeds with rebuild against pre-compaction state. This is safe.

## Regression Risk

- **Low for existing functionality**: The only behavioral change is that deprecated-endpoint edges are now removed by compaction. Previously they accumulated indefinitely. Removing stale edges from the graph improves PPR correctness; it does not degrade any current correct behavior.
- **Graph score shifts are expected**: Any co_access pair involving a deprecated entry will lose its CoAccess edge after the first post-deploy tick. PPR scores for active entries that were frequently co-accessed with now-deprecated entries may shift slightly. This is the intended outcome, not a regression.
- **Oscillation risk (Finding 1)**: If the promotion tick immediately re-promotes deleted edges, the net effect is zero: compaction deletes at start of tick, promotion re-inserts at end of same tick. The fix appears to have no net runtime effect on deprecated-endpoint edges until the promotion tick is also fixed. This is not a new regression but a pre-existing condition that limits the fix's effectiveness.
- **4 new tests**: All 4 added tests are correctly scoped, use isolated tempdir stores, and are symmetric across CoAccess and Supports edge types and source/target positions. They will continue to pass.

## PR Comments

- Posted 1 comment on PR #527 (see below).
- Blocking findings: No.

## Knowledge Stewardship

- Nothing novel to store — entry #4156 already captures the allowlist rule and notes that it applies to all compaction/promotion SQL passes. Entry #3978 already captures the oscillation pattern. The specific gap (promotion tick not fixed alongside compaction) is worth noting as a follow-up in the PR, which is done via PR comment.
