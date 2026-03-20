# Pseudocode: eval/runner.rs (D3)

**Location**: `crates/unimatrix-server/src/eval/runner.rs`

## Purpose

Replay eval scenarios through one or more named configuration profiles, computing
ranking metrics per scenario, and writing one JSON result file per scenario to an
output directory. This is the core compute engine of the eval pipeline.

Key design invariants enforced here:
- Each profile gets its own `EvalServiceLayer` (one VectorIndex per profile, FR architecture)
- Analytics are suppressed (`AnalyticsMode::Suppressed` via `from_profile()`)
- Kendall tau comes from `unimatrix_engine::test_scenarios::kendall_tau()` (C-10, FR-22)
- P@K uses dual-mode semantics: `expected` (hard labels) or `baseline.entry_ids` (soft GT)
- Profile name collisions detected before any replay begins
- `--k 0` rejected before any work

## Dependencies

| Dependency | Location | Role |
|------------|----------|------|
| `EvalServiceLayer::from_profile` | `eval/profile.rs` | Per-profile service layer |
| `EvalError`, `EvalProfile`, `parse_profile_toml` | `eval/profile.rs` | Error types, profile parsing |
| `ScenarioRecord`, `ScenarioBaseline` | `eval/scenarios.rs` | Scenario input format |
| `block_export_sync` | `crates/unimatrix-server/src/export.rs` | Async-to-sync bridge |
| `ServiceLayer.search.search()` | `crates/unimatrix-server/src/services/search.rs` | Search replay |
| `ServiceSearchParams`, `RetrievalMode` | `crates/unimatrix-server/src/services/search.rs` | Search parameterization |
| `AuditContext`, `AuditSource`, `CallerId` | `crates/unimatrix-server/src/services/mod.rs` | Required by search() |
| `kendall_tau` | `unimatrix_engine::test_scenarios` | Rank correlation metric (ADR-003, C-10) |
| `serde_json` | serde_json | Result JSON output |
| `std::time::Instant` | stdlib | Latency measurement |
| `std::fs` | stdlib | Directory creation, file output |

## Types (defined in this module)

```
pub struct ScenarioResult {
    pub scenario_id: String,
    pub query: String,
    pub profiles: HashMap<String, ProfileResult>,
    pub comparison: ComparisonMetrics,
}

pub struct ProfileResult {
    pub entries: Vec<ScoredEntry>,
    pub latency_ms: u64,
    pub p_at_k: f64,
    pub mrr: f64,
}

pub struct ScoredEntry {
    pub id: u64,
    pub title: String,
    pub final_score: f64,
    pub similarity: f64,
    pub confidence: f64,
    pub status: String,
    pub nli_rerank_delta: Option<f64>,  -- always None in nan-007 (NLI is W1-4)
}

pub struct ComparisonMetrics {
    pub kendall_tau: f64,
    pub rank_changes: Vec<RankChange>,
    pub mrr_delta: f64,             -- candidate.mrr - baseline.mrr
    pub p_at_k_delta: f64,          -- candidate.p_at_k - baseline.p_at_k
    pub latency_overhead_ms: i64,   -- candidate.latency_ms - baseline.latency_ms
}

pub struct RankChange {
    pub entry_id: u64,
    pub from_rank: usize,  -- 1-indexed position in baseline result list
    pub to_rank: usize,    -- 1-indexed position in candidate result list
}
```

All types derive `serde::Serialize`, `serde::Deserialize`.

## Function: `pub fn run_eval`

