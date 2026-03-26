# Security Review: nan-009-security-reviewer

## Risk Level: low

## Summary

nan-009 is a pure measurement instrumentation feature. It threads a nullable `phase` string
through the eval pipeline — from SQL extraction to JSONL scenario files to per-scenario
result JSON to Markdown report rendering. No new network surfaces, no new authentication,
no new deserialization of externally sourced data beyond what was already present. The
architectural design explicitly isolates phase as read-only metadata that never reaches
retrieval logic. One low-severity finding (free-form string rendering unsanitized into
Markdown table cells) is acknowledged and accepted in the project's own RISK-TEST-STRATEGY.

---

## Findings

### Finding 1: Phase label rendered verbatim into Markdown table cells
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/eval/report/render_phase.rs:351` and `render.rs:131`
- **Description**: `stat.phase_label` (from `query_log.phase`) and `phase_label` (from
  `result.phase`) are written directly into Markdown output without sanitization. A value
  containing `|` characters (e.g., `"x | injected"`) would break Markdown table structure.
  A value containing `\n` or `##` could inject bogus section headings.
- **Recommendation**: The risk is bounded because `query_log.phase` is populated only by
  `context_cycle` calls from controlled protocol definitions — it is not directly user-
  supplied input. The RISK-TEST-STRATEGY (SEC-01) explicitly documents and accepts this
  risk. No blocking action required. As a low-cost future hardening, the renderer could
  replace `|` with a Unicode vertical bar (`\u{2502}`) and strip newlines before
  interpolation. Not required for this feature.
- **Blocking**: no

### Finding 2: SQL dynamic string construction — source and limit clauses
- **Severity**: low (informational; pre-existing; not introduced by this diff)
- **Location**: `crates/unimatrix-server/src/eval/scenarios/output.rs:97-113`
- **Description**: The SQL query uses string interpolation for `source_clause` (from
  `ScenarioSource::to_sql_filter()` which returns only static literals `"mcp"` or `"uds"`)
  and `limit_clause` (from a `usize` integer that cannot contain SQL metacharacters). The
  `phase` column added in this diff is in the SELECT list only — it is never used as a
  filter. No injection surface exists in the new code. The pre-existing format!() SQL
  pattern is not a new risk introduced by nan-009.
- **Recommendation**: No action needed. The code comment at line 93 correctly documents why
  interpolation is safe here.
- **Blocking**: no

### Finding 3: No new dependencies introduced
- **Severity**: informational
- **Location**: workspace Cargo.toml files (unchanged in diff)
- **Description**: The diff introduces zero new external crate dependencies. All added code
  uses existing primitives from std (`HashMap`, `String`, `Vec`), existing serde
  annotations, and existing sqlx row access patterns. No `cargo audit` risk introduced.
- **Recommendation**: None.
- **Blocking**: no

### Finding 4: Phase value never forwarded to ServiceSearchParams (R-06 verified)
- **Severity**: informational — risk present in RISK-TEST-STRATEGY, verified clean
- **Location**: `crates/unimatrix-server/src/eval/runner/replay.rs:80`
- **Description**: The RISK-TEST-STRATEGY identified R-06 (phase injected into retrieval
  during replay) as a High severity, Low likelihood risk. Verification: `replay.rs` line 80
  assigns `phase: record.context.phase.clone()` directly onto `ScenarioResult`. The
  `run_single_profile` function constructs `ServiceSearchParams` at lines 96-108 and
  `AuditContext` at lines 110-121 — neither references `phase` in any form. The separation
  between the `replay_scenario` outer function (which sets phase on result) and
  `run_single_profile` (which constructs search params) is clean and correctly isolated.
- **Recommendation**: None. R-06 is resolved.
- **Blocking**: no

### Finding 5: Serde annotation placement (R-05 verified)
- **Severity**: informational — risk present in RISK-TEST-STRATEGY, verified clean
- **Location**:
  - `crates/unimatrix-server/src/eval/scenarios/types.rs:73-74`
  - `crates/unimatrix-server/src/eval/runner/output.rs:86-87`
  - `crates/unimatrix-server/src/eval/report/mod.rs:133-134`
