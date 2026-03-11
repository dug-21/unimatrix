# effectiveness-engine Pseudocode

## Purpose

Pure computation module for knowledge effectiveness analysis. Classifies entries into five categories, computes calibration buckets, aggregates by source. Zero I/O, fully deterministic. Lives at `crates/unimatrix-engine/src/effectiveness.rs`. Pattern follows `confidence.rs`.

## File: `crates/unimatrix-engine/src/lib.rs`

Add one line after existing module declarations:

```
pub mod effectiveness;
```

## File: `crates/unimatrix-engine/src/effectiveness.rs`

Estimated ~300 lines. No sub-modules needed.

### Constants

```
INEFFECTIVE_MIN_INJECTIONS: u32 = 3
OUTCOME_WEIGHT_SUCCESS: f64 = 1.0
OUTCOME_WEIGHT_REWORK: f64 = 0.5
OUTCOME_WEIGHT_ABANDONED: f64 = 0.0
NOISY_TRUST_SOURCES: &[&str] = &["auto"]   // ADR-004
```

### Types

All types derive Debug, Clone, Serialize. EffectivenessCategory also derives Copy, PartialEq, Eq.

```
enum EffectivenessCategory { Effective, Settled, Unmatched, Ineffective, Noisy }

struct EntryEffectiveness {
    entry_id: u64,
    title: String,
    topic: String,
    trust_source: String,
    category: EffectivenessCategory,
    injection_count: u32,
    success_rate: f64,        // weighted success rate from utility_score
    helpfulness_ratio: f64,   // helpful / (helpful + unhelpful), or 0.0 if no votes
}

struct SourceEffectiveness {
    trust_source: String,
    total_entries: u32,
    effective_count: u32,
    settled_count: u32,
    unmatched_count: u32,
    ineffective_count: u32,
    noisy_count: u32,
    aggregate_utility: f64,   // weighted success rate across all entries of this source
}

struct CalibrationBucket {
    confidence_lower: f64,
    confidence_upper: f64,
    entry_count: u32,
    actual_success_rate: f64,
}

struct DataWindow {
    session_count: u32,
    earliest_session_at: Option<u64>,
    latest_session_at: Option<u64>,
}

struct EffectivenessReport {
    by_category: Vec<(EffectivenessCategory, u32)>,
    by_source: Vec<SourceEffectiveness>,
    calibration: Vec<CalibrationBucket>,
    top_ineffective: Vec<EntryEffectiveness>,
    noisy_entries: Vec<EntryEffectiveness>,
    unmatched_entries: Vec<EntryEffectiveness>,
    data_window: DataWindow,
}
```

### Function: `utility_score`

```
pub fn utility_score(success: u32, rework: u32, abandoned: u32) -> f64

    total = success + rework + abandoned
    if total == 0:
        return 0.0

    weighted = (success as f64) * OUTCOME_WEIGHT_SUCCESS
             + (rework as f64) * OUTCOME_WEIGHT_REWORK
             + (abandoned as f64) * OUTCOME_WEIGHT_ABANDONED

    return weighted / (total as f64)
```

No overflow risk: u32 additions fit in u32 (max 3 * u32::MAX < u64::MAX, but since each count comes from SQL COUNT which caps at row count, practical overflow is impossible). Cast to f64 before division.

### Function: `classify_entry`

