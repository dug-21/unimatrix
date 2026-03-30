# Security Review: crt-034-security-reviewer

## Risk Level: low

## Summary

crt-034 adds a recurring background tick that promotes qualifying `co_access` pairs into
`GRAPH_EDGES` as `CoAccess`-typed edges. The change is pure internal SQL — no new external
input surface, no deserialization of untrusted data, no new dependencies, no secrets. All
SQL is parameterized via sqlx bindings. One non-blocking quality finding is noted
(test file over 500 lines). No blocking security findings.

---

## Findings

### Finding 1: All SQL is fully parameterized — no injection surface

- **Severity**: n/a (clean)
- **Location**: `crates/unimatrix-server/src/services/co_access_promotion_tick.rs`, lines 85–254
- **Description**: Every SQL statement (SELECT, INSERT OR IGNORE, SELECT weight, UPDATE) uses
  `?1`/`?2`/`?3` positional parameters bound via `.bind()`. No `format!`, `concat!`, or string
  interpolation appears anywhere in the production code path. `CO_ACCESS_GRAPH_MIN_COUNT` (i64)
  and `max_co_access_promotion_per_tick` (cast to i64) are typed values, not string-interpolated.
- **Recommendation**: None — this is correct practice. Confirm maintained on future changes.
- **Blocking**: no

### Finding 2: Config field `max_co_access_promotion_per_tick` has validated bounds

- **Severity**: low (noted, clean)
- **Location**: `crates/unimatrix-server/src/infra/config.rs`, lines 893–903
- **Description**: The new operator-settable config field is range-checked `[1, 10000]` at server
  startup. An operator with filesystem access could set this to 10000 causing the tick to process
  a larger batch per cycle — this is within operator privilege scope and does not create a
  privilege escalation vector. The value is used only as a LIMIT binding, not as a dynamic SQL
  fragment.
- **Recommendation**: No change required. The validation error names the field and range, which
  is appropriate for operator feedback.
- **Blocking**: no

### Finding 3: No external/untrusted input touches this tick

- **Severity**: n/a (clean)
- **Location**: `services/co_access_promotion_tick.rs` (full module)
- **Description**: `run_co_access_promotion_tick` reads from `co_access` (internal table written
  only by the co-access recording pipeline, not exposed to MCP callers) and writes to
  `GRAPH_EDGES`. No MCP parameters, no HTTP inputs, no file paths, no deserialization of external
  data. Trust boundary is entirely within the server process.
- **Recommendation**: None.
- **Blocking**: no

### Finding 4: No panic paths in production code

- **Severity**: n/a (clean)
- **Location**: `services/co_access_promotion_tick.rs` (full module)
- **Description**: No `panic!`, `unwrap()`, `todo!()`, or `unimplemented!()` appear in the
  production code path. The `unwrap_or(1)` on `max_count` at line 142 is followed immediately
  by a `max_count <= 0` guard at line 144 that exits cleanly. Division-by-zero risk is fully
  mitigated. All error arms log at `warn!` and continue or return.
- **Recommendation**: None.
- **Blocking**: no

