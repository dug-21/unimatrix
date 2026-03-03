# Component 2: Evidence-Synthesis — Test Plan

## Unit Tests (synthesis.rs)

### T-ES-01: synthesize_narratives produces one narrative per hotspot
- Input: 3 hotspots with varying evidence.
- Assert output has exactly 3 HotspotNarrative items.
- Each narrative.hotspot_type matches the corresponding hotspot.rule_name.

### T-ES-02: cluster_evidence groups by timestamp window
- Evidence with timestamps [1000, 1010000, 1020000, 60000000, 60010000]:
  - First 3 events within 30s of each other -> 1 cluster
  - Last 2 events within 30s of each other -> 1 cluster
- Assert 2 clusters returned.
- Assert first cluster: window_start=1000, event_count=3.
- Assert second cluster: window_start=60000000, event_count=2.

### T-ES-03: cluster_evidence with empty evidence
- Input: empty slice.
- Assert output: empty vec.

### T-ES-04: extract_sequence_pattern monotone increasing (AC-04)
- HotspotFinding with rule_name="sleep_workarounds".
- Evidence descriptions containing "sleep 30s", "sleep 60s", "sleep 90s", "sleep 120s".
- Assert sequence_pattern = Some("30s->60s->90s->120s").

### T-ES-05: extract_sequence_pattern non-monotone returns None (AC-04)
- Evidence with values [30, 60, 30, 120].
- Assert sequence_pattern = None.

### T-ES-06: extract_sequence_pattern non-sleep rule returns None
- HotspotFinding with rule_name="permission_retries".
- Assert sequence_pattern = None regardless of evidence content.

### T-ES-07: extract_top_files with > 5 distinct files
- Evidence referencing 8 distinct files.
- Assert top_files has exactly 5 items.
- Assert sorted by count descending.

### T-ES-08: build_summary is non-empty
- For every hotspot type (empty evidence, single event, multiple events):
- Assert summary is non-empty.

## Unit Tests (report.rs)

### T-ES-09: recommendations_for_hotspots templates (AC-05)
- Test each of the 4 recognized types:
  - "permission_retries" -> Recommendation with action containing "allowlist"
  - "coordinator_respawns" -> Recommendation with action containing "lifespan"
  - "sleep_workarounds" -> Recommendation with action containing "run_in_background"
  - "compile_cycles" with measured=15.0 -> Recommendation with action containing "incremental"
  - "compile_cycles" with measured=5.0 -> None (below threshold)
  - "unknown_type" -> None
- With empty hotspots -> empty vec.

## Unit Tests (types.rs)

### T-ES-10: RetrospectiveReport serde roundtrip with new fields
- Create report with narratives=Some([...]) and recommendations=[...].
- Serialize to JSON, deserialize back.
- Assert all fields preserved.

### T-ES-11: RetrospectiveReport serde skip_serializing_if
- Create report with narratives=None and recommendations=vec![].
- Serialize to JSON.
- Assert "narratives" key absent from JSON.
- Assert "recommendations" key absent from JSON.

### T-ES-12: Backward compat deserialization
- Deserialize pre-col-010b JSON (without narratives/recommendations fields).
- Assert narratives = None, recommendations = vec![].

## Integration Tests

### AC-03: Structured path narratives
- When structured events path is used: narratives is Some.
- JSONL fallback: narratives is None.
- (Note: current implementation uses JSONL path, so narratives = None.
  Test verifies None on JSONL path. Structured path test deferred until
  from_structured_events() is the active code path.)

### AC-04: Sequence pattern extraction
- Covered by T-ES-04 and T-ES-05 (unit tests).

### AC-05: Recommendation templates
- Covered by T-ES-09 (unit test).
