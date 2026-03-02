# Test Plan: structured-retrospective

Component: Structured Retrospective (P1)
Covers: AC-12, AC-13, AC-14 (persistence), AC-17, AC-18, AC-19
Risks: R-04, R-08

---

## Unit Tests

### Narrative synthesis

```
test_cluster_evidence_by_window_basic
  - 4 EvidenceRecords at ts: 0ms, 10000ms, 20000ms, 35000ms (CLUSTER_WINDOW_SECS=30)
  - Assert: 2 clusters — [0,10s,20s] (window 0-29s) and [35s] (window 35-64s)

test_cluster_evidence_single_event
  - 1 event at ts=5000ms
  - Assert: 1 cluster, event_count=1

test_cluster_evidence_empty_input
  - 0 events
  - Assert: empty Vec

test_cluster_evidence_all_in_one_window
  - 5 events at ts: 0, 5000, 10000, 15000, 29000 (all within 30s)
  - Assert: 1 cluster, event_count=5

test_extract_sleep_sequence_monotone_increasing  (AC-18)
  - Evidence with sleep details: 30s, 60s, 90s, 120s
  - Assert: Some("30s->60s->90s->120s")

test_extract_sleep_sequence_non_monotone  (AC-18)
  - Durations: 30, 60, 50, 90
  - Assert: None

test_extract_sleep_sequence_single_value
  - 1 sleep event
  - Assert: None (need >= 2)

test_extract_sleep_sequence_no_sleep_events
  - Evidence with no parseable sleep durations
  - Assert: None

test_recommendations_for_permission_retries  (AC-19)
  - hotspots = [HotspotFinding { rule_name: "permission_retries", measured: 5.0 }]
  - Assert: Vec with 1 Recommendation, non-empty action

test_recommendations_for_coordinator_respawns  (AC-19)
  - hotspots = [HotspotFinding { rule_name: "coordinator_respawns", measured: 3.0 }]
  - Assert: Vec with 1 Recommendation

test_recommendations_for_sleep_workarounds  (AC-19)
  - hotspots = [HotspotFinding { rule_name: "sleep_workarounds", measured: 8.0 }]
  - Assert: Vec with 1 Recommendation

test_recommendations_compile_cycles_above_threshold  (AC-19)
  - hotspots = [HotspotFinding { rule_name: "compile_cycles", measured: 12.0 }]
  - Assert: Vec with 1 Recommendation

test_recommendations_compile_cycles_below_threshold  (AC-19)
  - hotspots = [HotspotFinding { rule_name: "compile_cycles", measured: 8.0 }]
  - Assert: empty Vec (threshold is 10.0)

test_recommendations_unrecognized_hotspot_type  (AC-19)
  - hotspots = [HotspotFinding { rule_name: "unknown_type", measured: 99.0 }]
  - Assert: empty Vec

test_recommendations_empty_hotspots  (AC-19)
  - hotspots = []
  - Assert: empty Vec
```

### RetrospectiveReport::empty

```
test_retrospective_report_empty
  - RetrospectiveReport::empty("test-fc")
  - Assert: feature_cycle == "test-fc"
  - Assert: session_count == 0
  - Assert: total_records == 0
  - Assert: hotspots is empty
  - Assert: recommendations is empty
  - Assert: narratives == Some([])  (structured path indicator)
```

### Crate boundary (R-08)

```
test_observe_crate_no_store_dependency
  - Verify: unimatrix-observe Cargo.toml does NOT list unimatrix-store as dependency
  - (This is a build-time check; if it compiles, the boundary is maintained)
```

---

## Integration Tests

### from_structured_events / path selection (AC-12, AC-13)

```
test_structured_path_excludes_abandoned_sessions  (AC-12)
  - Insert 5 sessions for "fc-test":
    3 Completed (with injections), 1 Abandoned, 1 TimedOut
  - Call context_retrospective("fc-test") (structured path)
  - Assert: report.session_count == 3
  - Assert: Abandoned and TimedOut sessions NOT counted

test_structured_path_used_when_sessions_exist  (AC-13a)
  - Populate SESSIONS with 2 Completed sessions for "fc-struct-test"
  - Call context_retrospective("fc-struct-test")
  - Assert: debug log indicates "structured" path used
  - Assert: report.narratives == Some([...]) (structured path indicator)

test_jsonl_fallback_when_sessions_empty  (AC-13c)
  - No SESSIONS entries for "fc-jsonl-test"; but JSONL files exist for this feature_cycle
  - Call context_retrospective("fc-jsonl-test")
  - Assert: JSONL path used (report.narratives == None)

test_empty_report_when_no_data  (AC-13b)
  - No SESSIONS and no JSONL for "fc-empty"
  - Call context_retrospective("fc-empty")
  - Assert: report.session_count == 0, hotspots is empty

test_structured_path_narratives_present  (AC-17)
  - Populate SESSIONS + INJECTION_LOG for feature_cycle with hotspot-triggering data
  - Call context_retrospective without evidence_limit
  - Assert: report.narratives is Some
  - Assert: each narrative.summary is non-empty string
  - Assert: each hotspot.evidence.len() <= 3 (default evidence_limit)

test_jsonl_path_narratives_absent
  - JSONL-only feature_cycle (no SESSIONS)
  - Assert: report.narratives == None
```

### Recommendations integration

```
test_retrospective_report_includes_recommendations
  - Arrange: sessions with permission_retries hotspot triggers
  - call context_retrospective
  - Assert: report.recommendations is non-empty
  - Assert: first recommendation has non-empty action

test_build_report_jsonl_path_includes_empty_recommendations
  - JSONL-path retrospective (existing behavior)
  - Assert: report.recommendations is present (possibly empty Vec)
  - Assert: serde JSON includes "recommendations": [] field
```

---

## Type Compatibility Tests

```
test_retrospective_report_serde_with_narratives
  - Build report with narratives=Some(vec![...])
  - serde_json::to_string → serde_json::from_str
  - Assert: narratives field preserved; skip_serializing_if works for None case

test_retrospective_report_serde_without_narratives
  - Build report with narratives=None
  - Serialize to JSON
  - Assert: "narratives" key is absent from JSON output (skip_serializing_if = "Option::is_none")

test_observation_record_new_fields_default
  - Deserialize ObservationRecord from JSON without confidence_at_injection/session_outcome
  - Assert: both fields are None (serde default)
```