### Finding 5: Test file exceeds 500-line workspace rule (R-12)

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/co_access_promotion_tick_tests.rs`
- **Description**: The test file is 636 lines. The workspace code-quality rule
  (`.claude/rules/rust-workspace.md`) sets a 500-line maximum per file. The implementation file
  (288 lines) correctly extracted tests to avoid breaching the limit itself, but the test file
  now breaches the same limit. The RISK-TEST-STRATEGY.md (R-12) identified this risk and
  explicitly accepted it for tests; the gate-3b report also appears to have accepted it. However,
  the rule applies to all files without a test exception — this should be documented as a known
  deviation or the file should be split.
- **Recommendation**: Either split the test file into two modules (e.g., `_tests_basic.rs`,
  `_tests_edge.rs`) or document a policy exception for test files. No security consequence.
- **Blocking**: no

### Finding 6: `write_pool_server()` used for all reads inside the tick — acceptable but asymmetric

- **Severity**: low (noted)
- **Location**: `services/co_access_promotion_tick.rs`, lines 98, 182, 212, 254
- **Description**: All four database calls (batch SELECT, INSERT, weight SELECT, UPDATE) go
  through `write_pool_server()`. The SELECT queries could use `read_pool()` for separation of
  concerns, but this is deliberately chosen per ADR-001/#3821: using the write pool for the
  batch SELECT ensures read-consistent ordering with the same write sequence (avoiding
  TOCTOU-style inconsistency between the SELECT batch and subsequent INSERT/UPDATE). This is a
  correct design choice, not a misconfiguration.
- **Recommendation**: None — ADR-001 correctly justifies the write pool for all operations.
- **Blocking**: no

### Finding 7: Tick runs unconditionally — no `nli_enabled` guard

- **Severity**: low (noted)
- **Location**: `crates/unimatrix-server/src/background.rs`, line 556
- **Description**: `run_co_access_promotion_tick` is called unconditionally on every tick.
  Unlike NLI inference steps, it has no feature-flag guard. This is documented as intentional
  (architecture: "called unconditionally (no `nli_enabled` guard)"). The tick is purely SQL with
  no ML inference cost and reads only internal tables — no capability escalation risk. The worst
  case if the tick is unexpectedly active in an environment that expects it disabled: edges are
  promoted that the operator did not intend, degrading PPR graph quality silently. This is a
  correctness risk, not a security risk.
- **Recommendation**: If future deployment environments require disabling this tick independently,
  add a boolean config field (consistent with `nli_enabled`). Not required now.
- **Blocking**: no

### Finding 8: No new dependencies introduced

- **Severity**: n/a (clean)
- **Location**: `Cargo.toml`, `Cargo.lock`
- **Description**: `git diff main...HEAD -- Cargo.toml Cargo.lock` produces no output. The
  change introduces zero new crate dependencies. No CVE exposure from new crates.
- **Recommendation**: None.
- **Blocking**: no

### Finding 9: No hardcoded secrets, tokens, or credentials

- **Severity**: n/a (clean)
- **Location**: All changed files
- **Description**: Grep for `api_key`, `secret`, `password`, `token`, `credential` across
  all changed files returns no matches. No hardcoded credentials present.
- **Recommendation**: None.
- **Blocking**: no

### Finding 10: R-01 "write failure mid-batch" test coverage gap — actual injection failure not simulated

- **Severity**: low
- **Location**: `co_access_promotion_tick_tests.rs`, lines 975–1023 (Group F)
- **Description**: The R-01 (Critical priority) test `test_write_failure_mid_batch_warn_and_continue`
  does not actually inject a write failure. It seeds pair (1,2) with a pre-existing edge at the
  computed weight to produce a no-op path, then confirms remaining pairs are processed. This
  tests the "skip on no-op" path, not the "skip on SQL error + emit warn!" path called out in
  R-01. A true R-01 test would require a way to simulate a constraint error or pool exhaustion
  mid-batch. The RISK-TEST-STRATEGY.md acknowledged this constraint ("no injected write failures
  possible without pool shimming") but the acceptance criteria still use R-01 language for this
  test. This is a test-coverage gap, not a security defect in the production code.
- **Recommendation**: Document the limitation in the test's doc comment, or add a pool-shimming
  approach in a future cycle. The infallible contract itself is correct in production code.
- **Blocking**: no

---

## OWASP Checklist

| OWASP Concern | Assessment |
|---------------|-----------|
| A03 Injection (SQL) | Clean — all SQL uses positional `?N` bindings via sqlx |
| A01 Broken Access Control | Clean — tick operates entirely within server process on internal tables; no privilege boundaries crossed |
| A05 Security Misconfiguration | Clean — new config field is validated and defaults are safe |
| A08 Software and Data Integrity | Low risk — INSERT OR IGNORE + delta guard prevent data corruption; write errors are logged |
| A06 Vulnerable Components | Clean — no new dependencies |
| A09 Security Logging and Monitoring Failures | Clean — warn! on every individual write failure, info! always fires at end; SR-05 early-tick warn! for signal-loss detection |
| Deserialization | Not applicable — no external deserialization introduced |
| Path Traversal | Not applicable — no file path operations |
| Hardcoded Secrets | Clean — none present |

---

## Blast Radius Assessment

**Worst case if the fix has a subtle bug:**

The promotion tick writes only to `GRAPH_EDGES`. The downstream consumer is `TypedGraphState::rebuild()`, which runs immediately after in the same tick cycle. A worst-case subtle bug would be one of:

1. **Wrong weight normalization** — edges are promoted with incorrect weights, causing PPR to
   over- or under-weight co-access relationships. Effect: degraded retrieval ranking. No entry
   content, no auth state, no user data is affected.

2. **INSERT without UNIQUE constraint firing** — if `bootstrap_only` or `source` column value
   causes a constraint violation that INSERT OR IGNORE does not suppress, a pair's INSERT fails
   silently (logged at warn!) and the pair is not promoted. Effect: edges missing from the PPR
   graph. Retrieval degrades, but no corruption.

3. **Silent all-tick failure** — if `write_pool_server()` pool is exhausted, all 200 pairs in
   a tick fail. The info! log shows "0 inserted, 0 updated". The pairs are retried next tick.
   No data loss; recovery is automatic.

**Worst case is**: PPR graph has incorrect CoAccess edge weights for one tick cycle, producing
slightly degraded retrieval ranking. The failure heals on the next tick. No entry content, no
auth data, no user-visible state other than retrieval ordering is within blast radius.

---

## Regression Risk

**Existing functionality at risk:**

1. **`TypedGraphState::rebuild()`** — now reads freshly promoted CoAccess edges every tick. If
   the promotion tick inserts malformed edges (bad source_id/target_id not in entries), the
   rebuild may silently skip them (documented behavior: node_index lookup returns None → edge
   skipped) or raise a graph construction error. The orphaned-edge compaction in step 2 removes
   edges with deleted entry endpoints before promotion runs, which correctly limits this risk.

2. **Tick loop timing** — the promotion tick adds one SQL round-trip (batch SELECT) plus up to
   N×2 round-trips for the per-pair INSERT+weight-check cycle. For the default cap of 200 pairs,
   this is bounded. The tick is wrapped in `TICK_TIMEOUT` (GH #266), so a slow tick cannot block
   the background loop.

3. **`co_access` table ordering** — existing tests that seed co_access pairs should not be
   affected since the promotion tick only writes to `GRAPH_EDGES`, not back to `co_access`.

4. **Config deserialization** — existing configs without `max_co_access_promotion_per_tick` will
   deserialize cleanly via the serde default function (value = 200). No breaking change to
   config format.

**Regression risk is low.** The change is additive (new step, no modifications to existing
steps), and failure is contained (infallible contract, returns `()`).

---

## PR Comments

- Posted 1 comment on PR #457 via `gh pr review --comment`.
- Blocking findings: **no**.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the parameterized SQL safety pattern and infallible tick
  contract are well-established in this codebase. The test file >500 lines finding is already
  captured in existing lesson #3580 (gate-3b file size violations). The "test approximates
  R-01 without true write injection" gap is specific to this PR and not yet generalizable
  across 2+ features.
