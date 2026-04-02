# Wave 3: Path C Loop — Cosine Supports Detection

## Purpose

Add the Path C write loop to `run_graph_inference_tick` in `nli_detection_tick.rs`.
Path C runs after Path A (Informs writes + observability log) and before the Path B
entry gate. It iterates the Phase 4 `candidate_pairs` already in scope, applies four
guards in strict order, and writes `Supports` edges via `write_graph_edge`.

---

## File Modified

`crates/unimatrix-server/src/services/nli_detection_tick.rs`

---

## New Module-Level Constant

Add immediately after `MAX_INFORMS_PER_TICK` (around line 51):

```
/// Independent per-tick budget for cosine Supports edges (Path C).
///
/// Path C iterates Phase 4 `candidate_pairs` (already sorted by priority) and
/// writes up to this many `Supports` edges per tick. Independent of:
/// - `MAX_INFORMS_PER_TICK` (Path A budget)
/// - `max_graph_inference_per_tick` (Path B NLI budget)
///
/// Cost of Path C per candidate: one f32 comparison + one HashMap lookup +
/// one HashSet lookup + one INSERT OR IGNORE. No model invocation.
///
/// TODO: Config-promote to `InferenceConfig.max_cosine_supports_per_tick` if
/// operators require runtime tuning (ADR-004, SR-03). Not speculated in crt-040.
const MAX_COSINE_SUPPORTS_PER_TICK: usize = 50;
```

---

## Category Map Pre-Build

The category map must be built ONCE after Phase 2 completes and BEFORE the Path C loop.
It reuses `all_active` (already loaded in Phase 2) — no new DB query.

**Location**: After Phase 2 `all_active` is populated, before any path-specific code.
In the current tick function, Phase 2 loads `all_active` and `existing_supports_pairs`.
The category map build goes immediately after those two loads.

```
// --- Category map for Path C (MANDATORY — per-pair DB lookup is PROHIBITED, WARN-01) ---
// Built once from all_active. Key: entry_id (u64). Value: category (String).
// Path C uses O(1) HashMap lookup per candidate pair instead of O(n) linear scan.
let category_map: HashMap<u64, String> = all_active
    .iter()
    .map(|e| (e.id, e.category.clone()))
    .collect();
```

### Import required

`HashMap` must be in scope. Verify `std::collections::HashMap` is already imported in
`nli_detection_tick.rs`. A `category_map` using `&str` values is also acceptable
(`HashMap<u64, &str>`), but `String` is safer to avoid lifetime issues with `all_active`
borrow across the tick function body.

---

## Path C Loop — Full Pseudocode

Insert after the Path A observability `tracing::debug!(...)` log and before the
`// === PATH B entry gate ===` comment block.