```
pub fn run_eval(
    db: &Path,
    scenarios: &Path,
    configs: &[PathBuf],
    k: usize,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>>

BODY:
  1. Validate --k (RISK-TEST-STRATEGY edge case):
       if k == 0:
         return Err(Box::new(EvalError::InvalidK(0)))

  2. Live-DB path guard for --db (C-13, FR-44, ADR-001):
       paths = project::ensure_data_directory(None, None)
         -- skip guard if ensure_data_directory fails (eval scenarios model)
       if paths resolved:
         active_db = canonicalize(paths.db_path)?
         db_resolved = canonicalize(db)
                         .map_err(|e| EvalError::Io(e))?
         if db_resolved == active_db:
           return Err(Box::new(EvalError::LiveDbPath {
             supplied: db.to_path_buf(),
             active: active_db,
           }))

  3. Parse all profile TOMLs:
       profiles: Vec<EvalProfile> = Vec::new()
       for cfg_path in configs:
         profile = parse_profile_toml(cfg_path)
                     .map_err(|e| Box::new(e))?
         profiles.push(profile)

  4. Detect profile name collisions (RISK-TEST-STRATEGY edge case):
       seen_names: HashSet<String> = HashSet::new()
       for profile in &profiles:
         if seen_names.contains(&profile.name):
           return Err(Box::new(EvalError::ProfileNameCollision(profile.name.clone())))
         seen_names.insert(profile.name.clone())

  5. Create output directory:
       std::fs::create_dir_all(out)?

  6. Bridge to async for profile construction + scenario replay:
       block_export_sync(async {
         run_eval_async(db, scenarios, profiles, k, out).await
       })
```

## Function: `async fn run_eval_async` (private)

```
async fn run_eval_async(
    db: &Path,
    scenarios: &Path,
    profiles: Vec<EvalProfile>,
    k: usize,
    out: &Path,
) -> Result<(), Box<dyn std::error::Error>>

BODY:
  1. Construct EvalServiceLayer for each profile:
       layers: Vec<EvalServiceLayer> = Vec::new()
       for profile in &profiles:
         eprintln!("constructing EvalServiceLayer for profile '{}'", profile.name)
         layer = EvalServiceLayer::from_profile(db, profile, None).await?
         -- Wait for embedding model to load before proceeding (mirrors TestHarness):
         let mut attempts = 0
         loop:
           match layer.inner.embed_handle().get_adapter().await:
             Ok(_)  → break
             Err(e) if attempts < 30:
               tokio::time::sleep(Duration::from_millis(100)).await
               attempts += 1
             Err(e):
               return Err(format!("embed model failed to load for profile '{}': {e}",
                                  profile.name).into())
         layers.push(layer)

  2. Load scenarios from JSONL:
       scenario_records: Vec<ScenarioRecord> = load_scenarios(scenarios)?

  3. Print summary:
       eprintln!(
         "eval run: {} profiles × {} scenarios",
         profiles.len(), scenario_records.len()
       )

  4. Replay each scenario through each profile:
       for record in &scenario_records:
         result = replay_scenario(record, &profiles, &layers, k).await?
         write_scenario_result(result, out)?

  5. Close all pools:
       for layer in layers:
         -- pool drop closes connection; explicit close is optional

  6. eprintln!("eval run: complete. results in {}", out.display())

  7. return Ok(())
```

## Function: `fn load_scenarios` (private)

```
fn load_scenarios(
    scenarios: &Path,
) -> Result<Vec<ScenarioRecord>, Box<dyn std::error::Error>>

BODY:
  if !scenarios.exists():
    return Err(format!("scenarios file not found: {}", scenarios.display()).into())

  content = std::fs::read_to_string(scenarios)?
  records: Vec<ScenarioRecord> = Vec::new()

  for (line_no, line) in content.lines().enumerate():
    trimmed = line.trim()
    if trimmed.is_empty():
      continue  -- skip blank lines
    record = serde_json::from_str::<ScenarioRecord>(trimmed)
               .map_err(|e| format!("scenarios line {}: {e}", line_no + 1))?
    records.push(record)

  -- Empty scenarios file is valid: returns empty Vec (RISK-TEST-STRATEGY edge case)
  return Ok(records)
```

## Function: `async fn replay_scenario` (private)

```
async fn replay_scenario(
    record: &ScenarioRecord,
    profiles: &[EvalProfile],
    layers: &[EvalServiceLayer],
    k: usize,
) -> Result<ScenarioResult, Box<dyn std::error::Error>>

BODY:
  profile_results: HashMap<String, ProfileResult> = HashMap::new()

  for (profile, layer) in profiles.iter().zip(layers.iter()):
    result = run_single_profile(record, profile, layer, k).await?
    profile_results.insert(profile.name.clone(), result)

  -- Compute comparison metrics (baseline vs first candidate):
  -- The first profile in the list is the baseline by convention.
  baseline_name = profiles[0].name.clone()
  comparison = compute_comparison(&profile_results, &baseline_name, record, k)?

  return Ok(ScenarioResult {
    scenario_id: record.id.clone(),
    query: record.query.clone(),
    profiles: profile_results,
    comparison,
  })
```

