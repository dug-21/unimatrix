# Security Review: nan-010-security-reviewer

## Risk Level: low

## Summary

nan-010 adds a distribution gate feature to the eval harness, introducing a new
`distribution_change` profile flag, a `profile-meta.json` sidecar file, and a
new report rendering path. The change is an internal developer tool with no
network-exposed attack surface. The trust model is fully author-controlled
(profile TOMLs + eval output directories). All new inputs are validated at parse
time. No hardcoded secrets, no new dependencies, no privilege escalation vectors.
Two low-severity findings noted; neither is blocking.

---

## Findings

### Finding 1 — Silent HTML comment fallback in Section 5 dispatch

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/eval/report/render.rs`, Section 5
  dispatch block (line ~203)
- **Description**: When a profile declares `distribution_change = true` in the
  sidecar but the `distribution_targets` field is `None` in the loaded
  `ProfileMetaEntry`, the render path falls through to an HTML comment:
  `<!-- WARN: distribution gate targets missing for profile '{profile_name}' -->`.
  This is a silent degradation: the report emits neither a Distribution Gate nor a
  Zero-Regression Check for that profile. The operator sees a Markdown comment in
  the rendered output that is invisible in most Markdown renderers.

  The `profile_name` interpolated into the comment comes from `AggregateStats`
  (derived from the scenario result JSON), which is author-controlled data — not
  externally injected. Markdown comment injection is not a meaningful attack
  vector in this context.

  The silent degradation is primarily a correctness/observability concern: a
  misconfigured sidecar (e.g., manually edited to set `distribution_change: true`
  but omit `distribution_targets`) would silently produce a report with a blank
  Section 5 block.

- **Recommendation**: This path is structurally unreachable under normal operation
  because `parse_profile_toml` enforces the invariant that `distribution_change =
  true` implies `distribution_targets = Some(...)`, and `write_profile_meta`
  faithfully transcribes that invariant to the sidecar. The fallback would only
  trigger if someone manually edits `profile-meta.json` to set
  `distribution_change: true` with `distribution_targets: null`. No action
  required for the current scope; consider adding a note in the code comment
  explaining why this branch is unreachable in practice.
- **Blocking**: no

---

### Finding 2 — Baseline identity check is name-only (case-insensitive string match)

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/eval/profile/validation.rs`, line 95
- **Description**: The guard preventing a baseline profile from declaring
  `distribution_change = true` uses `name.eq_ignore_ascii_case("baseline")`. This
  means a profile named "BASELINE", "Baseline", or "baseline" is blocked, but a
  profile named "baseline-v2" or "baseline_2" is not. By design, the baseline
  profile identity is established by convention (name equals "baseline"), not by a
  structural marker in the TOML. A developer who creates a profile named
  "base-profile" and intends it as a baseline could declare `distribution_change =
  true` without triggering the guard.

  This is not a security risk — the operator is the author of the profile TOMLs.
  It is a correctness edge case. The architecture document (OQ-03) says the guard
  applies to profiles identified as baseline — the name convention is the intended
  identification mechanism.

- **Recommendation**: No change required. The existing behavior matches the stated
  design intent. Document the convention limitation in eval-harness.md if desired
  (not required for this PR).
- **Blocking**: no

---

## OWASP Concern Checklist

| Check | Assessment |
|-------|-----------|
| Injection (command, path, SQL) | No shell execution of TOML values. Profile names interpolated into Markdown output are author-controlled. No SQL. Path operations derive from `--results` CLI arg, already validated upstream. No new injection surface. |
| Path traversal | `load_profile_meta` joins `dir.join("profile-meta.json")` where `dir` is the `--results` path provided at CLI invocation. No user-supplied filename components. No traversal risk. |
| Broken access control | Eval harness is a local developer tool. No auth, no multi-tenant surface. Trust model unchanged. |
| Security misconfiguration | No new config with security implications. `distribution_change` is additive and opt-in. |
| Deserialization | `serde_json` deserializes `ProfileMetaFile`. Malformed JSON aborts with error (not panic, not silent fallback). Correct. |
| Input validation | All three `DistributionTargets` fields validated at parse time as `f64` via `toml`'s `as_float()`. NaN/Infinity cannot be produced by TOML parsers. No range validation (by design — out-of-range values are user responsibility per RISK-TEST-STRATEGY edge cases). Correct for this tool's threat model. |
| Data integrity | Atomic write (tmp+rename) prevents partial sidecar. Corrupt sidecar aborts report (not silent fallback). Both properties verified by tests. |
| Secrets | No hardcoded secrets, API keys, or credentials anywhere in the diff. |
| Vulnerable components | No new dependencies added to Cargo.toml or Cargo.lock. |

---

## Blast Radius Assessment

The worst case scenario for a subtle bug in this change is: `eval report` silently
renders a Zero-Regression Check when a Distribution Gate was expected (or vice
versa), causing the wrong gate to be applied to a distribution-changing feature.
This could lead to a distribution-changing PR being falsely blocked (false
negative) or falsely approved (false positive).

The blast radius is bounded to the eval reporting subsystem only. No runtime
data is written, no production database is modified, no MCP tools are affected,
no user data is at risk. The eval harness is a local developer tool that writes
Markdown reports to a local directory. A silent wrong-gate scenario would be
caught by the operator reading the Section 5 header in the report.