```
// === PATH C: Cosine Supports write loop ===
// Pure-cosine Supports detection. Runs unconditionally — NOT gated by nli_enabled or
// get_provider(). Reuses candidate_pairs from Phase 4 (no new HNSW scan, NFR-01).
// Writes Supports edges with source = EDGE_SOURCE_COSINE_SUPPORTS = "cosine_supports".
// See ADR-001 (write_graph_edge sibling), ADR-003 (placement), ADR-004 (budget).

let mut cosine_supports_written: usize = 0;
let mut cosine_supports_candidates: usize = 0;

for (src_id, tgt_id, cosine) in &candidate_pairs {

    // --- Finite guard (before threshold comparison — ARCHITECTURE.md R-09) ---
    // Cosine values from HNSW should be finite, but guard is required.
    // NaN/Inf would produce an invalid weight in graph_edges.weight (f32 REAL column).
    if !cosine.is_finite() {
        warn!(
            src_id = src_id,
            tgt_id = tgt_id,
            cosine = cosine,
            "Path C: non-finite cosine for candidate pair — skipping"
        );
        continue;
    }

    // --- Gate 1: cosine threshold (>= inclusive, per FR-01 and boundary AC edge case) ---
    // Pairs at exactly 0.65 qualify (>= not >). Pairs below threshold do not qualify.
    if cosine < &config.supports_cosine_threshold {
        continue;
    }

    // Candidate passed cosine threshold — count it for observability
    cosine_supports_candidates += 1;

    // --- Gate 2: per-tick budget cap (BEFORE category lookup — avoids HashMap overhead
    //     for candidates that would be discarded by budget anyway, ADR-004) ---
    // break (not continue): Phase 4 already sorted by priority (cross-category first,
    // isolated endpoint second, similarity desc). Once budget exhausted, remaining
    // candidates are lower-priority — no point inspecting them.
    if cosine_supports_written >= MAX_COSINE_SUPPORTS_PER_TICK {
        break;
    }

    // --- Gate 3: category pair filter (O(1) via HashMap — no DB lookup, WARN-01) ---
    // Both source and target category must be present in category_map (built from all_active).
    // If an entry was deprecated between Phase 2 DB read and this point, it will be absent.
    // Treatment: continue (no panic, no edge), emit warn! to signal the anomaly.
    let src_cat = match category_map.get(src_id) {
        Some(cat) => cat.as_str(),
        None => {
            warn!(
                src_id = src_id,
                "Path C: source entry not found in category_map (deprecated mid-tick?) — skipping"
            );
            continue;
        }
    };
    let tgt_cat = match category_map.get(tgt_id) {
        Some(cat) => cat.as_str(),
        None => {
            warn!(
                tgt_id = tgt_id,
                "Path C: target entry not found in category_map (deprecated mid-tick?) — skipping"
            );
            continue;
        }
    };

    // Check category pair against config.informs_category_pairs allow-list.
    // Uses the same inline check pattern as Phase 4b (no separate function).
    let category_allowed = config
        .informs_category_pairs
        .iter()
        .any(|pair| pair[0] == src_cat && pair[1] == tgt_cat);
    if !category_allowed {
        continue;
    }

    // --- Gate 4: pre-filter (INSERT OR IGNORE is authoritative backstop) ---
    // existing_supports_pairs was populated at Phase 2 (tick start).
    // It does NOT reflect intra-tick Path C writes — INSERT OR IGNORE handles those.
    // Canonical form: (lo, hi) where lo = min(src, tgt). Phase 4 normalizes to (lo, hi).
    let canonical = (src_id.min(tgt_id), src_id.max(tgt_id));
    if existing_supports_pairs.contains(&canonical) {
        continue;
    }

    // --- Write edge ---
    let metadata_json = format!(r#"{{"cosine":{}}}"#, cosine);
    let wrote = write_graph_edge(
        store,
        *src_id,
        *tgt_id,
        "Supports",    // must match RelationType::Supports.as_str() exactly
        *cosine,
        timestamp,     // reuse timestamp from Path A (already computed above as `let timestamp = ...`)
        EDGE_SOURCE_COSINE_SUPPORTS,
        &metadata_json,
    )
    .await;

    // Budget counter: increment ONLY on true return (row was inserted, rows_affected = 1).
    // false return = UNIQUE conflict (rows_affected = 0, no log inside fn) OR SQL error
    //   (warn! already emitted inside write_graph_edge — do NOT double-log).
    // In both false cases: do NOT increment budget, do NOT emit warn! here.
    if wrote {
        cosine_supports_written += 1;
    }
    // false return is NOT an error at the loop level — loop continues normally.
}

// --- Unconditional Path C observability log (MANDATORY — WARN-02, R-06, ADR-003) ---
// Fires after the loop, even when candidate_pairs is empty, all candidates filtered,
// or budget hit immediately. Field names must NOT collide with Path A's log fields
// (informs_candidates_found, informs_candidates_after_dedup, informs_candidates_after_cap,
// informs_edges_written). Different prefix "cosine_supports_*" ensures no collision.
tracing::debug!(
    cosine_supports_candidates,
    cosine_supports_edges_written = cosine_supports_written,
    "Path C: cosine Supports tick complete"
);
```

---

## Module Doc Comment Update

Update the module-level `//!` comment at the top of `nli_detection_tick.rs` to add
a Path C section alongside the existing Path A and Path B sections:

```
//! # Path C: Cosine Supports (crt-040)
//! Pure cosine detection. No NLI cross-encoder. Runs unconditionally on every tick.
//! Writes `Supports` edges with source = EDGE_SOURCE_COSINE_SUPPORTS = "cosine_supports".
//! Gated by: cosine >= supports_cosine_threshold (0.65 default), informs_category_pairs.
//! Budget: MAX_COSINE_SUPPORTS_PER_TICK = 50 (independent of Path A and Path B budgets).
//! Placement: after Path A observability log, before Path B entry gate (ADR-003).
```

---

## 500-Line Extraction Guidance (NFR-07)

If Path C's write loop plus observability log causes `run_graph_inference_tick`'s
function body to exceed ~150 lines, extract Path C into a private helper function in
the same file:

```
FUNCTION run_cosine_supports_path(
    store: &Store,
    config: &InferenceConfig,
    candidate_pairs: &[(u64, u64, f32)],
    existing_supports_pairs: &HashSet<(u64, u64)>,
    category_map: &HashMap<u64, String>,
    timestamp: u64,
) -> ()   // infallible — returns ()

    // Contains: constant, counters, loop body, observability log
    // Called from run_graph_inference_tick between Path A log and Path B gate
```

This helper is private to `nli_detection_tick.rs`. It is NOT a new module. If extracted,
`category_map` and `timestamp` must be passed in rather than re-computed inside the helper.

