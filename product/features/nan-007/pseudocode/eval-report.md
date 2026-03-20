# Pseudocode: eval/report.rs (D4)

**Location**: `crates/unimatrix-server/src/eval/report.rs`

## Purpose

Read per-scenario JSON result files from an `--results` directory, aggregate across all
scenarios, and write a Markdown report with five required sections. The report is a
human-reviewed artifact — it does not exit non-zero based on regression count (C-07,
FR-29, SR-06).

This module is entirely synchronous: pure filesystem reads and string formatting.
No database, no sqlx, no tokio runtime, no async. Dispatched directly in the sync
branch of `run_eval_command` (no `block_export_sync` needed).

## Dependencies

| Dependency | Location | Role |
|------------|----------|------|
| `ScenarioResult`, `ProfileResult`, `ComparisonMetrics`, `RankChange` | `eval/runner.rs` | Result JSON schema |
| `ScenarioRecord` | `eval/scenarios.rs` | Optional: annotate queries from scenarios.jsonl |
| `serde_json` | serde_json | Result file deserialization |
| `std::fs`, `std::io` | stdlib | File reading and writing |
| `std::collections::HashMap` | stdlib | Aggregation structures |

## Internal Aggregate Types

```
struct AggregateStats {
    profile_name: String,
    scenario_count: usize,
    mean_p_at_k: f64,
    mean_mrr: f64,
    mean_latency_ms: f64,
    p_at_k_delta: f64,     -- mean delta vs. baseline
    mrr_delta: f64,        -- mean delta vs. baseline
    latency_delta_ms: f64, -- mean delta vs. baseline
}

struct RegressionRecord {
    scenario_id: String,
    query: String,       -- from scenario_id → scenarios JSONL lookup if provided
    profile_name: String,
    baseline_mrr: f64,
    candidate_mrr: f64,
    baseline_p_at_k: f64,
    candidate_p_at_k: f64,
    reason: String,  -- "MRR dropped" | "P@K dropped" | "both dropped"
}

struct LatencyBucket {
    le_ms: u64,     -- upper bound in ms: 50, 100, 200, 500, 1000, 2000, inf
    count: usize,
}
```

## Function: `pub fn run_report`

```
pub fn run_report(
    results: &Path,
    scenarios: Option<&Path>,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>>

BODY:
  1. Load all scenario result JSON files from --results directory:
       result_files = std::fs::read_dir(results)?
         .filter_map(|e| e.ok())
         .filter(|e| e.path().extension() == Some("json".as_ref()))
         .map(|e| e.path())
         .collect::<Vec<_>>()

       if result_files.is_empty():
         -- Empty results directory is valid; produce report with empty-list indicators
         eprintln!("WARN: no result JSON files found in {}", results.display())

  2. Deserialize each result file (skip malformed files with WARN):
       scenario_results: Vec<ScenarioResult> = Vec::new()
       for path in &result_files:
         content = match std::fs::read_to_string(&path):
           Ok(c)  → c
           Err(e) → {
             eprintln!("WARN: skipping {} (read error: {e})", path.display())
             continue
           }
         result = match serde_json::from_str::<ScenarioResult>(&content):
           Ok(r)  → r
           Err(e) → {
             eprintln!("WARN: skipping {} (parse error: {e})", path.display())
             continue
           }
         scenario_results.push(result)

  3. Load scenarios JSONL for query text annotation (optional):
       query_map: HashMap<String, String> = HashMap::new()  -- id → query text
       if let Some(scenarios_path) = scenarios:
         query_map = load_scenario_query_map(scenarios_path)?

  4. Aggregate:
       aggregate_stats = compute_aggregate_stats(&scenario_results)
       regressions = find_regressions(&scenario_results, &query_map)
       latency_buckets = compute_latency_buckets(&scenario_results)
       entry_rank_changes = compute_entry_rank_changes(&scenario_results)

  5. Render Markdown:
       md = render_report(
         &aggregate_stats,
         &scenario_results,
         &regressions,
         &latency_buckets,
         &entry_rank_changes,
         &query_map,
       )

  6. Write output file:
       out_file = File::create(out)?
       out_file.write_all(md.as_bytes())?

  7. eprintln!("eval report: written to {}", out.display())

  8. return Ok(())
  -- NOTE: Never return Err based on regression count (C-07, FR-29)
```

## Function: `fn compute_aggregate_stats` (private)