The `load_profile_meta` abort path (corrupt sidecar) is the most impactful
failure mode: it causes `eval report` to exit non-zero and produce no report.
This is explicitly the correct behavior per the design and is tested by
`test_distribution_gate_corrupt_sidecar_aborts`.

---

## Regression Risk

**Low.** The key invariant is that `distribution_change = false` (the default)
leaves all existing behavior unchanged. This is tested by `test_parse_no_distribution_change_flag`,
`test_report_without_profile_meta_json`, and the existing suite of profile and
report tests (which continue to pass, as the new fields default to false/None).

The pre-split of `aggregate.rs → aggregate/mod.rs` and the extraction of
`render_zero_regression.rs` carry cosmetic regression risk (import path changes,
module path changes) that the Rust compiler catches at build time. No logic was
changed in those pre-splits.

The Section 5 dispatch refactor replaces a single-path render with a per-profile
dispatch loop. The single-profile zero-regression path is covered by
`test_report_without_profile_meta_json` and existing render tests. The
backward-compat (absent sidecar) path is explicitly tested. The per-profile
render_zero_regression_block function filters by `profile_name`, which is a
behavioral change from the prior implementation that rendered all regressions
in a single block — this is intentional per ADR-005 and covered by tests.

One latent regression risk: `render.rs` is at exactly 500 lines after this
change. Any future change that adds a single line of non-whitespace content to
`render.rs` will breach the 500-line constraint. This is not a security concern
but is flagged as a maintenance hazard.

---

## Dependency Safety

No new dependencies introduced. `serde_json` (already a dependency) is used for
sidecar serialization. The `tempfile` crate (already a dev-dependency) is used
in tests. No new Cargo.toml or Cargo.lock changes in the diff.

---

## Secrets Check

No hardcoded secrets, API keys, tokens, or credentials found anywhere in the diff.

---

## Required Test Name Verification

All 20 non-negotiable test names from RISK-TEST-STRATEGY were verified:

| Test Name | Location | Found |
|-----------|----------|-------|
| `test_parse_distribution_change_profile_valid` | `eval/profile/tests.rs` | yes |
| `test_parse_distribution_change_missing_targets` | `eval/profile/tests.rs` | yes |
| `test_parse_distribution_change_missing_cc_at_k` | `eval/profile/tests.rs` | yes |
| `test_parse_distribution_change_missing_icd` | `eval/profile/tests.rs` | yes |
| `test_parse_distribution_change_missing_mrr_floor` | `eval/profile/tests.rs` | yes |
| `test_parse_no_distribution_change_flag` | `eval/profile/tests.rs` | yes |
| `test_write_profile_meta_schema` | `eval/report/tests_distribution_gate.rs` | yes |
| `test_distribution_gate_section_header` | `eval/report/render_distribution_gate.rs` | yes |
| `test_distribution_gate_table_content` | `eval/report/render_distribution_gate.rs` | yes |
| `test_distribution_gate_pass_condition` | `eval/report/render_distribution_gate.rs` | yes |
| `test_distribution_gate_mrr_floor_veto` | `eval/report/render_distribution_gate.rs` | yes |
| `test_distribution_gate_distinct_failure_modes` | `eval/report/render_distribution_gate.rs` | yes |
| `test_report_without_profile_meta_json` | `eval/report/tests_distribution_gate.rs` | yes |
| `test_check_distribution_targets_all_pass` | `eval/report/tests_distribution_gate.rs` | yes |
| `test_check_distribution_targets_cc_at_k_fail` | `eval/report/tests_distribution_gate.rs` | yes |
| `test_check_distribution_targets_icd_fail` | `eval/report/tests_distribution_gate.rs` | yes |
| `test_check_distribution_targets_mrr_floor_fail` | `eval/report/tests_distribution_gate.rs` | yes |
| `test_distribution_gate_baseline_rejected` | `eval/report/tests_distribution_gate.rs` | yes |
| `test_distribution_gate_corrupt_sidecar_aborts` | `eval/report/tests_distribution_gate.rs` | yes |
| `test_distribution_gate_exit_code_zero` | `eval/report/tests_distribution_gate.rs` | yes |

All 20 required tests present.

---

## PR Comments

- Posted 1 comment on PR #417
- Blocking findings: no

---

## Self-Check

- [x] Full git diff was read (not just a summary)
- [x] Root cause analysis not applicable (feature, not bugfix) — ARCHITECTURE.md and RISK-TEST-STRATEGY.md read from disk
- [x] Affected source files read in full: validation.rs, types.rs, profile_meta.rs, distribution.rs, render_distribution_gate.rs, render_zero_regression.rs, render.rs (Section 5), report/mod.rs (load_profile_meta), tests_distribution_gate.rs
- [x] OWASP concerns evaluated for each changed file
- [x] Blast radius assessed — worst case scenario named
- [x] Input validation checked at system boundaries
- [x] No hardcoded secrets in the diff
- [x] Findings posted as PR comments via gh CLI
- [x] Risk level accurately reflects findings (not artificially low)
- [x] Report written to the correct agent report path

---

## Knowledge Stewardship

Nothing novel to store — the HTML-comment silent-fallback pattern is specific to
this PR's edge case and is structurally unreachable under correct operation.
The general principle (avoid silent fallbacks for misconfigured state) is already
stored in Unimatrix from the ADR-004/R-07 design work. No cross-feature
generalizable anti-pattern identified.