- **Description**: The RISK-TEST-STRATEGY identified R-05 (skip_serializing_if on wrong
  struct copy) as Med severity. Verification: `ScenarioContext.phase` in `types.rs` has
  `#[serde(default, skip_serializing_if = "Option::is_none")]` — correct (suppresses null
  in JSONL). `ScenarioResult.phase` in `runner/output.rs` has `#[serde(default)]` only —
  correct (always emits `"phase":null`). `ScenarioResult.phase` in `report/mod.rs` has
  `#[serde(default)]` only — correct (read-only consumer). All three are correctly annotated.
  Tests `test_scenario_result_phase_null_serialized_as_null` and
  `test_scenario_context_phase_null_absent_from_jsonl` guard these invariants.
- **Recommendation**: None. R-05 is resolved.
- **Blocking**: no

### Finding 6: Deserialization of result JSON files
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/eval/report/mod.rs` (run_report)
- **Description**: `eval report` reads per-scenario `.json` files from a directory path
  supplied by the operator. These files are deserialized via `serde_json`. The `phase`
  field is `Option<String>` — a scalar. No recursive structure, no gadget chain, no
  arbitrary code execution risk from serde_json deserialization of a string-typed field.
  The RISK-TEST-STRATEGY (SEC-03) explicitly assessed this as low risk with correct
  reasoning. The operator controls the results directory; this is not an untrusted-input
  surface in a networked context.
- **Recommendation**: None required. The assessment in SEC-03 is correct.
- **Blocking**: no

---

## Blast Radius Assessment

**Worst case scenario if this fix has a subtle bug**: The phase aggregation or render
logic produces an incorrect section 6 in eval reports. This is a reporting artifact
consumed by human reviewers — it does not affect the knowledge store, retrieval pipeline,
MCP server availability, or any stored data. A bug in section 6 (e.g., wrong means, wrong
sort order) would cause misleading metric interpretation but no data corruption or service
disruption. Section 7 (Distribution Analysis) and sections 1-5 are computed independently
and unaffected by a phase-aggregation bug.

**Failure mode safety**: All failure modes produce readable text output or omit the section
entirely. No panic paths in production code — the aggregation guards against empty slices
and returns empty vecs. No `unwrap()` in non-test code (confirmed by reading aggregate.rs,
render_phase.rs, render.rs, extract.rs, output.rs, replay.rs).

---

## Regression Risk

**Section renumbering risk (R-02)**: Distribution Analysis shifts from section 6 to section
7. Any external tooling or documentation that references `## 6. Distribution Analysis` by
exact string will break. This is an intentional contract change documented in the PR. The
tests assert `!content.contains("## 6. Distribution Analysis")` to guard against the old
heading surviving.

**Backward-compatibility for existing scenario files**: The `#[serde(default)]` annotation
on the report-side `ScenarioResult.phase` ensures legacy result files without a `phase` key
deserialize cleanly with `phase = None`. Existing eval runs are unaffected.

**Existing tests**: Tests that previously asserted "five sections" or "six sections" have
been updated. The removed test bodies (test_report_contains_all_five_sections renamed to
test_report_contains_all_seven_sections; test_report_contains_all_six_sections updated
to expect section 7) are the primary regression risk site — the review of the test diff
confirms these were correctly updated with both the new assertions and the negative
assertion for the old heading string.

---

## Dependency Safety

No new dependencies introduced. Cargo.toml files are unchanged in this diff. No CVE
exposure from this change.

---

## Secrets Check

No hardcoded secrets, API keys, tokens, or credentials in any changed file. The diff
contains only Rust source code, test code, Markdown documentation, and product
artifact files.

---

## PR Comments

- Posted 1 comment on PR #411 summarizing findings.
- Blocking findings: no

---

## Knowledge Stewardship

Nothing novel to store — SEC-01 (unsanitized Markdown rendering of free-form strings from
a trusted internal source) is feature-specific and already documented in the project's own
RISK-TEST-STRATEGY. The pattern of phase-as-passthrough-only with explicit R-06 guard is
adequately covered by the existing codebase review pattern. No recurring anti-pattern that
warrants a generalizable lesson entry.
