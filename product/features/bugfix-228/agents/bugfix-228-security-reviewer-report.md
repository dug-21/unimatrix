# Security Review: bugfix-228-security-reviewer

## Risk Level: low

## Summary

The change is minimal, well-scoped, and intentional. It adds a compile-time const `PERMISSIVE_AUTO_ENROLL` that grants Write capability to auto-enrolled (unknown) agents. Admin capability remains gated behind explicit enrollment. No new inputs, no new dependencies, no injection surfaces. The primary concern is the deliberate weakening of the access control model, which is acknowledged and documented as a temporary measure.

## Findings

### Finding 1: Intentional Access Control Relaxation
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/infra/registry.rs:27`
- **Description**: `PERMISSIVE_AUTO_ENROLL = true` grants Write to every unknown agent_id, including arbitrary strings from MCP tool parameters. Any caller can store, correct, and deprecate entries simply by providing any agent_id. This is a deliberate design choice per #228, not a bug. The issue correctly notes that agents were already bypassing this by falling back to "human", so the practical security posture is unchanged.
- **Recommendation**: When agent naming stabilizes, flip to `false`. Consider making this a runtime config (env var) rather than a compile-time const so the transition does not require a rebuild. Not blocking -- this is a known tradeoff documented in the issue.
- **Blocking**: no

### Finding 2: Admin Capability Correctly Excluded
- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/infra/registry.rs:190-194`
- **Description**: Verified that Admin is never included in the permissive path. Quarantine (`tools.rs:772`), enroll (`tools.rs:932`), and status-maintain (`tools.rs:1038`) all require Admin, which unknown agents do not receive. The security boundary for destructive operations is preserved.
- **Recommendation**: None.
- **Blocking**: no

### Finding 3: Test Coverage Correctly Updated
- **Severity**: informational
- **Location**: `product/test/infra-001/suites/test_security.py:121-152`
- **Description**: S-21, S-22, S-23 were flipped from `assert_tool_error` to `assert_tool_success`. S-24 (quarantine rejected for restricted agent) remains unchanged, confirming Admin-gated operations are still tested as denied. The test changes accurately reflect the new behavior. No negative security tests were removed without replacement.
- **Recommendation**: None.
- **Blocking**: no

### Finding 4: No Runtime Configuration Path
- **Severity**: low
- **Location**: `crates/unimatrix-server/src/infra/registry.rs:27`
- **Description**: The const is compile-time only. Switching to strict mode requires recompilation. This is fine for development but limits operational flexibility. Not a security risk per se, but worth noting for the production hardening plan.
- **Recommendation**: Future work -- consider `std::env::var("UNIMATRIX_PERMISSIVE_ENROLL")` or a server config option.
- **Blocking**: no

## OWASP Assessment

| Check | Result |
|-------|--------|
| Input validation | No new inputs. `agent_id` still goes through `extract_agent_id` (trim, empty-to-anonymous). No change to validation. |
| Injection | No shell commands, SQL injection (parameterized queries throughout), or format string issues. |
| Access control | Intentionally relaxed for Write; Admin boundary intact. |
| Deserialization | `serialize_capabilities` uses the existing `serde_json` path with `Capability as u8`. No new deserialization of untrusted data. |
| Error handling | No new error paths. Existing `ServerError::Registry` propagation unchanged. |
| Secrets | No hardcoded secrets, keys, or credentials in the diff. |
| Dependencies | No new dependencies introduced. |

## Blast Radius Assessment

Worst case if this fix has a subtle bug: an unknown agent could write, correct, or deprecate knowledge entries. This is exactly the intended behavior of the change. The worst case is already the design goal.

If the const were accidentally left as `true` in a production deployment where strict access control was desired, any MCP client could modify knowledge entries. However, Admin operations (quarantine, enroll, status-maintain) would remain protected. Data corruption risk is low because all mutations are audited with agent_id attribution, and corrections/deprecations create new entries rather than destroying originals.

## Regression Risk

Low. The change modifies one code path (`resolve_or_enroll` for unknown agents) and preserves the `false` branch for future use. Explicitly enrolled agents are unaffected (they take the early-return path at line 184-186). Bootstrap agents (system, human, cortical-implant) are unaffected (they exist before `resolve_or_enroll` is called for them).

The only regression scenario: code that previously relied on restricted agents being denied Write would now see those operations succeed. The integration tests (S-21, S-22, S-23) were correctly updated to reflect this.

## PR Comments
- Posted 1 review comment on PR #231
- Blocking findings: no

## Knowledge Stewardship
- Stored: nothing novel to store -- standard access control relaxation pattern with compile-time toggle, no recurring anti-pattern observed
