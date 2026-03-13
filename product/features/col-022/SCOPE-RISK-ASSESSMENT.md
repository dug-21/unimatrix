# Scope Risk Assessment: col-022

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Hook-side attribution races with eager attribution: if eager voting resolves before PreToolUse hook fires (e.g., strong file-path signals in early events), `set_feature_if_absent` will reject the explicit declaration silently | High | Med | Architect should ensure explicit `context_cycle(start)` can override or that SM calls it before any file-touching tool calls |
| SR-02 | 50ms hook latency budget is tight when adding UDS round-trip for cycle attribution on top of existing PreToolUse processing; budget overrun causes Claude Code timeout and dropped attribution | Med | Med | Architect should measure baseline PreToolUse latency and ensure cycle path adds <5ms marginal cost |
| SR-03 | Wire protocol backward compatibility: old hook binaries encountering new event types in `RecordEvent` may silently drop or misparse payloads (ADR-005 #246, col-012 ADR-003 #384 accepts silent loss) | Med | Low | Architect should validate that unknown `event_type` values in RecordEvent are safely ignored by existing deserialization |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Scope excludes protocol/agent file updates (Non-Goal 5) but the tool is useless until SM agents call it; delayed follow-up creates a shipped-but-unused tool | Med | High | Spec should define the exact follow-up issue content and acceptance criteria so integration is not forgotten |
| SR-05 | Keywords parameter is accepted and stored but injection behavior is deferred (Non-Goal 6); risk of designing a storage schema that does not serve the future injection use case | Med | Med | Architect should design keywords storage with the injection query pattern in mind, not just persistence |
| SR-06 | `cycle_end` semantics are ambiguous for downstream consumers: it records a boundary event but does not change session state, so retrospective must interpret it correctly | Low | Med | Spec should define exactly how retrospective pipeline uses the boundary event |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | MCP tool `context_cycle` is a no-op shell while real work happens in hook handler; two code paths must stay synchronized — tool validates params but hook does the work, creating a split-brain validation risk | High | Med | Architect should ensure validation logic is shared (single validation function called by both paths) |
| SR-08 | `set_feature_if_absent` first-writer-wins semantic (#1067) means if SessionStart already set feature_cycle (from `extra` field), `context_cycle(start)` silently does nothing; callers may not realize attribution failed | Med | Med | AC-05 response should clearly indicate whether attribution was actually set vs already present |
| SR-09 | Keywords storage in SQLite sessions table (Open Question 2) touches schema shared by observation pipeline, retrospective, and knowledge effectiveness (crt-018); schema change has wide blast radius | Med | Low | Architect should assess whether sessions table column or separate table minimizes migration risk |

## Assumptions

- **One session = one feature** (SCOPE line 25, #1067): If future workflow changes require multi-feature sessions, this entire design must be revisited. The constraint is load-bearing.
- **PreToolUse hook fires before MCP server processes the call** (SCOPE line 76-81): If Claude Code changes hook timing, the hook-side attribution path breaks entirely.
- **SM agents will be updated to call context_cycle** (SCOPE line 181): The feature's value depends entirely on this follow-up. Without it, heuristic attribution remains the only path.
- **Fire-and-forget persistence is sufficient** (SCOPE line 157): Attribution writes may be lost under server unavailability (per ADR-003 col-012 #384). Accepted risk but worth noting.

## Design Recommendations

- **SR-01**: Architect should consider whether `context_cycle(start)` should use a force-set semantic (overwrite) rather than `set_feature_if_absent`, or ensure protocol ordering guarantees SM calls it before any other tool use.
- **SR-07**: Extract a shared `validate_cycle_params()` function usable by both MCP tool and hook handler to prevent validation divergence.
- **SR-05/SR-09**: Design keywords storage with a forward-looking query pattern (semantic search input) rather than just a JSON blob column.
- **SR-04**: Include follow-up issue creation as a deliverable in the spec's definition of done.
