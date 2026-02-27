# Pseudocode: C6 Status Extension

## Purpose

Extend StatusReport with 10 coherence fields. Extend format_status_report for all three formats.

## Files Modified

- `crates/unimatrix-server/src/response.rs`

## StatusReport New Fields

```
pub coherence: f64,                           // Composite lambda [0.0, 1.0]
pub confidence_freshness_score: f64,          // Dimension 1
pub graph_quality_score: f64,                 // Dimension 2
pub embedding_consistency_score: f64,         // Dimension 3 (1.0 if not checked)
pub contradiction_density_score: f64,         // Dimension 4
pub stale_confidence_count: u64,              // Entries with stale confidence
pub confidence_refreshed_count: u64,          // Entries refreshed this call
pub graph_stale_ratio: f64,                   // Stale node ratio
pub graph_compacted: bool,                    // Compaction ran?
pub maintenance_recommendations: Vec<String>, // Actionable recs
```

Default values: scores=1.0, counts=0, ratio=0.0, compacted=false, recs=empty vec.

All StatusReport construction sites (production + tests) need updating. The Rust compiler will flag all of them since struct initialization requires all fields.

## Summary Format Addition

```
"Coherence: {coherence:.4} (confidence_freshness: {:.4}, graph_quality: {:.4}, embedding_consistency: {:.4}, contradiction_density: {:.4})"
"Stale confidence: {count} entries" (if count > 0)
"Confidence refreshed: {count} entries" (if count > 0)
"Graph stale ratio: {ratio:.2}%" (if ratio > 0)
"Graph compacted: yes" (if true)
"Recommendation: {rec}" (for each rec)
```

## Markdown Format Addition

```
## Coherence
- **Lambda**: {:.4}
- **Confidence Freshness**: {:.4}
- **Graph Quality**: {:.4}
- **Embedding Consistency**: {:.4}
- **Contradiction Density**: {:.4}

Stale confidence entries: {count}
Confidence refreshed: {count}
Graph stale ratio: {:.2}%
Graph compacted: yes/no

### Maintenance Recommendations (if non-empty)
- {rec1}
- {rec2}
```

## JSON Format Addition

Add all 10 fields as top-level keys. f64 values at full precision.

## Key Test Scenarios

1. JSON: all 10 fields present with f64 precision (R-12)
2. Markdown: Coherence section with lambda + dimensions (R-12)
3. Summary: coherence line with all scores (R-12)
4. Recommendations in all formats when lambda < 0.8
5. No recommendations when lambda >= 0.8
6. graph_compacted renders correctly
7. f64 JSON values have no f32 artifacts
