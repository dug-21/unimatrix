# col-018: Test Plan Overview

## Test Strategy

This feature adds ~15 lines of production code to a single dispatch arm. Testing focuses on:
1. **Unit tests** in the existing `listener.rs` test module verifying observation writes via direct DB queries
2. **Existing integration suites** covering ContextSearch response behavior (no MCP-visible changes)

## Risk-to-Test Mapping

| Risk ID | Risk Description | Test Coverage | Priority |
|---------|-----------------|---------------|----------|
| R-01 | Observation write fails silently | T-01 (verify row exists in DB) | Medium |
| R-02 | Topic signal false positives | T-03, T-04, T-05 (feature ID vs generic vs path) | Low |
| R-03 | Input field unbounded | T-06, T-07 (truncation + boundary) | Low |
| R-04 | Session ID None edge case | T-08, T-09 (guards) | Low |
| R-05 | Search pipeline regression | T-10, T-11 (search results unchanged) | High |
| R-06 | Topic signal accumulation missed | T-12 (session registry check) | Medium |

## Integration Harness Plan

### Existing Suite Coverage

| Suite | Relevance | Action |
|-------|-----------|--------|
| `smoke` | Covers ContextSearch basic response | Run (mandatory gate) |
| `tools` | Covers context_search tool parameters | Run (regression) |

### Gaps

The observation write is an internal side-effect with no MCP-visible behavioral change. Observations are stored in SQLite but not exposed through any MCP tool response. No new integration tests are needed.

### New Integration Tests

None required. All observation verification is through unit tests that query the SQLite observations table directly after `dispatch_request()`.

## Cross-Component Dependencies

None. Single-component feature.
