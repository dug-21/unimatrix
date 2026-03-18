# Scope Risk Assessment: alc-003

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `std::env::var` read at startup is not re-read on daemon reconnect. A new Claude Code session connecting to a running daemon inherits the startup-time identity, even if `settings.json` changed. | Med | High | Architect must document restart-to-update as the required contract; consider logging the session agent identity on every MCP initialize event so the mismatch is visible. |
| SR-02 | `UNIMATRIX_SESSION_AGENT` is a plain identifier, not a credential. Any process that can set env vars (e.g., a compromised child process or a misconfigured `settings.json`) can impersonate any registered agent. The scope acknowledges this; the risk is that it is not surfaced in the audit log in a way that makes impersonation detectable post-hoc. | Med | Med | Architect should log the full env-var value and the enrollment upsert result in the startup audit record, so forensics can detect unexpected identity changes across restarts. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | Capability set (`[Read, Write, Search]`) is hardcoded. SCOPE.md declares W0-3 makes it configurable, but the boundary is only one constant. If the implementation buries it inside startup logic rather than isolating it as a named parameter, W0-3 will require surgery inside identity resolution code. | Med | Med | Architect must expose the capability set as a named constant (`SESSION_AGENT_DEFAULT_CAPS`) in a location W0-3 can replace with a config-file read without touching identity.rs or main.rs logic. |
| SR-04 | The forward path to OAuth (W2-2/W2-3) requires the identity resolution layer to be swappable. SCOPE.md proposes this but does not name the abstraction boundary. If `read_session_agent_env()` is inlined into `tokio_main_daemon()` and `tokio_main_stdio()` rather than injected through a trait or function pointer, the swap will require touching both startup paths. | High | Med | Architect must define an explicit `SessionIdentitySource` abstraction (trait or enum) at the boundary between "how we learn who is connecting" and "what we do with that identity." The env-var implementation is one variant; JWT claim extraction is the next. |
| SR-05 | `agent_id` in AC-03 states capabilities resolve from the caller's own registry record, but Goals §5 states capabilities resolve from the session exclusively and no registry lookup occurs. These contradict. If misread during implementation, registry lookups for per-call `agent_id` will silently reintroduce the old behavior. | High | Med | Spec writer must resolve this contradiction before implementation begins. AC-03 should be rewritten to match Goals §5: per-call `agent_id` is attribution only; capabilities always come from the session. |
| SR-06 | `PERMISSIVE_AUTO_ENROLL` deletion removes the only mechanism for integration tests to grant Write access to test agents without setting up a session. The 27 usages of `SecurityGateway::new_permissive()` in `gateway.rs` are a separate concern (rate-limit bypass) and must not be conflated. However, the two tests in `infra/registry.rs` that assert `[Read, Write, Search]` for unknown agents will break and need explicit pre-enrollment. Additional integration tests across 185 infra tests may implicitly depend on permissive behavior without asserting it — a silent blast radius. | High | High | Before implementation, run the full test suite with `PERMISSIVE_AUTO_ENROLL` forced to false to enumerate all failures. Fix before coding alc-003, not after. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | Daemon mode clones `UnimatrixServer` into each MCP session task. The session agent field is read at daemon startup, before any client connects. A client that sets a different `UNIMATRIX_SESSION_AGENT` in its own environment (e.g., a non-Claude client connecting to the daemon) will silently use the daemon's startup identity. No error is surfaced. | Med | Low | Architect should log the resolved session agent identity in the MCP `initialize` response or response metadata so clients can verify the identity being applied to their session. |
| SR-08 | ADR #1839 (`UNIMATRIX_CLIENT_TOKEN`) is an open design that overlaps with `UNIMATRIX_SESSION_AGENT`. The SCOPE.md proposes sequential coexistence, but if ADR #1839 is implemented without reference to alc-003's session-capabilities-cached-at-startup model, the two mechanisms could produce conflicting capability states. | Med | Low | Architect must update ADR #1839 status in Unimatrix (mark as deferred/superseded-by-alc-003 for now) to prevent a future delivery team from implementing it without the alc-003 context. |

## Assumptions

- **§ "Proposed Approach" item 1** assumes `read_session_agent_env()` returns `None` (not `Err`) when the env var is absent, causing a fall-through to `permissive=false`. But Goals §1 and AC-04 state the server must refuse to start if the env var is absent. These two behaviors are mutually exclusive. If the fall-through path is coded first (as the proposed approach describes), an incomplete implementation will pass startup and silently run without session identity.
- **§ "Proposed Approach" item 3** assumes converting `PERMISSIVE_AUTO_ENROLL` to a runtime env var as an intermediate step. The Key Design Decisions in the spawn prompt state it is deleted entirely. Implementing the conversion step risks committing a partially-done state where the env var still exists and tests that set it pass.
- **§ "Constraints / Technical"** assumes `UnimatrixServer` is cloned per session task. If the struct clone semantics change (e.g., session state added in a future feature), the session agent field must remain `Clone`-safe. Document this invariant.

## Design Recommendations

- **SR-05 (Critical)**: Resolve the AC-03 vs. Goals §5 contradiction before architecture phase. The spec writer must rewrite AC-03 to be unambiguous.
- **SR-04 (High)**: Define a named `SessionIdentitySource` abstraction boundary. Name it in the architecture doc so W2-2 has a clear replacement target.
- **SR-06 (High)**: Run a pre-flight test suite scan with `PERMISSIVE_AUTO_ENROLL=false` before any code changes. Enumerate the real blast radius. The SCOPE.md's "27 tests" count conflates `SecurityGateway::new_permissive()` (rate limiting, unaffected) with registry permissive behavior (affected). The true count of registry-permissive-dependent tests is likely smaller, but must be confirmed.
- **SR-03 (Med)**: Isolate `SESSION_AGENT_DEFAULT_CAPS` as a named constant at the module boundary W0-3 will own.
- **SR-01 (Med)**: Log session agent identity at every MCP initialize event, not just at startup.
