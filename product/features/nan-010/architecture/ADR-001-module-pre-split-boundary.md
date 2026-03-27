## ADR-001: Module Pre-Split as First Implementation Step

### Context

`eval/report/render.rs` is at 499 lines. `eval/report/aggregate.rs` is at 488 lines.
The Rust workspace rule enforces a hard 500-line limit per file (SR-02, SR-03). nan-010 adds a
distribution gate render function and a distribution gate aggregation function. Neither can be
added inline to their parent files without immediately breaching the limit.

Past features (nan-008, nan-009) have used module extraction as a follow-on cleanup after the
fact. nan-010 cannot do this — both files are already at or within one line of the limit before
the feature work begins. Any incidental change to `render.rs` (an import line, a doc comment,
a new use statement) would breach the limit before the feature code is even written.

The scope risk assessment (SR-02, SR-03) classifies this as High severity/High likelihood.

### Decision

Module boundaries are established as the first implementation step — before any other code
changes in these files.

For `render.rs`: create `eval/report/render_distribution_gate.rs` as a new sibling module
containing `render_distribution_gate_section`. Add only `mod render_distribution_gate;` and
the corresponding `use` to `render.rs`. No other changes to `render.rs` until the boundary
module file exists.

For `aggregate.rs`: pre-split into `eval/report/aggregate/mod.rs` (all existing content,
re-exports unchanged) and `eval/report/aggregate/distribution.rs` (new file for
`check_distribution_targets` and its return types). The split is a pure refactor — no logic
changes — performed before `distribution.rs` receives any content.

Callers of `aggregate` (in `report/mod.rs`) use only the re-exported names from
`aggregate/mod.rs`, so the split is transparent at the call site.

### Consequences

Easier:
- Delivery agents have a clear starting point: create the boundary files first, confirm the
  workspace builds, then add feature code.
- The 500-line constraint cannot be inadvertently violated mid-feature.
- The module structure remains consistent with the existing `render_phase.rs` pattern.

Harder:
- The pre-split of `aggregate.rs` requires a non-trivial directory rename (`aggregate.rs` →
  `aggregate/mod.rs`) that is a separate commit from the feature code. This is a known
  coordination point for the delivery agent.
