# crt-046 — Component: behavioral_signals

## Purpose

New module `crates/unimatrix-server/src/services/behavioral_signals.rs`.
Declared `pub(crate)` in `services/mod.rs`.

Owns all behavioral signal logic to keep `mcp/tools.rs` under 500 lines (ADR-001).
Provides six public-to-crate functions:

- `collect_coaccess_entry_ids` — parse entry IDs from ObservationRow slices
- `build_coaccess_pairs` — enumerate and cap co-access pairs
- `outcome_to_weight` — map cycle outcome string to edge weight
- `emit_behavioral_edges` — write bidirectional Informs edges to graph_edges
- `populate_goal_cluster` — write one goal_clusters row
- `blend_cluster_entries` — pure interleaving function for briefing blending

Wave: 2 (depends on store-v22 for GoalClusterRow, insert_goal_cluster,
get_cycle_start_goal_embedding; also depends on InferenceConfig new fields).

---

## Module-Level Constants

```
/// Maximum number of candidate rows scanned from goal_clusters at briefing time.
/// O(RECENCY_CAP × D) at D=384 is ~0.1ms — well within latency budget.
/// If this cap must be raised above ~10,000, move cosine computation to spawn_blocking.
pub(crate) const RECENCY_CAP: u64 = 100;

/// Maximum canonical co-access pairs extracted per cycle.
/// Enforced at enumeration time (halt when pairs.len() == 200), not by post-hoc truncation.
pub(crate) const PAIR_CAP: usize = 200;
```

These constants are visible in tests via `use crate::services::behavioral_signals::{RECENCY_CAP, PAIR_CAP}`.

---

## write_graph_edge Return Contract (pattern #4041) — MUST READ FIRST

This contract governs ALL counter increments in `emit_behavioral_edges`.
Any deviation from it is a bug (root cause of crt-040 Gate 3a rework).

| `write_graph_edge` return | Meaning | Counter action |
|---------------------------|---------|----------------|
| `Ok(true)` | New row inserted (rows_affected == 1) | Increment `edges_enqueued` |
| `Ok(false)` | UNIQUE conflict — INSERT OR IGNORE silent no-op | Do NOT increment; not an error |
| `Err(_)` | SQL infrastructure failure | Log `warn!`, do NOT increment, continue loop |

`write_graph_edge` returns `Result<bool>` via `query_result.rows_affected() > 0`.
Counter increments key off `Ok(true)` ONLY.

---

## Module-Private Helper: write_graph_edge

```
async fn write_graph_edge(
    store: &SqlxStore,
    source_id: u64,
    target_id: u64,
    weight: f32,
    created_by: &str,  // "behavioral_signals"
) -> Result<bool>
```

Algorithm:
1. Get current Unix seconds: `now = unix_now_seconds()`.
2. Execute directly on `store.write_pool_server()` (not analytics drain — must return bool):
   ```sql
   INSERT OR IGNORE INTO graph_edges
       (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
   VALUES (?1, ?2, 'Informs', ?3, ?4, ?5, 'behavioral', 0)
   ```
   Bind: source_id as i64, target_id as i64, weight, now, created_by.
3. `rows = query_result.rows_affected()`.
4. Return `Ok(rows > 0)`.

On SQL error: return `Err(StoreError::from(e))`.

Note: `source = 'behavioral'` (not "nli", not "c_cosine"). `bootstrap_only = 0` (false).
`relation_type = 'Informs'`. This matches the UNIQUE constraint
`UNIQUE(source_id, target_id, relation_type)` — a behavioral edge for a pair already
owned by NLI Informs is silently dropped. `edges_enqueued` is not incremented.

---

## Function: collect_coaccess_entry_ids

```
pub(crate) fn collect_coaccess_entry_ids(
    obs: &[ObservationRow],
) -> (HashMap<String, Vec<(u64, i64)>>, usize)
```

Returns `(by_session_id -> [(entry_id, ts_millis)], parse_failure_count)`.

