# Risk-Based Test Strategy: alc-003

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Pre-flight blast radius skipped — permissive test failures interleaved with behavioral failures, masking regressions | High | High | Critical |
| R-02 | `require_cap()` call sites not updated atomically — one or more of the 12 tool handlers still pass `agent_id`, causing compilation failure or stale capability checks | High | High | Critical |
| R-03 | Startup refusal absent in one transport path — `tokio_main_daemon` or `tokio_main_stdio` omits the env-var read, creating a hole where the unguarded path starts without identity | High | Med | High |
| R-04 | `SessionIdentitySource` enum is only a compile-time deference for W2-2 — the enum shape prevents OAuth coupling but `UnimatrixServer` startup wiring is duplicated across two `main.rs` branches, meaning W2-2 must touch both regardless | High | Med | High |
| R-05 | `enroll_session_agent()` bypasses `AgentRegistry` protected-agent guard — a bug in the startup path could enroll `"system"` or `"human"` as the session agent, overwriting their trust levels | High | Low | High |
| R-06 | Audit attribution silent corruption — per-call `agent_id` whitespace-only values collapse to session agent name without being logged, making attribution gaps invisible in the audit log | Med | Med | Medium |
| R-07 | Idempotency regression — repeated restarts upsert the session agent record on every start, but if the upsert logic changes trust level or capability set based on what already exists rather than always overwriting, capabilities drift silently | Med | Med | Medium |
| R-08 | Daemon-mode clone carries stale session identity — `UnimatrixServer` is cloned into each MCP session task; if `SessionAgent` is mutated after construction (e.g., via an interior mutability bug introduced in a future feature), all in-flight tasks see the mutation | Med | Low | Medium |
| R-09 | `PERMISSIVE_AUTO_ENROLL` deletion misses external crate callers — `unimatrix-store` permissive parameter is called from outside `unimatrix-server` (e.g., `unimatrix-observe`); removing it breaks compilation in crates not in the direct blast radius measurement | Med | Med | Medium |
| R-10 | Breaking change: operator upgrades without setting env var — server refuses to start with no graceful fallback; if the error message on stderr is not clear, the operator cannot determine the required action | Med | High | Medium |
| R-11 | `SESSION_AGENT_DEFAULT_CAPS` not isolated — capability constant inlined in startup logic or `main.rs` rather than exported at module boundary, making W0-3 wiring a surgery not a seam | Med | Med | Medium |
| R-12 | `ValidatedAgentId` newtype bypassed — downstream code constructs a raw `String` and passes it to `enroll_session_agent()` outside the `resolve()` path, bypassing validation | Low | Low | Low |
| R-13 | Test process environment pollution — tests that set `UNIMATRIX_SESSION_AGENT` in the process environment without unsetting it after, causing interference across parallel test cases | Med | Med | Medium |
| R-14 | ADR #1839 future implementer ignores `SessionIdentitySource` seam — a future delivery team adds `UNIMATRIX_CLIENT_TOKEN` by bypassing `SessionIdentitySource`, re-introducing per-call registry lookups | Low | Med | Low |

---

## Risk-to-Scenario Mapping

### R-01: Pre-flight blast radius skipped
**Severity**: High
**Likelihood**: High
**Impact**: Test failures from missing fixture setup are indistinguishable from behavioral regressions. A behavioral bug is committed undetected because the developer attributes the red test to fixture scaffolding.

**Test Scenarios**:
1. Before any alc-003 behavioral code: change `PERMISSIVE_AUTO_ENROLL` to `false`, run `cargo test --workspace`, record the exact FAILED count. Verify the count is documented in the PR before Phase 1 begins.
2. After Phase 1 (fixture updates only): run `cargo test --workspace` with `PERMISSIVE_AUTO_ENROLL=false` stub still in place. Verify zero test failures. This is the clean baseline.
3. After Phase 2 (behavioral code): verify the FAILED count remains zero and no previously-passing test has changed to failing.

**Coverage Requirement**: Pre-flight measurement must be the first commit in the implementation sequence. Phase 2 may not begin until Phase 1 passes clean.

---

