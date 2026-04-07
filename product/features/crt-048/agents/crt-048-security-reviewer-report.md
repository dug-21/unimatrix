# Security Review: crt-048-security-reviewer

## Risk Level: low

## Summary

crt-048 is a pure deletion of computation logic and struct fields — removing the `confidence_freshness` dimension from the Lambda coherence metric. No new input surfaces, no new deserialization paths, no new file or shell operations, and no new dependencies are introduced. All changed code is confined to internal pure-function math, struct field removal, and test fixture cleanup within `unimatrix-server`. The single operational risk (breaking JSON output change) is a documented, intentional API reduction with zero confirmed live callers outside the Rust test suite.

---

## Findings

### Finding 1: Intentional Breaking JSON Change — Operational Risk, Not Security Risk

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/response/status.rs` — `StatusReportJson` struct
- **Description**: `confidence_freshness_score` and `stale_confidence_count` are removed from the JSON output of `context_status`. Any external script or operator tool parsing these field names will receive empty/null results silently after upgrade. OQ-2 (pre-delivery grep of `product/test/`) confirmed zero live callers in the test suite. Three independent tests verify the fields are absent from all output formats (Summary, Markdown, JSON) plus an integration test at the MCP wire level.
- **Recommendation**: Ensure the PR description includes the two removed field names as a release-note item (NFR-06 / C-07 requirement). No code change needed — the tests are correct and the removal is complete.
- **Blocking**: no

### Finding 2: coherence_by_source Now Returns Identical Lambda for All Sources

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/status.rs` lines 770–780
- **Description**: The `coherence_by_source` loop previously differentiated per-source lambda by computing per-source `confidence_freshness_score`. With freshness removed, the loop now passes global `report.graph_quality_score`, `embed_dim`, and `report.contradiction_density_score` — all of which are system-wide scalars — for every source. All sources will therefore receive an identical lambda value. This is architecturally correct: graph quality, contradiction density, and embedding consistency are not per-source properties. However, the diagnostic value of `coherence_by_source` is now reduced to a per-source enumeration with a constant lambda value. This is not a security issue but a semantic change in observable behavior that is not called out in the spec or PR description.
- **Recommendation**: Document in the PR body that `coherence_by_source` now reports the same lambda for all sources (since freshness was the only per-source-variable dimension). Operators who previously used per-source lambda to identify trust sources with stale confidence should be aware this diagnostic is gone.
- **Blocking**: no