```
fn compute_aggregate_stats(
    results: &[ScenarioResult],
) -> Vec<AggregateStats>

BODY:
  if results.is_empty():
    return Vec::new()

  -- Collect all profile names from first result (all results share same profile set):
  profile_names: Vec<String> = results[0].profiles.keys().cloned().collect()
    -- Sort for deterministic output order; baseline (first profile) should appear first.
    -- Convention: alphabetical sort, but with any profile named "baseline" forced first.
    -- IMPLEMENTATION NOTE: Use the iteration order from profiles.keys() if insertion-
    --   ordered HashMap is not used; otherwise sort explicitly.

  -- Identify baseline: first profile in sorted order, or the one named "baseline" if present
  baseline_name = profile_names.iter()
                    .find(|n| n.to_lowercase() == "baseline")
                    .cloned()
                    .unwrap_or_else(|| profile_names[0].clone())

  stats: Vec<AggregateStats> = Vec::new()

  for profile_name in &profile_names:
    p_at_k_sum = 0.0
    mrr_sum = 0.0
    latency_sum = 0.0
    p_at_k_delta_sum = 0.0
    mrr_delta_sum = 0.0
    latency_delta_sum = 0.0
    count = 0

    for result in results:
      if let Some(prof_result) = result.profiles.get(profile_name):
        p_at_k_sum += prof_result.p_at_k
        mrr_sum += prof_result.mrr
        latency_sum += prof_result.latency_ms as f64
        if profile_name != &baseline_name:
          p_at_k_delta_sum += result.comparison.p_at_k_delta
          mrr_delta_sum += result.comparison.mrr_delta
          latency_delta_sum += result.comparison.latency_overhead_ms as f64
        count += 1

    if count > 0:
      stats.push(AggregateStats {
        profile_name: profile_name.clone(),
        scenario_count: count,
        mean_p_at_k: p_at_k_sum / count as f64,
        mean_mrr: mrr_sum / count as f64,
        mean_latency_ms: latency_sum / count as f64,
        p_at_k_delta: if profile_name == &baseline_name: 0.0 else: p_at_k_delta_sum / count as f64,
        mrr_delta: if profile_name == &baseline_name: 0.0 else: mrr_delta_sum / count as f64,
        latency_delta_ms: if profile_name == &baseline_name: 0.0 else: latency_delta_sum / count as f64,
      })

  return stats
```

## Function: `fn find_regressions` (private)

```
fn find_regressions(
    results: &[ScenarioResult],
    query_map: &HashMap<String, String>,
) -> Vec<RegressionRecord>

BODY:
  regressions: Vec<RegressionRecord> = Vec::new()

  for result in results:
    baseline_profile = result.profiles.values().next()  -- first profile = baseline
    baseline_name = result.profiles.keys().next().map(|s| s.as_str()).unwrap_or("baseline")

    for (profile_name, prof_result) in &result.profiles:
      if profile_name.as_str() == baseline_name:
        continue  -- skip baseline vs. baseline comparison

      baseline_result = match result.profiles.get(baseline_name):
        Some(r) → r
        None    → continue  -- no baseline; skip

      -- OR semantics: regression if MRR OR P@K is lower (R-12, RISK-TEST-STRATEGY)
      mrr_regressed   = prof_result.mrr   < baseline_result.mrr
      p_at_k_regressed = prof_result.p_at_k < baseline_result.p_at_k

      if mrr_regressed || p_at_k_regressed:
        reason = match (mrr_regressed, p_at_k_regressed):
          (true, true)  → "both MRR and P@K dropped"
          (true, false) → "MRR dropped"
          (false, true) → "P@K dropped"
          _             → unreachable!()

        query_text = query_map.get(&result.scenario_id)
                       .cloned()
                       .unwrap_or_else(|| result.query.clone())

        regressions.push(RegressionRecord {
          scenario_id: result.scenario_id.clone(),
          query: query_text,
          profile_name: profile_name.clone(),
          baseline_mrr: baseline_result.mrr,
          candidate_mrr: prof_result.mrr,
          baseline_p_at_k: baseline_result.p_at_k,
          candidate_p_at_k: prof_result.p_at_k,
          reason,
        })

  -- Sort by MRR delta (worst regression first):
  regressions.sort_by(|a, b| {
    let delta_a = a.baseline_mrr - a.candidate_mrr
    let delta_b = b.baseline_mrr - b.candidate_mrr
    delta_b.partial_cmp(&delta_a).unwrap_or(std::cmp::Ordering::Equal)
  })

  return regressions
```

## Function: `fn render_report` (private)

