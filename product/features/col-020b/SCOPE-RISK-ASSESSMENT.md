# Scope Risk Assessment: col-020b

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Serde alias backward compat may silently fail — `#[serde(alias)]` only works for deserialization; serialization always uses the new name. Consumers reading serialized col-020b output with col-020 types will see unknown fields silently dropped. | High | Med | Architect should define the compat contract: is it read-old-with-new only, or bidirectional? Test both directions explicitly. (Evidence: Unimatrix #885 — col-020 gate failure from insufficient serde test coverage.) |
| SR-02 | `#[serde(alias)]` + `#[serde(rename)]` interaction is subtle — if any field already has `#[serde(rename = "...")]`, adding `alias` requires careful ordering. Misuse produces silent data loss (field defaults to `Default::default()`). | Med | Low | Verify no existing `serde(rename)` on affected fields before designing the rename approach. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | #193 root cause is explicitly unresolved in scope — "validate during implementation" means the fix scope is unbounded. If the bug is in Store-layer SQL queries or session_id format, the fix crosses into unimatrix-store, expanding the 2-crate scope to 3. | High | Med | Architect should define a time-box for root cause investigation and a fallback if the bug is in the Store layer (e.g., separate issue for Store fix, ship field renames independently). |
| SR-04 | "Integration tests" scope is ambiguous — scope mentions Rust unit-style tests with realistic inputs AND defers infra-001 harness decision to architect. These are fundamentally different efforts (hours vs days). | Med | Med | Architect should make a clear decision: Rust-only tests for col-020b, with infra-001 coverage as a separate follow-up issue. Mixing both in one feature risks scope expansion. |
| SR-05 | Scope touches `RetrospectiveReport` (the top-level output type) — field renames on a report type affect all downstream consumers including the `context_retrospective` MCP tool response and any stored/cached reports. | Med | Low | Confirm all consumers of `RetrospectiveReport` are internal (MCP tool output only, no persisted reports that would break). |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | Re-export path update in `unimatrix-observe/src/lib.rs` when renaming `KnowledgeReuse` to `FeatureKnowledgeReuse` — any crate importing the old name will fail to compile, but the error is clear. The risk is missing a re-export site. | Low | Low | grep for all `KnowledgeReuse` imports across workspace before implementing. |
| SR-07 | `classify_tool` changes (new `curate` category) affect `tool_distribution` in `SessionSummary` — downstream consumers parsing tool_distribution keys may not expect the new category. | Med | Low | Ensure `tool_distribution` is documented as extensible (new categories can appear). New category should use `serde(default)` semantics if consumed as a typed map. |
| SR-08 | Cross-crate test infrastructure gap — Unimatrix #729 documents that intelligence pipeline testing requires cross-crate integration tests, but no such infrastructure exists. Adding it for col-020b risks building scaffolding that should be a separate effort. | Med | Med | Architect should use the existing cross-crate feature flag pattern (Unimatrix #747) if cross-crate tests are needed, not build new infrastructure. |

## Assumptions

1. **Serde aliases are sufficient for backward compat** (SCOPE Constraints) — assumes no persisted `RetrospectiveReport` data exists that would need to round-trip through new types. If reports are cached/stored in SQLite, aliases alone may not suffice.
2. **`extract_file_path` does not need changes** (SCOPE Background Research) — assumes Claude-native tools are never MCP-prefixed. If Claude Code ever changes its hook format, this assumption breaks silently.
3. **2-crate scope** (SCOPE Problem Statement) — assumes #193 root cause is in computation or data flow logic, not in Store-layer SQL. SCOPE explicitly flags this as uncertain (Constraint line 122).
4. **Unit tests with realistic inputs are sufficient** (SCOPE Proposed Approach F) — assumes the data flow bug (#193) can be reproduced without a running Store. If the bug is in SQL query construction, unit tests with synthetic data will not catch it.

## Design Recommendations

1. **Time-box #193 investigation** (SR-03): Define a 2-hour investigation cap. If root cause is in Store SQL, split into separate issue and ship the rest of col-020b independently. The field renames, tool normalization, and new tests are independently valuable.
2. **Rust-only test scope** (SR-04, SR-08): Ship col-020b with Rust unit tests using MCP-prefixed inputs. Defer infra-001 integration tests to a follow-up. This keeps the feature small as scoped.
3. **Bidirectional serde test** (SR-01): Architect should require a test that serializes with new types and deserializes with old field names (via alias), AND a test that verifies old serialized data deserializes correctly with new types. Unimatrix #885 is direct precedent.
