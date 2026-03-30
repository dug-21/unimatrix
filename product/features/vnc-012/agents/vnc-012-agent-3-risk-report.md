# Agent Report: vnc-012-agent-3-risk

**Mode**: architecture-risk
**Output**: `/workspaces/unimatrix/.claude/worktrees/vnc-012/product/features/vnc-012/RISK-TEST-STRATEGY.md`

## Summary

10 risks identified. 2 Critical, 4 High, 4 Medium, 0 Low.

## Risk Count by Priority

| Priority | Count |
|----------|-------|
| Critical | 2 |
| High | 4 |
| Medium | 4 |
| Low | 0 |

## Top Risks for Human Attention

**R-01 (Critical)** — `#[serde(default)]` missing on optional fields. This is a compile-silent trap: absent optional fields produce a missing-field serde error instead of `None`, causing a regression on every call that omits `k`, `limit`, `max_tokens`, or `evidence_limit`. Mandated tests: 5 absent-field unit tests per ADR-004.

**R-02 (Critical)** — AC-13 integration test blocked by OQ-04 (`RequestContext<RoleServer>` constructibility). The rmcp dispatch path — the exact location of the bug — has no coverage until this is resolved. Options: expose `pub(crate) call_tool_for_test` on `UnimatrixServer`, or rely on infra-001 IT-01/IT-02 as the sole dispatch-path gate. Implementation agent must resolve before closing AC-13.

**R-06 (High)** — Float JSON Numbers (e.g., `3.0` as Number type, not string) are unhandled. OQ-05 is open in the spec. Risk strategy recommendation: reject all float Numbers via explicit `visit_f64` returning `serde::de::Error`. A test for this edge case is included as a required scenario under R-06.

## Open Questions Addressed

**OQ-04**: AC-13 requires `RequestContext<RoleServer>`. If not constructible from rmcp public API, the implementation agent must expose a `pub(crate)` test helper on `UnimatrixServer` to invoke the `ToolRouter` directly. Alternatively, IT-01 in infra-001 can serve as the sole rmcp dispatch gate. Both should be present — risk is Critical if neither exists.

**OQ-05**: Float JSON Numbers (e.g., `3.0`) should be **rejected** (strict). Accepting `3.0` as `3` creates ambiguity and invites `3.9` being silently truncated. Schema advertises `type: integer`. Visitor must implement `visit_f64` returning an explicit error, not rely on serde's default "unexpected type" message. R-06 includes test scenarios for this.

## Scope Risk Traceability

All 6 scope risks (SR-01 through SR-06) are traced. SR-04 and SR-06 have no architecture-level risk — both accepted/out-of-scope.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection serde deserialization" — entry #885 (gate failure for missing serde test coverage) and entry #3786 (MCP deserialization fixes require transport-level tests) directly informed R-02 severity rating.
- Queried: `/uni-knowledge-search` for "risk pattern integration test coverage MCP transport" — entry #3526 confirmed infra-001 as the correct vehicle for this class of boundary test.
- Queried: `/uni-knowledge-search` for "serde Visitor null absent optional field" — entry #3548 (test coverage weaker than specified) reinforced mandatory explicit assertion requirement for R-01 and R-03.
- Stored: nothing novel to store — the `#[serde(default)]` + `deserialize_with` optional field trap is not yet recurring across 2+ features to warrant a stored pattern entry.