### Finding 3: `_entries` Variable Suppression in coherence_by_source Loop

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/status.rs` line 771 — `for (source, _entries) in &source_groups`
- **Description**: The `_entries` underscore suppression is correct Rust idiom for "this binding is intentionally unused." The loop body no longer needs per-source entries since per-source freshness is removed. The `active_entries` allocation is retained (for the `source_groups` HashMap build) and this is architecturally consistent with FR-11. No dead-code security risk.
- **Recommendation**: No action needed. This is intentional and the Rust compiler accepts it without warning.
- **Blocking**: no

### Finding 4: No Hardcoded Secrets or Credentials

- **Severity**: n/a
- **Location**: all changed files
- **Description**: Full diff scan found no API keys, tokens, passwords, bearer strings, or credential literals in any added line. All `format!` macro calls in `generate_recommendations()` interpolate only numeric types (`u64`, `usize`, computed percentage as `u64`).
- **Recommendation**: none
- **Blocking**: no

### Finding 5: No New External Input Surfaces

- **Severity**: n/a
- **Location**: all changed files
- **Description**: The diff introduces no new MCP tool parameters, no new file path operations, no shell command invocations, no deserialization of external data, and no new database queries. `compute_lambda()` accepts only `f64` and `Option<f64>` — types that cannot carry injection payloads. The recommendation strings in `generate_recommendations()` use only computed numeric values.
- **Recommendation**: none
- **Blocking**: no

### Finding 6: No New Dependencies

- **Severity**: n/a
- **Location**: `Cargo.toml` files (unchanged)
- **Description**: No new crate dependencies appear in any `Cargo.toml`. The diff contains no `+` lines touching any manifest file.
- **Recommendation**: none
- **Blocking**: no

---

## OWASP Evaluation

| OWASP Concern | Applicable? | Assessment |
|---------------|-------------|------------|
| A03 — Injection | No | No new string interpolation of external data. All format! inputs are numeric. |
| A01 — Broken Access Control | No | No changes to trust-level checks, admin guards, or capability enforcement. |
| A05 — Security Misconfiguration | No | `DEFAULT_STALENESS_THRESHOLD_SECS` retained at 86400. No configuration changes. |
| A06 — Vulnerable Components | No | No new dependencies added. |
| A08 — Data Integrity Failures | No | All test fixture sites properly updated; build gate would catch any missed site. |
| A08 — Insecure Deserialization | No | No new deserialization paths. JSON output is reduced, not expanded. |
| A03 — Path Traversal | No | No file path operations in the diff. |

---

## Blast Radius Assessment

**Worst case if the fix contains a subtle bug**: Lambda is computed from wrong argument values (positional transposition of `graph_quality` and `contradiction_density`). The result remains in [0.0, 1.0] and passes range checks. The coherence gate fires based on the wrong structural dimension. Maintenance recommendations may be issued or suppressed incorrectly. This is an operational observability failure, not a security failure. No data corruption, no privilege escalation, no information disclosure occurs.

**Scope of blast radius**: Confined to `context_status` output — specifically the `coherence` value, per-dimension scores, and maintenance recommendations. Search ranking, confidence scores, and all other MCP tools are unaffected. The `run_maintenance()` background tick is unaffected (it uses `DEFAULT_STALENESS_THRESHOLD_SECS` directly, not Lambda).

**Detection**: The `lambda_specific_three_dimensions` and `lambda_single_dimension_deviation` tests use distinct per-dimension values and assert exact results (within 1e-10). Positional transposition of any two `f64` arguments would produce a detectably different result in those tests.

---

## Regression Risk

**Low.** All changes are deletions or simplifications. No new code paths are introduced. The Rust compiler enforces structural completeness — any missed field removal in `StatusReport` or `StatusReportJson` produces a compile error, not a silent regression. Verified:

- `DEFAULT_STALENESS_THRESHOLD_SECS` retained at line 13 of `coherence.rs` with correct doc comment — `run_maintenance()` unaffected.
- Both `compute_lambda()` call sites (main path line 751, per-source loop line 772) use identical 4-argument form matching the new function signature.
- `generate_recommendations()` call site passes 5 arguments matching the new 5-parameter signature.
- All 8 fixture sites in `mcp/response/mod.rs` have both field references removed (confirmed by diff — all 8 sites match the architecture's enumeration).
- No references to `confidence_freshness_score` or `stale_confidence_count` remain in `crates/unimatrix-server/src/` except in test assertion strings checking for their absence.
- `lambda_weight_sum_invariant` test uses `< f64::EPSILON` per NFR-04, not exact `==`.
- `lambda_renormalization_without_embedding` includes a non-trivial case (R-07) verifying 2-of-3 re-normalization with distinct values.

**Existing tests deleted**: 11 freshness-function tests in `coherence.rs` and 4 coherence field tests in `mod.rs` were removed. These tested deleted functions and deleted struct fields — their removal is correct. The deleted `test_coherence_json_all_fields` test no longer makes sense post-removal and is replaced by `test_status_json_no_freshness_keys` which asserts field absence.

---

## PR Comments

- Posted 1 comment on PR #537 (informational — coherence_by_source behavioral change note)
- Blocking findings: no

---

## Knowledge Stewardship

- nothing novel to store — this feature's security profile is uniquely low-surface (pure deletion of pure-function math). No generalizable anti-pattern emerged. The relevant patterns (#325 StatusReportJson backward-compat, #2909 ingest security bounds) already exist in Unimatrix and did not apply here.
