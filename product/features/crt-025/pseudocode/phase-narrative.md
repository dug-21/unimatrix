# Component 9: Phase Narrative
## Files: `crates/unimatrix-observe/src/types.rs`, `crates/unimatrix-observe/src/phase_narrative.rs` (new), `crates/unimatrix-observe/src/lib.rs`

---

## Purpose

Three additions to `unimatrix-observe`:

1. **New types** in `types.rs`: `CycleEventRecord`, `PhaseNarrative`, `PhaseCategoryComparison`; extend `RetrospectiveReport` with `phase_narrative: Option<PhaseNarrative>`.
2. **New module** `phase_narrative.rs`: pure function `build_phase_narrative` that constructs a `PhaseNarrative` from raw query results.
3. **Module declaration** in `lib.rs` and public re-export of new types.

All logic is pure (no I/O, no `async`). The function takes slices/maps and returns a struct. Fully unit-testable without a database.

---

## 9a: `types.rs` — New Types

### `CycleEventRecord`

```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleEventRecord {
    pub seq:        i64,
    pub event_type: String,
    pub phase:      Option<String>,
    pub outcome:    Option<String>,
    pub next_phase: Option<String>,
    pub timestamp:  i64,
}
```

Mapped row-by-row from the `cycle_events` SQL query result. The `event_type` field will be one of `"cycle_start"`, `"cycle_phase_end"`, `"cycle_stop"`.

### `PhaseNarrative`

```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseNarrative {
    /// Ordered sequence of active phases (may repeat if phase was re-entered).
    pub phase_sequence: Vec<String>,

    /// Phases that appear more than once in phase_sequence (rework signal).
    pub rework_phases: Vec<String>,

    /// Count of feature_entries by (phase, category) for this feature.
    /// Outer key: phase token. Inner key: category. Value: count.
    pub per_phase_categories: HashMap<String, HashMap<String, u64>>,

    /// Cross-cycle comparison; None when fewer than 2 prior features have phase data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_cycle_comparison: Option<Vec<PhaseCategoryComparison>>,
}
```

### `PhaseCategoryComparison`

```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseCategoryComparison {
    pub phase:              String,
    pub category:           String,
    pub this_feature_count: u64,
    pub cross_cycle_mean:   f64,
    pub sample_features:    usize,
}
```

### Modified: `RetrospectiveReport`

Add at the end of the struct (after `attribution`):

```
/// Phase lifecycle narrative derived from CYCLE_EVENTS and FEATURE_ENTRIES (crt-025).
/// Absent from JSON (not null) when no cycle_events exist for the feature.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub phase_narrative: Option<PhaseNarrative>,
```

All existing construction sites of `RetrospectiveReport` must add `phase_narrative: None`. Since the field has `#[serde(default)]`, deserialization of old JSON (without the field) continues to work.

---

## 9b: `phase_narrative.rs` — New Module

### Type Aliases (module-local)

```
// Outer: phase name. Inner: category name. Value: count.
type PhaseCategoryDist = HashMap<String, HashMap<String, u64>>;

// Outer: feature_id. Inner: (phase, category) → count.
// Used only for cross-cycle computation.
type CrossDist = HashMap<String, HashMap<(String, String), u64>>;
```

The caller (component 2, `context_cycle_review` handler) builds these maps from raw SQL rows before calling `build_phase_narrative`.

### Function: `build_phase_narrative`

```
/// Build a PhaseNarrative from ordered cycle events and category distributions.
///
/// Pure function: no I/O, no panics. All inputs are &-refs; output is owned.
///
/// Parameters:
///   events:       cycle_events rows ordered by (timestamp ASC, seq ASC)
///   current_dist: feature_entries phase/category counts for the current feature
///   cross_dist:   feature_entries phase/category counts per prior feature
///                 (current feature already excluded by SQL WHERE clause)
pub fn build_phase_narrative(
    events:       &[CycleEventRecord],
    current_dist: &PhaseCategoryDist,
    cross_dist:   &HashMap<String, PhaseCategoryDist>,  // keyed by feature_id
) -> PhaseNarrative
```

### Pseudocode Body