---

## Error Handling

| Condition | Guard | Behavior |
|-----------|-------|----------|
| Non-finite cosine (NaN/Inf) | `!cosine.is_finite()` | `warn!`, `continue` |
| Cosine below threshold | `cosine < threshold` | silent `continue` |
| Budget exhausted | `written >= MAX_COSINE_SUPPORTS_PER_TICK` | `break` |
| Source entry absent from category_map | `get(src_id) == None` | `warn!`, `continue` |
| Target entry absent from category_map | `get(tgt_id) == None` | `warn!`, `continue` |
| Category pair not allowed | `!category_allowed` | silent `continue` |
| Pair in existing_supports_pairs | `contains(&canonical)` | silent `continue` |
| SQL error in write_graph_edge | `wrote == false` (logged inside fn) | silent `continue`, no double-warn |
| UNIQUE conflict (INSERT OR IGNORE) | `wrote == false` (no log inside fn for Ok path) | silent `continue`, no counter increment |

No condition propagates an error. `run_graph_inference_tick` remains infallible (`-> ()`).

---

## AC-19 Resolution: Removal of the Joint Early-Return (MANDATORY)

AC-19 requires the Path C observability log to fire unconditionally — even when both
`candidate_pairs` and `informs_metadata` are empty. The existing codebase contains this
early-return in Phase 5 (confirmed by reading `nli_detection_tick.rs` line 452):

```rust
// If both are empty after caps, return now (no writes needed).
if candidate_pairs.is_empty() && informs_metadata.is_empty() {
    tracing::debug!("graph inference tick: no candidates after HNSW expansion and caps");
    return;
}
```

This early-return fires BEFORE Path C. When both are empty, Path C never executes and
the AC-19 observability log is suppressed.

**Resolution: Remove this joint early-return entirely.**

Rationale:
- Path A's loop is already guarded: if `informs_metadata` is empty, zero iterations run — no writes, no cost.
- Path C's loop is already guarded: if `candidate_pairs` is empty, zero iterations run — but the unconditional observability `tracing::debug!` log AFTER the loop still fires. This satisfies AC-19.
- Path B's existing entry gate at line 514 (`if candidate_pairs.is_empty() { return; }`) already prevents the NLI batch from running when `candidate_pairs` is empty. This early-return is RETAINED — it guards Phase 6/7/8 only and does not suppress Path C.

**The delivery agent MUST:**
1. Delete the `if candidate_pairs.is_empty() && informs_metadata.is_empty()` block (lines ~451–455 in current codebase).
2. Do NOT add a replacement early-return before Path C.
3. Retain the `if candidate_pairs.is_empty()` early-return that sits between Path C and Path B (line ~514 in current codebase). This one is correct and must not be removed.

The net effect: when both inputs are empty, the tick runs Path A (0 iterations), Path C (0 iterations + observability log fires with zeros), then Path B entry gate returns early. No write overhead. AC-19 satisfied unconditionally.

---

## Tick-Level Sequencing (ADR-003)

```
[Phase 5: caps applied — candidate_pairs truncated, informs_metadata truncated]
// REMOVED: joint early-return (was: if candidate_pairs.is_empty() && informs_metadata.is_empty())
=== PATH A: Structural Informs write loop ===
    for candidate in &informs_metadata: write_nli_edge(...)  // 0 iterations if empty
[Path A observability log — informs_edges_written, etc.]
=== PATH C: Cosine Supports write loop (NEW — Wave 3) ===
    category_map built from all_active (built once after Phase 2, in scope here)
    let cosine_supports_written = 0
    let cosine_supports_candidates = 0
    for (src_id, tgt_id, cosine) in &candidate_pairs: [guards + write_graph_edge]
    // 0 iterations if empty — but observability log fires unconditionally (AC-19)
[Path C observability log — cosine_supports_candidates, cosine_supports_edges_written]
=== PATH B entry gate ===
    if candidate_pairs.is_empty() { return; }  // RETAINED — guards NLI batch only
    let provider = match nli_handle.get_provider().await { ... }
[Phase 6/7/8: NLI Supports — unchanged]
```

The `timestamp` variable (declared as `let timestamp = current_timestamp_secs()` in
Path A) is reused in Path C. Do not call `current_timestamp_secs()` again in Path C.

---

## Key Test Scenarios

### AC-01: qualifying pair produces Supports edge with correct source

```
fn test_path_c_writes_supports_edge_for_qualifying_pair() {
    // candidate_pairs contains (lesson-learned entry, decision entry, cosine=0.70)
    // config.supports_cosine_threshold = 0.65
    // category_map has both entries
    // informs_category_pairs includes ["lesson-learned", "decision"]
    // existing_supports_pairs is empty
    // run tick
    // assert graph_edges has row: relation_type="Supports", source="cosine_supports"
    // assert NOT source="nli"
}
```

