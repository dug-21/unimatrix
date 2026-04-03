# Eval Baselines

Append one JSON object to `log.jsonl` after every eval run. This log is the
persistent record of platform retrieval quality over time.

## Format

```jsonc
{
  "date":           "YYYY-MM-DD",        // date of the run
  "scenarios":      1528,                // number of scenarios replayed
  "p_at_k":         0.3256,             // precision at K (K=5 default)
  "mrr":            0.4466,             // mean reciprocal rank
  "avg_latency_ms": 7.2,               // average query latency
  "cc_at_k":        0.2636,            // mean category coverage at K [0.0, 1.0]
  "icd":            0.5244,            // mean intra-query category diversity (raw Shannon entropy)
  "feature_cycle":  "bugfix-323",       // the PR/feature context for this run
  "note":           "..."               // short human description
}
```

## Field Specification

| Field | Type | Description |
|-------|------|-------------|
| `date` | string (YYYY-MM-DD) | Date of the eval run |
| `scenarios` | integer | Number of scenarios replayed |
| `p_at_k` | f64 | Mean precision at K (K=5 default) |
| `mrr` | f64 | Mean reciprocal rank |
| `avg_latency_ms` | f64 | Mean query latency in milliseconds |
| `cc_at_k` | f64 \| null | Mean category coverage at K; range [0.0, 1.0]; null if not computed |
| `icd` | f64 \| null | Mean intra-query category diversity (raw Shannon entropy); range [0.0, ln(n)]; null if not computed |
| `feature_cycle` | string | Feature or PR cycle identifier for this run |
| `profile` | string | (optional) Profile name if non-baseline profile |
| `snapshot_hash` | string | (optional) First 12 hex chars of the SHA-256 of the snapshot DB used for the eval run |
| `scenarios_date` | string | (optional ISO 8601) The `generated_at` timestamp from `scenarios_meta.json` — identifies the DB state the scenarios were generated from |
| `note` | string | Short human description of the run |

## Rules

- Append; never edit past entries.
- Run before **and** after any intelligence change (model swap, weight tuning,
  ranking logic) so regressions are visible as adjacent rows.
- Never commit snapshot `.db` files — only the aggregated metrics row.
- `k` defaults to 5. If you change `--k`, record the value used in `note`.