```
pub fn classify_entry(
    entry_id: u64,
    title: &str,
    topic: &str,
    trust_source: &str,
    helpful_count: u32,
    unhelpful_count: u32,
    injection_count: u32,       // distinct sessions where injected
    success_count: u32,
    rework_count: u32,
    abandoned_count: u32,
    topic_has_sessions: bool,   // whether any session exists for this topic
    noisy_trust_sources: &[&str],
) -> EntryEffectiveness

    // Compute derived values
    rate = utility_score(success_count, rework_count, abandoned_count)
    helpfulness_ratio = if (helpful_count + unhelpful_count) > 0:
        helpful_count as f64 / (helpful_count + unhelpful_count) as f64
    else:
        0.0

    // Classification priority: Noisy > Ineffective > Unmatched > Settled > Effective
    // (FR-01, R-01: first match wins)

    category = if noisy_trust_sources.contains(&trust_source)
                  AND helpful_count == 0
                  AND injection_count >= 1:
        Noisy

    else if injection_count >= INEFFECTIVE_MIN_INJECTIONS AND rate < 0.3:
        Ineffective

    else if injection_count == 0 AND topic_has_sessions:
        Unmatched

    else if NOT topic_has_sessions AND injection_count > 0 AND success_count > 0:
        // Topic has no sessions in available window, but entry has historical
        // success injection. Knowledge that served its era.
        Settled

    else:
        // Default: injected with acceptable success rate, or insufficient negative signal
        Effective

    return EntryEffectiveness {
        entry_id,
        title: title.to_string(),
        topic: topic.to_string(),
        trust_source: trust_source.to_string(),
        category,
        injection_count,
        success_rate: rate,
        helpfulness_ratio,
    }
```

Key points:
- Priority order is the if/else chain order (R-01)
- Noisy checks trust_source via NOISY_TRUST_SOURCES.contains() (ADR-004, R-10)
- Settled requires BOTH inactive topic AND at least one success injection (R-09, AC-03)
- Entries with zero injections and inactive topic that have NO success injection fall through to Effective (default)

### Function: `aggregate_by_source`

```
pub fn aggregate_by_source(entries: &[EntryEffectiveness]) -> Vec<SourceEffectiveness>

    // Group entries by trust_source
    groups: HashMap<String, Vec<&EntryEffectiveness>> = group entries by .trust_source

    result = Vec::new()
    for (source, group) in groups (sorted by source name for determinism):
        total_entries = group.len() as u32
        effective_count = count where .category == Effective
        settled_count = count where .category == Settled
        unmatched_count = count where .category == Unmatched
        ineffective_count = count where .category == Ineffective
        noisy_count = count where .category == Noisy

        // Aggregate utility: average success_rate across entries that have injections
        // (R-13: guard against division by zero when no entries have injections)
        injected_entries = group where .injection_count > 0
        aggregate_utility = if injected_entries is empty:
            0.0
        else:
            sum of .success_rate / injected_entries.len() as f64

        result.push(SourceEffectiveness { source, total_entries, effective_count,
            settled_count, unmatched_count, ineffective_count, noisy_count,
            aggregate_utility })

    return result
```

### Function: `build_calibration_buckets`

```
pub fn build_calibration_buckets(rows: &[(f64, bool)]) -> Vec<CalibrationBucket>

    // FR-04: 10 buckets of 0.1 width
    // Lower inclusive, upper exclusive, except last bucket [0.9, 1.0] inclusive both ends
    // R-04: boundary handling is critical

    buckets: array of 10 elements, each with (count: u32, weighted_sum: f64)
    initialize all to (0, 0.0)

    for (confidence, succeeded) in rows:
        // Determine bucket index
        index = if confidence >= 1.0:
            9                           // clamp to last bucket
        else if confidence < 0.0:
            0                           // clamp to first bucket
        else:
            min((confidence * 10.0).floor() as usize, 9)
            // floor(0.1 * 10) = 1, floor(0.9 * 10) = 9 -- correct
            // min guards against floating point producing 10

        buckets[index].count += 1
        buckets[index].weighted_sum += if succeeded: 1.0 else: 0.0

    result = Vec with capacity 10
    for i in 0..10:
        lower = i as f64 * 0.1
        upper = (i + 1) as f64 * 0.1
        count = buckets[i].count
        actual_rate = if count > 0:
            buckets[i].weighted_sum / count as f64
        else:
            0.0

        result.push(CalibrationBucket {
            confidence_lower: lower,
            confidence_upper: upper,
            entry_count: count,
            actual_success_rate: actual_rate,
        })

    return result
```

Note: The spec mentions using weighted outcomes (success=1.0, rework=0.5) for calibration actual_success_rate (FR-04). However, the calibration_rows from the store are `(f64, bool)` where bool = (outcome == 'success'). This means rework is treated as false (not success). This matches the store SQL which uses `(s.outcome = 'success') as succeeded`. If weighted calibration is desired, the store would need to return the outcome string instead of a bool. The architecture specifies `(f64, bool)` -- we follow the architecture. Rework counts as not-success in calibration buckets.

