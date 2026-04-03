# Test Plan Overview: crt-041 — Graph Enrichment (S1, S2, S8 Edge Sources)

## Overall Test Strategy

crt-041 introduces three new SQL-only background tick edge sources (S1, S2, S8) plus five
new InferenceConfig fields and three EDGE_SOURCE constants. All behavior is internal —
no new MCP tool is exposed. The primary testing surface is:

1. **Unit tests** in `crates/unimatrix-server/src/services/graph_enrichment_tick.rs`
   and `crates/unimatrix-server/src/infra/config.rs` — exercising SQL correctness, weight
   formulas, quarantine guards, early-return paths, and config validation.
2. **Unit tests** in `crates/unimatrix-store/src/read.rs` — verifying constant values.
3. **Integration tests** added to `product/test/infra-001/suites/test_lifecycle.py` —
   verifying MCP-visible behavior after a tick cycle (graph edge counts change, inferred_edge_count
   backward compat, quarantine exclusion visible through context_status).

The feature is NOT observable through any new MCP tool parameter. Integration tests must
exercise existing tools (`context_status`, `context_search`, `context_quarantine`) and inspect
the effects reported through status metrics and search exclusion.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Description | Test Location | Test Function(s) |
|---------|----------|-------------|---------------|-----------------|
| R-01 | Critical | Dual-endpoint quarantine guard missing | unit: graph_enrichment_tick | `test_s1_excludes_quarantined_source`, `test_s1_excludes_quarantined_target`, `test_s2_excludes_quarantined_endpoint`, `test_s8_excludes_quarantined_endpoint` |
| R-02 | Critical | S2 SQL injection via vocabulary term | unit: graph_enrichment_tick | `test_s2_sql_injection_single_quote`, `test_s2_sql_injection_double_dash` |
| R-03 | Critical | InferenceConfig dual-site default divergence | unit: config | `test_inference_config_s1_s2_s8_defaults_match_serde` |
| R-04 | High | S1 GROUP BY full materialization at large corpus | unit: graph_enrichment_tick | `test_s1_tick_completes_within_500ms_at_1200_entries` |
| R-05 | High | S8 watermark stuck on malformed JSON row | unit: graph_enrichment_tick | `test_s8_watermark_advances_past_malformed_json_row` |
| R-06 | High | S8 watermark written before edge writes | unit: graph_enrichment_tick | `test_s8_watermark_written_after_edges` |
| R-07 | High | S1/S2/S8 edges tagged source='nli' | unit: edge_constants + graph_enrichment_tick | `test_edge_source_constants_values`, `test_s1_edges_have_source_s1` |
| R-08 | High | crt-040 prerequisite absent | delivery gate | shell grep pre-flight (AC-28) |
| R-09 | Med | Orphaned edges after tag change (CLOSED) | none required | See R-09 resolution in RISK-TEST-STRATEGY.md |
| R-10 | Med | S8 cap on rows not pairs | unit: graph_enrichment_tick | `test_s8_pair_cap_not_row_cap`, `test_s8_partial_row_watermark_semantics` |
| R-11 | Med | S2 false-positive substring match | unit: graph_enrichment_tick | `test_s2_no_false_positive_capabilities_for_api`, `test_s2_no_false_positive_cached_for_cache` |
| R-12 | Med | S8 processes briefing or failed search rows | unit: graph_enrichment_tick | `test_s8_excludes_briefing_operation`, `test_s8_excludes_failed_search` |
| R-13 | Med | inferred_edge_count counts S1/S2/S8 edges | unit + integration | `test_inferred_edge_count_excludes_s1_s2_s8`, integration: test_lifecycle |
| R-14 | Med | S2 with empty vocabulary errors | unit: graph_enrichment_tick | `test_s2_empty_vocabulary_is_noop` |
| R-15 | Low | Eval gate before TypedGraphState::rebuild | integration: test_lifecycle | `test_cohesion_metrics_readable_without_ppr_rebuild` |
| R-16 | Low | File size violation | delivery gate | `wc -l` check at PR review |
| R-17 | Med | validate() missing range check for zero-value fields | unit: config | `test_inference_config_validate_rejects_zero_s1_cap`, `test_inference_config_validate_rejects_zero_s8_interval` |

---

## Cross-Component Test Dependencies

- `edge_constants` tests must pass before `graph_enrichment_tick` tests can assert
  source values — the constants are imported by tick functions.
- `config` tests must pass before `background` tests — the tick receives `InferenceConfig`
  and relies on validated field values.
- `graph_enrichment_tick` unit tests are prerequisite to meaningful integration tests —
  the integration tests verify the tick orchestration, not the SQL logic.

---

## Integration Harness Plan

### Which Existing Suites Apply

