# Security Review: bugfix-230-security-reviewer

## Risk Level: low

## Summary

The fix is a minimal, well-scoped change that adds the missing `agent_id` and `format` fields to `CycleParams` and wires `agent_id` into the handler's identity resolution call. No new attack surface is introduced. The change aligns `context_cycle` with the identity resolution pattern used by all other MCP tool handlers. No blocking findings.

## Findings

### Finding 1: format field accepted but unused in handler
- **Severity**: low (informational)
- **Location**: crates/unimatrix-server/src/mcp/tools.rs:265 (struct) and lines 1526-1580 (handler)
- **Description**: The `format: Option<String>` field was added to `CycleParams` for consistency with other tool param structs, but the `context_cycle` handler never reads or validates it. The response is always a hardcoded text acknowledgment. This means any value (including malformed strings) is silently accepted and ignored. This is not exploitable -- the value is never interpolated into output, never deserialized further, and never used in any operation. However, it creates a minor inconsistency: other handlers parse format via `build_context()` which validates it, while this handler skips that step.
- **Recommendation**: Consider either (a) adding a `build_context` call like other handlers (would validate format and provide consistent ToolContext), or (b) documenting that format is intentionally ignored for this acknowledgment-only tool. Not blocking -- this is a consistency observation, not a vulnerability.
- **Blocking**: no

### Finding 2: No session_id field on CycleParams
- **Severity**: low (informational)
- **Location**: crates/unimatrix-server/src/mcp/tools.rs:256-267
- **Description**: Other param structs (SearchParams, LookupParams, GetParams, BriefingParams) include `session_id: Option<String>` for usage tracking. CycleParams omits it. This was pre-existing (not introduced by this fix), and the handler's audit log uses an empty session_id (`String::new()`). Since the fix's scope was limited to adding `agent_id` and `format`, omitting `session_id` is acceptable -- but it means cycle events in the audit log cannot be correlated to specific MCP sessions.
- **Recommendation**: Track as a follow-up enhancement if session correlation for cycle events becomes valuable.
- **Blocking**: no

### Finding 3: xfail markers weaken security test coverage
- **Severity**: low
- **Location**: product/test/infra-001/suites/test_tools.py:86, 376, 452
- **Description**: Three security-relevant tests (Write rejection for restricted agents on store, correct, and deprecate operations) are now xfailed due to PERMISSIVE_AUTO_ENROLL granting Write to unknown agents (GH#233). These tests previously validated that restricted agents could not perform write operations. The xfail markers are correctly attributed to bugfix-228, not this fix, and a tracking issue (GH#233) exists. However, until GH#233 is resolved, there is reduced automated coverage for access control enforcement on write operations.
- **Recommendation**: Prioritize GH#233 resolution to restore access control test coverage.
- **Blocking**: no

## OWASP Assessment

| Check | Result |
|-------|--------|
| Input validation | PASS -- `agent_id` passes through `extract_agent_id()` (trims whitespace, defaults empty to "anonymous") and then `resolve_or_enroll()` in the registry. `format` is unused. Cycle-specific params (type, topic, keywords) validated by `validate_cycle_params()` which sanitizes control chars and validates structure. No change to validation logic. |
| Injection | PASS -- `agent_id` is used as a registry key and in audit log detail strings via `format!()`. No shell execution, SQL, or path traversal. The `format!()` usage in audit detail is safe (Rust's format macro does not evaluate expressions). |
| Access control | PASS -- The fix upgrades access control: previously hardcoded `&None` always resolved as anonymous. Now callers can pass their actual identity, enabling proper capability checking against the registry. The handler correctly requires `Capability::Write`. |
| Deserialization | PASS -- `CycleParams` uses serde derive with `Option<String>` fields. No custom deserializers. serde rejects type mismatches. Extra unknown JSON fields are silently ignored (serde default behavior with `#[derive(Deserialize)]`). |
| Error handling | PASS -- Errors from `resolve_agent` and `require_cap` are propagated as `rmcp::ErrorData`. Validation errors return structured tool error responses. No panics in the handler path. |
| Secrets | PASS -- No hardcoded secrets, tokens, API keys, or credentials in the diff. |
| Dependencies | PASS -- No new dependencies introduced. Cargo.toml and Cargo.lock are unchanged. |

## Blast Radius Assessment

The worst case for this fix is minimal. The change is to a single tool handler (`context_cycle`) that:

1. Resolves identity (now correctly using the caller-provided agent_id instead of always None)
2. Checks Write capability
3. Validates cycle parameters (type, topic, keywords -- unchanged)
4. Returns a text acknowledgment
5. Fires-and-forgets an audit event

If the fix had a subtle bug (e.g., if `params.agent_id` somehow contained unexpected data), the failure modes are:
- **Best case**: Agent resolution fails, returning an error to the caller (safe failure)
- **Middle case**: Agent resolves as anonymous (same behavior as before the fix)
- **Worst case**: An unrecognized agent_id auto-enrolls as Restricted (with PERMISSIVE_AUTO_ENROLL granting Write) and the cycle proceeds -- this is the current behavior for all other tools

The `context_cycle` handler performs no data mutations beyond the audit log. It does not write to ENTRIES, VECTOR_MAP, or any data tables. The response is a static acknowledgment string. Even if identity resolution produces unexpected results, the handler cannot corrupt data.

## Regression Risk

Low. The change is additive (new struct fields with `Option` type, default `None` for backward compatibility). Existing callers that omit `agent_id` will continue to resolve as anonymous, exactly as before. The deserialization tests confirm backward compatibility (test_cycle_params_agent_id_absent_is_none). The Python client wrapper is a new method, not a modification of an existing one.

The only regression risk is if serde's handling of the new `Option<String>` fields somehow conflicts with existing JSON payloads -- but serde's `Option` deserialization for missing fields is well-established and tested.

## PR Comments
- Posted 1 comment on PR #234
- Blocking findings: no

## Knowledge Stewardship
- Stored: nothing novel to store -- the fix follows the established identity resolution pattern used by all other MCP tool handlers. No new anti-patterns, no recurring vulnerability types observed. The unused-format observation is specific to this handler's minimal nature, not a generalizable concern.
