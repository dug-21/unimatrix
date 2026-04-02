## ADR-001: Generalized Edge Writer — write_graph_edge Sibling, Not Parameterized write_nli_edge

### Context

`write_nli_edge` in `nli_detection.rs` hardcodes both `created_by = 'nli'` and
`source = 'nli'` in the INSERT statement:

```rust
"INSERT OR IGNORE INTO graph_edges \
 (source_id, target_id, relation_type, weight, created_at, created_by, \
  source, bootstrap_only, metadata) \
 VALUES (?1, ?2, ?3, ?4, ?5, 'nli', 'nli', 0, ?6)"
```

Path C must write edges with `source = 'cosine_supports'` (SCOPE.md Goal 2, AC-11).
Two generalization options exist:

**Option A — Parameterize write_nli_edge:** Add a `source: &str` parameter to the
existing function signature. All callers (Path A write loop in `nli_detection_tick.rs`
and Path B write in Phase 8) would pass `"nli"` explicitly.

**Option B — Add write_graph_edge sibling:** Introduce a new
`pub(crate) async fn write_graph_edge(store, source_id, target_id, relation_type,
weight, created_at, source, metadata) -> bool` that accepts `source` as a parameter.
`write_nli_edge` delegates to it or becomes an independent thin wrapper. All existing
callers of `write_nli_edge` remain unchanged.

The constraint from SCOPE.md is explicit: "Changing the hardcoded literal in
`write_nli_edge` would silently retag all existing Informs and NLI Supports edges —
NOT acceptable."

Option A would require updating every `write_nli_edge` call site to pass `"nli"`
explicitly. This is a low-risk mechanical change, but it creates a correctness hazard:
if any future call site accidentally passes a wrong source string to the
parameterized function, existing NLI edges get silently mis-tagged. The function name
`write_nli_edge` also becomes misleading if it accepts non-NLI sources.

Pattern #4025 (Unimatrix entry) states this decision directly: "Do not reuse
write_nli_edge() for new edge signal sources — add a write_graph_edge(source: &str, ...)
sibling." This pattern was stored after crt-039 analysis, specifically anticipating
crt-040.

### Decision

Add `write_graph_edge(store, source_id, target_id, relation_type, weight, created_at,
source, metadata) -> bool` as a new `pub(crate)` function in `nli_detection.rs`.

`write_nli_edge` is refactored to call `write_graph_edge` with hardcoded `source = "nli"`
and `created_by = "nli"`. Its signature does not change. All existing callers of
`write_nli_edge` are unmodified.

Path C calls `write_graph_edge` directly with `source = EDGE_SOURCE_COSINE_SUPPORTS`.

The `created_by` field for Path C writes shall be `"cosine_supports"` — matching the
source value — so that DB-level auditing can distinguish the write origin even when
filtering on `created_by` directly.

### Consequences

**Easier:**
- Path A and Path B call sites are unchanged. No risk of accidentally mis-tagging NLI
  edges by passing the wrong string.
- Source value isolation is structural: the only call site passing `"cosine_supports"`
  is Path C. Any future signal source adds its own call to `write_graph_edge` with its
  own named constant, following the same pattern.
- The function name `write_nli_edge` remains accurate for its callers; it is now
  internally implemented via the generalized helper.
- Pattern #4025 is satisfied: `EDGE_SOURCE_*` constants in `read.rs` plus a generalized
  writer in `nli_detection.rs` form the standard pattern for all future edge sources.

**Harder:**
- `nli_detection.rs` gains one additional function. Given the file's current size and
  the deferred module-merge decision from crt-039, this is acceptable.
- Delivery must ensure `write_nli_edge` forwards all parameters correctly to
  `write_graph_edge` — a straightforward delegation, but requires a focused review step.