**Open question**: FR-04 says calibration uses "weighted outcomes: success=1.0, rework=0.5, abandoned=0.0" but the architecture's calibration_rows type is `(f64, bool)`. A bool cannot represent 0.5. The architecture type wins per our rules. If the spec intent was weighted calibration, the store query and type would need to change to `(f64, f64)` instead of `(f64, bool)`. Flagging this for review.

### Function: `build_report`

```
pub fn build_report(
    classifications: Vec<EntryEffectiveness>,
    calibration_rows: &[(f64, bool)],
    data_window: DataWindow,
) -> EffectivenessReport

    // Category counts
    by_category = Vec of (category, count) for all 5 categories in enum order
    for each category in [Effective, Settled, Unmatched, Ineffective, Noisy]:
        count = classifications where .category == category
        by_category.push((category, count))

    // Source aggregates
    by_source = aggregate_by_source(&classifications)

    // Calibration
    calibration = build_calibration_buckets(calibration_rows)

    // Top ineffective: up to 10, sorted by injection_count descending (ties broken by lowest success_rate)
    top_ineffective = classifications
        .filter(|e| e.category == Ineffective)
        .sort_by(|a, b| b.injection_count.cmp(&a.injection_count)
                         .then(a.success_rate.partial_cmp(&b.success_rate)))
        .take(10)
        .cloned()

    // Noisy: all entries classified as Noisy (no cap per spec)
    noisy_entries = classifications
        .filter(|e| e.category == Noisy)
        .cloned()

    // Unmatched: up to 10, sorted by topic then entry_id for determinism
    unmatched_entries = classifications
        .filter(|e| e.category == Unmatched)
        .sort_by(|a, b| a.topic.cmp(&b.topic).then(a.entry_id.cmp(&b.entry_id)))
        .take(10)
        .cloned()

    return EffectivenessReport {
        by_category,
        by_source,
        calibration,
        top_ineffective,
        noisy_entries,
        unmatched_entries,
        data_window,
    }
```

### Error Handling

All engine functions are infallible. No Result types. Degenerate inputs produce valid but empty/zero results:
- `utility_score(0, 0, 0)` -> 0.0
- `build_calibration_buckets(&[])` -> 10 empty buckets
- `build_report(vec![], &[], DataWindow { session_count: 0, .. })` -> report with all zero counts
- `aggregate_by_source(&[])` -> empty Vec

### Key Test Scenarios

1. **Classification priority overlap (R-01)**: auto entry with 0 helpful, 5 injections, 10% success -> Noisy (not Ineffective)
2. **Settled requires success injection (R-09, AC-03)**: inactive topic + historical success injection -> Settled; inactive topic + zero injections -> Effective (default); inactive topic + injections but all rework/abandoned -> Effective (not Settled)
3. **Noisy boundary (AC-05)**: auto + 0 helpful + 1 injection -> Noisy; auto + 1 helpful + 1 injection -> NOT Noisy; agent + 0 helpful + 1 injection -> NOT Noisy
4. **Ineffective boundary (AC-04)**: 2 injections + 0% -> NOT Ineffective; 3 injections + 29% -> Ineffective; 3 injections + 30% -> NOT Ineffective
5. **utility_score zero denominator (R-05)**: (0,0,0) -> 0.0
6. **utility_score mixed**: (2, 1, 1) -> (2.0 + 0.5 + 0.0) / 4.0 = 0.625
7. **Calibration bucket boundaries (R-04)**: conf=0.0 -> bucket 0; conf=0.1 -> bucket 1; conf=0.9 -> bucket 9; conf=1.0 -> bucket 9
8. **Empty data**: empty classifications -> report with all zero counts, 10 empty calibration buckets
9. **aggregate_by_source zero-injection trust source (R-13)**: source with entries but none injected -> aggregate_utility = 0.0
10. **Unattributed topic (AC-16)**: entry with empty topic is classified (topic appears as whatever caller passes, expected "(unattributed)")
