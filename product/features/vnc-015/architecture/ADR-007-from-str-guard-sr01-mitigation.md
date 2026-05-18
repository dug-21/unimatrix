## ADR-007: from_str() Guard and SR-01 Mitigation — 10×4 Explicit Checklist

### Context

SR-01 from the scope risk assessment identifies the highest-probability implementation defect:
"10 new RelationType variants × 4 required sites = 40 coordinated changes; a missing `from_str()`
arm causes silent row-drop at Pass 2b with no compile error."

Pattern #3950 (Unimatrix knowledge base) documents this precisely:
> A missing `from_str()` arm causes the R-10 guard in `build_typed_relation_graph` to silently
> drop rows and emit a warn — the edge is never added to the graph.

The 4 required sites per variant are:
1. Enum body in `graph.rs`
2. `as_str()` match arm in `graph.rs`
3. `from_str()` match arm in `graph.rs`
4. PPR positive type inclusion in `graph_ppr.rs` (only for `RelatedTo`)

For the 9 write-only variants (`Advances`, `Motivates`, `Cites`, `Asserts`, `Mentions`, `Refutes`,
`Tests`, `DerivedFrom`, `About`), site 4 is intentionally absent in this feature — they are not PPR
types. `Advances` and `Motivates` PPR expansion is deferred to Phase 2.
The checklist must document this as intentional absence, not an oversight.

`graph_expand.rs` is a fifth site for `RelatedTo` only (positive BFS set).

### Decision

The implementation spec for vnc-015 MUST include an explicit 10×4 checklist table as an
acceptance criterion. The spec writer produces this table; Gate-3a verification greps each cell.

Table format:

| Variant | graph.rs enum | graph.rs as_str() | graph.rs from_str() | graph_ppr.rs positive | graph_expand.rs positive |
|---------|--------------|-------------------|---------------------|-----------------------|--------------------------|
| Advances | required | required | required | intentionally absent (Phase 2) | intentionally absent (Phase 2) |
| Cites | required | required | required | intentionally absent | intentionally absent |
| Asserts | required | required | required | intentionally absent | intentionally absent |
| Mentions | required | required | required | intentionally absent | intentionally absent |
| Refutes | required | required | required | intentionally absent | intentionally absent |
| Tests | required | required | required | intentionally absent | intentionally absent |
| DerivedFrom | required | required | required | intentionally absent | intentionally absent |
| Motivates | required | required | required | intentionally absent (Phase 2) | intentionally absent (Phase 2) |
| About | required | required | required | intentionally absent | intentionally absent |
| RelatedTo | required | required | required | REQUIRED | REQUIRED |

Gate-3a must verify:
1. `grep` all 10 variant strings in `graph.rs` enum body
2. `grep` all 10 variant strings in `graph.rs` `as_str()` match
3. `grep` all 10 variant strings in `graph.rs` `from_str()` match
4. `grep` `RelatedTo` in `graph_ppr.rs` `positive_out_degree_weight`
5. `grep` `RelatedTo` in `graph_ppr.rs` `personalized_pagerank`
6. `grep` `RelatedTo` in `graph_expand.rs` positive BFS set
7. Negative check: confirm `Advances` and `Motivates` do NOT appear in `graph_ppr.rs` positive sets

Additionally, a new integration test must verify that all 10 new variants survive
`build_typed_relation_graph` Pass 2b without being silently dropped by the R-10 guard:
for each new variant, insert a GRAPH_EDGES row with that `relation_type` string, build the
graph, assert the edge count equals 1.

### Consequences

Easier: silent row-drop is caught at Gate-3a review and in the integration test, not discovered
post-merge via missing graph traversal. The checklist table is self-documenting and makes
intentional absences (8 write-only variants not in PPR) explicit.

Harder: the spec writer must produce the checklist table before implementation begins. The Gate-3a
verifier must run the 6 greps. This is mechanical but mandatory.

Supersedes: none.
Related: pattern #3950 (four-site extension requirement), ADR-006 (RelatedTo-only PPR expansion).