```
FUNCTION build_phase_narrative(events, current_dist, cross_dist) -> PhaseNarrative:

    // === Part 1: Phase Sequence ===
    //
    // Walk events in order. A phase is "entered" when:
    //   - cycle_start has next_phase: the session is entering that phase
    //   - cycle_phase_end has next_phase: the next phase becomes active
    //   - cycle_phase_end has phase (and no next_phase): the phase being ended
    //     is appended (represents the phase that was just completed)
    //
    // Phase sequence records transitions in order. Repeated phases indicate rework.

    phase_sequence = Vec::new()
    seen_start_phase = None   // phase established by cycle_start.next_phase

    FOR event IN events (ordered):
        match event.event_type.as_str():
            "cycle_start" →
                IF let Some(np) = &event.next_phase:
                    // Record the phase being started
                    IF phase_sequence.last() != Some(np):   // avoid immediate duplicate
                        phase_sequence.push(np.clone())
                    seen_start_phase = Some(np.clone())

            "cycle_phase_end" →
                // The phase identified by `phase` is being ended
                // The phase identified by `next_phase` is being started
                IF let Some(p) = &event.phase:
                    // Append the completed phase if it's not already last
                    // (prevents duplicate when start + phase_end sequence is clean)
                    // Design decision: phase_sequence represents the ordered set of
                    // phases entered, not phases ended. Append next_phase when present.
                    pass   // phase (completed) is informational; not appended to sequence
                IF let Some(np) = &event.next_phase:
                    phase_sequence.push(np.clone())

            "cycle_stop" →
                // No phase to add; stop is the terminus
                pass

            _ → pass  // Unknown event types: skip

    // Note: if all events are cycle_stop with no prior cycle_start, phase_sequence is empty.
    // This is a valid state for orphaned events (R-13).

    // === Part 2: Rework Detection ===
    //
    // A phase is "rework" if it appears more than once in phase_sequence.

    counts: HashMap<String, usize> = count occurrences of each element in phase_sequence
    rework_phases = [phase for phase, count in counts if count > 1]
    rework_phases.sort()   // deterministic order

    // === Part 3: Per-Phase Categories ===
    //
    // Directly from current_dist (already a HashMap<phase, HashMap<category, count>>).
    // Clone it — no further computation needed.

    per_phase_categories = current_dist.clone()

    // === Part 4: Cross-Cycle Comparison ===
    //
    // cross_dist is keyed by feature_id, each value is a PhaseCategoryDist.
    // sample_features = number of distinct feature_ids with any phase data.
    //
    // FR-10.2: omit when fewer than 2 prior features have phase-tagged rows.

    sample_features = cross_dist.len()   // distinct feature_id count

    IF sample_features < 2:
        cross_cycle_comparison = None
    ELSE:
        // Build a flat map: (phase, category) → [count_per_feature]
        sums: HashMap<(phase, category), Vec<u64>> = HashMap::new()

        FOR (feature_id, dist) IN cross_dist:
            FOR (phase, cat_map) IN dist:
                FOR (category, count) IN cat_map:
                    sums.entry((phase.clone(), category.clone())).or_default().push(*count)

        // For each (phase, category) in the current feature, compute cross-cycle mean
        comparisons = Vec::new()
        FOR (phase, cat_map) IN current_dist:
            FOR (category, &this_count) IN cat_map:
                key = (phase.clone(), category.clone())
                IF let Some(cross_counts) = sums.get(&key):
                    n = cross_counts.len()
                    mean = cross_counts.iter().sum::<u64>() as f64 / n as f64
                    comparisons.push(PhaseCategoryComparison {
                        phase:              phase.clone(),
                        category:           category.clone(),
                        this_feature_count: this_count,
                        cross_cycle_mean:   mean,
                        sample_features:    n,
                    })
                ELSE:
                    // No prior features had data for this (phase, category) pair
                    comparisons.push(PhaseCategoryComparison {
                        phase:              phase.clone(),
                        category:           category.clone(),
                        this_feature_count: this_count,
                        cross_cycle_mean:   0.0,
                        sample_features:    0,
                    })

        // Sort for deterministic output
        comparisons.sort_by(|a, b| a.phase.cmp(&b.phase).then(a.category.cmp(&b.category)))
        cross_cycle_comparison = Some(comparisons)

    // === Return ===
    return PhaseNarrative {
        phase_sequence,
        rework_phases,
        per_phase_categories,
        cross_cycle_comparison,
    }
```

---

## Edge Cases

| Input | Expected Output |
|-------|----------------|
| `events` is empty slice | `phase_sequence = []`, `rework_phases = []`, no crash |
| Events start with `cycle_phase_end` (orphaned, no prior `cycle_start`) | Phase from `next_phase` added to sequence; no panic (R-13) |
| `current_dist` is empty | `per_phase_categories = {}`, `cross_cycle_comparison = None` (no pairs to compare) |
| 0 prior features in `cross_dist` | `cross_cycle_comparison = None` |
| 1 prior feature in `cross_dist` | `cross_cycle_comparison = None` (threshold not met, FR-10.2) |
| 2 prior features in `cross_dist` | `cross_cycle_comparison = Some(...)` |
| Phase appears twice in `phase_sequence` | That phase in `rework_phases` |

---

## 9c: `lib.rs` — Module Declaration and Re-exports

```
// Add module declaration:
pub mod phase_narrative;

// Add to re-exports (in pub use block or separate line):
pub use phase_narrative::build_phase_narrative;
pub use types::{
    // ... existing re-exports ...
    CycleEventRecord,
    PhaseCategoryComparison,
    PhaseNarrative,
};
```

---

## Key Test Scenarios

1. `build_phase_narrative([], {}, {})` → `PhaseNarrative { phase_sequence: [], rework_phases: [], per_phase_categories: {}, cross_cycle_comparison: None }`
2. Events: start(next_phase=scope) → phase_end(next_phase=design) → stop → `phase_sequence = ["scope", "design"]`
3. Events with re-entry: start(scope) → phase_end(next_phase=design) → phase_end(next_phase=scope) → `phase_sequence = ["scope", "design", "scope"]`, `rework_phases = ["scope"]`
4. Orphaned phase_end event (no prior start): no panic, phase_sequence includes next_phase if present
5. `RetrospectiveReport` with `phase_narrative = None` → serialized JSON has no `phase_narrative` key
6. `RetrospectiveReport` with `phase_narrative = Some(...)` → key present in JSON
7. Deserialization of pre-crt-025 JSON (no `phase_narrative` key) → `phase_narrative = None` (via `#[serde(default)]`)
8. Cross-cycle comparison with exactly 2 prior features → `Some(...)` returned
9. Cross-cycle comparison with 1 prior feature → `None` returned (threshold)
10. `per_phase_categories` mirrors `current_dist` exactly
11. `sample_features` in `PhaseCategoryComparison` reflects distinct feature count for that (phase, category) pair
