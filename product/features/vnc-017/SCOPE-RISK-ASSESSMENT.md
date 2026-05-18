# Scope Risk Assessment: vnc-017

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `redirect_graph_edge` opens one transaction per redirect call. Under a large incoming-edge fan-in, N sequential transactions amplify write-pool contention and latency inline on the `context_correct` hot path. | Med | Low | Architect should document the acceptable edge-cardinality ceiling; consider a batched transaction path if the fan-in bound is not enforced externally. |
| SR-02 | `rows_affected() > 0` return semantics on redirect differ from `write_graph_edge` silent-UNIQUE-discard behaviour (lesson #4041). A redirect of an already-redirected edge returns `false` (conflict) — not an error. The redirect loop must not log a warning or count it as a failure. | Med | Med | Spec must explicitly define the three-case return contract for `redirect_graph_edge` (inserted / conflict-ignored / SQL-error) and require the loop to treat conflicts as success, not failure. |
| SR-03 | `query_incoming_edges` uses `read_pool()` which is the correct pool; however `write_pool_server()` and `write_pool` share the same underlying pool (`db.rs:294`). A future pool split could silently break write ordering if callers use the wrong accessor. | Low | Low | Architect should use the canonical accessor names explicitly and add a comment citing the implementation detail. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | `Supersedes` edge exclusion (OQ-03) is implemented by a skip in the redirect loop, not at the query level. Any `Supersedes` rows in `GRAPH_EDGES` are fetched then discarded — wasteful and potentially confusing if such rows are numerous. | Low | Low | Spec should clarify whether `Supersedes` rows should be excluded at `query_incoming_edges` SQL level (cleaner) or at the loop level (current plan). If excluded at query level, note that the intent comment belongs in the SQL. |
| SR-05 | The response text change ("Redirected N incoming edges (M failed, see logs)") appended unconditionally would be noisy for the zero-edge case (AC-08). SCOPE says the zero path is skipped, but the append logic must also be conditional. | Low | Med | Spec must define the zero-edge response: either omit the append entirely or emit "Redirected 0 incoming edges" — both are acceptable, but the choice must be explicit. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | `Contradicts` edges are bidirectional (4-row atomicity inside `redirect_graph_edge`). If one of the Contradicts pairs has a source that is itself deprecated or quarantined, the 4-row transaction will attempt to write a new edge from a quarantined source. The caller contract states validation happens before calling — but the redirect loop does not re-validate source entries. | High | Med | Architect must decide: does the redirect loop validate source entry status before calling `redirect_graph_edge`? Skipping validation is consistent with ADR-003 (partial-write posture), but could silently create edges from quarantined sources. Spec must make this explicit. |
| SR-07 | The `DependencyOnDeprecated` detection rule (vnc-016, 23rd rule) runs on the graph state tick. If auto-redirect is partial (some edges redirected, some not due to SR-06), stale detection may still fire on unredirected edges during the next tick. This is an acceptable degraded state per ADR-003, but test coverage must confirm the detection rule clears after a successful full redirect. | Med | Med | AC-05 integration test should assert no `DependencyOnDeprecated` fires after a successful redirect, not only that the edge rows are updated. |
| SR-08 | `read.rs` is 3,465 lines (OQ-04). Adding `query_incoming_edges` is low-risk, but any refactor of read.rs for unrelated reasons during or after delivery could cause merge conflicts. | Low | Low | No action needed; note the file size for context. |

## Assumptions

- **SCOPE.md §Constraints (terminal-active)**: Assumes `context_correct` can only be called on an Active entry, making `new_entry.id` always terminal-active at creation time. If this invariant is ever relaxed (e.g., correction of a Deprecated entry by admin), the no-cache-traversal shortcut breaks silently.
- **SCOPE.md §Proposed Approach (synchronous)**: Assumes production entries have "at most a handful" of incoming edges. No enforcement mechanism exists. If a high-fan-in entry is ever corrected, the inline redirect loop could add significant latency to the MCP call.
- **SCOPE.md §Background Research (Supersedes direction)**: Assumes `graph_edges` Supersedes rows exist and are derived/duplicated from `entries.supersedes`. If they do not exist in practice (never written), the exclusion filter is dead code — harmless but should be verified.

## Design Recommendations

- **SR-06 (Critical)**: Architect must define source-validation behaviour before the redirect loop calls `redirect_graph_edge`. Recommend explicit skip-with-warn for quarantined sources, consistent with the existing ADR-003 blast-radius posture.
- **SR-02 (Spec discipline)**: Spec must include an explicit return-contract table for the redirect loop's handling of `redirect_graph_edge` return values (true / false-conflict / Err), referencing lesson #4041.
- **SR-05 (Minor)**: Spec should define zero-edge response text behaviour to avoid an AC ambiguity at gate.
