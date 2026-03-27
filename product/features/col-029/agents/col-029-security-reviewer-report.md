# Security Review: col-029-security-reviewer

## Risk Level: low

## Summary

col-029 adds six read-only graph cohesion metrics to `context_status` via two parameterless SQL aggregate queries over trusted internal data. The attack surface is zero — no external input is interpolated into SQL, no new dependencies are introduced, and the function is strictly read-only. One informational finding is noted: `EDGE_SOURCE_NLI` is defined and exported but not yet adopted by `nli_detection.rs`, which still uses bare `'nli'` string literals in embedded SQL. The architecture documents this as an intended follow-up, not a defect. No blocking findings.

## Findings

### Finding 1: EDGE_SOURCE_NLI constant not adopted in nli_detection.rs

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/nli_detection.rs` lines 545, 599, 1054, 1129
- **Description**: The `EDGE_SOURCE_NLI` constant is defined in `unimatrix-store/src/read.rs` and re-exported from `lib.rs` as required by ADR-001. However, `nli_detection.rs` still uses bare `'nli'` string literals in embedded SQL strings (e.g. `'nli', 'nli'` in the INSERT at line 545, `'nli', 'nli'` at line 599, `source='nli'` at line 1054) and in a Rust string comparison (`e.source != "nli"` at line 1129 in `background.rs`). The constant does not resolve SR-01 (silent string divergence risk) until the callers also reference it. ADR-001 notes this as a follow-up task for the #412 implementation; the scope intentionally omits changing existing callers. This is informational — the constant exists and the coupling risk is reduced for new code, but the existing coupling in `nli_detection.rs` remains.
- **Recommendation**: File a follow-up issue (or add to GH #412) to migrate the bare `'nli'` literals in `nli_detection.rs` and `background.rs` to use `EDGE_SOURCE_NLI`. This is a housekeeping item, not a blocker.
- **Blocking**: no

### Finding 2: Summary format suppression condition silently omits supports_edge_count > 0 case

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/response/status.rs` lines 256-258
- **Description**: The Summary format graph cohesion line is suppressed when `isolated_entry_count == 0 AND cross_category_edge_count == 0 AND inferred_edge_count == 0`. A store where only `supports_edge_count > 0` (and no NLI, no cross-category, no isolated entries) would suppress the Summary line while having real edge data. The RISK-TEST-STRATEGY documents this as R-10 (Low severity) and explicitly accepts it, noting the Markdown format always shows the sub-section. No security impact; informational only.
- **Recommendation**: No action required — the risk was accepted in the risk strategy. The Markdown format provides full visibility. Consider noting in operator documentation that Summary may omit the cohesion line on homogeneous stores.
- **Blocking**: no

### Finding 3: i64 to u64 cast without range check for SQL aggregate results

- **Severity**: low
- **Location**: `crates/unimatrix-store/src/read.rs` lines 1134-1137 and 1129
- **Description**: SQL aggregate counts (`supports_count`, `inferred_count`, `cross_cat`) are fetched as `i64` and cast directly to `u64` without range checking. SQLite's COUNT() and SUM() with COALESCE cannot return negative values, and the COALESCE guards ensure no NULL. The isolated count uses `saturating_sub` which is correct. In practice these values are non-negative counts, so the cast is safe. The pattern is consistent with the existing codebase (dozens of similar casts in the surrounding code).
- **Recommendation**: No change required. The defensive `saturating_sub` on the isolated count (line 1129) covers the one case where underflow could theoretically occur. The other three fields (supports, inferred, cross_cat) cannot be negative given their SQL definitions.
- **Blocking**: no

## OWASP Assessment

| OWASP Concern | Applicable | Assessment |
|---------------|-----------|------------|
| Injection (SQL) | No | `compute_graph_cohesion_metrics()` is parameterless. No user-supplied values appear in any SQL string. Both queries are compile-time string constants. |
| Broken Access Control | No | `context_status` is Admin-only (unchanged by this feature). The new fields are returned only to callers who already cleared the Admin gate. |
| Security Misconfiguration | No | No new configuration surface. `read_pool()` is existing infrastructure. |
| Vulnerable Components | No | No new crates or dependency changes. Cargo.lock is unchanged. |
| Data Integrity Failures | No | The function is strictly read-only (SELECT aggregates only). It cannot modify store state. |
| Deserialization Risks | No | No deserialization of external data. |
| Input Validation Gaps | No | No inputs. The function takes `&self` only. |

## Blast Radius Assessment

The worst-case scenario if this code has a subtle bug:

- **Incorrect metrics reported**: An operator sees incorrect graph topology values in `context_status` output. This leads to incorrect operational decisions (e.g., concluding NLI inference is not producing connected graph when it is). No data is corrupted; the knowledge base is unaffected.
- **SQL error at query time**: If `compute_graph_cohesion_metrics()` returns `Err`, the non-fatal error path (`tracing::warn! + skip`) means the report is still returned with all six cohesion fields at zero. The tool remains operational. Operators see zero metrics, which looks identical to an empty store, but no data is lost.
- **No write path involvement**: The function uses `read_pool()` exclusively. A bug cannot corrupt any stored data. The blast radius is strictly limited to the diagnostic output of `context_status`, which is an Admin-only observability tool.

## Regression Risk

- **StatusReport struct additions**: Six new public fields are appended to an existing struct with a hand-written `Default` impl. The diff correctly adds all six fields to `StatusReport::default()`, `StatusReportJson`, the `From<&StatusReport>` impl, and all test struct constructors in `mod.rs`. Compile-time verification ensures nothing is missed.
- **Existing tests**: The migration test `migration_v16_to_v17.rs` change is a cosmetic `use` statement reordering (import sort) with no semantic change. Existing test fixtures in `mod.rs` are updated with the six new zero-value fields — this is structural, not behavioral.
- **context_status behavior**: The Markdown format now always includes a `#### Graph Cohesion` sub-section. This is an additive change to the response format. Callers that parse the Markdown response structurally (e.g., asserting exact string content) could break if they assert the absence of this section. This is a documentation-level regression risk, not a security risk.
- **No schema migration**: No database schema changes. The feature reads existing tables.

## Dependency Safety

No new dependencies introduced. Cargo.lock unchanged. No known CVE exposure.

## Secrets Check

No hardcoded secrets, API keys, credentials, or tokens in the diff. The only string constants added are `EDGE_SOURCE_NLI: &str = "nli"` (a database column value, not a credential).

## PR Comments

- Posted 1 comment on PR #416
- Blocking findings: no

## Knowledge Stewardship

- Nothing novel to store — the `EDGE_SOURCE_NLI` adoption gap is feature-specific (documented in ADR-001 as an intended follow-up) and does not represent a generalizable anti-pattern beyond what is already captured in the existing codebase knowledge about string coupling risks.
