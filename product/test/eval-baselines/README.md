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
  "feature_cycle":  "bugfix-323",       // the PR/feature context for this run
  "note":           "..."               // short human description
}
```

## Rules

- Append; never edit past entries.
- Run before **and** after any intelligence change (model swap, weight tuning,
  ranking logic) so regressions are visible as adjacent rows.
- Never commit snapshot `.db` files — only the aggregated metrics row.
- `k` defaults to 5. If you change `--k`, record the value used in `note`.
