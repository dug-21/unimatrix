# Agent Report: vnc-012-agent-2-spec

## Output

SPECIFICATION.md written to:
`product/features/vnc-012/specification/SPECIFICATION.md`

## Key Decisions Made

**1. Null vs. absent made explicit (SR-02)**
FR-04 and C-03 codify null and absent as two distinct code paths with different serde
mechanics. Absent requires `#[serde(default)]` paired with `#[serde(deserialize_with)]`
— this is a greenfield trap in the Visitor pattern with no prior art in the codebase. A
dedicated AC per optional field was added (AC-03-ABSENT-ID, AC-03-ABSENT-LIMIT,
AC-04-ABSENT, AC-04-NULL, AC-05-ABSENT, AC-05-NULL, AC-06-ABSENT, AC-06-NULL) rather
than a single generic "optionals default to None" test as the risk assessment required.

**2. Integration test concretized (SR-03, Q4)**
AC-13 expands Open Question 4 into a concrete, verifiable acceptance criterion: a Rust
test that calls `rmcp::ServerHandler::call_tool` with a `CallToolRequestParams` where
the `id` field is a JSON String, asserts `Ok(_)`, and asserts non-empty content. This
exercises the exact `serde_json::from_value(Value::Object(arguments))` path in rmcp
that unit tests in `tools.rs` cannot reach. One open question (OQ-04) is raised for the
architect: how to construct a minimal `RequestContext<RoleServer>` for the test, or
whether a `pub(crate)` test helper is needed.

**3. Float Number rejection specified explicitly**
OQ-05 flags the unspecified behavior for JSON float Number inputs (e.g., `3.0` as a
Number). The spec recommends strict rejection. This was not addressed in SCOPE.md and
needs architect confirmation.

**4. Schema snapshot test added**
AC-10 requires a concrete schema-snapshot unit test (call `list_all()`, assert
`type == "integer"` for each affected property). This addresses SR-01 from the risk
assessment, which noted that schema correctness was not verified.

**5. `serde_util` module visibility**
FR-06 specifies `mod serde_util;` (private, not `pub mod`) in `mcp/mod.rs`. The helpers
are `pub(crate)` within `serde_util.rs` — visible to `tools.rs` in the same crate but
not exported to downstream crates.

## Open Questions for Architect

- **OQ-04**: How to construct `RequestContext<RoleServer>` for the AC-13 integration
  test. The `tool_router` field is private; `ServerHandler::call_tool` is the public
  entry point but requires a `RequestContext`. Resolution determines the exact test
  vehicle.
- **OQ-05**: Whether JSON float Numbers (e.g., `3.0`) passed to integer fields should
  be rejected (strict) or coerced. Spec recommends rejection; architect to confirm.

## Self-Check

- [x] SPECIFICATION.md covers all acceptance criteria from SCOPE.md (AC-01 through
      AC-12 present; AC-13 added for SR-03)
- [x] Every functional requirement is testable
- [x] Non-functional requirements include measurable targets where possible
- [x] Domain Models section defines key terms including null vs. absent distinction
- [x] NOT in scope section is explicit
- [x] Output file is in `product/features/vnc-012/specification/` only
- [x] No placeholder or TBD sections — two open questions flagged explicitly
- [x] Knowledge Stewardship report block included in SPECIFICATION.md

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — entries #3786 and #3784 directly
  relevant. Entry #3786 confirmed the integration test requirement pattern for
  deserialization fixes. The live `context_get` call failure with string id during
  this session confirmed the bug is active.
