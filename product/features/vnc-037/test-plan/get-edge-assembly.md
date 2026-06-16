# Test Plan — get-edge-assembly (`mcp/get_edges.rs` + handler)

The `context_get` handler edge path: resolve `include_edges`, issue ranked select + split count +
batched title join, project rows → `GetEdge`, **project `EdgeCountSplit { inbound, outbound, both }`
→ `EdgeTotals { inbound, outbound, both }` and thread the digest-only `authored_total` into
`EdgesView`**, build `EdgesView`. Owns **R-14 (opt-out)**, **R-16 / AC-14 (fail-loud)**, **R-13
(latency)**, **R-15 (dangling)**, **R-19 (corrected transient)**, and the projection direction logic
(R-10, shared with get-edge-vocabulary). `unimatrix-server` integration/unit tests.

## Unit / Integration Test Expectations

### R-16 / AC-14 — Fail-loud edge contract (the RED test, FR-19)

**`test_edge_query_failure_fails_loud`** (named, RED, #4876)
Inject a failure into the **ranked/edge query** *after* a successful primary `entry_store.get`.
Assert `context_get` **returns an error** (mapped `ServerError`, same mapping as the primary-read
failure) — **NOT** a success payload with edges omitted. Run it **RED** (the test observes a real
injected failure surfacing as an error).

**`test_count_query_failure_fails_loud`** — same, injecting into the split `COUNT(*)`.
**`test_title_join_failure_fails_loud`** — same, injecting into the batched title join. A join
failure must surface as a mapped error, **never** a silent `target_title: null` fill or an
omitted edge set.

**`test_zero_edges_is_success_distinct_from_failure`** (the zero-vs-failure distinction)
A **genuine zero-edge** entry returns a **success** with the explicit empty state (`edges: []` /
"No related entries" / `edges: none`). Assert this success is **structurally distinguishable**
from the error result of the failure tests — a success payload is **never** produced by a failed
edge path, and a zero-edge success is never conflated with a failure (the silent-omit failure
mode this feature forbids).

**`grep_no_unwrap_on_edge_path`** (C-11, static)
Grep `graph_queries_ranked.rs`, `get_edges.rs`, the edge-assembly block in `tools.rs`, and the
`map_edge_row` change for `.unwrap()` / `.expect()` in non-test code → **none**.

### R-14 — Opt-out actually skips (High)

**`test_opt_out_skips_all_edge_queries`** (AC-11, query-count/instrumentation)
`include_edges: Some(false)` → assert **zero** ranked/count/title queries issued (query-count or
instrumentation), and the response carries **no `edges` key** — byte-indistinguishable from a
list-view payload. The opt-out branch must not even reach the fail-loud path.

**`test_include_edges_three_resolutions`** (AC-11)
`None` → surface; `Some(true)` → surface; `Some(false)` → suppress. All three asserted.

**`test_internal_caller_opt_out_enumerated`** (OQ-03, each a named assertion)
The enumerated internal call sites pass `Some(false)`:
- the hook / write-back path,
- the briefing pipeline's by-ID fetches,
- by-ID loop fetches (bulk machine reads).
Assert **each** call site sets `Some(false)` (not assumed). Cross-check: **no agent-facing
`context_get` MCP path is flipped default-off** (the MCP tool stays `None`/default-on).

### R-10 — Direction / `target_id` projection (High; logic shared with get-edge-vocabulary)

**`test_projection_outbound_inbound_far_endpoint`**
Seed one outbound asymmetric (anchor = `source_id`) and one inbound asymmetric (anchor =
`target_id`). Assert `direction` outbound/inbound respectively and `target_id` is the **other**
endpoint each time (never the anchor itself).

**`test_projection_symmetric_both_no_arrow`** (D-02 fix)
A canonicalized symmetric edge projects `direction = "both"` (renders `↔`) and emits **no**
`→`/`←`.

### Totals projection + digest authored threading (R-03/AC-08; 3-bucket contract)

**`test_totals_projection_three_buckets`** (ADR-005 TOTALS BUCKET CONTRACT)
The store's `EdgeCountSplit { inbound, outbound, both }` projects 1:1 to
`EdgeTotals { inbound, outbound, both }` on `EdgesView` — assert all three buckets carry through
unchanged (no fold, no drop of `both`). On a fixture with all three bucket types populated, assert
the view's `edge_totals` equals the store split exactly.

**`test_authored_total_threaded_from_full_set`** (TOTALS BUCKET CONTRACT §3)
The digest-only `authored` aggregate from `count_neighbors_split` (over the FULL uncapped set) is
threaded into `EdgesView` as `authored_total` (NOT re-derived from the capped ≤3 `edges` vec).
Assert with a >cap fixture where authored over the full set ≠ count of `authored==true` among the
displayed ≤3: `EdgesView.authored_total` must equal the full-set value, proving the summary renderer
reads it from the view rather than recomputing off the displayed slice.

### R-15 — Dangling target retained, no panic (Medium)

**`test_dangling_title_null_retained_no_panic`** (DNB-1)
An edge whose `target_id` has no `entries` row → `target_title: null`, edge **retained** in
`EdgesView`, no `.unwrap()` panic on the null path.

**`test_mixed_resolved_and_dangling`** — a dangling edge does not drop resolved ones; titles
resolve for the resolved targets, `null` for the dangling, all in **one** batched join (no N+1).

### R-13 — Latency (High; manual/bench, gated AC-12)

**`bench_edge_free_baseline_high_degree`** (AC-12, manual)
Measure the opt-out (`include_edges:false`) `context_get` path on a representative store
**including a high-degree node**; record p50/p95.

**`bench_default_on_delta_within_budget`** (AC-12, manual)
Measure default-on on the same store/node; assert added delta within the **confirmed** budget
(proposed ≤5ms p50 / ≤15ms p95, locked only after the baseline). **If unattainable on hubs,
escalate to the human** (relax / mandate OQ-03 opt-out / revisit default-on — do not silently
pass).

**`test_edge_queries_use_read_pool`** (NFR-3)
Assert the ranked select, count, and title join run on `read_pool_server`, not the write pool.

### R-19 — Corrected-entry transient (Low, expected)

**`test_corrected_entry_authored_fill_first_inferred_sparse`** (DNB-2)
`context_correct` an entry with authored + inferred edges; `context_get` → authored (carried
forward) fill slots first, inferred sparse by design. Encoded as **expected behavior**, not a
defect.

## Integration Expectations (through MCP)
- `test_get_surfaces_ranked_edges_default`, `test_get_edges_freshness_no_tick` (AC-01).
- `test_get_include_edges_opt_out` (R-14/AC-11).
- `test_get_zero_edge_is_success_not_error` (AC-14 zero-vs-failure, MCP side).
- `test_correct_then_get_authored_carry_forward` (lifecycle, DNB-2).
- `test_get_high_degree_node_caps_at_three` (R-04/R-13).
- The injected-failure RED test is a **server integration test** (no MCP failure-injection seam) —
  see OVERVIEW integration section.

## Edge Cases
- Opt-out skips the fail-loud path entirely (cannot reach AC-14 failure).
- Dangling + resolved mixed in one batched title join.
- Hub node: ≤3 displayed, honest totals.
- Corrected entry: authored win slots, inferred sparse (expected).

## Security
- Title `IN (…)` uses **positional binds** over the ≤3 displayed `target_id`s — never a
  string-built IN-list. Bounded to ≤3 (no chunking needed).
- Suppressed/quarantined target: confirm the title lookup does not leak a filtered target's title
  beyond the `target_id` already present in `graph_edges` (#4166 endpoint-guard awareness).