Algorithm:
1. Initialize `by_session: HashMap<String, Vec<(u64, i64)>> = HashMap::new()`.
2. Initialize `parse_failures: usize = 0`.
3. For each `row` in `obs`:
   a. If `row.tool.as_deref() != Some("context_get")`: skip (continue).
   b. Let `input_json = match row.input.as_deref() { Some(s) => s, None => { parse_failures += 1; continue } }`.
   c. Parse `input_json` as `serde_json::Value`.
      On error: `parse_failures += 1; continue`.
   d. Extract `id` field: `val["id"]`.
      - If not present or not a JSON number: `parse_failures += 1; continue`.
      - If the number is negative or exceeds u64::MAX: `parse_failures += 1; continue`.
   e. Convert to `u64`: let `entry_id = val["id"].as_u64().ok_or else { parse_failures += 1; continue }`.
   f. Append `(entry_id, row.ts_millis)` to `by_session.entry(row.session_id.clone()).or_default()`.
4. Return `(by_session, parse_failures)`.

Edge cases:
- E-04 (duplicate entries in same session): same `entry_id` appears multiple times in a
  session's Vec. This is NOT deduplicated here. Deduplication of canonical pairs happens
  in `build_coaccess_pairs` via the canonical `(min, max)` dedup step.
- I-03: `tool = "context_get"` filter is applied inside this function, not assumed
  from the call site.

---

## Function: build_coaccess_pairs

```
pub(crate) fn build_coaccess_pairs(
    by_session: HashMap<String, Vec<(u64, i64)>>,
) -> (Vec<(u64, u64)>, bool)
```

Returns `(canonical_pairs, cap_hit)`.

Algorithm:
1. Initialize `seen: HashSet<(u64, u64)> = HashSet::new()`.
2. Initialize `pairs: Vec<(u64, u64)> = Vec::new()`.
3. Initialize `cap_hit: bool = false`.
4. For each `(_session_id, mut entries)` in `by_session`:
   a. Sort `entries` by `ts_millis` ascending: `entries.sort_by_key(|(_, ts)| *ts)`.
   b. For `i` in `0..entries.len()`:
      For `j` in `(i+1)..entries.len()`:
        i.   Let `a = entries[i].0`, `b = entries[j].0`.
        ii.  If `a == b`: skip (Resolution 4 — self-pair exclusion, DN-3).
        iii. Let `canonical = (a.min(b), a.max(b))`.
        iv.  If `seen.contains(&canonical)`: skip (dedup).
        v.   `seen.insert(canonical)`.
        vi.  `pairs.push(canonical)`.
        vii. If `pairs.len() == PAIR_CAP`:
               `cap_hit = true; return (pairs, cap_hit)`.
              (Cap enforced at enumeration time — not post-hoc truncation. NFR-04.)
5. Return `(pairs, cap_hit)`.

Note on step 4.b.ii: `filter(|(a, b)| a != b)` — self-pairs excluded BEFORE dedup.
Note on cap: when `pairs.len() == PAIR_CAP`, return immediately. The inner loops do NOT
continue generating pairs that would then be discarded. This is the key invariant for NFR-04.

---

## Function: outcome_to_weight

```
pub(crate) fn outcome_to_weight(outcome: Option<&str>) -> f32
```

Algorithm:
```
match outcome {
    Some("success") => 1.0,
    _ => 0.5,     // covers rework, None, any unknown string
}
```

R-16 note: any future outcome string silently maps to 0.5. This is the accepted behavior;
a unit test documents it explicitly to prevent accidental breakage.

---

## Function: emit_behavioral_edges

```
pub(crate) async fn emit_behavioral_edges(
    store: &SqlxStore,
    pairs: &[(u64, u64)],
    weight: f32,
) -> (usize, usize)
```

Returns `(edges_enqueued, pairs_skipped_on_conflict)`.

PRECONDITION: pairs is non-empty (caller checks pairs.is_empty() before calling).

