## ADR-005: Pre-flight Blast Radius Measurement Before Any alc-003 Code

### Context

SR-06 (High severity, High likelihood) identifies that deleting `PERMISSIVE_AUTO_ENROLL`
has an unknown test blast radius. The SCOPE.md's "27 tests" figure conflates two
different uses of the word "permissive":

1. `SecurityGateway::new_permissive()` — a rate-limit bypass in `gateway.rs`, completely
   unrelated to agent enrollment. These tests are not affected.
2. Registry `permissive=true` — the enrollment mode that grants `[Read, Write, Search]`
   to unknown agents. These tests ARE affected.

The 185 infra integration tests were counted at the time of nxs-011 delivery. An unknown
subset of them rely on permissive enrollment implicitly (by calling Write-capability
tools without explicitly enrolling a test agent). The count is unmeasured.

Starting alc-003 implementation without measuring this blast radius risks:
- Writing alc-003 code that causes a cascade of test failures discovered only at the
  end of implementation
- A false sense that the change is small (based on the "27 tests" miscount)
- Sequencing problems: if test fixture changes are interleaved with behavioral changes,
  regression attribution becomes ambiguous

Two sequencing options were considered:

**Option A — Fix tests as they break during implementation:**
Write alc-003 code, run tests, fix failures encountered. This is the typical "fix as
you go" approach.

Problems: failures from test fixture issues obscure failures from behavioral bugs. The
implementer cannot distinguish "this test fails because of a test setup problem" from
"this test fails because the identity resolution logic is wrong." The two causes require
different fixes. Interleaving them increases the chance of committing a regression.

**Option B — Pre-flight measurement before any behavioral code (chosen):**
Before writing any alc-003 implementation code, perform a minimal change that surfaces
all test failures attributable to the `PERMISSIVE_AUTO_ENROLL` deletion. Fix all test
infrastructure first. Then write alc-003 behavioral code against a clean test baseline.

The pre-flight change is a single-line edit: `const PERMISSIVE_AUTO_ENROLL: bool = false`.
This change is NOT committed as part of alc-003 (it will be superseded by the full
deletion). Its purpose is measurement only.

### Decision

The alc-003 implementation MUST begin with a pre-flight measurement phase:

**Phase 0 — Measure (not committed):**
1. Edit `infra/registry.rs`: change `PERMISSIVE_AUTO_ENROLL` from `true` to `false`
2. Run `cargo test --workspace 2>&1 | grep -c FAILED` — record the count
3. Run `cargo test --workspace 2>&1 | grep FAILED` — enumerate each failing test
4. Categorize: registry-permissive tests (expected, ~2-5) vs. implicit integration test
   failures (the true unknown)
5. Revert the single-line change

**Phase 1 — Fix test infrastructure (committed separately from alc-003 behavioral code):**
Update all test fixtures identified in Phase 0 to either:
- Explicitly enroll the agent under test before calling Write tools
- Use the new `make_server_with_session()` helper that provides a pre-configured
  `SessionAgent` with `[Read, Write, Search]`

Commit this as a standalone test infrastructure PR with message:
`test: update fixtures for alc-003 capability enforcement (pre-flight)`

**Phase 2 — alc-003 behavioral implementation:**
With the test baseline clean, implement alc-003 code. Any new test failure is a
behavioral regression, not a fixture problem. Attribution is unambiguous.

The spec writer's AC-10 ("185 infra integration tests continue to pass") is verifiable
only if the pre-flight measurement and Phase 1 fix are complete before Phase 2 begins.

### Consequences

**Easier:**
- Test failures during behavioral implementation are unambiguously behavioral, not
  fixture-related
- The pre-flight measurement produces an exact count for the spec writer's blast-radius
  estimate, replacing the misleading "27 tests" figure
- Phase 1 test infrastructure changes can be reviewed independently of alc-003 behavior,
  reducing PR review complexity

**Harder:**
- Implementation requires two PRs or a carefully sequenced commit series rather than
  a single PR
- The pre-flight phase adds roughly half a day before implementation code begins
- The Phase 1 fixture changes touch many test files; the diff is large and mechanical,
  which can be distracting in review — the PR description must explain the pre-flight
  rationale clearly
