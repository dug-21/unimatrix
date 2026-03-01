# Test Plan: observe-types

## Risk Coverage

| Risk | Scenarios |
|------|-----------|
| R-04 (MetricVector bincode breaks) | Roundtrip, defaults, phases, serde(default) contract |

## Unit Tests: `crates/unimatrix-observe/src/types.rs`

### Serialization (AC-32, R-04)

1. **test_metric_vector_roundtrip** -- Serialize MetricVector with populated fields, deserialize, assert_eq
2. **test_metric_vector_all_defaults** -- MetricVector::default(), serialize/deserialize, verify all zeros
3. **test_metric_vector_with_phases** -- MetricVector with 3 phase entries, roundtrip, verify phase names and values
4. **test_metric_vector_serde_default_annotations** -- Verify that deserializing from a MetricVector with all default universal metrics produces valid UniversalMetrics
5. **test_hooktype_serde_roundtrip** -- All 4 HookType variants serialize/deserialize correctly
6. **test_observation_record_serde** -- Full ObservationRecord roundtrip including Option fields

### Enum Display/Debug

7. **test_hotspot_category_variants** -- All 4 HotspotCategory variants are distinct
8. **test_severity_variants** -- All 3 Severity variants are distinct

### Error Type

9. **test_observe_error_display** -- Each ObserveError variant formats without leaking Rust types
