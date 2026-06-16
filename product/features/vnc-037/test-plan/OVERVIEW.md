# vnc-037 — Test Strategy Overview

A **ranked, capped (≤3) next-hop affordance** on `context_get`. Under cap-3 every
canonicalization / ranking / classification defect is **1/3 of the visible affordance**
(SR-13), so the tests that matter here are **discriminating, not smoke**. This OVERVIEW maps
the 20 risks (4 Critical) to the test surface, names the integration harness plan, and points
each per-component file at its slice.

## Test Layers

| Layer | Where | What it proves |
|-------|-------|----------------|
| **Store unit / `#[sqlx::test]`** | `unimatrix-store` (`graph_queries_ranked.rs`, `read.rs`) | SQL correctness at the **store boundary**: canonicalization-before-rank-and-count, exact `ORDER BY`, LEFT JOIN NULLS-LAST, split COUNT, `LIMIT` bound to the constant, ≤3 rows returned (not Rust-sliced), positional binds. The Critical risks live here — the SQL is where they fail silently. |
| **Server unit** | `unimatrix-server` (`response/edges.rs`, `mcp/get_edges.rs`, `mcp/tools.rs`) | Projection (`→`/`←`/`↔`, far-endpoint `target_id`, `authored`), `EdgesView` assembly, 3-format render strings, `None ⇒ key absent` serializer invariant, fail-loud error mapping, opt-out skip. |
| **Integration (infra-001)** | `product/test/infra-001/suites/` | End-to-end through the MCP JSON-RPC binary: default-on surfacing, freshness, opt-out, byte-identity of list-view tools, neighbors-suite-unedited, fail-loud RED. |
| **Latency (manual/bench)** | store + server, high-degree fixture | AC-12 measured edge-free baseline + default-on delta (gated, escalates to human if unattainable). |

## Discipline rules applied to THIS feature (lessons)

- **#3886 (proof-outside-cap):** any `ORDER BY…LIMIT` ranking test MUST seed the discriminating
  value OUTSIDE the cap, with a LOWER edge weight, so a weight-instead-of-confidence or
  batch-local bug yields a **visibly different top-3**. Governs R-02. Tests seed
  `GET_EDGE_DISPLAY_LIMIT + N` edges and assert relative to the constant, never a literal 3/5.
- **#3645 (trace, don't intuit):** every ranking/canonicalization expected value carries an
  **explicit per-edge rank trace** in the component test plan (authored?, confidence, weight →
  derived slot). No intuited top-3.
- **#3621 (trace JOIN-heavy SQL):** the confidence LEFT JOIN + canonicalization query is traced
  against each named scenario before it is accepted.
- **#1268 (real producer):** byte-identity goldens are captured through the REAL serializer /
  tool handler, never hand-crafted strings.
- **#4876 (run it RED):** the fail-loud path (FR-19/AC-14) is exercised by an injected failure
  that actually returns an error — verified red, not reasoned.
- **#4166 (guard ALL passes):** the `source` column / canonicalization guards are asserted on
  every SELECT branch, not one.

## Risk → Test Mapping