write_graph_edge return contract (pattern #4041 — governs ALL counter increments):

| `write_graph_edge` return | Meaning | Counter action |
|---------------------------|---------|----------------|
| `Ok(true)` | New row inserted | Increment `edges_enqueued` |
| `Ok(false)` | UNIQUE conflict, silently ignored | Do NOT increment; not an error |
| `Err(_)` | SQL infrastructure failure | Log `warn!`, do NOT increment, continue |

Algorithm:
1. Initialize `edges_enqueued: usize = 0`, `pairs_skipped: usize = 0`.
2. For each `(a, b)` in `pairs`:
   a. Emit forward edge `(a → b)`:
      ```
      match write_graph_edge(store, a, b, weight, "behavioral_signals").await {
          Ok(true)  => edges_enqueued += 1,
          Ok(false) => { /* UNIQUE conflict — do not increment */ }
          Err(e)    => { warn!("emit_behavioral_edges: write_graph_edge ({a}->{b}) failed: {e}"); /* continue */ }
      }
      ```
   b. Emit reverse edge `(b → a)`:
      ```
      match write_graph_edge(store, b, a, weight, "behavioral_signals").await {
          Ok(true)  => edges_enqueued += 1,
          Ok(false) => { /* UNIQUE conflict — do not increment */ }
          Err(e)    => { warn!("emit_behavioral_edges: write_graph_edge ({b}->{a}) failed: {e}"); /* continue */ }
      }
      ```
   c. If both directions returned `Ok(false)`: `pairs_skipped += 1`.
      Note: track pair-level skip, not edge-level. A "pair skipped on conflict" means
      both (a→b) AND (b→a) returned Ok(false). Partial conflicts count as one edge_enqueued.
3. Return `(edges_enqueued, pairs_skipped)`.

R-10 note: both directions must be emitted for every pair. A test asserting `COUNT(*) >= 1`
is insufficient — both `(source_id=A, target_id=B)` and `(source_id=B, target_id=A)` rows
must be verified.

Drain flush note (I-02, entry #2148): integration tests querying `graph_edges` after calling
`context_cycle_review` must flush the analytics drain. However, `emit_behavioral_edges`
uses `write_graph_edge` (direct write_pool_server), not `enqueue_analytics`, so the drain
is NOT involved here. Tests querying `graph_edges` immediately after `emit_behavioral_edges`
do NOT need a drain flush.

---

## Function: populate_goal_cluster

```
pub(crate) async fn populate_goal_cluster(
    store: &SqlxStore,
    feature_cycle: &str,
    goal_embedding: Vec<f32>,
    entry_ids: &[u64],
    phase: Option<&str>,
    outcome: Option<&str>,
) -> Result<bool>
```

Returns `Ok(true)` on new row, `Ok(false)` on UNIQUE conflict.

Algorithm:
1. Serialize `entry_ids` to JSON: `entry_ids_json = serde_json::to_string(entry_ids)`.
   On error: return `Err(StoreError::InvalidInput { field: "entry_ids", reason: ... })`.
2. Get current Unix millis: `created_at = SystemTime::now() ... as_millis() as i64`.
3. Call `store.insert_goal_cluster(feature_cycle, goal_embedding, phase, &entry_ids_json, outcome, created_at).await`.
4. On `Ok(true)`: debug!("populate_goal_cluster: new row for {}", feature_cycle); return `Ok(true)`.
5. On `Ok(false)`: debug!("populate_goal_cluster: UNIQUE conflict for {} — INSERT OR IGNORE no-op", feature_cycle); return `Ok(false)`.
6. On `Err(e)`: propagate `Err(e)`.

R-06 note: `insert_goal_cluster` is called ONLY after `entry_ids` is fully assembled.
The partial-record risk (R-06) is avoided because the insert is the final step after
all observation loading and parsing is complete. If `load_observations_for_sessions`
fails, this function is never reached.

---

## Function: blend_cluster_entries

```
pub(crate) fn blend_cluster_entries(
    semantic: Vec<IndexEntry>,
    cluster_entries_with_scores: Vec<(IndexEntry, f32)>,  // (entry, cluster_score)
    k: usize,
) -> Vec<IndexEntry>
```

PURE FUNCTION — no store access, no async. Takes pre-fetched, pre-scored cluster entries.

Naming collision note (ADR-005 — critical):
- `IndexEntry.confidence` (f64) = raw HNSW cosine similarity (from `briefing.index()`)
- `EntryRecord.confidence` (f64) = Wilson-score composite (from `store.get()` per ID)
The `cluster_score` formula (computed by caller, not here) uses `EntryRecord.confidence`.
This function receives `cluster_score` as the pre-computed f32 — it does NOT recompute it.

Algorithm:
1. Build candidate list `Vec<(IndexEntry, f64)>`:
   a. From `semantic`: each entry contributes `(entry, entry.confidence as f64)`.
      `entry.confidence` here is the f64 field on IndexEntry, which is raw cosine from
      briefing.index(). This is the correct score for semantic entries in the merged sort.
   b. From `cluster_entries_with_scores`: each `(entry, cluster_score)` contributes
      `(entry, cluster_score as f64)`.
2. Sort candidates by score descending (stable sort preferred to preserve relative order
   within equal-scored entries):
   `candidates.sort_by(|(_, s1), (_, s2)| s2.partial_cmp(s1).unwrap_or(Ordering::Equal))`.
3. Deduplicate by entry ID (first occurrence wins):
   ```
   let mut seen_ids: HashSet<u64> = HashSet::new();
   let result: Vec<IndexEntry> = candidates
       .into_iter()
       .filter(|(entry, _)| seen_ids.insert(entry.id))
       .map(|(entry, _)| entry)
       .collect();
   ```
4. Truncate to top-k: `result.truncate(k)`.
5. Return `result`.

Edge cases:
- E-05: cluster row with empty entry_ids_json `[]` → `cluster_entries_with_scores` is empty
  → only semantic entries in candidates → result identical to pure-semantic output.
- AC-07: cluster entry with higher cluster_score than weakest semantic entry displaces it.
- AC-08/AC-09/R-11: when `cluster_entries_with_scores` is empty, result is identical to
  `semantic.truncate(k)` — pure-semantic behavior preserved.

---

## Module Declaration (services/mod.rs)

Add to `services/mod.rs`:
```rust
pub(crate) mod behavioral_signals;
```

Add after existing `pub(crate) mod` declarations.

---

## Key Test Scenarios (behavioral_signals)

### collect_coaccess_entry_ids

| Scenario | Assertion |
|----------|-----------|
| All valid context_get rows | parse_failures == 0, all IDs in by_session |
| Non-context_get rows filtered | skipped; not in result |
| Malformed JSON in input | parse_failures incremented; other rows continue |
| Missing id field | parse_failures incremented |
| Non-integer id value | parse_failures incremented |
| NULL input field | parse_failures incremented |
| AC-13 mix | valid rows produce entries + parse_failures >= 1 |

### build_coaccess_pairs

| Scenario | Assertion |
|----------|-----------|
| Single session, 2 IDs | 1 canonical pair, cap_hit = false |
| Same pair in two sessions | 1 pair after dedup |
| Self-pair (A, A) — E-02 / DN-3 | excluded; zero pairs in result |
| All same ID | zero pairs (all self-pairs excluded) |
| 21 distinct IDs (produces 210) | pairs.len() == 200, cap_hit = true |
| Cap at 200 — enumeration halt | pairs.len() == 200, NOT 201 (no post-hoc truncate) |
| ts_millis ordering | pairs generated in ts order within session |

### outcome_to_weight

| Input | Expected |
|-------|---------|
| Some("success") | 1.0 |
| None | 0.5 |
| Some("rework") | 0.5 |
| Some("unknown-future-string") | 0.5 |

### emit_behavioral_edges (R-02 — critical)

| Scenario | Assertion |
|----------|-----------|
| New pair, no prior edge | edges_enqueued == 2 (both directions) |
| Pair already owned by NLI | edges_enqueued == 0, pairs_skipped == 1 (R-02 contract) |
| SQL error on write | edges_enqueued not incremented, warn! logged, loop continues |
| N pairs → 2N new edges | edges_enqueued == 2*N |
| Both directions present | assert rows for (A→B) AND (B→A) in graph_edges |

### populate_goal_cluster

| Scenario | Assertion |
|----------|-----------|
| New feature_cycle | Ok(true), row exists in goal_clusters |
| Duplicate feature_cycle | Ok(false), still one row, no error |
| entry_ids serialization roundtrip | entry_ids_json parseable back to Vec<u64> |

### blend_cluster_entries

| Scenario | Assertion |
|----------|-----------|
| Empty cluster_entries | result == semantic[:k] (pure-semantic) |
| Cluster entry outscores weakest semantic | cluster entry appears in top-k (AC-07) |
| Duplicate ID in semantic + cluster | appears only once (first occurrence wins) |
| k=20, both lists full | result.len() == 20 |
| E-05: cluster entries = [] | result identical to semantic-only |
| Cluster entry below all semantic scores | cluster entry excluded from top-k |