| Suite | Relevance | Rationale |
|-------|-----------|-----------|
| `smoke` | **Mandatory minimum gate** | Any change at all; confirms server starts, tools respond |
| `lifecycle` | **Run** | Graph edge count changes visible via context_status; quarantine interaction; inferred_edge_count backward compat |
| `tools` | **Run** | context_status and context_quarantine are used to verify S1/S2/S8 effects |
| `edge_cases` | **Optional** | No new edge-case surface; existing coverage sufficient |
| `security` | **Not applicable** | S2 SQL injection is tested at unit level (no MCP parameter surface) |
| `confidence` | **Not applicable** | Confidence scoring is not affected by S1/S2/S8 |
| `contradiction` | **Not applicable** | No contradiction logic changes |
| `volume` | **Not applicable** | Volume tests exercise search/store scale; no direct graph-edge coverage |

**Suites to run in Stage 3c:** `smoke`, `lifecycle`, `tools`

### Gaps in Existing Suite Coverage

The existing `test_lifecycle.py` suite has two graph-edge related tests for crt-040
(Path C cosine Supports edges). Both are marked `@pytest.mark.xfail` because CI has no
ONNX model. crt-041 S1/S2/S8 are pure SQL with no ONNX dependency, so new tests for
them should NOT be marked xfail.

**Existing coverage that applies to crt-041:**
- `test_inferred_edge_count_unchanged_by_cosine_supports` — validates the backward-compat
  invariant that `inferred_edge_count` counts only `source='nli'`. crt-041 inherits this
  coverage (already xfail due to embedding model absence, but the assertion logic is correct).

**New integration tests needed** (to be added to `test_lifecycle.py`):

| Test Name | Suite File | Fixture | Risk Coverage |
|-----------|-----------|---------|--------------|
| `test_s1_edges_visible_in_status_after_tick` | test_lifecycle.py | `shared_server` | R-07, R-13, AC-26 |
| `test_inferred_edge_count_unchanged_by_s1_s2_s8` | test_lifecycle.py | `shared_server` | R-13, AC-30 |
| `test_quarantine_excludes_endpoint_from_s1_edges` | test_lifecycle.py | `server` (via admin_server) | R-01, AC-03 |

These three tests are the minimum MCP-visible validation that S1/S2/S8 work correctly:
1. S1 edges appear in graph metrics after a tick (proves the tick runs and writes).
2. inferred_edge_count does not change when S1/S2/S8 edges are written.
3. Quarantining an entry removes it from edge endpoints.

**Note on xfail requirement:** All three new integration tests depend on background tick
execution. Since the tick runs in CI (SQL-only, no ONNX), these tests should NOT be
xfail unless the shared_server fixture timeout is insufficient to see a tick fire. If
the tick interval is too long for the integration test timeout, mark them xfail with
a clear reason and file a GH Issue for CI tick interval configuration.

### New Integration Tests — Specification

```python
# test_lifecycle.py additions

@pytest.mark.xfail(
    reason="No background tick in integration harness — server starts but tick "
    "interval (15 min default) exceeds test timeout. Test validates MCP-visible "
    "S1 edge count increase after one complete tick."
)
def test_s1_edges_visible_in_status_after_tick(shared_server):
    """crt-041 AC-26/R-07: S1 edges appear in graph_edges after tick runs.

    Stores two entries with shared tags, waits for tick, asserts
    cross_category_edge_count or total edge count increases.
    Cannot directly observe source='S1' through MCP — but if count
    increases while inferred_edge_count is unchanged, S1/S2/S8 are
    the source.
    """

def test_inferred_edge_count_unchanged_by_s1_s2_s8(shared_server):
    """crt-041 AC-30/R-13: inferred_edge_count counts only source='nli' after crt-041.

    Baseline inferred_edge_count; store entries that would qualify for S1/S2;
    wait for tick; assert inferred_edge_count unchanged (S1/S2/S8 edges are
    NOT counted in this field).
    """

def test_quarantine_excludes_endpoint_from_s1_edges(admin_server):
    """crt-041 AC-03/R-01: quarantined entry does not appear as S1 edge endpoint.

    Store two entries sharing tags. Quarantine one. Verify through context_search
    that the quarantined entry is not returned (indirect evidence the entry is
    excluded from graph traversal). Direct S1 edge table inspection not available
    through MCP — but quarantine exclusion from search confirms the guard works.
    """
```

### Smoke Test Verification

The smoke test gate (`pytest -m smoke`) does not directly exercise S1/S2/S8 behavior
but confirms the server starts successfully with the new InferenceConfig fields loaded
(default values). Any misconfigured field that panics at startup will fail the smoke
gate. This is the minimum coverage gate before detailed suite runs.

---

## Test File Locations

| Component | Unit Test Location |
|-----------|-------------------|
| graph_enrichment_tick | `crates/unimatrix-server/src/services/graph_enrichment_tick.rs` (or `..._tests.rs` if split) |
| config | `crates/unimatrix-server/src/infra/config.rs` (existing `tests` mod) |
| edge_constants | `crates/unimatrix-store/src/read.rs` (existing `tests` mod) |
| background | `crates/unimatrix-server/src/background.rs` (existing `tests` mod) |
| integration | `product/test/infra-001/suites/test_lifecycle.py` |
