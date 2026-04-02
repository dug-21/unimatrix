# Test Plan Overview: crt-040 — Cosine Supports Edge Detection

## Overall Strategy

crt-040 spans four components across three delivery waves. The test strategy is risk-driven:
every test in every component file traces to a risk ID from RISK-TEST-STRATEGY.md. Testing
is organized in three levels:

1. **Unit tests** — all in `#[cfg(test)]` modules inside the modified files, using
   `#[tokio::test]` for async helpers. These cover individual functions in isolation.
2. **Integration tests** — via the infra-001 harness exercising the compiled binary through
   the MCP JSON-RPC interface. Focus on path isolation, tick ordering, and source-agnostic
   `supports_edge_count`.
3. **Eval gate** — `python product/research/ass-039/harness/run_eval.py` for AC-14 (MRR).

## Risk-to-Test Mapping

| Risk ID | Priority | Component | Test File(s) | Key Assertion |
|---------|----------|-----------|--------------|---------------|
| R-01 | Critical | path-c-loop | path-c-loop.md | HashMap lookup used (not DB), None branch continues without panic, disallowed category produces no edge |
| R-02 | High | write-graph-edge | write-graph-edge.md | `write_nli_edge` still writes `source='nli'`; `write_graph_edge` writes `source='cosine_supports'` |
| R-03 | High | inference-config | inference-config.md | Three independent assertions: impl Default, serde, backing fn all return 0.65 |
| R-04 | Medium | inference-config | inference-config.md | AC-17 grep gate + AC-18 forward-compat serde test |
| R-05 | Medium | path-c-loop | path-c-loop.md | AC-15: `inferred_edge_count` unchanged after cosine_supports write |
| R-06 | Medium | path-c-loop | path-c-loop.md | AC-19: observability log fires with zero counts when `candidate_pairs` empty |
| R-07 | Medium | write-graph-edge, path-c-loop | both | `write_graph_edge` returning `false` emits no warn, budget counter not incremented |
| R-08 | Low | path-c-loop | path-c-loop.md | Canonical `(lo,hi)` form: `(A,B)` and `(B,A)` in input → exactly one edge written |
| R-09 | Low | path-c-loop | path-c-loop.md | NaN/Inf cosine → no edge, `warn!` emitted, loop continues |
| R-10 | Medium | path-c-loop | path-c-loop.md | Performance gate: implementation must use `category_map` HashMap, not linear scan per pair |
| R-11 | Medium | path-c-loop | path-c-loop.md | Code review gate: tick function body line count; extract `run_cosine_supports_path` if needed |
| R-12 | High | (eval gate) | — | AC-14: MRR >= 0.2875 after delivery with Path C active |
| R-13 | Medium | inference-config | inference-config.md | Merge function test: project-level 0.70 overrides base 0.65 |

## Cross-Component Test Dependencies

- **store-constant → write-graph-edge**: `write_graph_edge` uses `EDGE_SOURCE_COSINE_SUPPORTS`
  as the `source` argument. The constant test (AC-08) must pass before the edge-writer test
  can assert the written `source` column value is `"cosine_supports"`.

- **inference-config → path-c-loop**: Path C's `config.supports_cosine_threshold` default
  (0.65) is exercised in tick tests. If the config default is wrong (R-03), Path C tests
  using `InferenceConfig::default()` will pass or fail at wrong thresholds.

- **write-graph-edge → path-c-loop**: Path C calls `write_graph_edge`. The write-graph-edge
  unit test must establish that the function correctly writes `source='cosine_supports'`
  before path-c-loop integration tests assert the tick populates `graph_edges` with that
  source value.

- **AC-06/AC-07 regression dependency**: existing Path A and Path B tests must pass
  unchanged. These are pre-existing — not new tests — but they serve as a regression gate
  for changes to `nli_detection.rs` and `config.rs`.

## Delivery Wave Test Ordering

| Wave | Components | Minimum tests before next wave |
|------|-----------|-------------------------------|
| Wave 1a | store-constant | AC-08 passes |
| Wave 1b | inference-config | AC-09, AC-10, AC-16, AC-17, AC-18 pass |
| Wave 2 | write-graph-edge | R-02 unit tests (write_nli_edge still writes `source='nli'`) pass |
| Wave 3 | path-c-loop | All R-01, R-06, R-07, R-09 unit tests + AC-12 budget cap pass |

---