## Function: `async fn run_single_profile` (private)

```
async fn run_single_profile(
    record: &ScenarioRecord,
    profile: &EvalProfile,
    layer: &EvalServiceLayer,
    k: usize,
) -> Result<ProfileResult, Box<dyn std::error::Error>>

BODY:
  1. Build search params from scenario context:
       retrieval_mode = match record.context.retrieval_mode.as_str():
         "strict"  → RetrievalMode::Strict
         _         → RetrievalMode::Flexible  -- default

       params = ServiceSearchParams {
         query: record.query.clone(),
         k,
         filters: None,
         similarity_floor: None,
         confidence_floor: None,
         feature_tag: None,
         co_access_anchors: None,
         caller_agent_id: Some(record.context.agent_id.clone()),
         retrieval_mode,
       }

       audit_ctx = AuditContext {
         source: AuditSource::Internal { service: "eval-runner".to_string() },
         caller_id: record.context.agent_id.clone(),
         session_id: Some(record.context.session_id.clone()),
         feature_cycle: if record.context.feature_cycle.is_empty():
                          None
                        else:
                          Some(record.context.feature_cycle.clone()),
       }

       caller_id = CallerId::Agent(record.context.agent_id.clone())

  2. Time the search:
       start = Instant::now()
       search_result = layer.inner.search
                         .search(params, &audit_ctx, &caller_id)
                         .await
                         .map_err(|e| format!("search failed for scenario {}: {e}", record.id))?
       latency_ms = start.elapsed().as_millis() as u64

  3. Build ScoredEntry list:
       entries: Vec<ScoredEntry> = search_result.entries
         .into_iter()
         .map(|se| ScoredEntry {
           id: se.entry.id as u64,
           title: se.entry.title.clone().unwrap_or_default(),
           final_score: se.final_score,
           similarity: se.similarity,
           confidence: se.entry.confidence,
           status: se.entry.status.to_string(),
           nli_rerank_delta: None,  -- W1-4 not in scope
         })
         .collect()

  4. Compute P@K:
       ground_truth: Vec<u64> = determine_ground_truth(record)
       p_at_k = compute_p_at_k(&entries, &ground_truth, k)

  5. Compute MRR:
       mrr = compute_mrr(&entries, &ground_truth)

  6. Return:
       return Ok(ProfileResult { entries, latency_ms, p_at_k, mrr })
```

## Function: `fn determine_ground_truth` (private)

```
fn determine_ground_truth(record: &ScenarioRecord) -> Vec<u64>

BODY:
  -- Dual-mode semantics (AC-07, FR-20, R-08):
  -- Priority: explicit `expected` field (hard labels) over `baseline.entry_ids` (soft GT)

  if let Some(expected) = &record.expected:
    -- Hand-authored scenario: use explicit expected IDs as hard labels
    return expected.clone()
  else if let Some(baseline) = &record.baseline:
    -- Query-log scenario: use baseline entry IDs as soft ground truth
    return baseline.entry_ids.clone()
  else:
    -- No ground truth available: P@K and MRR are undefined
    return Vec::new()

NOTES:
  - When ground_truth is empty, compute_p_at_k returns 0.0 and compute_mrr returns 0.0.
  - This case should be rare but must not panic.
```

## Function: `fn compute_p_at_k` (private)

```
fn compute_p_at_k(
    entries: &[ScoredEntry],
    ground_truth: &[u64],
    k: usize,
) -> f64

BODY:
  if ground_truth.is_empty() || entries.is_empty():
    return 0.0

  gt_set: HashSet<u64> = ground_truth.iter().copied().collect()
  top_k = entries.iter().take(k)
  hits = top_k.filter(|e| gt_set.contains(&e.id)).count()
  return hits as f64 / k.min(entries.len()) as f64
```

## Function: `fn compute_mrr` (private)

