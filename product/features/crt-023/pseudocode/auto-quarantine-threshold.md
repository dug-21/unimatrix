# Auto-Quarantine Threshold Guard — Pseudocode

**File**: `crates/unimatrix-server/src/background.rs` (modified — auto-quarantine path only)

**Purpose**: Extend the background tick's auto-quarantine logic to apply a higher confidence
threshold when an entry is penalized exclusively by NLI-origin `Contradicts` edges. Entries
penalized by a mix of NLI-origin and manually-curated edges continue to follow the existing
auto-quarantine logic unchanged. Implements FR-22b and ADR-007.

**Context**: The existing auto-quarantine logic (crt-018b, entry #1544) computes a topology
penalty from `GRAPH_EDGES` and quarantines entries exceeding a threshold. crt-023 adds a
second, higher bar: if an entry's penalty comes ONLY from NLI-origin `Contradicts` edges,
those edges must all carry an NLI score exceeding `nli_auto_quarantine_threshold` (default 0.85)
before quarantine fires.

---

## Current Auto-Quarantine Logic (existing, do not change)

For reference. The extension adds a guard before the quarantine decision, not a replacement.

```
// Existing pattern (pseudocode of current behavior):
for each entry_candidate_for_quarantine:
    topology_penalty = compute_topology_penalty(entry.id, typed_graph, ...)
    if topology_penalty meets auto_quarantine_criteria:
        quarantine(entry)   // crt-018b hold-on-error remains in effect
```

---

## Extension: NLI-Only Penalty Classification

The extension inserts a guard before the `quarantine(entry)` call. Only entries that pass the
existing auto-quarantine criteria are evaluated by this guard.

```
/// Check whether an entry that meets the existing auto-quarantine criteria should be
/// protected from quarantine because its penalty is driven exclusively by NLI-origin edges
/// that have not yet reached the higher nli_auto_quarantine_threshold.
///
/// Returns true  -> quarantine MAY proceed (existing logic applies)
/// Returns false -> quarantine BLOCKED (NLI-only penalty, insufficient score)
async fn nli_auto_quarantine_allowed(
    store: &Store,
    entry_id: u64,
    nli_contradiction_threshold: f32,     // edge creation threshold (lower bar)
    nli_auto_quarantine_threshold: f32,   // quarantine threshold (higher bar, ADR-007)
) -> bool
    // Step 1: Fetch all Contradicts edges for this entry as source.
    // We check edges where this entry is the "accused" (has Contradicts edges pointing at it,
    // or originating from it, depending on GRAPH_EDGES directionality conventions).
    // Check ARCHITECTURE.md / GRAPH_EDGES schema for directionality: source_id = penalized entry.

    let contradicts_edges = match store.query_contradicts_edges_for_entry(entry_id).await:
        Ok(edges) -> edges
        Err(e)    ->
            tracing::warn!(entry_id=entry_id, error=%e,
                          "auto-quarantine NLI guard: failed to query edges; allowing quarantine")
            return true  // conservative: allow quarantine on DB error

    if contradicts_edges.is_empty():
        return true  // no Contradicts edges; existing logic applies normally

    // Step 2: Classify edge origins.
    // Split into NLI-origin edges (source='nli') and non-NLI-origin edges (source != 'nli').
    let (nli_edges, non_nli_edges): (Vec<_>, Vec<_>) = contradicts_edges.iter()
        .partition(|edge| edge.source == "nli")

    // Step 3: If there are ANY non-NLI-origin edges (manually curated, bootstrap, etc.),
    // the penalty is "mixed" — existing auto-quarantine logic applies unchanged (ADR-007).
    if !non_nli_edges.is_empty():
        return true  // mixed origin; apply existing logic

    // Step 4: Penalty is from NLI-origin edges ONLY.
    // Apply the higher bar: ALL NLI-origin Contradicts edges must exceed nli_auto_quarantine_threshold.
    //
    // Read nli_contradiction score from each edge's metadata JSON.
    // metadata format: '{"nli_entailment": f32, "nli_contradiction": f32}'
    for edge in &nli_edges:
        let nli_contradiction_score = match parse_nli_contradiction_from_metadata(&edge.metadata):
            Some(score) -> score
            None        ->
                // Metadata missing or malformed: be conservative, allow quarantine.
                tracing::debug!(entry_id=entry_id, edge_id=edge.id,
                               "auto-quarantine NLI guard: missing metadata score; allowing quarantine")
                return true

        // Higher bar check: if any NLI edge score is BELOW nli_auto_quarantine_threshold,
        // do NOT quarantine (the NLI signal is not sufficiently confident).
        if nli_contradiction_score < nli_auto_quarantine_threshold:
            tracing::debug!(
                entry_id=entry_id,
                nli_contradiction_score=nli_contradiction_score,
                nli_auto_quarantine_threshold=nli_auto_quarantine_threshold,
                "auto-quarantine NLI guard: NLI-only penalty below higher threshold; quarantine blocked"
            )
            return false  // block quarantine

    // All NLI-origin edges exceed nli_auto_quarantine_threshold: allow quarantine.
    true
```

---

## `parse_nli_contradiction_from_metadata` (private helper)

```
fn parse_nli_contradiction_from_metadata(metadata: &Option<String>) -> Option<f32>
    let json_str = metadata.as_deref()?  // None metadata -> None (conservative: allow quarantine)
    let value: serde_json::Value = serde_json::from_str(json_str).ok()?
    value.get("nli_contradiction")?.as_f64().map(|v| v as f32)
```

---

## Integration Into Background Tick Auto-Quarantine Path

The existing auto-quarantine path in `background.rs` is modified to call this guard. The
existing hold-on-error behavior (crt-018b, entry #1544) is unaffected.

```
// In background.rs, in the auto-quarantine logic block:
// (pseudocode; integrate with existing control flow)

for each entry_candidate for auto-quarantine:
    // ... existing topology penalty computation ...

    if meets_existing_auto_quarantine_criteria(entry_id):
        // crt-023: Apply NLI-only higher threshold guard (ADR-007, FR-22b).
        // Skip this guard entirely if NLI is disabled (config.inference.nli_enabled = false).
        let quarantine_allowed = if config.inference.nli_enabled {
            nli_auto_quarantine_allowed(
                &store,
                entry_id,
                config.inference.nli_contradiction_threshold,
                config.inference.nli_auto_quarantine_threshold,
            ).await
        } else {
            true  // NLI disabled: existing logic applies unchanged
        }

        if quarantine_allowed:
            // Proceed with existing quarantine logic (crt-018b hold-on-error unchanged)
            quarantine(entry_id, ...)
```

---

## `store.query_contradicts_edges_for_entry` (new Store method)

```
/// Fetch all Contradicts edges for the given entry (as source).
/// Returns edge rows including the source field and metadata column.
pub async fn query_contradicts_edges_for_entry(
    &self,
    entry_id: u64,
) -> Result<Vec<GraphEdgeRow>, StoreError>
    let conn = self.read_pool().acquire().await?
    // Note: directionality depends on GRAPH_EDGES schema convention.
    // Verify whether the penalized entry is source_id or target_id for a Contradicts edge.
    // Assumption: source_id = entry that "contradicts" the target (the penalized entry is target_id).
    // Verify this against the crt-021 GRAPH_EDGES schema before implementation.
    let rows = conn.query(
        "SELECT id, source_id, target_id, source, metadata \
         FROM graph_edges \
         WHERE target_id = ?1 AND relation_type = 'Contradicts'",
        params![entry_id]
    )?
    .map(|row| Ok(GraphEdgeRow {
        id:        row.get::<u64>(0)?,
        source_id: row.get::<u64>(1)?,
        target_id: row.get::<u64>(2)?,
        source:    row.get::<String>(3)?,
        metadata:  row.get::<Option<String>>(4)?,
    }))
    .collect::<Result<Vec<_>, _>>()?
    Ok(rows)

struct GraphEdgeRow {
    id:        u64,
    source_id: u64,
    target_id: u64,
    source:    String,         // 'nli', 'bootstrap', 'manual', etc.
    metadata:  Option<String>, // JSON or None
}
```

**OPEN QUESTION (flag for implementation)**: The directionality convention for `Contradicts`
edges in `GRAPH_EDGES` must be verified against the crt-021 schema. The auto-quarantine check
queries edges where the PENALIZED entry is the target (the hypothesis in NLI terminology). If
the crt-021 schema uses a different convention (e.g., the penalized entry is source_id), adjust
the WHERE clause accordingly. This must be verified before coding `query_contradicts_edges_for_entry`.

---

## Config Access in background.rs

`background.rs` already imports `InferenceConfig` or has access to config via `AppState`. The
new fields `nli_enabled`, `nli_contradiction_threshold`, and `nli_auto_quarantine_threshold`
are accessed from the existing config object — no new wiring needed beyond the InferenceConfig
extension.

---

## Error Handling

| Failure | Behavior |
|---------|----------|
| `query_contradicts_edges_for_entry` fails | Log warn; return `true` (allow quarantine — conservative) |
| Metadata JSON missing or malformed | Return `true` (allow quarantine — conservative) |
| `nli_enabled = false` | Guard not called; existing logic runs unchanged |
| Mixed NLI + non-NLI edges | Return `true` (allow quarantine — existing logic) |
| All NLI edges below `nli_auto_quarantine_threshold` | Return `false` (block quarantine) |
| All NLI edges above `nli_auto_quarantine_threshold` | Return `true` (allow quarantine) |

The conservative default (return `true` on errors) means the guard never silently suppresses
quarantine due to infrastructure failures. It only suppresses quarantine when there is positive
evidence that all NLI scores are below the higher threshold.

---

## Key Test Scenarios

1. **AC-25 / NLI-only below threshold**: Write NLI `Contradicts` edges with `nli_contradiction=0.7` (above `nli_contradiction_threshold=0.6`, below `nli_auto_quarantine_threshold=0.85`); run background tick; assert entry NOT auto-quarantined (R-10).
2. **AC-25 / NLI-only above threshold**: Write NLI `Contradicts` edges with `nli_contradiction=0.9` (above `nli_auto_quarantine_threshold=0.85`); run background tick; assert existing auto-quarantine logic can fire.
3. **AC-25 / mixed edges**: Write one NLI `Contradicts` edge (score 0.7) and one manual `Contradicts` edge; assert existing auto-quarantine logic applies (guard returns `true` for mixed origin).
4. **AC-25 / no NLI edges**: Entry with only manually-curated edges; guard returns `true` unconditionally; existing logic applies.
5. **R-10 / cascade prevention**: Store one entry; mock NLI to return `contradiction=0.9` for all 10 neighbors (cap = 10 writes max_contradicts_per_tick); run background tick; assert no auto-quarantine due to insufficient NLI scores (entries below `nli_auto_quarantine_threshold` are protected).
6. **NLI disabled**: `nli_enabled = false`; guard not called; existing logic runs; assert behavior identical to pre-crt-023.
7. **Metadata malformed**: NLI edges with NULL or invalid metadata JSON; guard returns `true` (conservative); existing logic may quarantine.
