# ADR-001: Response Abstraction Pattern

## Status

Accepted

## Context

The test harness makes assertions against MCP tool responses — JSON structures returned by the server. With 225+ tests across 8 suites, directly parsing and asserting on raw JSON creates tight coupling between every test and the exact response format. If the server changes response structure (field names, nesting, format variations), dozens of tests break simultaneously.

Scope risk SR-07 identified this as a high-likelihood maintenance risk.

## Decision

All tests assert through an abstraction layer in `harness/assertions.py` rather than directly on raw JSON-RPC responses. The abstraction provides:

1. **Response parsing functions** (`parse_entry`, `parse_entries`, `parse_status_report`) that convert raw tool responses into Python dicts/dataclasses with stable field names.

2. **Assertion helpers** (`assert_tool_success`, `assert_tool_error`, `assert_entry_has`, `assert_search_contains`) that encapsulate both parsing and verification.

3. **Format awareness** — the same assertion works regardless of whether the response is in summary, markdown, or json format, by normalizing internally.

## Consequences

- **Positive**: Response format changes require updating one module, not 225 tests.
- **Positive**: Test code reads as intent ("assert entry has confidence > 0.5") not mechanics ("parse response text as JSON, navigate to entries[0].confidence, compare").
- **Negative**: Additional layer of indirection. Bugs in assertions.py could mask test failures.
- **Mitigation**: The assertion layer itself is tested implicitly by every test that uses it. A bug in assertions.py would cause widespread, obvious failures.

## Alternatives Considered

1. **Raw JSON assertions everywhere** — simpler initially but unsustainable at 225+ tests. Rejected per SR-07.
2. **Generated client from JSON Schema** — over-engineered for a test harness. Schema changes would require regeneration. Rejected.
3. **Snapshot testing** — captures full responses and compares. Brittle: any response change (timestamps, IDs) breaks snapshots. Rejected.