```
fn compute_mrr(
    entries: &[ScoredEntry],
    ground_truth: &[u64],
) -> f64

BODY:
  if ground_truth.is_empty() || entries.is_empty():
    return 0.0

  gt_set: HashSet<u64> = ground_truth.iter().copied().collect()
  for (i, entry) in entries.iter().enumerate():
    if gt_set.contains(&entry.id):
      return 1.0 / (i + 1) as f64
  return 0.0
```

## Function: `fn compute_comparison` (private)

```
fn compute_comparison(
    profile_results: &HashMap<String, ProfileResult>,
    baseline_name: &str,
    record: &ScenarioRecord,
    k: usize,
) -> Result<ComparisonMetrics, Box<dyn std::error::Error>>

BODY:
  baseline = profile_results.get(baseline_name)
               .ok_or(format!("baseline profile '{}' not found in results", baseline_name))?

  -- For two-profile runs, candidate is the non-baseline profile.
  -- For N-profile runs with N > 2, comparison is baseline vs. the second profile.
  -- All profiles are stored in ScenarioResult.profiles for inspection.
  -- The comparison metrics are defined for a single baseline-vs-candidate pair.
  -- Convention: candidate = first non-baseline profile in iteration order.
  candidate_name = profile_results.keys()
                     .find(|k| k.as_str() != baseline_name)
  candidate = match candidate_name:
    Some(name) → profile_results.get(name).unwrap()
    None       → baseline  -- only one profile; self-comparison produces tau=1.0

  -- Kendall tau (C-10, FR-22, ADR-003):
  -- Uses unimatrix_engine::test_scenarios::kendall_tau()
  baseline_ids: Vec<u64> = baseline.entries.iter().map(|e| e.id).collect()
  candidate_ids: Vec<u64> = candidate.entries.iter().map(|e| e.id).collect()

  tau = kendall_tau(&baseline_ids, &candidate_ids)
    -- kendall_tau handles empty lists and single-element lists without panic.
    -- Single-element: returns 1.0 by convention (RISK-TEST-STRATEGY edge case).
    -- IMPLEMENTATION NOTE: verify kendall_tau signature accepts &[u64] or &[usize].
    --   If it requires &[usize], cast or use a wrapper.

  -- Rank changes:
  rank_changes = compute_rank_changes(&baseline_ids, &candidate_ids)

  return Ok(ComparisonMetrics {
    kendall_tau: tau,
    rank_changes,
    mrr_delta: candidate.mrr - baseline.mrr,
    p_at_k_delta: candidate.p_at_k - baseline.p_at_k,
    latency_overhead_ms: candidate.latency_ms as i64 - baseline.latency_ms as i64,
  })
```

## Function: `fn compute_rank_changes` (private)

```
fn compute_rank_changes(
    baseline_ids: &[u64],
    candidate_ids: &[u64],
) -> Vec<RankChange>

BODY:
  baseline_pos: HashMap<u64, usize> = baseline_ids.iter()
    .enumerate()
    .map(|(i, &id)| (id, i + 1))  -- 1-indexed
    .collect()
  candidate_pos: HashMap<u64, usize> = candidate_ids.iter()
    .enumerate()
    .map(|(i, &id)| (id, i + 1))
    .collect()

  changes: Vec<RankChange> = Vec::new()

  -- Collect all IDs that appear in either list:
  all_ids: HashSet<u64> = baseline_pos.keys().chain(candidate_pos.keys()).copied().collect()

  for id in all_ids:
    from = baseline_pos.get(&id).copied()
    to   = candidate_pos.get(&id).copied()
    match (from, to):
      (Some(f), Some(t)) if f != t:
        changes.push(RankChange { entry_id: id, from_rank: f, to_rank: t })
      (Some(f), None):
        -- Entry dropped out of candidate results; record as rank len+1
        changes.push(RankChange {
          entry_id: id,
          from_rank: f,
          to_rank: candidate_ids.len() + 1,
        })
      (None, Some(t)):
        -- Entry appeared in candidate but not baseline
        changes.push(RankChange {
          entry_id: id,
          from_rank: baseline_ids.len() + 1,
          to_rank: t,
        })
      _ → continue  -- no change or not in either list

  -- Sort by magnitude of rank change (largest first):
  changes.sort_by(|a, b| {
    let delta_a = (a.to_rank as i64 - a.from_rank as i64).unsigned_abs()
    let delta_b = (b.to_rank as i64 - b.from_rank as i64).unsigned_abs()
    delta_b.cmp(&delta_a)
  })

  return changes
```

