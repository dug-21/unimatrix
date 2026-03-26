## ADR-005: Baseline Recording Is a Named Delivery Step with Snapshot Check

### Context

SCOPE.md goal 7 requires recording the current baseline CC@k and ICD in
`product/test/eval-baselines/log.jsonl`. This requires running `eval run`
against a snapshot of the production database using the new binary.

SCOPE.md open question 3 left ambiguous whether a current snapshot exists.
The eval baseline workflow requires a snapshot (not the live database) due to
the live-DB path guard in `run_eval` (ADR-001 of nan-007). If no snapshot
exists, the delivery agent must create one as part of this feature.

Without an explicit procedure, delivery agents may:
- Skip the baseline recording step entirely (treating it as implied post-work)
- Fail silently when no snapshot is available
- Use a stale snapshot from a previous feature cycle

### Decision

Baseline recording is a named acceptance criterion step (AC-09) that the
delivery agent must complete explicitly. The procedure is:

**Step 1 — Snapshot check**: Verify whether
`product/test/eval-baselines/` contains a snapshot `.db` file with a
modification time newer than the last log.jsonl entry's `date` field.
If a valid snapshot exists, proceed to step 3.

**Step 2 — Create snapshot (if absent)**: Run:
```
eval snapshot --db <live-db-path> --out product/test/eval-baselines/snap-nan-008.db
```
This is an authorized scope expansion as documented in SCOPE-RISK-ASSESSMENT.md
(SR-04 resolution).

**Step 3 — Run eval**: Run:
```
eval run \
  --db product/test/eval-baselines/snap-nan-008.db \
  --scenarios <scenarios-path> \
  --profiles <baseline-profile-toml> \
  --out /tmp/nan-008-baseline-results/
```

**Step 4 — Extract metrics**: Compute mean `cc_at_k` and mean `icd` from
the result JSON files in `/tmp/nan-008-baseline-results/`.

**Step 5 — Append to log.jsonl**: Append one JSON object with fields:
`date`, `scenarios` (count), `p_at_k`, `mrr`, `avg_latency_ms`, `cc_at_k`,
`icd`, `feature_cycle: "nan-008"`, `note: "initial CC@k and ICD baseline"`.

**Step 6 — Update README.md**: Add `cc_at_k` and `icd` to the field spec
table in `product/test/eval-baselines/README.md`.

This procedure must be completed before the PR is marked ready for review.
The baseline log entry is evidence that the new binary computes the metrics
end-to-end.

### Consequences

- Baseline recording is unambiguous and reproducible.
- The delivery agent cannot accidentally skip it, as AC-09 requires the
  log.jsonl entry to exist.
- If `eval snapshot` does not exist as a subcommand, the delivery agent must
  use the existing snapshot tooling (check `eval --help`) and record the
  exact command used in the PR description.
- The snapshot file created by this step may be reused by future features
  if its modification date is still current.
