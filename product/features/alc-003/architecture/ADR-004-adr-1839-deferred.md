## ADR-004: ADR #1839 (`UNIMATRIX_CLIENT_TOKEN`) Deferred; alc-003 Is the Named-Identity Layer

### Context

ADR #1839 (Unimatrix entry #1839) designs `UNIMATRIX_CLIENT_TOKEN` as a token-based
identity mechanism for STDIO. The token is hashed, stored in AGENT_REGISTRY, and
validated at MCP `initialize` time. Unknown tokens reject the connection entirely.
This is a stronger security posture than alc-003's env-var named-identity approach.

Both mechanisms address session-level identity for STDIO. If both shipped, there would
be two competing env-var identity mechanisms — a configuration hazard and an audit
attribution ambiguity.

SCOPE.md §"ADR #1839 Reconciliation" proposes sequential staging: alc-003 ships first
as named-identifier identity, ADR #1839 is the future hardening layer.

The risk assessment (SR-08) flags that if ADR #1839 is implemented later without
reference to alc-003's session-capabilities-cached-at-startup model, the two mechanisms
could produce conflicting capability states.

### Decision

ADR #1839 is **deferred** to a future feature. It remains an open design in Unimatrix
(entry #1839 is NOT deprecated — it describes a valid future hardening step). However,
its status is updated to acknowledge that alc-003 ships first and establishes the
identity layer that ADR #1839 would extend.

The relationship between the two:

| Layer | Feature | Mechanism | Trust claim |
|-------|---------|-----------|-------------|
| Named identity | alc-003 (W0-2) | `UNIMATRIX_SESSION_AGENT` env var | Attribution only — not a credential |
| Token hardening | future (ADR #1839) | `UNIMATRIX_CLIENT_TOKEN` hashed + pre-enrolled | Weak credential — token proves enrollment |
| Full auth | W2-2/W2-3 | OAuth JWT via HTTP transport | Strong credential — cryptographically signed |

ADR #1839 does not conflict with alc-003 at the implementation level because:
1. alc-003 caches session capabilities at startup in `SessionAgent`; ADR #1839 would
   validate the token at `initialize` time and then populate the same `SessionAgent`
   cache. The capability resolution path (ADR-002) is unchanged.
2. `UNIMATRIX_SESSION_AGENT` is a plain identifier; `UNIMATRIX_CLIENT_TOKEN` is an
   opaque token. They can coexist: the token identifies which registered session-agent
   record to load, while the env var names the agent directly.

If ADR #1839 is implemented post-alc-003, the implementer must:
- Use `SessionIdentitySource::JwtClaims` (or add a new `TokenBased` variant) rather
  than bypassing `SessionIdentitySource`
- Populate `SessionAgent` via the same `enroll_session_agent()` path
- Not re-introduce per-call capability lookups

This ADR is stored in Unimatrix as a cross-reference record so future implementers of
ADR #1839 find this constraint.

### Consequences

**Easier:**
- No conflicting identity mechanisms ship simultaneously
- alc-003 implementation is not complicated by token hashing, bcrypt, schema changes,
  or enrollment CLI — all of which ADR #1839 requires
- The `SessionIdentitySource` abstraction (ADR-001) gives ADR #1839 a clear insertion
  point without touching capability resolution

**Harder:**
- STDIO security posture in alc-003 is attribution-only, not credential-based. This is
  an accepted limitation documented in SCOPE.md §"Security Posture Acknowledgement"
- ADR #1839 must be implemented with awareness of alc-003's session-capabilities model
  or it will conflict; this ADR is the breadcrumb that communicates that constraint