### R-02: `require_cap()` call sites not updated atomically
**Severity**: High
**Likelihood**: High
**Impact**: A tool handler that still passes `agent_id` to `require_cap()` either fails to compile (best case) or silently ignores the argument if the parameter was made optional rather than removed (worst case, capability check still passes but on wrong identity logic). Historical: entry #317 (ToolContext pattern) documents this class of stale-caller-at-refactoring risk.

**Test Scenarios**:
1. Compile the workspace after removing the `agent_id` parameter from `require_cap()`. Any handler that still passes the argument produces a compile error. The test is the build itself.
2. Integration test: for each of the 12 tool handlers, call the tool with a non-empty `agent_id` and assert the operation succeeds or fails based solely on session capabilities — not on whether the `agent_id` string is enrolled.
3. Negative test: call a Write tool with `agent_id: "unenrolled-agent"` when the session has `[Read, Search]` only. Assert `CapabilityDenied` is returned. The `agent_id` value must have no effect on the outcome.

**Coverage Requirement**: All 12 tool handlers must have at least one call-time capability test that is decoupled from `agent_id` enrollment state. Enumerate the 12 tools explicitly in the test file.

---

### R-03: Startup refusal absent in one transport path
**Severity**: High
**Likelihood**: Medium
**Impact**: If `tokio_main_daemon()` reads the env var but `tokio_main_stdio()` does not (or vice versa), one deployment mode bypasses the identity requirement. This creates a security gap: operators using stdio would get the old permissive behavior while daemon operators get enforcement. The two paths are structurally identical but maintained separately.

**Test Scenarios**:
1. Subprocess test (stdio path): spawn the server binary in stdio mode without `UNIMATRIX_SESSION_AGENT` set. Assert non-zero exit code and `"UNIMATRIX_SESSION_AGENT"` in stderr.
2. Subprocess test (daemon path): spawn the server binary in daemon mode without `UNIMATRIX_SESSION_AGENT` set. Assert non-zero exit code and `"UNIMATRIX_SESSION_AGENT"` in stderr.
3. Grep check: confirm both `tokio_main_daemon` and `tokio_main_stdio` contain the `SessionIdentitySource::EnvVar.resolve()` call. This is a code-review-level check that can be enforced as an automated pattern test.

**Coverage Requirement**: Both startup paths must have subprocess-level tests that assert the refusal behavior independently.

---

### R-04: `SessionIdentitySource` enum — deference or true seam
**Severity**: High
**Likelihood**: Medium
**Impact**: The enum resolves to a `ValidatedAgentId` and then the session agent is enrolled and stored in `UnimatrixServer`. In STDIO/daemon mode, `UnimatrixServer` is constructed once at startup. For W2-2 HTTP, the architecture note says `session_agent: Option<SessionAgent>` set per-connection. If `UnimatrixServer` startup wiring is hard-coded to the STDIO assumption (construction-time identity), W2-2 must refactor `UnimatrixServer` itself — defeating the purpose of the seam. The enum correctly abstracts the identity source; the `UnimatrixServer` field model is the actual seam risk.

**Test Scenarios**:
1. Unit test: construct `SessionIdentitySource::EnvVar` with env var set, call `.resolve()`, assert it returns a `ValidatedAgentId` matching the env var value. Verify the seam compiles with the `JwtClaims` variant dead.
2. Inspection test (code review): verify `main.rs` constructs `UnimatrixServer` with `session_agent` as a constructor parameter — identity is resolved before `UnimatrixServer::new()` is called, not inside it. This keeps the construction call site compatible with a per-connection identity pattern.
3. Verify `#[allow(dead_code)]` is applied only to the `JwtClaims` variant body, not to the enum or its `resolve()` method, so CI does not silently suppress real dead-code warnings.

**Coverage Requirement**: `SessionIdentitySource::EnvVar` resolve path fully tested. `JwtClaims` variant compiles but is explicitly untested in alc-003 scope (document this as a gap for W2-2).

---

### R-05: `enroll_session_agent()` bypasses protected-agent guard
**Severity**: High
**Likelihood**: Low
**Impact**: `enroll_session_agent()` calls `store.agent_enroll()` directly, bypassing the `AgentRegistry` protected-agent check applied in `context_enroll`. If the caller does not validate `"system"` or `"human"` before calling `enroll_session_agent()`, the session agent enrollment overwrites the `"system"` or `"human"` record with `TrustLevel::Internal` and `[Read, Write, Search]`, undermining the administrative identity model. The validation must occur in `SessionIdentitySource::resolve()`, not assumed from a higher caller.

