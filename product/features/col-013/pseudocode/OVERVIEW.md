# Pseudocode Overview: col-013 Extraction Rule Engine

## Component Interaction

```
unimatrix-core (types: ObservationRecord, HookType, ParsedSession, ObservationStats)
      |
      v
unimatrix-observe (extraction module: ExtractionRule trait, 5 rules, quality gate)
      |
      v
unimatrix-server (background.rs: tick loop -> maintenance_tick + extraction_tick)
      |
      v
unimatrix-engine (confidence.rs: trust_score "auto" => 0.35)
unimatrix-server/infra/contradiction.rs (check_entry_contradiction extracted)
```

## Data Flow

### Extraction Pipeline (per tick)
1. Query observations WHERE id > last_watermark
2. Group observations by feature_cycle (via session_id -> SESSIONS table)
3. For each ExtractionRule: rule.evaluate(observations, store) -> Vec<ProposedEntry>
4. For each ProposedEntry: quality_gate(entry, context) -> Accept | Reject(reason)
5. Accepted entries: store via Store API with trust_source="auto"
6. Update watermark to max(processed observation ids)

### Maintenance Tick (per tick)
1. Co-access stale pair cleanup (>30 day pairs)
2. Confidence refresh (batch 100 stale entries)
3. HNSW graph compaction (if stale_ratio > 10%)
4. Session GC (timed-out sessions)
5. Observation retention cleanup (>90 days)

## Shared Types

### New types in unimatrix-observe::extraction
- `ExtractionRule` trait: name() + evaluate(observations, store)
- `ProposedEntry`: title, content, category, topic, tags, source_rule, source_features, extraction_confidence
- `QualityGateResult`: Accept | Reject { reason, check_name }
- `ExtractionContext`: watermark (u64), rate_counter (u64), rate_hour (u64), extraction_stats
- `ExtractionStats`: entries_extracted_total, entries_rejected_total, last_extraction_run, rules_fired

### Moved types (unimatrix-observe::types -> unimatrix-core)
- ObservationRecord, HookType, ParsedSession, ObservationStats
- Re-exported from unimatrix-observe for backward compatibility

## Component List

| Component | Wave | Crates Modified |
|-----------|------|-----------------|
| type-migration | W1 | unimatrix-core, unimatrix-observe, unimatrix-engine, unimatrix-server |
| extraction-rules | W2 | unimatrix-observe |
| background-tick | W3 | unimatrix-server |

## Integration Harness Plan

### Existing suites to run
- smoke (mandatory gate)
- tools (context_status field changes)
- confidence (trust_score "auto")
- lifecycle (auto-entry persistence)
- contradiction (refactor regression)
- edge_cases (concurrent ops)

### New tests to add (Stage 3c)
- T-S10: test_status_reports_maintenance_fields (tools suite)
- T-S11: test_status_reports_extraction_stats (tools suite)
- T-S12: test_status_reports_coherence_by_source (tools suite)
- T-S13: test_status_maintain_true_silently_ignored (tools suite)
- C-21: test_auto_trust_entry_has_lower_confidence (confidence suite)
- L-26: test_auto_entry_searchable (lifecycle suite)
- L-27: test_auto_entry_in_briefing (lifecycle suite)

## Patterns Used

- DetectionRule trait pattern (from unimatrix-observe::detection) -> mirrored for ExtractionRule
- default_rules() pattern -> default_extraction_rules()
- spawn_blocking pattern (from existing maintenance, coaccess, contradiction scanning)
- tokio::time::interval pattern for background tick
- Re-export pattern (pub use) for backward-compatible type migration
