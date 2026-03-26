# Security Review: bugfix-408-security-reviewer

## Risk Level: low

## Summary

The change is a single-constant update in `crates/unimatrix-engine/src/coaccess.rs`,
increasing `CO_ACCESS_STALENESS_SECONDS` from 30 days to 365 days. No new inputs, no new
code paths, no new dependencies, and no unsafe code are introduced. The fix is data-only
in the sense that it changes a threshold used in read-side filtering and a maintenance-tick
deletion operation. There are no OWASP-relevant concerns introduced by this change.

## Findings

### Finding 1: No Input Validation Concerns

- **Severity**: N/A (no finding)
- **Location**: `crates/unimatrix-engine/src/coaccess.rs:20`
- **Description**: The changed constant is consumed by callers as `staleness_cutoff = now.saturating_sub(CO_ACCESS_STALENESS_SECONDS)`. `saturating_sub` is used throughout, so no integer underflow on freshly initialised stores (where `now < constant`). No external input touches this constant.
- **Recommendation**: None required.
- **Blocking**: no

### Finding 2: Blast Radius — Longer Retention Increases In-Memory and DB Load Marginally

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/search.rs:740`, `status.rs:606`, `status.rs:872`
- **Description**: Increasing the staleness window 12x means that co-access pairs are retained in the SQLite `CO_ACCESS` table for up to 365 days before being pruned by the maintenance tick. This is the intended behaviour. The worst-case scenario is a high-velocity knowledge base that accumulates a very large number of co-access pairs over the full year before cleanup. The boost computation iterates over live partners per anchor in `compute_search_boost`; more live pairs = slightly more DB reads per search. This is a data-volume concern, not a security concern, and is bounded by `MAX_CO_ACCESS_ENTRIES=10` and `MAX_CO_ACCESS_BOOST=0.03`.
- **Recommendation**: No action required from a security perspective. The bounded constants provide a natural ceiling.
- **Blocking**: no

### Finding 3: No Injection, Access Control, or Deserialization Risks

- **Severity**: N/A (no finding)
- **Location**: all changed lines
- **Description**: The diff adds one constant, expands one doc comment, and adds one assertion test. No user-supplied data flows through any changed line. No file I/O, shell commands, SQL string construction, or deserialization is introduced or modified.
- **Recommendation**: None.
- **Blocking**: no

### Finding 4: No Hardcoded Secrets or Credentials

- **Severity**: N/A (no finding)
- **Location**: entire diff
- **Description**: Grep over the diff for `secret`, `password`, `token`, `api_key`, `credential`, and `private_key` returns zero results. No `Cargo.toml` changes appear in the diff (confirmed via `git diff main...HEAD -- '*.toml'`).
- **Recommendation**: None.
- **Blocking**: no

### Finding 5: No New Dependencies

- **Severity**: N/A (no finding)
- **Location**: N/A
- **Description**: The diff contains no changes to any `Cargo.toml` or `Cargo.lock`. No new crates are introduced.
- **Recommendation**: None.
- **Blocking**: no

### Finding 6: No Unsafe Code

- **Severity**: N/A (no finding)
- **Location**: `crates/unimatrix-engine/src/coaccess.rs`
- **Description**: The file contains no `unsafe` blocks. Confirmed by reading the full source file. The constant arithmetic (`365 * 24 * 3600 = 31_536_000`) fits comfortably in `u64` (max ~1.8e19).
- **Recommendation**: None.
- **Blocking**: no

## Blast Radius Assessment

Worst case: the maintenance tick (`cleanup_stale_co_access` in `status.rs:873`) no longer
prunes pairs that were previously pruned at 30 days. This means pairs accumulate for up to
one year. If this fix itself has a subtle bug (e.g., the constant is accidentally set to an
astronomically large value), the result is unbounded pair retention until a server restart
and re-deploy. The failure mode is storage bloat and search performance degradation — not
data corruption, privilege escalation, or information disclosure. The failure is observable
(slow searches, larger DB file), not silent.

The three production consumers of the constant all use it identically:
`staleness_cutoff = now.saturating_sub(CO_ACCESS_STALENESS_SECONDS)`. All use
`saturating_sub`, so there is no integer underflow risk even if the constant exceeds
the current UNIX timestamp (which it cannot: 31_536_000 << 1_700_000_000).

## Regression Risk

Low. The constant governs only:

1. Which co-access pairs are visible to the boost scorer during search (read-side filter).
2. Which pairs are returned by `co_access_stats` and `top_co_access_pairs` in status
   (reporting only).
3. Which pairs are deleted during maintenance ticks.

Existing functionality changes only in that previously-stale (31–365 day old) pairs now
contribute to search boost. This is the intended correction. No data is deleted by this
change; pairs that would have been cleaned up at the 30-day mark are simply retained
longer. There is no semantic inversion of the filter condition — the `>=` guard in the
regression test confirms the direction.

The verify agent ran 3671 unit tests + 20/20 integration smoke tests with 0 failures,
including co-access-specific integration tests in the adaptation suite. Regression risk
is low.

## PR Comments

- Posted 1 comment on PR #410 with the full security assessment.
- Blocking findings: no

## Knowledge Stewardship

- Stored: nothing novel to store — this is a constant-value change with no OWASP surface.
  The pattern of "single-constant fix with bounded downstream consumers = low security risk"
  is too generic to be worth a dedicated lesson. No novel anti-pattern observed.
