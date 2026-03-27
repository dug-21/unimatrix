## ADR-003: Source-Candidate Bound Derived from `max_graph_inference_per_tick`, No Separate Config Field

### Context

SR-02 flags that `get_embedding` is O(N) per call over the HNSW in-memory index (confirmed
at `crates/unimatrix-vector/src/index.rs:312`). If source candidate selection is unbounded,
the tick may perform O(N) embedding lookups for N candidates even when `max_graph_inference_per_tick`
limits NLI calls to 100. The scope risk assessment asks for "a source-candidate bound that is
enforced before any embedding lookup, independent of (and ≤) `max_graph_inference_per_tick`."

Three options were considered:

1. **Add a separate `max_source_candidates_per_tick` config field** — adds a knob that most
   operators will never tune, and whose correct value is always `<= max_graph_inference_per_tick`.
   Two knobs for one concern; risk of misconfiguration (e.g. source_candidates=1000 but
   pairs=100 means 1000 embedding lookups for 100 scored pairs).

2. **Derive the bound as a multiple of `max_graph_inference_per_tick`** (e.g. 2× or 3×) —
   adds a hidden constant whose justification is not obvious. HNSW expansion from K sources
   each producing K neighbours can yield K² pairs before deduplication, so a multiplier of
   K is theoretically defensible but hard to explain to an operator.

3. **Cap source candidates to exactly `max_graph_inference_per_tick`** — conservative and
   self-explanatory: "at most 100 sources selected, each queried for up to K neighbours."
   In the worst case (K=10, 100 sources, no deduplication) this yields 1,000 candidate
   pairs before the pair-level truncation to 100. Embedding lookups are bounded to 100.

### Decision

`select_source_candidates` returns at most `max_graph_inference_per_tick` source IDs.
The call in `run_graph_inference_tick` passes `config.max_graph_inference_per_tick` as the
`max_sources` argument. No new config field is added.

This means:
- `get_embedding` is called at most `max_graph_inference_per_tick` times per tick.
- The pair count before Phase 5 truncation is at most
  `max_graph_inference_per_tick × graph_inference_k` (before deduplication).
- After Phase 5 truncation the NLI batch is at most `max_graph_inference_per_tick` pairs.

The constraint is expressed in code comments on `select_source_candidates` and in
`run_graph_inference_tick`'s documentation.

### Consequences

SR-02 is fully mitigated: the embedding scan is bounded before any embedding is looked up.

The bound is slightly conservative for deployments with a highly connected graph (many
deduplication hits reduce the effective pair count well below `max_graph_inference_per_tick`).
In that case fewer source candidates would suffice, but the cap is cheap to enforce.

If a future deployment needs more sources (e.g. very large graphs where 100 sources are
insufficient to reduce isolation quickly), the existing `max_graph_inference_per_tick` field
can be raised. There is no hidden coupling that makes this unsafe.