### AC-02: pair below threshold produces no edge

```
fn test_path_c_no_edge_below_threshold() {
    // candidate_pairs contains (lesson-learned, decision, cosine=0.60)
    // config.supports_cosine_threshold = 0.65
    // run tick
    // assert no Supports edge in graph_edges
}
```

### AC-03 (R-01): disallowed category pair above threshold produces no edge

```
fn test_path_c_no_edge_disallowed_category() {
    // candidate_pairs contains (decision, decision, cosine=0.80)
    // informs_category_pairs does NOT contain ["decision", "decision"]
    // run tick
    // assert no Supports edge
}
```

### R-01: missing entry in category_map does not panic

```
fn test_path_c_missing_entry_in_category_map() {
    // candidate_pairs contains (known_entry_id, unknown_entry_id, cosine=0.80)
    // category_map does NOT contain unknown_entry_id
    // run tick
    // assert no panic, no edge written, warn! emitted
    // loop continues for next pair
}
```

### R-06 (AC-19): observability log fires with zero counts when candidate_pairs is empty

```
fn test_path_c_observability_log_fires_when_empty() {
    // candidate_pairs is empty (or all below threshold)
    // run tick
    // assert debug! log emitted with cosine_supports_candidates=0 AND
    //   cosine_supports_edges_written=0
}
```

### R-07: false return from write_graph_edge does not emit warn, does not increment budget

```
fn test_path_c_false_return_not_error() {
    // Insert the pair first (pre-populate graph_edges)
    // candidate_pairs contains same pair above threshold
    // run tick
    // assert graph_edges still has exactly 1 row for the pair (no duplicate)
    // assert cosine_supports_written == 0 (false return not counted)
    // assert no warn! emitted at the loop level
}
```

### R-09: NaN cosine guard fires, loop continues

```
fn test_path_c_nan_cosine_produces_no_edge() {
    // candidate_pairs contains (id_a, id_b, f32::NAN)
    // run tick
    // assert no edge written for that pair
    // assert warn! emitted
    // assert loop continues (other qualifying pairs still processed)
}
```

### AC-12 (budget cap): 60 qualifying pairs produces exactly 50 edges

```
fn test_path_c_budget_cap_at_50() {
    // candidate_pairs contains 60 qualifying pairs (all above threshold, correct category)
    // existing_supports_pairs is empty
    // run tick
    // assert graph_edges has exactly 50 Supports rows with source="cosine_supports"
    // assert cosine_supports_written == 50
}
```

### AC-05: Path C runs when nli_enabled = false

```
fn test_path_c_runs_without_nli_enabled() {
    // config.nli_enabled = false (get_provider() returns Err → Path B exits early)
    // candidate_pairs has qualifying pairs
    // run tick
    // assert Supports edges written by Path C (source="cosine_supports")
    // assert NOT gated by nli_enabled
}
```

---

## Checklist

- [ ] Joint early-return (`if candidate_pairs.is_empty() && informs_metadata.is_empty()`) REMOVED (AC-19)
- [ ] Path B entry gate (`if candidate_pairs.is_empty()` after Path C) RETAINED — guards NLI batch only
- [ ] `MAX_COSINE_SUPPORTS_PER_TICK = 50` constant placed adjacent to `MAX_INFORMS_PER_TICK`
- [ ] TODO comment at constant site noting config-promotion as future extension (ADR-004)
- [ ] `category_map: HashMap<u64, String>` built from `all_active` BEFORE the loop
- [ ] Per-pair DB lookup absent from loop body
- [ ] Guard order: finite → threshold → budget → category → pre-filter
- [ ] `cosine_supports_candidates` incremented AFTER threshold gate, BEFORE budget gate
- [ ] Budget gate uses `break` (not `continue`) — terminates loop on exhaustion
- [ ] `write_graph_edge` called with `EDGE_SOURCE_COSINE_SUPPORTS` (not literal string)
- [ ] `timestamp` reused from Path A (not recomputed)
- [ ] Budget counter incremented ONLY on `true` return from `write_graph_edge` (rows_affected=1)
- [ ] `false` return (UNIQUE conflict or SQL error): no additional `warn!` at loop level
- [ ] Observability `tracing::debug!` placed AFTER loop, unconditional — fires even when candidate_pairs is empty
- [ ] Field names: `cosine_supports_candidates` and `cosine_supports_edges_written`
- [ ] Module doc comment updated with Path C section
- [ ] `use` import in tick file extended to include `write_graph_edge`
- [ ] `EDGE_SOURCE_COSINE_SUPPORTS` import verified in tick file
- [ ] 500-line extraction evaluated after implementation (NFR-07)
