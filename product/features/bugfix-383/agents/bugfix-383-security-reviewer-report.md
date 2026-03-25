# Security Review: bugfix-383-security-reviewer

## Risk Level: low

## Summary

PR #392 is a pure identifier rename across the `unimatrix-observe` and `unimatrix-server` crates:
`PermissionRetriesRule` → `OrphanedCallsRule`, and the rule_name string `"permission_retries"` →
`"orphaned_calls"`. All changed lines are string literals, struct names, function names, comments,
and test identifiers. No computation logic, no trust boundaries, no data serialization format, and
no public API surface changed. The PR introduces no new dependencies. No security-relevant findings
were identified.

## Findings

### Finding 1: `remediation_for_rule` visibility widened from private to `pub(crate)`
- **Severity**: low
- **Location**: `crates/unimatrix-observe/src/extraction/recurring_friction.rs:107`
- **Description**: `remediation_for_rule` changed from an implicit private function to `pub(crate)`.
  The same change was applied to `recommendation_for` in `report.rs:63`. Both are exposed only
  within the `unimatrix-observe` crate (not across the public crate boundary), and the new contract
  test in `report.rs` imports them directly to assert coverage. The visibility change is the minimal
  necessary to enable the contract test — no wider API surface is exposed.
- **Recommendation**: No action required. `pub(crate)` confines the symbols to the crate.
- **Blocking**: no

### Finding 2: Fifteen new `remediation_for_rule` match arms for rules unrelated to the rename
- **Severity**: low
- **Location**: `crates/unimatrix-observe/src/extraction/recurring_friction.rs:322–386`
- **Description**: Fifteen string-literal match arms were added for rules that are not part of the
  rename (e.g., `lifespan`, `file_breadth`, `session_timeout`, `cold_restart`, etc.). These were
  latent defects — all 22 default rules previously fell through to the generic catch-all remediation
  string. The new contract test `test_all_default_rules_have_non_fallback_recommendation_and_remediation`
  enforces coverage and required these additions to pass. No logic is involved — every arm returns a
  `&'static str` literal. There is no injection risk; none of the strings incorporate external input.
  The gate report correctly classified this as WARN (not FAIL) and noted the arms are semantically
  coupled to the same module.
- **Recommendation**: No action required. The additions are safe and fix a pre-existing quality gap.
- **Blocking**: no

### Finding 3: `TODO(col-028)` deferred field rename in `metrics.rs`
- **Severity**: low
- **Location**: `crates/unimatrix-observe/src/metrics.rs:499`
- **Description**: A comment `// TODO(col-028): rename field to orphaned_call_events once metric
  migration is complete` documents a deferred rename of the `permission_friction_events` metric field.
  The field name is an internal struct field (not a public protocol string), and renaming it requires a
  SQLite schema migration tracked separately. This is accurate documentation of deferred work, not a
  code placeholder. There is no security implication; the field is read-only from external callers.
- **Recommendation**: No action required. The TODO references a tracked issue and is appropriately
  scoped as documentation.
- **Blocking**: no

## OWASP Evaluation

| Concern | Assessment |
|---------|------------|
| Injection (SQL, command, path traversal) | Not applicable. All changed strings are static literals matched against internal rule_name fields. No external input flows into any changed code path. |
| Broken access control | Not applicable. No access control logic was modified. The `pub(crate)` visibility changes are intra-crate only and do not cross trust boundaries. |
| Security misconfiguration | Not applicable. No configuration, default values, or server initialization changed. |
| Vulnerable components | Not applicable. No new dependencies were added. |
| Data integrity failures | Not applicable. The rename is applied atomically across all call sites; the contract test enforces that all 22 rules have matching arms. No silent fallthrough is possible post-fix. |
| Deserialization risks | Not applicable. No serialization formats changed. The `rule_name` string appears in `HotspotFinding` which is serialized in MCP responses, but the new name `"orphaned_calls"` is a valid, non-exploitable identifier. |
| Input validation | Not applicable. No new inputs from external sources are introduced. Existing source_domain guards (`r.source_domain == "claude-code"`) are preserved unchanged. |
| Secrets / credentials | None present. No hardcoded secrets, API keys, or tokens were introduced. |

## Blast Radius Assessment

Worst case if the rename has a subtle error: a `HotspotFinding` with `rule_name: "orphaned_calls"`
would not match the old `"permission_retries"` arm in `recommendation_for()` or
`remediation_for_rule()`, resulting in a `None` recommendation or the generic fallback remediation
text for that hotspot type. The effect is a degraded human-readable diagnostic message, not data
corruption, a security vulnerability, or a denial of service. The new contract test
`test_all_default_rules_have_non_fallback_recommendation_and_remediation` directly catches this
failure mode at compile-time (test failure before merge).

The MCP tool `context_retrospective` would produce a report with a different hotspot type string
(`"orphaned_calls"` instead of `"permission_retries"`). Clients parsing this field by string
comparison (e.g., retro skill files, agent documentation) would need to be updated — the PR
correctly updates both retro skill files. No breaking protocol changes affect external consumers
because the MCP schema does not enumerate valid `rule_name` values.

## Regression Risk

Low. The diff is a mechanical string rename with no logic change. All computation in `OrphanedCallsRule::detect` is identical to the pre-rename `PermissionRetriesRule::detect`. Test coverage is extensive:

- 10+ unit tests directly exercise `OrphanedCallsRule`
- Contract test asserts all 22 rules return non-fallback arms
- Integration test `test_retrospective_report_backward_compat_claude_code_fixture` validates
  end-to-end detection with the representative fixture
- `test_default_rules_names` in `detection/mod.rs` asserts the name string is `"orphaned_calls"`

The one potential regression gap is that any historical `HotspotFinding` records persisted with
`rule_name: "permission_retries"` in the Unimatrix knowledge store will no longer match the updated
`recommendation_for` and `remediation_for_rule` arms. These would fall through to generic text.
This is acceptable: historical lessons are read-only archives, and future observations will produce
findings with the correct new name. No data migration is needed.

## PR Comments

- Posted 1 approval comment on PR #392.
- Blocking findings: no.

## Knowledge Stewardship

- Stored: nothing novel to store — this rename pattern (struct + rule_name string + claim/recommendation/remediation text updated atomically) follows directly from prior lessons on detection rule naming. The new contract test is the generalizable takeaway, but it is already encoded in the codebase as a test that future rule authors must pass. No cross-feature anti-pattern generalizable beyond what is already tested.