| Risk | Priority | Primary tests | Component file |
|------|----------|---------------|----------------|
| R-01 symmetric canonicalization (display AND totals, before cap AND count; symmetric → `both` bucket, never `inbound`) | **Critical** | `canon_display_one_arrow_*`, `count_symmetric_counted_once` (in `both`), `count_symmetric_increments_both_never_inbound` (#744 inbound-integrity), `canon_before_cap_authored_wins`, `asymmetric_untouched` — display and totals asserted **independently**; order-of-ops asserted | store-ranked-query, store-split-count |
| R-02 ranking ORDER BY silently wrong | **Critical** | `rank_by_target_confidence_proof_outside_cap` (#3886, lower-weight high-conf target), `authored_priority_under_cap`, `inferred_fill_only_when_authored_lt_3`, `deterministic_tiebreak` — each with rank trace | store-ranked-query |
| R-03 split COUNT divergence / canon mismatch (3-bucket `{inbound,outbound,both}` + digest `authored` aggregate) | **Critical** | `count_uncapped_exact` (in+out+both), `count_canon_parity_with_rank`, `count_direction_split`, `count_symmetric_increments_both_never_inbound` (#744), `count_authored_aggregate_over_full_set`, `count_nested_shape_three_buckets` | store-split-count |
| R-04 rank-and-limit in Rust not SQL | **Critical** | `high_degree_returns_exactly_cap_rows` + `count_returns_two_scalars` at **store boundary**; SQL carries `LIMIT ?`/`COUNT(*)` not `SELECT *`+slice | store-ranked-query, store-split-count |
| R-05 carried-forward / context_edge misclassified | High | `carried_forward_classifies_authored` (FR-17 named), `context_edge_classifies_authored`, `mixed_corrected_authored_first` | store-neighbor-source, get-edge-assembly |
| R-06 confidence LEFT JOIN skews/drops | High | `dangling_target_retained_nulls_last`, `null_confidence_deterministic`, `cold_start_uniform_tiebreak`, `join_is_left_not_inner` | store-ranked-query |
| R-07 serializer byte-identity regression | High | `byte_identity_4_tools_x_3_formats` via real producer (#1268), `none_key_absent_structural` | serializer-seam |
| R-08 additive `source` breaks neighbors | High | `neighbors_suite_passes_unedited` (empirical), `map_edge_row_all_4_branches`, `no_canon_leak_into_neighbors` | store-neighbor-source |
| R-09 `authored` mislabel | High | `authored_only_for_agent`, `authored_exact_match_no_near_miss` | store-neighbor-source, get-edge-vocabulary |
| R-10 direction/`target_id` inverted | High | `direction_outbound_inbound_far_endpoint`, `symmetric_carries_both_no_arrow`, `projection_matches_edgerecord` | get-edge-assembly, get-edge-vocabulary |
| R-11 untraced JOIN-heavy query | High | enforced as a **plan discipline** — each R-01/R-02/R-06 case carries a written trace | store-ranked-query (traces embedded) |
| R-12 Supersedes leak | Medium | `supersedes_absent_display_and_totals` | store-ranked-query, store-split-count |
| R-13 latency budget unbacked | High | `measured_edge_free_baseline`, `default_on_delta_within_budget`, `read_pool_indexed_join` (manual/bench) | get-edge-assembly (latency section) |
| R-14 opt-out does not skip | High | `opt_out_zero_edge_queries`, `opt_out_no_edges_key`, `internal_caller_opt_out_*` (enumerated) | get-params, get-edge-assembly |
| R-15 dangling dropped/panics | Medium | `dangling_title_null_retained_all_formats`, `mixed_resolved_dangling_no_panic` | get-edge-assembly, serializer-seam |
| R-16 edge/title failure handling | Medium→AC-14 | `edge_query_failure_fails_loud` (RED), `count_failure_fails_loud`, `title_join_failure_fails_loud`, `zero_vs_failure_distinct`, `no_unwrap_on_edge_path` (grep) | get-edge-assembly |
| R-17 format-string drift (locked digest `↔{both} ({K} authored)`, 3-key `edge_totals`, `…N more` from in+out+both) | Medium | `summary_digest_locked_byte_form`, `markdown_flat_no_subsplit`, `json_edges_and_totals_shape` (3 keys), `capped_pointer_present` | serializer-seam |
| R-18 file-size breach | Low | `line_count_le_500` (file-check across touched/new files) | (cross-cutting; serializer-seam + store-ranked-query) |
| R-19 corrected-entry transient | Low | `corrected_entry_authored_fill_first_inferred_sparse` (expected, not a bug) | get-edge-assembly |
| R-20 future non-statistical source | Low | `authored_precondition_documented` (grep), `source_string_retained` | store-neighbor-source, get-edge-vocabulary |

All scope risks SR-01…SR-14 traced (RISK-TEST-STRATEGY traceability table). SR-08/09/10/12/14
receive Critical/High **discriminating** coverage.

## Cross-Component Test Dependencies

- **The two queries that must agree** (highest-probability silent defect): the ranked select
  (`store-ranked-query`) and the split COUNT (`store-split-count`) MUST apply the **same**
  canonicalization. R-01 is asserted on display from the rank query AND on totals from the count
  query, **independently** — a fix to render-dedup that misses count-dedup must fail the totals
  test. The order-of-ops test (canon BEFORE cap AND count) spans both.
- **The cap constant** (`store-display-cap-constant`) is referenced by `store-ranked-query` (SQL
  `LIMIT ?`) and `serializer-seam` (`…N more` threshold). The cap-isolation test overrides the
  constant and asserts **only** the rendered set changes — totals (`store-split-count`) and
  canonicalization (both queries) are byte-unchanged.
- **`authored` predicate parity:** the boolean projection (`get-edge-vocabulary`) and the
  `(source='agent')` rank term (`store-ranked-query`) must use the **same exact** match; R-09's
  near-miss test guards both.
- **Serializer `None ⇒ key absent`** (`serializer-seam`) is the precondition for byte-identity
  (R-07) — proven structurally, then end-to-end via the integration byte-identity golden.

---

## Integration Harness Plan (infra-001)

Reference: `product/test/infra-001/USAGE-PROTOCOL.md`. The harness drives the compiled
`unimatrix` binary over MCP JSON-RPC. vnc-037 touches **server tool logic**, **store/retrieval
behavior**, **schema-adjacent read path**, and **confidence**, so the suite selection table maps
to: `tools`, `protocol`, `lifecycle`, `edge_cases`, `confidence`, plus the mandatory `smoke`
gate. Tests seed `graph_edges` / `entries` via direct SQLite at `_compute_db_path(project_dir)`
(established pattern in `test_tools.py`) and assert through the MCP response.

### Suites that apply

| Suite | Why it applies | Run in 3c |
|-------|----------------|-----------|
| `smoke` (`-m smoke`) | Any change — **mandatory minimum gate** | YES (first) |
| `tools` | New `context_get` edge surfacing, `include_edges` param, all response formats, the new ranked path | YES |
| `protocol` | `context_get` response shape must stay JSON-RPC compliant with the new `edges`/`edge_totals` keys | YES |
| `lifecycle` | store→edge→get freshness (AC-01 no-tick-wait), correction-chain carry-forward (R-05/DNB-2), restart persistence of edges | YES |
| `edge_cases` | dangling target, zero-edge entry, empty-DB get, unicode titles in the batched title join | YES |
| `confidence` | the inferred rank key is `entries.confidence`; confirm the JOIN reads the live cached composite (re-rank after confidence change) | YES (targeted) |
| `volume` | high-degree node behavior at scale (R-04 / AC-12) — confirm a hub get returns ≤3 + totals, never the full fan-out | YES (targeted, high-degree case) |
| `security` | positional-bind / no-injection on edge queries (read-only blast radius); confirm suppressed-target title not leaked beyond `target_id` | YES (targeted) |
| `contradiction` | NOT a direct target, but `Contradicts` is a symmetric canon type — confirm the existing contradiction suite stays green (canon is get-only, must not affect contradiction scan) | regression-only |

### Existing-suite coverage vs gaps

**Already covered** by existing suites (assert they stay green): MCP handshake, `context_get`
existing fields, all-formats render skeleton, neighbors via `context_graph` (the **unedited**
neighbors regression — R-08/AC-09 is precisely "existing `context_graph` neighbors tests pass
with zero edits").

**Gaps requiring NEW integration tests** (behavior only visible through MCP, new to vnc-037):

New tests for `suites/test_tools.py` (extend the `context_get` block, ~line 311; seed edges via
`_compute_db_path`):

1. `test_get_surfaces_ranked_edges_default` — AC-01: default-on (no `include_edges`) surfaces
   depth-1 edges both directions; exact 5-field shape (AC-02).
2. `test_get_edges_freshness_no_tick` — AC-01: write an edge then get → appears immediately.
3. `test_get_include_edges_opt_out` — AC-11/R-14: `include_edges:false` → **no `edges` key**.
4. `test_get_symmetric_canonicalized_one_arrow` — R-01: a `Contradicts` reciprocal pair surfaces
   ONE `↔` edge in the displayed set; extend to `CoAccess`/`Informs`.
5. `test_get_edge_totals_symmetric_once` — R-01/R-03: 3-key `edge_totals`
   (`{inbound,outbound,both}`) counts the symmetric pair once in `both` with `inbound` unchanged
   (#744 inbound-integrity); separate assertion from #4. Optionally assert the summary digest's
   locked `↔{both} ({K} authored)` byte form end-to-end.
6. `test_get_authored_priority_under_cap` — R-02/AC-05a: ≥3 authored among >3 edges → only
   authored show.
7. `test_get_inferred_fill_when_authored_lt_3` — R-02/AC-05b.
8. `test_get_rank_by_target_confidence` — R-02/AC-05c through-the-MCP confirmation of the
   discriminating case (the proof-outside-cap discrimination is primarily a store unit test;
   this is the end-to-end echo).
9. `test_get_capped_pointer_when_more_than_cap` — AC-05e/AC-08: `…and N more — use context_graph`.
10. `test_get_zero_edge_empty_state_all_formats` — AC-06/DNB-3.
11. `test_get_dangling_title_null_retained` — AC-02/DNB-1.
12. `test_get_supersedes_absent` — AC-04.
13. `test_get_authored_flag_agent_vs_inferred` — AC-03.
14. `test_get_carried_forward_classifies_authored` — R-05/FR-17 (lifecycle-flavored; may live in
    `test_lifecycle.py`).

New test for `suites/test_lifecycle.py`:

15. `test_correct_then_get_authored_carry_forward` — DNB-2/R-19: post-`context_correct`, authored
    edges carried forward fill slots, inferred sparse — asserted as **expected**.

New byte-identity test (extend `test_tools.py` or `test_protocol.py`):

16. `test_list_view_tools_no_edges_key` — AC-07/R-07: `context_search`/`lookup`/`store`/`correct`
    responses carry **no** `edges` key / `### Related` / `edges:` digest. Captured via the real
    MCP response (the harness IS the real producer — satisfies #1268).

New high-degree test (`test_volume.py` or targeted in `test_tools.py`):

17. `test_get_high_degree_node_caps_at_three` — R-04/AC-12: seed ≥50 edges on one node; get
    returns ≤3 edges + honest uncapped totals. (Store-boundary "never materialized" proof is a
    Rust unit test; this is the MCP-visible confirmation.)

New fail-loud test — see note below.

### Fail-loud (AC-14/FR-19) — primarily a Rust unit/integration test

The injected-failure RED test (`edge_query_failure_fails_loud`) is **hard to express through the
black-box MCP harness** (no failure-injection seam over JSON-RPC). It is planned as a
**`unimatrix-server` integration test** (component `get-edge-assembly`) using a failing
pool/seam, run RED per #4876. The MCP harness contributes the complementary **zero-vs-failure
distinction**: `test_get_zero_edge_is_success_not_error` confirms a genuine zero-edge get is a
**success** with the explicit empty state — structurally distinct from an error response.

### New harness infrastructure

None beyond test functions. All new tests reuse `server` / `populated_server` / `admin_server`
fixtures and the `_compute_db_path` direct-SQLite seeding pattern (CLAUDE.md: test infra is
cumulative — extend, do not scaffold). Symmetric-edge seeding inserts **both** reciprocal
`graph_edges` rows to exercise canonicalization. If a reusable `seed_edge(db, src, tgt, type,
source, weight)` helper emerges, it is added to the existing `test_tools.py` edge-helper block
(near `_count_behavioral_edges`, ~line 2028), not a new module.

### No integration test needed for

- The cap-as-constant single-source check (AC-13a) — a **grep/file-check**, not MCP-visible.
- The store-boundary "≤3 rows allocated, full set never materialized" proof (R-04) — a Rust unit
  test at the query boundary; MCP only sees the rendered ≤3.
- Significant harness infra changes — none required.

---

## Self-Check
- Risk→test mapping covers all 20 risks and AC-01…AC-14 (see per-component files for assertions).
- Integration harness section names applicable suites, existing coverage, gaps, and new tests.
- The two-queries-must-agree dependency and the cap-isolation cross-component dependency are
  documented.
- Component plan files map 1:1 to the brief's Component Map.
