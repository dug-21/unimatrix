# Test Plan: C6 Status Extension

## Component

C6: Status Extension (`crates/unimatrix-server/src/response.rs`)

## Risks Covered

| Risk | Description | Priority |
|------|-------------|----------|
| R-12 | StatusReport coherence section missing or malformed | Med |

## Unit Tests (response.rs)

### UT-C6-01: JSON format includes all 10 coherence fields
- Construct StatusReport with known coherence values
- Format as JSON
- Parse JSON output
- Assert all 10 fields present: coherence, confidence_freshness_score, graph_quality_score, embedding_consistency_score, contradiction_density_score, stale_confidence_count, confidence_refreshed_count, graph_stale_ratio, graph_compacted, maintenance_recommendations
- Assert f64 values serialized without f32 artifacts
- Covers: R-12 scenario 1, R-12 scenario 6

### UT-C6-02: JSON f64 precision verification
- Set coherence=0.845 (exact in f64)
- Format as JSON
- Assert JSON contains "0.845" not "0.8450000286102295" (f32 artifact)
- Covers: R-12 scenario 6

### UT-C6-03: Markdown format includes coherence section
- Construct StatusReport with coherence=0.75, all dimension scores set
- Format as Markdown
- Assert output contains "## Coherence" section
- Assert output contains "Lambda", all 4 dimension score labels
- Assert dimension scores formatted to 4 decimal places
- Covers: R-12 scenario 2

### UT-C6-04: Summary format includes coherence line
- Construct StatusReport with known coherence values
- Format as Summary
- Assert output contains "Coherence:" line with lambda and all dimension breakdowns
- Covers: R-12 scenario 3

### UT-C6-05: Recommendations present in all formats when lambda < threshold
- Construct StatusReport with maintenance_recommendations=["rec1", "rec2"]
- Format as JSON: assert "maintenance_recommendations" array with 2 elements
- Format as Markdown: assert "### Maintenance Recommendations" section
- Format as Summary: assert "Recommendation:" lines
- Covers: R-12 scenario 4

### UT-C6-06: Recommendations absent when lambda >= threshold
- Construct StatusReport with empty maintenance_recommendations
- Format as JSON: assert "maintenance_recommendations" is empty array
- Format as Markdown: assert no "Maintenance Recommendations" section
- Format as Summary: assert no "Recommendation:" lines
- Covers: R-12 scenario 5

### UT-C6-07: graph_compacted renders correctly
- Construct StatusReport with graph_compacted=true
- Format as Summary: assert "Graph compacted: yes"
- Format as Markdown: assert "Graph compacted: yes"
- Format as JSON: assert "graph_compacted": true
- Construct with graph_compacted=false:
- Format as Summary: no "Graph compacted" line (or "no")
- Covers: R-12 scenario 7

### UT-C6-08: Stale confidence count rendering
- Construct StatusReport with stale_confidence_count=15
- Summary: assert "Stale confidence: 15 entries"
- Markdown: assert "Stale confidence entries: 15"
- stale_confidence_count=0: assert no stale confidence line in summary

### UT-C6-09: Confidence refreshed count rendering
- Construct StatusReport with confidence_refreshed_count=50
- Summary: assert "Confidence refreshed: 50 entries"
- Markdown: assert "Confidence refreshed: 50"
- confidence_refreshed_count=0: assert no refreshed line in summary

### UT-C6-10: Graph stale ratio rendering
- Construct StatusReport with graph_stale_ratio=0.15
- Summary: assert "Graph stale ratio: 15.00%"
- Markdown: assert "Graph stale ratio: 15.00%"
- graph_stale_ratio=0.0: assert no stale ratio line in summary

### UT-C6-11: Default StatusReport coherence values
- Construct StatusReport with default coherence values
- Assert: coherence=1.0 (or whatever default), all dimension scores=1.0, counts=0, ratio=0.0, compacted=false, recommendations=empty
- Verifies that all construction sites can use defaults

## Existing Test Updates

### StatusReport construction sites
- All existing tests that construct StatusReport must add the 10 new fields
- The Rust compiler will flag every missing construction site
- Use consistent defaults: scores=1.0, counts=0, ratio=0.0, compacted=false, recs=vec![]
- Estimated: 20-30 existing test construction sites need updating

## Dependencies

- C4 (coherence module): CoherenceWeights and score types defined there
- StatusReport struct changes must be made before any test can compile

## Estimated Test Count

- 11 new unit tests
- 20-30 existing test updates (mechanical: add new fields to construction sites)
