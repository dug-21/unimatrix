## ADR-004: Cosine Supports Budget — MAX_COSINE_SUPPORTS_PER_TICK = 50 Constant

### Context

Path A (Informs) uses `MAX_INFORMS_PER_TICK = 25` as a module-level constant that is
independent of `max_graph_inference_per_tick` (the Path B NLI Supports budget). This
independence was established in bugfix-473 to prevent Path B filling its budget from
reducing Path A write slots.

Path C (cosine Supports) needs its own per-tick budget cap. SCOPE.md §Resolved Design
Decisions item 1 states: "Budget: `MAX_COSINE_SUPPORTS_PER_TICK = 50` (constant). Cosine
lookup against an already-scanned candidate set has no model cost. Follows the
`MAX_INFORMS_PER_TICK` pattern. Config promotion is easy later if an operator needs it
— don't speculate now."

Two options exist for encoding this budget:

**Option A — Module constant:** `const MAX_COSINE_SUPPORTS_PER_TICK: usize = 50;`
adjacent to `MAX_INFORMS_PER_TICK`. Not configurable at runtime. Follows the exact
pattern of `MAX_INFORMS_PER_TICK` and `MAX_SOURCES_PER_TICK`.

**Option B — InferenceConfig field:** `max_cosine_supports_per_tick: usize` with
serde default 50 and range validation. Operator-tunable via config.toml. Requires
the dual-site pattern from ADR-002 plus validate() range guard. Increases
`InferenceConfig` surface area.

The rationale against speculative config promotion is the same as for `MAX_INFORMS_PER_TICK`:
the cost of cosine comparison against an already-fetched candidate set is negligible
(O(1) float comparison + HashSet lookup per pair). There is no per-model invocation cost.
The budget is a throughput safety valve, not a latency-critical tuning parameter. If an
operator genuinely needs a different value in production, the constant is a one-line
change with a code deployment — no runtime config is required for a value that changes
at most once per deployment.

SR-03 (SCOPE-RISK-ASSESSMENT.md) flags that the hard-coded constant "has no range
guard or config promotion path" and recommends a TODO comment at the constant site.

### Decision

Use a module-level constant: `const MAX_COSINE_SUPPORTS_PER_TICK: usize = 50;`

The constant is placed adjacent to `MAX_INFORMS_PER_TICK: usize = 25` in
`nli_detection_tick.rs`, with a doc comment noting:
- It is independent of `max_graph_inference_per_tick` (Path B budget).
- It is independent of `MAX_INFORMS_PER_TICK` (Path A budget).
- Config promotion is deferred; a `max_cosine_supports_per_tick` InferenceConfig field
  is the natural extension point if operator tuning is required in the future.

The cap applies after Path C's cosine/category filter. Path C iterates `candidate_pairs`
in order (Phase 4 sort order by cross-category/isolated/similarity descending already
applied), writes up to `MAX_COSINE_SUPPORTS_PER_TICK` edges, then stops. Unlike Path A
which shuffles `informs_metadata` before truncating, Path C takes Phase 4's existing
priority order as-is — the sort already prioritizes high-value pairs and no additional
randomization is needed.

### Consequences

**Easier:**
- No `InferenceConfig` field means no dual-site trap risk for this budget value.
- `InferenceConfig::validate()` does not need a new range check.
- Follows the established `MAX_INFORMS_PER_TICK` / `MAX_SOURCES_PER_TICK` constant
  pattern. Consistency makes the intent immediately clear to future agents.

**Harder:**
- Operators cannot tune the budget without a code deployment and server restart.
  This is acceptable at current graph density but must be revisited if Path C
  consumes its full budget on every tick (a signal that the candidate set is
  consistently larger than 50 qualifying pairs).
- The TODO comment at the constant site is the only mechanism to signal that
  config promotion is a future option. Delivery must include this comment.