## Function: `fn write_scenario_result` (private)

```
fn write_scenario_result(
    result: ScenarioResult,
    out_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>>

BODY:
  -- Sanitize scenario_id for use as a filename:
  filename = result.scenario_id.replace('/', "_").replace('\\', "_") + ".json"
  out_path = out_dir.join(&filename)

  json = serde_json::to_string_pretty(&result)?
  std::fs::write(&out_path, json.as_bytes())?
  return Ok(())
```

## Edge Cases

| Condition | Behavior |
|-----------|----------|
| Empty scenarios file | Zero result files written; exit 0 |
| Single-element result list | Kendall tau = 1.0 (single element convention) |
| No ground truth (no expected, no baseline) | P@K = 0.0, MRR = 0.0, noted in result |
| Profile name collision | `EvalError::ProfileNameCollision` before any replay |
| Only one profile (no candidate) | Self-comparison: tau=1.0, all deltas=0 |
| `--k 0` | `EvalError::InvalidK(0)` before any work |
| Embed model load fails (30 attempts × 100ms) | Error returned, no scenarios replayed |

## Error Handling

| Failure | Behavior |
|---------|----------|
| Profile TOML parse fails | Error before any layer construction |
| `from_profile` returns EvalError | Propagated; eval run exits non-zero |
| Search fails for a scenario | Error with scenario ID in message; run aborts |
| Result file write fails | Error propagated; run aborts |
| Kendall tau with empty lists | Returns 0.0; no panic |

## Key Test Scenarios

1. **Two-profile A/B run**: baseline + candidate against a snapshot with known results;
   assert result JSON has both profile keys and all ComparisonMetrics fields (AC-06).

2. **P@K dual-mode — hard labels**: scenario with explicit `expected = [id1, id2]`;
   assert P@K computed against expected, not baseline (AC-07, R-08).

3. **P@K dual-mode — soft ground truth**: scenario with `expected = null`,
   `baseline.entry_ids = [id3]`; assert P@K against baseline (AC-07, R-08).

4. **Snapshot unchanged after eval run**: SHA-256 of snapshot db unchanged (AC-05, NFR-04, R-01).

5. **No drain task**: assert AnalyticsMode::Suppressed by confirming no analytics writes
   to the snapshot (R-01).

6. **kendall_tau callable from runner**: direct unit test calling kendall_tau() from
   within eval/runner.rs compile context — fails if test-support feature missing (R-03).

7. **Profile name collision**: two configs with same name → EvalError::ProfileNameCollision
   before any layer construction.

8. **--k 0**: EvalError::InvalidK returned immediately.

9. **Empty scenarios file**: exit 0; out directory exists but is empty.

10. **Rank change list**: scenario where candidate promotes an entry; assert RankChange
    records have correct from_rank and to_rank (1-indexed).

## Knowledge Stewardship

Queried: /uni-query-patterns for "evaluation harness patterns conventions" (category: pattern) — 5 results; #426 (Shadow Mode Evaluation Pipeline, crt-007) is the closest prior eval pattern. nan-007 eval runner is a snapshot-based offline replay rather than shadow mode; architecturally distinct but the principle of in-process metric computation without production side-effects is shared.
Queried: /uni-query-patterns for "nan-007 architectural decisions" (category: decision, topic: nan-007) — ADR-003 (#2586, test-support feature for kendall_tau) directly governs compute_comparison(). ADR-002 (#2585, AnalyticsMode::Suppressed) governs EvalServiceLayer::from_profile() calls in run_eval_async(). Both followed exactly.
Queried: /uni-query-patterns for "block_export_sync async bridge pattern" — #2126 (block_in_place) and #1758 (named sync helper) confirm that wrapping the full async replay loop in block_export_sync is the correct pattern for pre-tokio dispatch.
Stored: nothing novel to store — pseudocode agents are read-only; patterns are consumed not created