```
fn render_report(
    stats: &[AggregateStats],
    results: &[ScenarioResult],
    regressions: &[RegressionRecord],
    latency_buckets: &[LatencyBucket],
    entry_rank_changes: &EntryRankSummary,
    query_map: &HashMap<String, String>,
) -> String

BODY:
  md = String::new()

  -- Title:
  md += "# Unimatrix Eval Report\n\n"
  md += format!("Generated: {timestamp} | Scenarios: {n}\n\n", ...)

  -- SECTION 1: Summary Table (FR-27 item 1):
  md += "## 1. Summary\n\n"
  md += "| Profile | P@K | MRR | Avg Latency (ms) | ΔP@K | ΔMRR | ΔLatency (ms) |\n"
  md += "|---------|-----|-----|-----------------|------|------|---------------|\n"
  for stat in stats:
    delta_p = if stat.p_at_k_delta == 0.0: "—".to_string() else: format!("{:+.4}", stat.p_at_k_delta)
    delta_mrr = if stat.mrr_delta == 0.0: "—".to_string() else: format!("{:+.4}", stat.mrr_delta)
    delta_lat = if stat.latency_delta_ms == 0.0: "—".to_string() else: format!("{:+.1}", stat.latency_delta_ms)
    md += format!(
      "| {} | {:.4} | {:.4} | {:.1} | {} | {} | {} |\n",
      stat.profile_name, stat.mean_p_at_k, stat.mean_mrr, stat.mean_latency_ms,
      delta_p, delta_mrr, delta_lat
    )
  md += "\n"

  -- SECTION 2: Notable Ranking Changes (FR-27 item 2):
  -- Find scenarios with lowest Kendall tau (most changed ordering):
  md += "## 2. Notable Ranking Changes\n\n"
  notable = find_notable_ranking_changes(results, query_map, top_n=10)
  if notable.is_empty():
    md += "_No ranking changes across all scenarios._\n\n"
  else:
    for (scenario_id, query, tau, baseline_entries, candidate_entries) in &notable:
      md += format!("### {scenario_id}\n")
      md += format!("**Query**: {query}  \n")
      md += format!("**Kendall τ**: {tau:.4}\n\n")
      md += "| Rank | Baseline Entry | Candidate Entry |\n"
      md += "|------|---------------|-----------------|\n"
      max_rows = std::cmp::max(baseline_entries.len(), candidate_entries.len()).min(10)
      for i in 0..max_rows:
        b_entry = baseline_entries.get(i).map(|e| format!("{}: {}", e.id, &e.title[..30.min(e.title.len())])).unwrap_or("-".to_string())
        c_entry = candidate_entries.get(i).map(|e| format!("{}: {}", e.id, &e.title[..30.min(e.title.len())])).unwrap_or("-".to_string())
        md += format!("| {} | {} | {} |\n", i+1, b_entry, c_entry)
      md += "\n"

  -- SECTION 3: Latency Distribution (FR-27 item 3):
  md += "## 3. Latency Distribution\n\n"
  md += "| ≤ ms | Count |\n"
  md += "|------|-------|\n"
  for bucket in latency_buckets:
    label = if bucket.le_ms == u64::MAX: "> 2000".to_string() else: format!("{}", bucket.le_ms)
    md += format!("| {} | {} |\n", label, bucket.count)
  md += "\n"

  -- SECTION 4: Entry-Level Analysis (FR-27 item 4):
  md += "## 4. Entry-Level Analysis\n\n"
  md += render_entry_analysis(entry_rank_changes)

  -- SECTION 5: Zero-Regression Check (FR-27 item 5, AC-09):
  md += "## 5. Zero-Regression Check\n\n"
  if regressions.is_empty():
    -- Explicit empty-list indicator (AC-09, FR-28):
    md += "**No regressions detected.** All candidate profiles maintain or improve MRR and P@K across all scenarios.\n\n"
  else:
    md += format!("**{} regression(s) detected:**\n\n", regressions.len())
    md += "| Scenario | Profile | Reason | Baseline MRR | Candidate MRR | Baseline P@K | Candidate P@K |\n"
    md += "|----------|---------|--------|-------------|--------------|-------------|---------------|\n"
    for reg in regressions:
      md += format!(
        "| {} | {} | {} | {:.4} | {:.4} | {:.4} | {:.4} |\n",
        reg.scenario_id, reg.profile_name, reg.reason,
        reg.baseline_mrr, reg.candidate_mrr,
        reg.baseline_p_at_k, reg.candidate_p_at_k,
      )
    md += "\n"
    md += "_This list is a human-reviewed artifact. No automated gate logic is applied._\n\n"

  return md
```

## Function: `fn compute_latency_buckets` (private)

```
fn compute_latency_buckets(
    results: &[ScenarioResult],
) -> Vec<LatencyBucket>

BODY:
  const BOUNDARIES: &[u64] = &[50, 100, 200, 500, 1000, 2000, u64::MAX]
  mut counts: Vec<usize> = vec![0; BOUNDARIES.len()]

  for result in results:
    for prof_result in result.profiles.values():
      lat = prof_result.latency_ms
      for (i, &bound) in BOUNDARIES.iter().enumerate():
        if lat <= bound:
          counts[i] += 1
          break

  return BOUNDARIES.iter().zip(counts.iter())
    .map(|(&le_ms, &count)| LatencyBucket { le_ms, count })
    .collect()
```