**Test Scenarios**:
1. Unit test: call `SessionIdentitySource::EnvVar.resolve()` with `UNIMATRIX_SESSION_AGENT=system`. Assert `SessionIdentityError::ProtectedName` is returned. Server must not start.
2. Unit test: same for `"human"`, `"HUMAN"`, `"System"`, `"SYSTEM"` — case-insensitive check per FR-04.
3. Integration test: verify `AGENT_REGISTRY` record for `"system"` after a successful server start has unchanged trust level and capabilities (bootstrap_defaults values, not the session agent values).

**Coverage Requirement**: All protected name variants (both cases × both names = 4 test cases minimum). Post-enrollment bootstrap agent integrity assertion.

---

### R-06: Audit attribution silent corruption
**Severity**: Medium
**Likelihood**: Medium
**Impact**: A tool call with `agent_id: "   "` (whitespace only) trims to empty and falls back to the session agent name. This is correct behavior, but if not logged explicitly, the audit trail shows the session agent name for what was in fact an anonymous call — making it impossible to distinguish "the session agent itself made this call" from "an unnamed caller made this call that was attributed to the session agent." The distinction matters for post-hoc forensic review.

**Test Scenarios**:
1. Integration test: call a tool with `agent_id: ""` (empty string). Assert audit log records session agent name.
2. Integration test: call a tool with `agent_id: "   "` (whitespace only). Assert audit log records session agent name (not the whitespace value).
3. Integration test: call a tool with `agent_id: "alc-003-researcher"` (named specialist). Assert audit log records `"alc-003-researcher"`, not the session agent name.
4. Integration test: call a tool with no `agent_id` parameter. Assert audit log records session agent name.

**Coverage Requirement**: All four attribution paths tested. The audit log assertion must check the stored value in `AUDIT_LOG`, not just the return value from the tool.

---

### R-07: Idempotency regression
**Severity**: Medium
**Likelihood**: Medium
**Impact**: AC-08 requires that restarting the server with the same env var produces exactly one row in `AGENT_REGISTRY`. If the upsert logic has a conditional branch that reads the existing record before writing (rather than always overwriting), a future change to that branch could cause the upsert to preserve old trust level or capabilities on restart. The authoritative-overwrite invariant must be explicitly tested.

**Test Scenarios**:
1. Integration test: enroll `"my-agent"` via a manual `registry.enroll_agent()` call with `TrustLevel::Restricted` and `[Read]`. Then call `enroll_session_agent()` with the same name. Assert the final record has `TrustLevel::Internal` and `[Read, Write, Search]`.
2. Integration test: construct a `UnimatrixServer` with `UNIMATRIX_SESSION_AGENT=my-agent`, tear it down, construct again with same env var. Assert `AGENT_REGISTRY` row count for `"my-agent"` is 1 after both cycles.
3. Integration test: same as scenario 2 but with a different agent name on the second cycle. Assert first name record still exists; second name has exactly one row.

**Coverage Requirement**: Upsert must be tested against a pre-existing conflicting record (different trust level and capabilities) to confirm the overwrite is unconditional.

---

### R-08: Daemon-mode clone carries stale session identity
**Severity**: Medium
**Likelihood**: Low
**Impact**: In daemon mode, `UnimatrixServer` is cloned per session task. If `SessionAgent` ever gains interior mutability (e.g., a future feature wraps `capabilities` in a `RwLock` to support dynamic revocation), in-flight task clones may see inconsistent state mid-session. The alc-003 design relies on `SessionAgent` being immutable after construction. This invariant must be documented and tested.

