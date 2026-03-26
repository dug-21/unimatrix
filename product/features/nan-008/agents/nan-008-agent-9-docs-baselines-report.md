# Agent Report: nan-008-agent-9-docs-baselines

## Task

Update documentation (docs/testing/eval-harness.md) with CC@k and ICD metric
descriptions, and record the initial nan-008 baseline in
product/test/eval-baselines/.

## Files Modified

- `/workspaces/unimatrix-nan-008/docs/testing/eval-harness.md`
- `/workspaces/unimatrix-nan-008/product/test/eval-baselines/README.md`
- `/workspaces/unimatrix-nan-008/product/test/eval-baselines/log.jsonl`

## Baseline Outcome

Recorded with real values from a live eval run.

**Procedure followed (ADR-005):**

1. Snapshot check: `/tmp/crt023-eval-snapshot.db` existed but was dated
   2026-03-20, older than the most recent log entry (2026-03-22 crt-024).
   A fresh snapshot was required.

2. Created new snapshot via:
   ```
   /workspaces/unimatrix-nan-008/target/release/unimatrix \
     --project-dir /workspaces/unimatrix snapshot --out /tmp/snap-nan-008.db
   ```
   Unimatrix daemon was confirmed running at pid 79725.

3. Extracted 3307 scenarios:
   ```
   unimatrix eval scenarios --db /tmp/snap-nan-008.db --out /tmp/nan-008-scenarios.jsonl
   ```

4. Ran eval with baseline profile:
   ```
   unimatrix eval run \
     --db /tmp/snap-nan-008.db \
     --scenarios /tmp/nan-008-scenarios.jsonl \
     --configs /tmp/nan-008-baseline.toml \
     --out /tmp/nan-008-baseline-results/ \
     --k 5
   ```

5. Extracted means from 3307 result JSON files:

   | Metric | Value |
   |--------|-------|
   | Scenarios | 3307 |
   | P@5 | 0.3058 |
   | MRR | 0.4181 |
   | Avg latency ms | 8.7 |
   | CC@5 | 0.2636 |
   | ICD | 0.5244 |

6. Appended to log.jsonl:
   ```json
   {"date":"2026-03-26","scenarios":3307,"p_at_k":0.3058,"mrr":0.4181,"avg_latency_ms":8.7,"cc_at_k":0.2636,"icd":0.5244,"feature_cycle":"nan-008","note":"initial CC@k and ICD baseline"}
   ```

## Documentation Changes Summary

### eval-harness.md

- "Understanding the metrics" section: added CC@k subsection with formula
  (intersection semantics, range [0.0, 1.0], CC@5 >= 0.7 target for PPR/phase
  features) and ICD subsection (raw Shannon entropy, range [0.0, ln(n)],
  comparability caveat, ln(n) annotation note).
- Updated current platform baseline table to reflect 2026-03-26 values including
  CC@5 and ICD.
- Updated result JSON example to show cc_at_k, icd, cc_at_k_delta, icd_delta
  fields with inline comments.
- Updated Step 5 "Record the baseline" echo example to include cc_at_k and icd
  fields.
- Updated full walkthrough echo example to include cc_at_k and icd fields.
- Updated "Reading the report" to say six sections (was five); added section 6
  Distribution Analysis description.

### eval-baselines/README.md

- Added cc_at_k and icd to the format example block with inline comments.
- Added a Field Specification table documenting all fields including cc_at_k
  and icd (type f64|null, ranges, null semantics for pre-nan-008 entries).

## Issues / Blockers

None. The eval snapshot, scenarios extraction, eval run, and metrics computation
all completed cleanly. The nan-008 binary already computed cc_at_k and icd fields
in the result JSON (other agents implemented this), so metric extraction was
straightforward.

## Knowledge Stewardship

- Queried: /uni-query-patterns for unimatrix-server eval harness -- not called
  (docs/baseline task; no Rust code patterns to query)
- Stored: nothing novel to store -- this task was documentation and baseline
  recording; no runtime gotchas discovered. The ADR-005 6-step procedure was
  clear and executed without surprises. Note that `eval snapshot` is NOT a
  subcommand of `eval` — it is the top-level `unimatrix snapshot` command;
  this matches what ADR-005 documents.
