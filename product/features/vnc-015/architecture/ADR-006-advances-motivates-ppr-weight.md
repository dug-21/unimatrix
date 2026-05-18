## ADR-006: PPR Positive-Type Expansion — RelatedTo Only

### Context

`positive_out_degree_weight` in `graph_ppr.rs:168-187` is hardcoded to 4 positive types:
`Supports`, `CoAccess`, `Prerequisite`, `Informs`. These 4 types are used in the PPR denominator
normalization. Similarly, `graph_expand.rs:123-136` hardcodes the same 4 types for positive BFS
traversal.

An earlier draft of this ADR (superseded by this document) proposed adding `Advances` and
`Motivates` to the positive type set on the grounds that they are Goal-tracing edges with direct
SDLC semantic value. That decision was reversed during vnc-015 design review.

The question being answered here: which of the 10 new RelationType variants added in vnc-015
should be included in `positive_out_degree_weight` and the BFS positive set?

Two options considered:
- Option A (original): Add `Advances` and `Motivates` to PPR and graph_expand alongside the
  existing four positive types.
- Option B (adopted): Add only `RelatedTo` to PPR and graph_expand. All other 9 new variants
  (including `Advances` and `Motivates`) are write-only in this feature.

### Decision

Option B: `RelatedTo` only. Only `RelatedTo` is added to `positive_out_degree_weight` in
`graph_ppr.rs` and to the positive BFS set in `graph_expand.rs`. `Advances` and `Motivates` are
write-only in this feature — agents can declare these edges but they will not flow through PPR
or graph_expand until Phase 2.

Rationale:
1. `RelatedTo` is semantically undirected/symmetric — bidirectional PPR flow is correct by
   definition. There is no ambiguity about which entry accumulates authority: both sides benefit
   symmetrically.
2. `Advances` and `Motivates` are directed edge types with asymmetric authority semantics. The
   design question "does the goal accumulate authority, or does the advancing entry?" requires
   careful analysis to get right. Getting this wrong ships incorrect PPR behavior that must be
   undone — a more disruptive change than simply deferring.
3. Phase 2 (`context_graph`) is where directed-edge traversal semantics belong. That phase will
   have the full graph populated by Phase 1 edges and can make empirical decisions about weight
   and direction based on observed graph structure.
4. Deferring `Advances` and `Motivates` does not block any vnc-015 use case. Agents can declare
   these edges immediately; serendipitous retrieval for `RelatedTo` edges ships; Goal-tracing
   PPR is a Phase 2 concern.

Implementation sites (pattern #3950):
- `graph_ppr.rs`: `positive_out_degree_weight` function — add only `RelationType::RelatedTo`
- `graph_ppr.rs`: `personalized_pagerank` function — same addition
- `graph_expand.rs`: positive BFS set — add only `RelationType::RelatedTo`

Estimated: ~6 lines total across the two files (per SCOPE.md Goal 8 estimate).

### Consequences

Easier: PPR mass flows through `RelatedTo` edges from day one. Associative discovery works
immediately for the most common broad-relatedness relationship. Implementation is ~6 lines —
simple, low-risk, verifiable.

Harder: Agents that declare `Advances(feature → goal)` or `Motivates(lesson → decision)` edges
will not see PPR benefit until Phase 2. This is an accepted deferral — the edges are stored and
will become active when Phase 2 adds them to the positive type set.

Supersedes: original ADR-006 (Advances/Motivates equal-weight PPR inclusion, entry #4423).
Related: ADR-007 (from_str guard — all 10 new variants including `Advances` and `Motivates`
must still have `from_str()` arms, as they are stored to GRAPH_EDGES even though write-only).
