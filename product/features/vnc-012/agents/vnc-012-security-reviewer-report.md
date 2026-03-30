# Security Review: vnc-012-security-reviewer

## Risk Level: low

## Summary

PR #450 adds three `pub(crate)` serde deserializer helpers (`mcp/serde_util.rs`) and applies
them via `#[serde(deserialize_with)]` annotations to nine numeric fields across five MCP tool
parameter structs. No handler logic, validation layer, or trust-boundary enforcement is
altered. The change is additive: only struct-field annotations and a new submodule are
introduced. No new dependencies are added. No security findings require blocking the merge.

---

## Findings

### Finding 1: `v as u64` intermediate cast in `UsizeOrStringVisitor::visit_i64`
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/serde_util.rs:124`
- **Description**: After the `v < 0` guard on line 120, the code executes
  `usize::try_from(v as u64)`. The `as u64` cast on a non-negative `i64` is safe and lossless
  (non-negative i64 values fit in u64). However, it is a naked `as` cast, which is generally
  flagged as a code smell in security-sensitive Rust code. The guard directly preceding it
  makes it correct, but a reader without context might question it.
- **Recommendation**: No code change required — the guard is present and the cast is safe.
  The comment on the same line ("convert via u64 to usize safely") is adequate justification.
  Non-blocking.
- **Blocking**: no

### Finding 2: Error messages include client-supplied input via `{v:?}` Debug format
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/serde_util.rs:37, 131, 136`
- **Description**: The three `format!()` calls in `visit_str` and `inner_deserialize_usize`
  include the client-supplied string `v` using the `{v:?}` Debug format. This means the raw
  (potentially adversarial) string is included in the serde error message returned to the MCP
  caller via rmcp's `ErrorData::invalid_params` path. In the current deployment model
  (stdio-transport MCP server, agent clients only), this is not a concern — the error message
  is returned directly to the calling agent that provided the value. There is no risk of
  information disclosure to a third party, and no injection surface (the string is in an error
  message, not a query or log sink).
- **Recommendation**: Acceptable as-is for the stdio transport model. If the server is later
  exposed over a network transport, consider whether full client-supplied strings should appear
  in error messages. Not blocking.
- **Blocking**: no

### Finding 3: Coercion widens acceptance surface for mutation tools
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs` — `DeprecateParams.id`,
  `QuarantineParams.id`, `CorrectParams.original_id`
- **Description**: The coercion now allows agents to pass string-encoded IDs to
  `context_deprecate`, `context_quarantine`, and `context_correct` — all write/mutation
  operations. Before this fix, a string ID caused an immediate serde error before any handler
  code ran. After the fix, a string `"3770"` reaches the same handler path as integer `3770`.
  This is the intended behavior. The downstream `validated_id()` call in each handler enforces
  non-negative range. There is no new path to bypass access control or entry ID validation.
  The trust model (caller agent identity checked separately via `agent_id`) is unchanged.
- **Recommendation**: No action required. The change is correct and the layered validation
  (`validated_id` post-coercion) remains intact.
- **Blocking**: no

### Finding 4: Non-numeric field coercion explicitly out of scope
- **Severity**: informational
- **Location**: Architecture doc, "Remaining failure surface (out of scope, SR-04)"
- **Description**: String-typed fields (`format`, `category`, `status`, `agent_id`, `topic`,
  `action`) are not coerced. Agents that stringify those values will still receive serde errors.
  The architecture explicitly documents this boundary. This is an accepted scope decision, not
  a gap — there is no injection or access-control risk from leaving string fields at their
  current strictness. GH #448 notes the follow-on scope.
- **Recommendation**: No action required in this PR. Track as known limitation in GH #448.
- **Blocking**: no

---

## OWASP Evaluation

| OWASP Concern | Verdict |
|---------------|---------|
| A03 Injection (SQL, command, path traversal) | Not applicable. Coerced integers flow to `validated_id` -> typed u64 store queries. No string interpolation into SQL or shell commands. |
| A01 Broken access control | Not applicable. No access control logic changed. Trust level and capability checks in handlers are unaffected. |
| A08 Software and data integrity failures | Not applicable. Deserialization is additive widening only (string -> same integer value). No deserialization of complex untrusted object graphs. |
| A04 Security misconfiguration | Not applicable. No new configuration paths. Module is `pub(crate)`, not public API. |
| A05 Vulnerable components | Not applicable. No new dependencies introduced. `serde` and `serde_json` already in `Cargo.toml`. |
| Input validation gaps | Checked. All three helpers reject non-numeric strings, floats, booleans, arrays, objects. Negative values rejected for usize variant. Overflow rejected at parse time. |
| Hardcoded secrets | None present. Confirmed by inspection of full diff. |

---

## Blast Radius Assessment

Worst case if a helper has a subtle bug:

1. **Silent coercion to 0**: If `visit_str` returned `Ok(0)` on parse failure instead of `Err`,
   every call with a non-numeric string ID (e.g., `"abc"`) would look up or mutate entry ID 0.
   This is the highest-consequence failure mode. It is guarded by 9 explicit rejection tests in
   `serde_util.rs` and 9 more in `vnc012_coercion_tests` in `tools.rs`. The test suite would
   catch this before the binary runs.

2. **None-for-absent regression**: If `#[serde(default)]` were absent from an optional field,
   callers omitting that field would get a serde error. This would be a regression on the most
   common usage pattern (omitting optional params). Covered by 5 absent-field tests.

3. **Float truncation to integer**: If `visit_f64` were omitted, `serde_json` would call
   `visit_i64` with truncated floats. The explicit `visit_f64 -> Err` implementations prevent
   this. Covered by float-number rejection tests.

The failure mode for cases 1 and 3 is a wrong entry being accessed or mutated — bounded to the
entries accessible to the calling agent in the same session. No cross-session data corruption.
No privilege escalation. No process crash. All failure modes produce bounded, reversible errors.

---

## Regression Risk

**Low.** The change is additive only:
- No existing validation functions (`validated_id`, `validated_k`, `validated_limit`,
  `validated_max_tokens`) were modified.
- No handler logic was changed.
- No lines were deleted from production code.
- All nine affected fields previously accepted only JSON integers; they now also accept string
  representations of the same integers. Callers that already send integers see identical behavior.
- The schema type (`"type": "integer"`) is preserved via `#[schemars(with = "T")]`, verified by
  the AC-10 schema snapshot test.
- The existing `test_retrospective_params_evidence_limit` test (which passes an integer) is
  unaffected by the annotation change.

---

## Dependency Safety

No new crate-level dependencies. `serde` (with `derive`) and `serde_json` were already present
in `crates/unimatrix-server/Cargo.toml`. No `Cargo.toml` files were modified. No CVE exposure
introduced.

---

## Secrets Check

No hardcoded secrets, API keys, tokens, or credentials are present anywhere in the diff.
Confirmed by inspection of all four changed source files.

---

## PR Comments
- Posted 1 comment on PR #450 (findings summary, no blocking items)
- Blocking findings: no

---

## Knowledge Stewardship
- nothing novel to store -- the security properties of this specific pattern (additive serde
  coercion with downstream validated_id, explicit float/bool/null rejection) are feature-
  specific and do not yet represent a recurring anti-pattern across 2+ PRs. The lesson about
  needing absent-field tests for optional serde fields (R-01/R-03) is already captured as
  entry #885 and was referenced during RISK-TEST-STRATEGY.md authorship.