**Test Scenarios**:
1. Unit test: assert `SessionAgent` implements `Clone` and `Debug`. Assert all fields are `Clone`-able without indirection (no `Arc`, no `RwLock`). This is a compile-time check.
2. Daemon integration test using the existing `daemon_server + tmp_project` fixture pattern (entry #1928): connect two concurrent MCP clients to the same daemon. Assert both see the same session agent identity in their `initialize` responses.
3. Code review: verify `UnimatrixServer` fields `session_agent_id` and `session_capabilities` have no `Cell`, `RefCell`, `Mutex`, `RwLock`, or `Arc` wrappers.

**Coverage Requirement**: Concurrent client scenario in daemon mode. Clone-safety verified by type system.

---

### R-09: `PERMISSIVE_AUTO_ENROLL` deletion misses external crate callers
**Severity**: Medium
**Likelihood**: Medium
**Impact**: ADR-003 notes that `unimatrix-store`'s `agent_resolve_or_enroll` permissive parameter is also removed. If any crate outside `unimatrix-server` calls this function (e.g., `unimatrix-observe`, test binaries, or integration helpers), those callers will fail to compile after the signature change. The full workspace compile is the detector, but this must be verified before the PR is submitted.

**Test Scenarios**:
1. After removing the permissive parameter from `agent_resolve_or_enroll`: run `cargo build --workspace`. Assert zero compilation errors outside `unimatrix-server`.
2. Grep check: `grep -r "agent_resolve_or_enroll" crates/` to enumerate all callers before beginning the removal. This becomes the checklist for the PR.
3. AC-06 grep check: `grep -r "PERMISSIVE_AUTO_ENROLL" crates/` returns no results. This is a CI-enforced check per the specification.

**Coverage Requirement**: Full workspace compilation is the primary coverage gate. Grep inventory before and after.

---

### R-10: Breaking change — operator upgrades without setting env var
**Severity**: Medium
**Likelihood**: High
**Impact**: Any existing Unimatrix deployment that upgrades to the alc-003 binary without adding `UNIMATRIX_SESSION_AGENT` to `settings.json` will find the server silently non-starting. Depending on how Claude Code handles server startup failure, the operator may see no error at all — only that Unimatrix tools stop responding. The failure message must be clear and actionable.

**Test Scenarios**:
1. Subprocess test: spawn server without env var. Assert stderr contains `"UNIMATRIX_SESSION_AGENT"` (the variable name) AND a description of the required action (e.g., `"set UNIMATRIX_SESSION_AGENT"`), not merely a Rust backtrace.
2. Subprocess test: assert exit code is non-zero (any non-zero value). If a specific exit code (e.g., 78 for EX_CONFIG) is chosen, add a test asserting that specific value.
3. Manual validation (documented in PR): verify the error message is actionable — i.e., a developer unfamiliar with alc-003 who reads only the stderr output would know what to add to `settings.json`.

**Coverage Requirement**: Both exit code and stderr message content tested. NFR-02 (no Rust panic trace as primary output) must be verified.

---

### R-11: `SESSION_AGENT_DEFAULT_CAPS` not isolated
**Severity**: Medium
**Likelihood**: Medium
**Impact**: If the `[Read, Write, Search]` constant is inlined as a literal inside `main.rs` startup logic or inside `enroll_session_agent()`, W0-3 cannot wire the config-file reader to it without modifying identity logic. SR-03 was flagged at scope-risk time; the architecture resolved it with a named constant in `session_identity.rs`. If the implementation drifts from this design, W0-3 will require surgery.

**Test Scenarios**:
1. Code review / grep: `grep -r "Read, Write, Search" crates/unimatrix-server/src/` should return exactly one match — the `SESSION_AGENT_DEFAULT_CAPS` definition. Any other match is an inlined literal.
2. Unit test: assert `SESSION_AGENT_DEFAULT_CAPS` contains exactly `[Read, Write, Search]` — not fewer, not more. This documents the expected value and detects accidental future changes.
3. Verify `enroll_session_agent()` takes `capabilities: Vec<Capability>` as a parameter (does not read `SESSION_AGENT_DEFAULT_CAPS` internally). The constant is read at the call site in `main.rs`, allowing W0-3 to replace the argument without touching `session_identity.rs`.

**Coverage Requirement**: Single-definition guarantee enforced by grep. Parameter-not-constant design verified by function signature inspection.

---

### R-12: `ValidatedAgentId` newtype bypassed
**Severity**: Low
**Likelihood**: Low
**Impact**: If test code or future call sites construct `ValidatedAgentId` directly via a `pub` constructor rather than going through `resolve()`, validation is silently bypassed. Protected names or illegal characters could reach `enroll_session_agent()`.

**Test Scenarios**:
1. Verify `ValidatedAgentId` has no public constructor other than through `SessionIdentitySource::resolve()`. The struct's inner field must be private (`ValidatedAgentId(String)` not `ValidatedAgentId(pub String)`).
2. Unit test: confirm that calling `resolve()` with an invalid value returns `Err` and produces no `ValidatedAgentId`. The only valid `ValidatedAgentId` values are those produced by a successful `resolve()`.

**Coverage Requirement**: Visibility constraint is a type-system guarantee. One negative unit test covering the error path.

---

### R-13: Test process environment pollution
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Tests that set `UNIMATRIX_SESSION_AGENT` in the process environment using `std::env::set_var()` without restoring it afterward will pollute other tests running in the same process. Because Rust test threads share a process, this can cause test ordering-dependent failures that are difficult to diagnose. Historical: the SCOPE.md Constraints note that "the test fixture has no isolation from the process environment."

**Test Scenarios**:
1. Any test that calls `std::env::set_var("UNIMATRIX_SESSION_AGENT", ...)` must use a guard pattern (set → run → unset in a drop guard) or must be marked `#[serial]` to prevent concurrent interference.
2. Code review: grep for `set_var("UNIMATRIX_SESSION_AGENT"` in test files. Every match must have a corresponding cleanup or be isolated to a subprocess (the subprocess pattern is preferred for AC-04/AC-07 tests).
3. Integration test structure: prefer spawning the server as a subprocess for startup-refusal tests rather than setting env vars in the test process. This fully isolates environment state.

**Coverage Requirement**: All env-var-setting tests must be demonstrably isolated. Subprocess spawn pattern is the reference implementation.

---

### R-14: ADR #1839 future implementer ignores `SessionIdentitySource` seam
**Severity**: Low
**Likelihood**: Medium
**Impact**: A future implementer of ADR #1839 (`UNIMATRIX_CLIENT_TOKEN`) may bypass `SessionIdentitySource` entirely, adding a parallel capability resolution path that conflicts with the startup-cached `SessionAgent` model. ADR-004 documents the constraint; the risk is whether the future implementer reads it.

**Test Scenarios**:
1. This risk is mitigated by documentation, not by runtime tests. Verify ADR-004 is stored in Unimatrix (per the architecture's commitment) so it surfaces in future pre-implementation queries.
2. If ADR #1839 implementation begins: require a design review gate that shows the new `SessionIdentitySource` variant and confirms `enroll_session_agent()` is the enrollment path.

**Coverage Requirement**: Documentation-only in alc-003 scope. Future feature gate requirement.

---

## Integration Risks

### Component Boundary: `main.rs` → `mcp/session_identity.rs`

The startup identity chain crosses a module boundary: `main.rs` calls `SessionIdentitySource::EnvVar.resolve()` then `enroll_session_agent()`. The risk is that the error propagation from `SessionIdentityError` through `ServerError::SessionIdentity(String)` loses specificity. A `Missing` error and an `Invalid` error should produce different stderr messages (NFR-02); if they are both stringified to the same `ServerError::SessionIdentity` wrapper, the specific reason is lost.

**Scenario**: Parameterized test across `Missing`, `Invalid`, and `ProtectedName` error variants — assert each produces a distinct stderr substring.

### Component Boundary: `infra/registry.rs` → `unimatrix-store/src/registry.rs`

`enroll_session_agent()` calls `store.agent_enroll()` directly, bypassing the infra-level `AgentRegistry`. This means the store-level registry is the actual gate for protected-agent enforcement. Post-alc-003, protected-agent validation occurs at two points: `SessionIdentitySource::resolve()` (startup) and `AgentRegistry::enroll_agent()` (context_enroll tool). A gap exists if a future code path calls `store.agent_enroll()` directly without going through either guard.

**Scenario**: Confirm `store.agent_enroll()` does NOT have its own protected-agent guard — that guard lives only in `AgentRegistry`. Document this as an architecture constraint for future store callers.

### Component Boundary: `server.rs` (`build_context`) → 12 tool handlers

`build_context()` no longer performs a registry lookup, but it still produces a `ToolContext`. If `ToolContext::trust_level` is used anywhere in tool handler logic for conditional behavior (not just audit), removing the per-call lookup changes that behavior. The architecture note flags this as an open question.

**Scenario**: Grep for `trust_level` usages in tool handler files beyond audit context construction. For each usage, assert the handler behavior is correct when `trust_level` is always `TrustLevel::Internal` (the session agent level).

---

## Edge Cases

| Edge Case | Risk | Test Scenario |
|-----------|------|---------------|
| `UNIMATRIX_SESSION_AGENT` value is exactly 64 characters | Boundary validation — must accept | Test with a 64-char alphanumeric string. Assert server starts. |
| `UNIMATRIX_SESSION_AGENT` value is exactly 65 characters | Boundary validation — must reject | Test with a 65-char string. Assert non-zero exit and named failure. |
| `UNIMATRIX_SESSION_AGENT=HUMAN` (uppercase) | Case-insensitive protected name check | Assert startup failure with protected-name reason. FR-04 is explicit. |
| `agent_id` parameter contains only whitespace | Attribution fallback path | Assert audit log records session agent name, not whitespace. |
| Session agent name matches a previously enrolled swarm specialist | Upsert overwrites specialist record | Assert final record has `TrustLevel::Internal` and `[Read, Write, Search]`. |
| Server restarted 10 times with same env var | Idempotency under repeated upserts | Assert row count = 1 in `AGENT_REGISTRY` after 10 server construction cycles. |
| Concurrent Write tool calls in daemon mode | Capability check is thread-safe | Assert no panic or data race under concurrent tool calls (Tokio task-level concurrency). |
| `UNIMATRIX_SESSION_AGENT` set to an empty string `""` | Empty value must reject | Assert startup failure with named reason. Distinct from "env var absent" (FR-02 vs FR-03). |

---

## Security Risks

### Untrusted input surface: `UNIMATRIX_SESSION_AGENT`

**What untrusted input enters**: The env var value is set by the operator in `settings.json`. It is not LLM-controlled. However, `settings.json` itself is a file on the filesystem that a compromised tool or malicious repository could modify. The value reaches `SessionIdentitySource::resolve()` which applies regex validation before any store interaction.

**Damage from malformed input**: A value that bypasses validation could (a) overwrite the `"system"` or `"human"` registry record, or (b) inject characters into a SQL query if the store layer does not parameterize the enrollment insert. The regex `[a-zA-Z0-9_-]{1,64}` eliminates SQL metacharacters — this is defense-in-depth alongside parameterized queries.

**Blast radius if compromised**: An attacker who can set `UNIMATRIX_SESSION_AGENT` to any valid identifier can name any session agent identity — but cannot grant themselves capabilities beyond `[Read, Write, Search]`, which is the same as the old permissive default. The security improvement in alc-003 is structural (attribution moved out of LLM control), not a permissions reduction.

**Test scenario**: Attempt `UNIMATRIX_SESSION_AGENT="system'; DROP TABLE AGENT_REGISTRY; --"` — assert startup failure due to regex rejection before any store interaction.

### Untrusted input surface: `agent_id` tool parameter

**What untrusted input enters**: The LLM controls `agent_id` on every tool call. Post-alc-003, this value is used only for audit attribution — it is stored in the audit log as-is (after trimming).

**Damage from malformed input**: A very long `agent_id` value could cause audit log row bloat. An `agent_id` containing control characters could corrupt log output if not sanitized before display. The value should be length-limited or truncated for the audit log.

**Blast radius**: Attribution-only. Capability resolution is not affected. The worst case is a polluted audit log, not a capability escalation.

**Test scenario**: Call a tool with `agent_id` of 10,000 characters. Assert the operation either succeeds with a truncated attribution or rejects the call with a clear error — but does not panic or produce a malformed audit record.

### No transport-level authentication

By design (STDIO model, W0-2 acceptance), any process that can write to the MCP server's stdin has full `[Read, Write, Search]` access. The env var in `settings.json` proves the operator configured the server intentionally, but it does not authenticate the connecting process. This is an accepted risk documented in the specification. The test strategy cannot mitigate this — only document it.

---

## Failure Modes

| Failure | Expected Behavior | Test |
|---------|------------------|------|
| `UNIMATRIX_SESSION_AGENT` absent | Non-zero exit; stderr names variable; no tool calls served | AC-04 subprocess test |
| `UNIMATRIX_SESSION_AGENT` invalid format | Non-zero exit; stderr names validation failure; no tool calls served | AC-07 parameterized test (6 cases) |
| `UNIMATRIX_SESSION_AGENT` is protected name | Non-zero exit; stderr names protected-name reason; no tool calls served | R-05 test |
| `enroll_session_agent()` DB error at startup | Non-zero exit; error propagates through `ServerError::SessionIdentity`; no silent swallow | Integration test with a corrupted/locked store |
| Tool call with no session identity (code bug — invariant violated) | `UnimatrixServer::new()` requires `session_agent: SessionAgent` — cannot be constructed without it; compiler prevents this state | Compile-time guarantee |
| Daemon restart with identity change | New identity enrolled; old in-flight connections continue to use startup identity until reconnect | Documented operational constraint; logging at `initialize` event makes the active identity visible |
| Write tool call when session has `[Read, Search]` only | `CapabilityDenied` error returned; operation not performed; audit log records the denied attempt | NFR-03 verifiable by inspection; CapabilityDenied integration test |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (daemon reconnect uses startup identity) | R-08 | Architecture requires `initialize`-event logging so identity mismatches are visible. No runtime re-read on reconnect — documented operational constraint. |
| SR-02 (impersonation not detectable in audit log) | R-06 | Architecture adds startup audit event (`session_agent_enrolled`) and `initialize`-event logging. R-06 tests audit log attribution paths explicitly. |
| SR-03 (capability constant buried in startup logic) | R-11 | `SESSION_AGENT_DEFAULT_CAPS` defined as a named constant in `session_identity.rs`, passed as a parameter to `enroll_session_agent()`. R-11 enforces single-definition via grep check. |
| SR-04 (SessionIdentitySource abstraction undefined) | R-04 | `SessionIdentitySource` enum defined in ADR-001. R-04 tests the seam shape and verifies `main.rs` calls `resolve()` before `UnimatrixServer::new()`. |
| SR-05 (AC-03 vs Goals §5 contradiction) | R-02 | Specification resolves: per-call `agent_id` is attribution only; `require_cap()` loses `agent_id` parameter. R-02 tests all 12 call sites. |
| SR-06 (unknown test blast radius) | R-01 | ADR-005 mandates pre-flight measurement before behavioral code. R-01 tests the three-phase sequence gate. |
| SR-07 (daemon clone with different client identity) | R-08 | Accepted operational constraint. Logging at `initialize` time makes active identity visible per architecture. R-08 tests concurrent client scenario. |
| SR-08 (ADR #1839 conflict with session caps model) | R-14 | ADR-004 defers #1839 and documents the constraint. R-14 is documentation-only in alc-003 scope. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | 6 scenarios (3 for R-01, 3 for R-02) |
| High | 3 (R-03, R-04, R-05) | 9 scenarios (3 per risk) |
| Medium | 6 (R-06, R-07, R-08, R-09, R-10, R-11, R-13) | 16 scenarios |
| Low | 2 (R-12, R-14) | 3 scenarios |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found entry #1609 (ServiceError variant names lesson, crt-014), entry #1928 (daemon-mode integration test fixture pattern, vnc-005). Both inform R-03 (transport-path parity) and R-08 (daemon clone risk).
- Queried: `/uni-knowledge-search` for "risk pattern capability identity authentication" — found entry #261 (AuditSource-Driven Behavior Differentiation), entry #317 (ToolContext pre-validated context pattern). Entry #317 directly informs R-02 (stale call site risk at refactoring). Entry #261 informs R-06 (attribution path correctness).
- Queried: `/uni-knowledge-search` for "breaking change migration deployment upgrade" — found entry #376 (DDL-before-migration ordering failure). Informs R-10 (operator upgrade path).
- Stored: nothing novel to store — the pre-flight blast-radius measurement pattern (ADR-005) is already addressed by the architecture, and the ToolContext stale-caller risk is already in entry #317. No new cross-feature pattern evident yet. If alc-003 delivery confirms the blast radius was underestimated, a new lesson-learned entry will be warranted.