## Function: `fn compute_entry_rank_changes` (private)

```
struct EntryRankSummary {
    most_promoted: Vec<(u64, String, i64)>,   -- (id, title, avg_rank_gain)
    most_demoted:  Vec<(u64, String, i64)>,   -- (id, title, avg_rank_loss)
}

fn compute_entry_rank_changes(results: &[ScenarioResult]) -> EntryRankSummary

BODY:
  -- Accumulate per-entry rank delta across all scenarios and comparisons:
  entry_deltas: HashMap<u64, (String, Vec<i64>)> = HashMap::new()
    -- key: entry_id, value: (title, list of rank deltas across scenarios)
    -- rank_delta = from_rank - to_rank (positive = promoted, negative = demoted)

  for result in results:
    for change in &result.comparison.rank_changes:
      delta = change.from_rank as i64 - change.to_rank as i64
      entry_deltas
        .entry(change.entry_id)
        .or_insert_with(|| ("unknown".to_string(), Vec::new()))
        .1.push(delta)

  -- For each entry, find its title from any profile result:
  for result in results:
    for prof_result in result.profiles.values():
      for scored in &prof_result.entries:
        if let Some(record) = entry_deltas.get_mut(&scored.id):
          if record.0 == "unknown":
            record.0 = scored.title.clone()

  -- Compute mean delta per entry:
  mean_deltas: Vec<(u64, String, f64)> = entry_deltas.into_iter()
    .map(|(id, (title, deltas))| {
      let mean = deltas.iter().sum::<i64>() as f64 / deltas.len() as f64
      (id, title, mean)
    })
    .collect()

  -- Sort and take top 10 promoted and demoted:
  promoted = mean_deltas sorted by delta DESC, take 10
               → Vec<(u64, String, i64)> (as i64 rounded)
  demoted  = mean_deltas sorted by delta ASC, take 10
               → Vec<(u64, String, i64)>

  return EntryRankSummary { most_promoted: promoted, most_demoted: demoted }
```

## Edge Cases

| Condition | Behavior |
|-----------|----------|
| Empty results directory | All sections have empty-list/no-data indicators; exit 0 |
| Malformed result JSON file | Skip with WARN to stderr; continue with valid files |
| No regressions | Section 5 shows explicit "No regressions detected." (AC-09) |
| Only one profile (no candidate) | All delta columns show "—"; Kendall tau = 1.0 throughout |
| Single scenario | All sections rendered; no errors |

## Error Handling

| Failure | Behavior |
|---------|----------|
| `read_dir` on results fails | Propagated; non-zero exit |
| Output file creation fails | Propagated; non-zero exit |
| All result files malformed | WARN for each; produces report with empty sections; exit 0 |

Never exits non-zero due to regression count (C-07, FR-29).

## Key Test Scenarios

1. **Five section headers present**: `eval report` on a prepared results directory;
   assert output Markdown contains all five H2 headings (AC-08, R-17).

2. **Zero-regression check — regressions present**: candidate profile degrades one
   scenario MRR; assert that scenario appears in Section 5 with correct profile name
   and reason (AC-09).

3. **Zero-regression check — no regressions**: no candidate profile degrades any metric;
   assert Section 5 contains "No regressions detected." (AC-09, FR-28).

4. **OR semantics — MRR drops only**: candidate MRR < baseline MRR, P@K equal; assert
   scenario appears in regression list (R-12).

5. **OR semantics — P@K drops only**: candidate P@K < baseline P@K, MRR equal; assert
   scenario appears in regression list (R-12).

6. **Empty results directory**: report written with empty-list indicators; exit 0.

7. **Malformed result file**: skipped with WARN; valid files processed normally.

8. **Exit code**: always 0, regardless of regression count (C-07).

## Knowledge Stewardship

Queried: /uni-query-patterns for "evaluation harness patterns conventions" (category: pattern) — 5 results; #724 (Behavior-Based Ranking Tests) informs section 2 (Notable Ranking Changes): the report surfaces ordering deltas, not raw scores, consistent with the assertion-on-ordering principle.
Queried: /uni-query-patterns for "nan-007 architectural decisions" (category: decision, topic: nan-007) — no ADRs directly govern report.rs (it is fully synchronous with no DB or async dependency). ADR-004 (#2587, eval in unimatrix-server not a new crate) constrains module placement; followed.
Queried: /uni-query-patterns for "snapshot vacuum database patterns" — not applicable to this module; report.rs reads only JSON files, not SQLite. Search returned no relevant entries for this scope.
Stored: nothing novel to store — pseudocode agents are read-only; patterns are consumed not created
