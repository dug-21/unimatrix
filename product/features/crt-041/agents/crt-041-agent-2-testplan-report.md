# Agent Report: crt-041-agent-2-testplan

**Phase:** Test Plan Design (Stage 3a)
**Agent ID:** crt-041-agent-2-testplan
**Feature:** crt-041 — Graph Enrichment: S1, S2, S8 Edge Sources

---

## Output Files

All files written to `product/features/crt-041/test-plan/`:

| File | Lines | Purpose |
|------|-------|---------|
| `OVERVIEW.md` | 161 | Overall strategy, risk-to-test mapping, integration harness plan |
| `graph_enrichment_tick.md` | 328 | S1/S2/S8 unit + integration test expectations |
| `config.md` | 150 | InferenceConfig 5-field test expectations |
| `edge_constants.md` | 125 | EDGE_SOURCE constant value tests |
| `background.md` | 300 | Tick ordering, S8 gate, shell verification tests, integration specs |

---

## Risk Coverage Summary

| Risk ID | Priority | Coverage | Test Location |
|---------|----------|----------|--------------|
| R-01 | Critical | Full | unit: 6 tests (S1/S2/S8 both endpoint positions each) |
| R-02 | Critical | Full | unit: single-quote + double-dash vocabulary injection tests |
| R-03 | Critical | Full | unit: `test_inference_config_s1_s2_s8_defaults_match_serde` (BLOCKS DELIVERY) |
| R-04 | High | Full | unit: 1200-entry timing test ≤500ms (NFR-03) |
| R-05 | High | Full | unit: malformed JSON watermark test with row between two valid rows |
| R-06 | High | Full | unit: write ordering + crash-recovery idempotency simulation |
| R-07 | High | Full | unit: source value assertions in every S1/S2/S8 test; constant value tests |
| R-08 | High | Delivery gate | shell grep pre-flight (AC-28) |
| R-09 | Med | CLOSED | No test needed (compaction is source-agnostic, verified in RISK-TEST-STRATEGY.md) |
| R-10 | Med | Full | unit: pair-cap test + partial-row watermark semantics test |
| R-11 | Med | Full | unit: "api"/"capabilities" false-positive test + "cache"/"cached" + true-positive |
| R-12 | Med | Full | unit: briefing-operation exclusion + failed-search exclusion |
| R-13 | Med | Full | unit: `inferred_edge_count` excludes S1/S2/S8; integration xfail test |
| R-14 | Med | Full | unit: empty vocabulary no-op + no panic |
| R-15 | Low | Covered | integration: cohesion metrics readable without PPR rebuild |
| R-16 | Low | Delivery gate | `wc -l` check at PR review (AC-31) |
| R-17 | Med | Full | unit: validate() rejects 0 for all 4 numeric fields; accepts 1 as minimum |

**Total: 17 risks covered. R-09 CLOSED (no test needed).**

---

## Integration Suite Plan

**Suites to run in Stage 3c:** `smoke` (mandatory gate), `lifecycle`, `tools`

**New integration tests to add to `test_lifecycle.py`:**

| Test | Fixture | xfail? | Risk |
|------|---------|--------|------|
| `test_s1_edges_visible_in_status_after_tick` | `shared_server` | Yes — tick timing | R-07, AC-26 |
| `test_inferred_edge_count_unchanged_by_s1_s2_s8` | `shared_server` | Yes — tick timing | R-13, AC-30 |
| `test_quarantine_excludes_endpoint_from_graph_traversal` | `admin_server` | No | R-01, AC-03 |

The xfail tests are marked due to tick interval (15 min default) exceeding integration test
timeout — NOT due to ONNX model absence (S1/S2/S8 are pure SQL). Pattern #4045 documents this
distinction for future agents.

---

## Open Questions

1. **R-04 / NFR-03 timing test corpus construction:** The 1,200-entry S1 timing test requires
   inserting entries with distributed tags. Delivery agent should use a loop with direct sqlx
   inserts (not MCP tool calls) for speed. The exact tag distribution needed to produce ~50
   qualifying pairs (≥3 shared tags per pair) without excessive Cartesian product must be
   tuned — a suggested approach is 20 distinct tags, 5 tags per entry, random assignment.

2. **S8 partial-row watermark semantics:** ADR-003 specifies the watermark advances to the last
   FULLY-processed row's event_id. The unit test `test_s8_partial_row_watermark_semantics` must
   match the actual implementation's partial-row semantics exactly. Delivery agent must document
   the exact watermark value written when the cap is reached mid-row.

3. **`write_graph_edge` signature compatibility (R-08):** If crt-040 shipped `write_graph_edge`
   with a different parameter order or type, crt-041 call sites will fail to compile. Stage 3c
   tester must verify the exact function signature from `nli_detection.rs` matches the
   IMPLEMENTATION-BRIEF.md specification before reporting AC-28 as passed.

4. **Integration test tick timeout:** If the Stage 3c executor runs the integration tests and
   the tick-dependent tests time out even with the xfail markers, confirm the GH Issue for CI
   tick interval configuration. The test structure is correct; the xfail reason must be accurate.

---

## Self-Check Results

### Stage 3a Checklist
- [x] OVERVIEW.md maps risks from RISK-TEST-STRATEGY.md to test scenarios
- [x] OVERVIEW.md includes integration harness plan — which suites to run (`smoke`, `lifecycle`, `tools`), 3 new tests specified with exact function names and fixtures
- [x] Per-component test plans match architecture component boundaries (4 components: graph_enrichment_tick, config, edge_constants, background)
- [x] Every high-priority risk has at least one specific test expectation
- [x] Integration tests defined for component boundaries (background.md §Integration Test Expectations)
- [x] All output files within `product/features/crt-041/test-plan/`
- [x] Knowledge Stewardship report block included below

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — found #4031 (ADR-001 module structure), #4035 (ADR-005 dual-maintenance), #3822 (promotion tick idempotency), #3913 (quarantine guard production bug). All directly relevant.
- Queried: `context_search` for crt-041 ADRs — found #4031, #4034, #4035. Retrieved full content of all three.
- Queried: `context_search` for background tick integration test patterns — found #3822 (near-threshold oscillation), #3913 (quarantine compaction ADR).
- Stored: entry #4045 "SQL-only background tick integration tests: xfail on timing not model absence" via `/uni-store-pattern` — novel pattern distinguishing timing-based xfail from model-absence xfail, important for future pure-SQL tick features.
