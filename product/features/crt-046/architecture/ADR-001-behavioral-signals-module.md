## ADR-001: Behavioral Signal Logic in a Separate Module

### Context

`mcp/tools.rs` already contains the `context_cycle_review` and
`context_briefing` handler implementations, along with all their supporting
helpers. The file is large and approaching the 500-line limit enforced by the
Rust workspace conventions.

crt-046 adds two logically cohesive groups of behaviour:

1. Co-access pair extraction, behavioral Informs edge emission, and
   goal-cluster population at cycle review time.
2. Goal embedding retrieval and cluster-entry blending at briefing time.

Both groups share helper functions (outcome weight mapping, pair building,
entry blending) and constants (cosine threshold, recency cap, pair cap). If
placed directly in `mcp/tools.rs` they would push the file well over 500 lines
and bury the handler logic under an unrelated signal-processing concern.

An alternative is placing the logic in `services/index_briefing.rs` (close to
the blending call site), but that module already serves the `IndexBriefingService`
struct contract and blending is only one side of the feature. A third
alternative is adding it to `services/co_access_promotion_tick.rs`, but that
module owns the background tick path and this feature operates in the explicit
handler path — a different execution context with different error-handling
semantics.

### Decision

All behavioral signal logic ships in a new module:
`unimatrix-server/src/services/behavioral_signals.rs`.

The module is declared `pub(crate)` in `services/mod.rs`. Its public surface is
limited to the functions that `mcp/tools.rs` and `services/index_briefing.rs`
need to call:

- `collect_coaccess_entry_ids`
- `build_coaccess_pairs`
- `outcome_to_weight`
- `emit_behavioral_edges`
- `populate_goal_cluster`
- `blend_cluster_entries`

Constants (`COSINE_THRESHOLD`, `RECENCY_CAP`, `PAIR_CAP`) are defined in this
module and not re-exported. Callers that need to reference threshold values in
tests should import the module directly.

`mcp/tools.rs` calls into `behavioral_signals` for step 8b and for the
briefing blend. Neither handler file contains behavioral signal logic directly.

### Consequences

Easier:
- `mcp/tools.rs` stays under 500 lines.
- Behavioral signal logic is unit-testable without constructing a full server
  struct — tests in `behavioral_signals.rs` can take `&[ObservationRow]`
  directly.
- Future extensions (e.g., phase-stratified edge weighting in a later roadmap
  group) are localized to this module.
- The module boundary makes it straightforward to replace or extend the cosine
  scan strategy (e.g., approximate NN in the future) without touching handler
  code.

Harder:
- One additional file to maintain. Any change to `ObservationRow` or
  `AnalyticsWrite::GraphEdge` struct fields requires updating this module as
  well as the existing call sites.
