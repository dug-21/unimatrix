# Security Review: bugfix-491-security-reviewer

## Risk Level: low

## Summary

The fix replaces an inclusive SQL filter (`source = 'nli'`) with an exclusive one
(`source NOT IN ('co_access', '')`) in `compute_graph_cohesion_metrics()`. The changed
field is a monitoring-only counter in the `context_status` response — it has no effect
on search ranking, write paths, confidence scoring, or access control. The SQL constant
interpolated via `format!()` is a compile-time Rust `pub const &str` whose value is the
fixed string `"co_access"`, containing no SQL metacharacters and no user-controlled
content. No blocking findings.

## Findings

### Finding 1: format!() SQL construction — constant-only, no injection surface
- **Severity**: informational
- **Location**: `crates/unimatrix-store/src/read.rs:1016-1025`
- **Description**: The SQL is built with `format!()` rather than a bound parameter.
  This is the only new `format!()` SQL call introduced by this PR. The interpolated
  value is `EDGE_SOURCE_CO_ACCESS`, a `pub const &str = "co_access"` defined at
  compile-time in the same file. The string contains only ASCII lowercase letters and
  an underscore — no SQL metacharacters (`'`, `"`, `\`, `--`, `;`, etc.). There is
  no user input involved. The value is used to construct the static IN-list literal
  `NOT IN ('co_access', '')` that is baked into the query text before it reaches
  SQLite. Sqlx bound parameters (e.g., `?1`) would be the preferred approach when
  the value is external; here the value is internal and compile-time, so the risk is
  purely theoretical and zero in practice. This pattern is consistent with other
  existing `format!()` SQL constructions in `read.rs` (lines 125, 164, 242, 281, 305,
  440) that interpolate column-name constants, none of which carry user input.
- **Recommendation**: Acceptable as-is. If a future EDGE_SOURCE constant ever
  contains a quote or metacharacter, this pattern would need a bound parameter or
  explicit escaping. Document the constraint in the format!() call site or in the
  EDGE_SOURCE constant declaration: "value must contain only [a-z_] characters."
  This is a low-priority hardening suggestion, not a blocking issue.
- **Blocking**: no

### Finding 2: "behavioral" source value has no named constant (pre-existing gap)
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/behavioral_signals.rs:76`
- **Description**: The `"behavioral"` edge source is written as a bare string literal
  in `behavioral_signals.rs` — no `EDGE_SOURCE_BEHAVIORAL` constant exists. This is a
  pre-existing condition introduced before this PR; the fix correctly references
  `"behavioral"` in TC-15 as a test-only string (no production path via the test
  fixture), and the doc comments in `status.rs:86` and `read.rs:1765` list it
  as a future inference source. The risk is the same as the original bug (#491) — a
  future rename of the bare literal would silently diverge from any filter that uses
  the string. This is not introduced by the current fix.
- **Recommendation**: Follow-up issue to introduce `EDGE_SOURCE_BEHAVIORAL` constant
  and use it in `behavioral_signals.rs`, consistent with ADR-001 col-029 (EDGE_SOURCE
  named constant mandate). Not blocking for this PR.
- **Blocking**: no

### Finding 3: No input validation change at any trust boundary
- **Severity**: informational
- **Location**: N/A (no change in input validation paths)
- **Description**: The changed code reads from the `graph_edges` table. No MCP tool
  parameters or user-controlled input flow into the modified query. The `source` column
  values in `graph_edges` are written only by internal Rust code paths using named
  constants or hardcoded literals. No external deserialization path touches this column
  in a way that could inject arbitrary values, and the query reads those values with an
  aggregate CASE expression rather than returning them verbatim.
- **Recommendation**: No action needed.
- **Blocking**: no

### Finding 4: No new dependencies introduced
- **Severity**: informational
- **Location**: N/A
- **Description**: No new crates or external dependencies are added. Imports in
  `nli_detection_tick.rs` expand only to additional already-present constants from
  `unimatrix-store` (`EDGE_SOURCE_CO_ACCESS`, `EDGE_SOURCE_NLI`, `EDGE_SOURCE_S1`,
  `EDGE_SOURCE_S2`, `EDGE_SOURCE_S8`). No `cargo audit` concerns.
- **Recommendation**: No action needed.
- **Blocking**: no

### Finding 5: No secrets, credentials, or hardcoded tokens
- **Severity**: informational
- **Location**: N/A
- **Description**: The diff contains no API keys, passwords, tokens, or secrets.
- **Recommendation**: No action needed.
- **Blocking**: no

## OWASP Checklist

| Concern | Status | Notes |
|---------|--------|-------|
| A03 Injection (SQL) | Clear | format!() interpolates a compile-time constant with no metacharacters; no user input reaches the SQL fragment |
| A03 Injection (path/command) | N/A | No file paths or shell commands in scope |
| A01 Broken Access Control | N/A | Monitoring-only read path; no access control change |
| A05 Security Misconfiguration | Clear | No configuration changes |
| A08 Data Integrity (deserialization) | N/A | No new deserialization of untrusted data |
| A06 Vulnerable Components | Clear | No new dependencies |
| A09 Security Logging | Clear | Error path logs via tracing::warn; no internal state leak |

## Blast Radius Assessment

`inferred_edge_count` is consumed by exactly one production code path: `StatusService::compute_report()`,
which maps it directly into `StatusReport::inferred_edge_count`. That field is serialized
to the `context_status` MCP tool response (text and JSON formats). It does not feed into
lambda/coherence computation, search ranking, confidence scoring, or any write path.

Worst case if the fix has a subtle bug:
- The counter returns a wrong value (higher or lower than correct).
- The `context_status` response shows an inaccurate inferred edge count.
- No data is corrupted. No query results are affected. No write is triggered.
- The failure mode is information disclosure at monitoring granularity (one wrong integer
  in a diagnostic report) — not data corruption, not denial of service, not privilege
  escalation.

The blast radius is confined to observability accuracy. This is the lowest-impact
blast radius class in this codebase.

## Regression Risk

**Low.** The change is narrow: one SQL aggregate CASE expression in one query, in one
method called only from the status service. The table-driven TC-15 test now covers
seven source values (six counted, one excluded) and would catch regression in either
direction — a filter that is too broad (counting co_access) or too narrow (missing
any of the named inference sources). The two integration test functions updated in
`test_lifecycle.py` are both marked `xfail` due to CI infrastructure constraints
(no embedding model / tick timeout), so they provide no regression protection in CI
but also introduce no false-positive risk.

The existing graph cohesion metric tests in `read.rs` (lines 2130–2351) cover the
`inferred_edge_count = 0` cases for bootstrap-only edges and the `inferred_edge_count = 1`
case for a non-co_access non-bootstrap edge — these continue to pass unchanged.

## PR Comments

- Posted 1 comment on PR #531 via `gh pr review`.
- Blocking findings: no.

## Knowledge Stewardship

- nothing novel to store — Finding 2 (bare "behavioral" literal) is a pre-existing gap
  already covered by the spirit of ADR-001 col-029 (EDGE_SOURCE named constant mandate).
  The pattern itself is stored as entry #3591. A new lesson-learned entry would duplicate
  that ADR without adding signal. The behavioral gap warrants a follow-up GH issue, not
  a Unimatrix entry.
