# Agent Report: alc-003-agent-2-spec

**Agent ID**: alc-003-agent-2-spec
**Role**: Specification Writer
**Feature**: alc-003 — Session Identity via Env Var

## Output

- `product/features/alc-003/specification/SPECIFICATION.md`

## Key Decisions Made

**AC-03 contradiction resolved (SR-05)**: SCOPE.md AC-03 contained contradictory language stating
capabilities resolve from "the caller's own registry record" while Goals §5 states they always come
from the session. The specification rewrites AC-03 to be unambiguous: per-call `agent_id` is
attribution only; capabilities always come from `session_capabilities` on `UnimatrixServer`; no
registry lookup occurs. This was flagged as critical in the risk assessment.

**Startup refusal is unambiguous (Assumptions §1 conflict)**: SCOPE.md "Proposed Approach" item 1
described `read_session_agent_env()` returning `None` (fall-through) when env var is absent. This
contradicts Goals §1 and AC-04 (refuse to start). The specification picks Goals §1 — absent env
var is a hard startup failure (FR-02). The fall-through path does not exist.

**SessionIdentitySource named in spec**: SR-04 flagged that SCOPE.md proposed this abstraction but
did not name it. The specification names it explicitly in the Domain Model and FR-10, making it a
deliverable requirement the architect must implement, not a suggestion.

**SESSION_AGENT_DEFAULT_CAPS named in spec**: SR-03 flagged the risk of inline literals. The
specification requires this as a named constant (FR-12) isolated at a module boundary W0-3 will own.

**Protected name comparison is case-insensitive**: FR-04 specifies that `"HUMAN"`, `"Human"`, etc.
all fail validation. SCOPE.md only listed `"system"` and `"human"` as lowercase. Case-insensitive
comparison prevents spoofing via casing variation.

**Breaking change explicitly declared**: SR-01 required the spec to document the migration path.
A dedicated "Breaking Change Declaration" and operator migration workflow (Workflow 2) are included.
The message is unambiguous: no `UNIMATRIX_SESSION_AGENT` means no server start.

**Daemon restart constraint formalized**: SR-02 required the spec to document single-read-at-startup
as an operational constraint. Workflow 4 covers the daemon restart scenario explicitly, including
the `initialize`-event logging requirement (FR-11) that makes identity mismatches visible.

**Test migration strategy specified**: SR-06 required test guidance. NFR-07 and AC-10 define the
two allowed update patterns: explicit enrollment before Write calls, or session agent path. Workflow
3 shows the developer experience. AC-06 requires a CI grep check enforcing `PERMISSIVE_AUTO_ENROLL`
absence.

## Open Questions

**OQ-01 — ADR #1839 disposition (architect action required)**: The spec requires the architect to
update ADR #1839 status in Unimatrix to "deferred — superseded in scope by alc-003 for W0-2." This
prevents a future delivery team from implementing `UNIMATRIX_CLIENT_TOKEN` without alc-003 context.
The architect report should confirm this update was made.

**OQ-02 — `AgentCapabilities` Clone constraint**: FR-06 and NFR-04 require `AgentCapabilities` to
be `Clone`. The specification assumes this is satisfiable without wrapping in `Arc`/`Mutex`. The
architect should confirm `AgentCapabilities` is or can be made `Clone` without structural changes
to the type.

**OQ-03 — CI grep enforcement for AC-06**: AC-06 requires a CI-enforced grep check for
`PERMISSIVE_AUTO_ENROLL`. The delivery team must decide whether this is a `cargo test` assertion,
a pre-commit hook, or a CI step. The spec does not prescribe the mechanism — only the requirement.

**OQ-04 — Regex vs. pattern match for FR-04**: The spec requires `[a-zA-Z0-9_-]{1,64}` validation
but does not mandate `regex` crate usage. The architect should decide whether to add a `regex`
dependency or implement the check with a simple char-by-char iterator. Given the pattern's
simplicity, the latter is preferable to avoid a new dependency.

## Knowledge Stewardship

- Queried: /uni-query-patterns for agent authentication, capability resolution, session identity — found ADR-003 (entries #31 and #79), ADR #1839 (entry #1839), ToolContext pattern (entry #317), specification duties (entries #110 and #223), AC coverage convention (entry #138), testable requirements convention (entry #133), and pre-flight enrollment lesson (entry #265). No prior specification used `SessionIdentitySource` as a named abstraction — this is a new pattern introduced in alc-003.
