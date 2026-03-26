# Security Review: nan-008-security-reviewer

## Risk Level: low

## Summary

nan-008 adds two distribution-aware metrics (CC@k and ICD) to the eval harness,
extending four existing files in-process and adding no new modules, network
surfaces, or runtime dependencies. All new inputs originate from local filesystem
files operated by development tooling only. No production server paths, MCP tools,
or authenticated endpoints are modified. One pre-existing byte-slicing pattern was
extended to a second call site in this PR; it is documented below but is
non-blocking for a local-only CLI tool.

## Findings

### Finding 1: Byte-slice truncation of query strings may panic on multi-byte UTF-8 input

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/eval/report/aggregate.rs:369` (new in nan-008)
  and `crates/unimatrix-server/src/eval/report/render.rs:308,337` (new in nan-008)
- **Description**: `result.query[..60]` and `row.query[..query_len]` are byte-index
  slices on `String` values. If a query string contains multi-byte UTF-8 characters
  (e.g., Unicode queries containing `ln(n)` notation, Japanese, or emoji) and the
  byte boundary falls in the middle of a multi-byte sequence, Rust will panic at
  runtime with "byte index N is not a char boundary". This is a pre-existing
  pattern in `render.rs` for title truncation (`title_len`) introduced before
  nan-008; nan-008 extends it to two new call sites for `query` and `scenario_id`
  strings. Scenario queries are sourced from the MCP `query_log` table, which can
  contain arbitrary UTF-8 user queries.
- **Recommendation**: Use `str::char_indices` or `String::chars().take(N)` to truncate
  at a safe character boundary rather than a raw byte index. Example:
  `query.chars().take(60).collect::<String>()`. This is consistent with the fix that
  should also be applied to the pre-existing `title_len` truncation pattern.
- **Blocking**: No. This tool is a local CLI development artifact, not a server
  path. A panic terminates `eval report` — it does not affect the running Unimatrix
  server, expose data, or corrupt state. The blast radius is limited to the report
  generation step failing for any snapshot that happens to contain a multi-byte query
  near the truncation boundary. However, a fix is recommended to harden the tooling.

---

### Finding 2: Query and scenario_id strings from eval result JSON are interpolated directly into Markdown without escaping

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/eval/report/render.rs:309–317`, `:338–346`
  (new in nan-008); also pre-existing at lines 113-114
- **Description**: Query text and scenario IDs (originating from the `query_log` table
  and JSON result files) are written directly into Markdown table cells via `format!`
  without any pipe (`|`) or backtick escaping. A query containing a literal `|`
  character would break the Markdown table structure. A query or scenario_id
  containing Markdown syntax (e.g., `**bold**`, HTML tags) would render as styled
  content in any system that renders the report. This is not an injection risk in the
  traditional sense because the output is a Markdown file read by developers, not a
  web-facing endpoint — but it can corrupt the report's table structure.
- **Recommendation**: Escape pipe characters in string values inserted into Markdown
  table cells: `value.replace('|', "\\|")`. This is a cosmetic fix; no security gate
  is triggered.
- **Blocking**: No. Pre-existing pattern; nan-008 extends it to two new table rows.

---

### Finding 3: ICD metric iterates over HashMap values — determinism depends on hash randomization

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/eval/runner/metrics.rs:1152–1161` (new in nan-008)
- **Description**: `compute_icd` accumulates Shannon entropy over a `HashMap<&str, usize>`.
  Since entropy is computed by summing `p * ln(p)` over all categories, and
  floating-point addition is not commutative, the iteration order of `HashMap`
  (which is randomized per-process in Rust's default hasher) means ICD values may
  differ slightly across runs on the same input due to floating-point accumulation
  order. For the scenario counts involved (k=5 entries), the epsilon is negligible
  (< 1e-15), and two determinism tests in `tests_metrics.rs` confirm reproducibility
  within a single process run. This is noted for completeness, not as a defect.
- **Recommendation**: No action required for the current use case. If exact
  byte-for-byte reproducibility across different process invocations is ever required,
  switch to `BTreeMap` or sort the count pairs before iterating.
- **Blocking**: No.

---

## Blast Radius Assessment

The eval harness is a local development CLI tool (`eval run`, `eval report`). It
opens snapshot databases read-only and writes only to `--out` paths specified by the
operator. It shares no code paths with the MCP server's request-handling layer,
database write paths, or any authenticated endpoint.

Worst case if this change has a subtle bug:
- `eval report` panics on a multi-byte query boundary (Finding 1) — operator retries
  or uses a snapshot with ASCII-only queries.
- ICD produces a slightly wrong mean due to FP accumulation order — affects only the
  eval report artifact used for development decisions.
- CC@k silently returns 0.0 for a profile whose TOML omits `[knowledge]` — mitigated
  by the `tracing::warn!` guard (ADR-004) and confirmed by tests.

None of these paths can corrupt the live database, expose secrets, elevate privileges,
or affect the running MCP server.

## Regression Risk

**Low.** The changes are purely additive:

- No existing function signatures visible externally are changed. `run_single_profile`
  gains a new parameter (`configured_categories`) but it is `pub(super)` — internal
  to the `runner` module.
- `render_report` gains a new `cc_at_k_rows` parameter but is also `pub(super)` —
  internal to the `report` module.
- All new fields on serialized types use `#[serde(default)]` in the report-side copy,
  ensuring pre-nan-008 result JSON files deserialize successfully.
- The round-trip test and section-order test (new in this PR) guard against the two
  highest-likelihood regressions (R-01 dual type copy divergence and R-02 section
  order).
- Existing tests that construct `ScoredEntry`, `ProfileResult`, and `ComparisonMetrics`
  literals required mechanical updates to add the new fields; all were updated in this
  PR.

The one area of pre-existing regression risk now made more visible: the byte-slice
truncation pattern (Finding 1) was already present before nan-008 for `title` strings.
nan-008 extends it to `query` strings. In practice both are sourced from the same
database and are expected to contain only ASCII search terms in the current dataset.

## OWASP Evaluation

| Check | Assessment |
|-------|-----------|
| Injection (SQL, command, path traversal) | No SQL, no shell commands. File paths come from operator CLI flags, not user input. No path traversal introduced. |
| Broken access control | No access control changes. Eval harness operates on local files with operator-controlled paths. |
| Security misconfiguration | No configuration changes. No new defaults that weaken security. |
| Deserialization of untrusted data | Result JSON is read from files written by the same binary in a prior step. `serde(default)` ensures missing fields default gracefully. No untrusted remote deserialization. |
| Input validation | New string fields (`category`, `query`) are not sanitized before insertion into Markdown table output, which is acceptable for a local developer tool. No new network-facing inputs. |
| Vulnerable components | No new dependencies introduced (confirmed: `Cargo.toml` diff is empty). |
| Hardcoded secrets | None found. |
| Data integrity | Baseline log (`log.jsonl`) is append-only. New entry format documented and validated by the delivery agent. |

## PR Comments

- Posted 1 comment on PR #404 with findings summary.
- Blocking findings: No.

## Knowledge Stewardship

- Stored: nothing novel to store -- the byte-slice truncation on `String` values is a
  well-known Rust footgun but is scoped to a pre-existing pattern in this codebase
  that predates nan-008. It does not meet the threshold of a recurring cross-feature
  anti-pattern specific to this project's MCP tool or security boundary conventions.
