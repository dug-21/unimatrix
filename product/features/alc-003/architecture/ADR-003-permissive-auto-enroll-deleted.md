## ADR-003: `PERMISSIVE_AUTO_ENROLL` Deleted Entirely — No Escape Hatch

### Context

`PERMISSIVE_AUTO_ENROLL = true` is a compile-time constant in `infra/registry.rs` that
causes unknown agents to be enrolled with `[Read, Write, Search]` on first call. It was
introduced to make development frictionless: new swarm agents don't need pre-enrollment.

Three disposal options were considered:

**Option A — Convert to an environment variable (env var escape hatch):**
Keep the behavior but move the toggle to `PERMISSIVE_AUTO_ENROLL=true` env var (default
`false` in production). Tests that need permissive behavior set the env var.

Problems: an env-var escape hatch means permissive enrollment can be re-enabled in
production by anyone who can set environment variables. The SCOPE.md explicitly states
"no env var, no escape hatch." The Proposed Approach in SCOPE.md initially described
this option — the SCOPE-RISK-ASSESSMENT.md (Assumptions section) flags it as
inconsistent with the stated goals and warns that an intermediate conversion step risks
committing a partially-done state.

**Option B — Convert to a test-only feature flag:**
Use `#[cfg(test)]` to conditionally enable permissive enrollment in tests.

Problems: feature flags that change security behavior between `cfg(test)` and production
builds are dangerous. A bug in the flag logic could silently enable permissive behavior
in release builds. Testing under different security posture than production is itself
a risk.

**Option C — Delete entirely; fix tests to not depend on it (chosen):**
Remove `PERMISSIVE_AUTO_ENROLL` from all code. Update tests that relied on it to either:
(a) explicitly enroll the agent under test before calling Write tools, or
(b) use the `make_server_with_session()` test helper that provides a pre-configured
session agent with `[Read, Write, Search]`.

### Decision

`PERMISSIVE_AUTO_ENROLL` is deleted from `infra/registry.rs`. The constant, all
references to it, and the `permissive` parameter to `resolve_or_enroll()` are removed.
The store-level `agent_resolve_or_enroll()` permissive parameter is also removed
(cleaning the dead parameter rather than leaving it in the store crate).

Unknown agents encountered via `resolve_or_enroll()` are always enrolled with
`[Read, Search]` (the existing non-permissive path). This is a test infrastructure
change — in production post-alc-003, `resolve_or_enroll()` is never called for per-call
`agent_id` values (ADR-002 removed that lookup). The only callers of `resolve_or_enroll()`
after alc-003 are the startup bootstrap path and any admin-level enrollment operations.

**Pre-flight measurement (SR-06):** Before writing alc-003 implementation code, the
implementer MUST run the full test suite with `PERMISSIVE_AUTO_ENROLL` forced to `false`
to enumerate the actual number of affected tests. See the ARCHITECTURE.md pre-flight
section for the exact procedure.

**Test fix pattern:**
- Tests asserting `[Read, Write, Search]` for unknown agents: rewrite to assert
  `[Read, Search]` (the new correct non-permissive default) or to explicitly enroll
  the agent before asserting Write
- Integration tests that implicitly got Write access via permissive enrollment: add
  explicit enrollment via `registry.enroll_agent()` in test setup, or use the session
  agent path via `make_server_with_session()`

### Consequences

**Easier:**
- The capability system is unconditionally enforced — no code path grants Write to an
  unknown caller
- Security audit is simpler: there is no toggle to check; the behavior is fixed
- Future features cannot accidentally re-enable permissive enrollment (the constant and
  parameter no longer exist)

**Harder:**
- Test infrastructure requires explicit enrollment for any test that needs Write access;
  this is one-time work that makes tests more honest about their preconditions
- The `unimatrix-store` crate API changes (permissive parameter removed from
  `agent_resolve_or_enroll`); any external code calling this method directly will need
  updating — check if `unimatrix-observe` or other crates call it
- The blast radius is unknown until the pre-flight measurement runs; this must be done
  before implementation begins
