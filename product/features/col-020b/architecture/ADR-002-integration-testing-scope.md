## ADR-002: Rust-Only Tests for col-020b; infra-001 Integration Tests Deferred

### Context

col-020b fixes two computation bugs (#192, #193) and renames types/fields. The human asked whether the retrospective computation path should be tested through infra-001 style integration testing (Python, MCP JSON-RPC over stdio) in addition to Rust unit tests.

The infra-001 harness already supports `context_retrospective` calls and has a `_seed_observation_sql` helper that injects observation data directly into the server's SQLite database. In principle, an integration test could:
1. Seed observation records with `mcp__unimatrix__context_search` tool names
2. Call `context_retrospective` via MCP
3. Verify that `knowledge_served > 0` in the response

However, several factors inform the decision:

- **The bugs are in pure computation functions.** `classify_tool`, the knowledge flow counters, and `compute_knowledge_reuse` are all pure functions that take data in and return results. They have no I/O, no async, no database access. Unit tests with realistic (MCP-prefixed) inputs directly validate the fix.
- **The integration path adds complexity without proportional value.** The existing infra-001 observation seeder inserts raw rows, but does not control the exact tool names in the observation data. Extending it to insert MCP-prefixed tool names is feasible but requires modifying the seeder, and the test would be verifying the same computation that the unit test already covers -- just through more layers.
- **SR-04 from the scope risk assessment** explicitly warns that mixing Rust unit tests and infra-001 tests in the same feature risks scope expansion ("hours vs days").
- **The data flow bug (#193)** is the one case where integration tests would add value (validating Store -> computation end-to-end). But the root cause is still under investigation, and the fix may or may not touch the Store layer. Adding an integration test for an unresolved bug is premature.
- **infra-001 already has 3 retrospective tests** (T-R01 through T-R06). These validate that the MCP tool returns structured reports. They do not validate specific metric values, which is appropriate -- metric values depend on synthetic observation data patterns that are hard to make deterministic in an integration context.

### Decision

Ship col-020b with Rust unit tests only. The unit tests will use MCP-prefixed tool names as inputs (realistic data format) to validate the normalization fix.

Defer infra-001 integration test coverage for retrospective knowledge metrics to a follow-up issue. That follow-up should:
1. Extend `_seed_observation_sql` to accept custom tool names
2. Add a test that seeds MCP-prefixed knowledge tool calls and verifies `knowledge_served > 0`
3. Optionally add a test for `feature_knowledge_reuse.delivery_count > 0` if query_log/injection_log seeding is feasible

### Consequences

- **Easier:** col-020b stays small and focused on the computation fixes. No Python test infrastructure work.
- **Easier:** Unit tests with synthetic data are deterministic and fast (~ms). Integration tests require binary compilation and server startup (~seconds).
- **Harder:** The end-to-end data flow from Store through computation to MCP response remains untested by automated integration tests. This is acceptable because: (a) the computation functions are pure and well-tested, (b) the Store query methods have their own unit tests, and (c) the data flow glue in `tools.rs` is thin (three `spawn_blocking` calls and a delegation).
- **Risk:** If #193's root cause is a session_id format mismatch between tables, unit tests will not catch it. The debug tracing added in C6 provides the diagnostic path for that scenario.