## Integration Harness Plan (infra-001)

### Suite Selection

Based on the feature-touches mapping:

| Criterion | Applies? | Suites to run |
|-----------|----------|---------------|
| Any server tool logic | No — tick runs in background, not via MCP tool | — |
| Store/retrieval behavior | Indirect — `graph_edges` written by tick | `lifecycle`, `edge_cases` |
| Confidence system | No | — |
| Contradiction detection | No | — |
| Security | No | — |
| Schema or storage changes | No schema change; `graph_edges.source` column pre-exists | — |
| Any change at all | Yes — minimum gate always | `smoke` |

**Required suites for crt-040:**
- `smoke` — mandatory minimum gate (`pytest -m smoke --timeout=60`)
- `lifecycle` — multi-step flows; `context_status` returns `supports_edge_count > 0` after tick
- `tools` — verify existing tool behavior unchanged (AC-06/AC-07 regression)

### Existing Suite Coverage of crt-040 Risks

| Risk | Covered by existing suite? | Notes |
|------|---------------------------|-------|
| R-01 (category HashMap) | No — unit test only | HashMap is a tick-internal mechanism; not MCP-visible |
| R-02 (write_nli_edge source) | No — unit test only | source column not directly exposed via MCP tools |
| R-03 (impl Default/serde) | Partial — `tools` suite exercises InferenceConfig indirectly | config deserialization tested in unit tests |
| R-05 (inferred_edge_count backward compat) | Yes — `lifecycle` suite queries `context_status` | `inferred_edge_count` is in `GraphCohesionMetrics` returned by `context_status` |
| R-06 (observability log) | No — log output not visible via MCP | unit test with tracing subscriber |
| R-07 (collision false return) | Partial — `lifecycle` suite exercises multi-tick state | direct collision test requires NLI-enabled setup |
| AC-05 (Path C unconditional) | Yes — new integration test needed | see below |
| AC-12 (budget cap 50) | No — tick internals not exposed | unit test only |
| AC-14 (MRR) | No — eval harness is separate | `run_eval.py`, not pytest |

### New Integration Tests Required

The following scenarios are only observable through the MCP interface and require new tests in
the infra-001 harness. Add to `suites/test_lifecycle.py`:

**Test 1: `test_context_status_supports_edge_count_increases_after_tick`**
- Fixture: `shared_server` (state must persist across steps)
- Steps:
  1. Store two entries with categories `lesson-learned` and `decision`
  2. Wait for at least one background tick to complete (check `context_status` polling)
  3. Call `context_status`; assert `supports_edge_count > 0`
- Covers: NFR-05, R-05, AC-05 (Path C wrote edges without NLI)
- Note: this test depends on the server being built with Path C active. If tick timing is
  non-deterministic, use a longer timeout or retry. If test environment has no embeddings,
  mark `@pytest.mark.xfail(reason="no embedding model in CI")`.

**Test 2: `test_inferred_edge_count_unchanged_by_cosine_supports`**
- Fixture: `shared_server`
- Steps:
  1. Record baseline `inferred_edge_count` via `context_status`
  2. After a tick where Path C would write edges (qualifying pairs present), call `context_status` again
  3. Assert `inferred_edge_count` unchanged (backward compat, AC-15)
  4. Assert `supports_edge_count >= baseline` (Path C edges counted in this field)
- Covers: R-05, AC-15, NFR-06

These two tests require a live server with an active embedding pipeline to populate
`candidate_pairs`. If the CI environment does not have an embedding model loaded, both
should be marked `xfail` with a descriptive reason, not deleted.

**No new tests needed** in `test_tools.py` — crt-040 does not add or modify any MCP tools.
**No new tests needed** in `test_security.py` — the only new configurable value
(`supports_cosine_threshold`) is validated in unit tests; its blast radius is documented.
**No new tests needed** in `test_confidence.py` — no confidence pipeline changes.

### Integration Test Commands (Stage 3c)

```bash
# Build binary first
cargo build --release

# From product/test/infra-001/
cd product/test/infra-001

# Mandatory minimum gate
python -m pytest suites/ -v -m smoke --timeout=60

# Lifecycle suite (supports_edge_count, backward compat)
python -m pytest suites/test_lifecycle.py -v --timeout=60

# Tools suite (regression: AC-06/AC-07 Path A/B unchanged)
python -m pytest suites/test_tools.py -v --timeout=60
```
