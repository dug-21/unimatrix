# Agent Report: col-023-agent-1-pseudocode

## Summary

Produced 9 pseudocode files for col-023 (Observation Pipeline Generalization).
All 8 components from the implementation brief are covered. The OVERVIEW.md provides
the wave dependency order and shared type definitions.

## Files Produced

| File | Component | Wave |
|------|-----------|------|
| `pseudocode/OVERVIEW.md` | Cross-cutting overview | — |
| `pseudocode/observation-record.md` | observation-record | 1 |
| `pseudocode/domain-pack-registry.md` | domain-pack-registry | 2 |
| `pseudocode/rule-dsl-evaluator.md` | rule-dsl-evaluator | 2 |
| `pseudocode/config-extension.md` | config-extension | 2 |
| `pseudocode/detection-rules.md` | detection-rules | 3 |
| `pseudocode/metrics-extension.md` | metrics-extension | 3 |
| `pseudocode/schema-migration.md` | schema-migration | 3 |
| `pseudocode/ingest-security.md` | ingest-security | 4 |

## Critical Implementation Obligations (Forwarded to Implementation Agents)

1. **source_domain guard is mandatory first filter** in every domain-specific
   `DetectionRule::detect()` — 21 rules + `RuleEvaluator::detect()` (ADR-005, R-01).

2. **DomainPackRegistry must be threaded as Arc into SqlObservationSource** at startup.
   Failure causes `source_domain = "unknown"` for all claude-code events and all 21
   rules silently produce no findings (IR-01).

3. **compute_universal() must guard on source_domain == "claude-code"** before counting
   any events (IR-03).

4. **Wave compilation gates are non-negotiable**: `cargo check --workspace` after each
   wave (ADR-004). Merging before a wave compiles is prohibited.

5. **R-03 fixture gap**: every `ObservationRecord` in test code must supply a non-empty
   `source_domain`. Empty `source_domain: ""` causes false-green tests.

6. **EC-04 "unknown" is reserved**: `DomainPackRegistry::new()` must reject any pack
   with `source_domain = "unknown"` at startup.

7. **window_secs = 0 is rejected at startup** (Constraint 11, EC-08).

8. **Size check before parse, depth check after parse**: the 64 KB check operates on
   `input_str.len()` (raw bytes). The depth-10 check operates on the deserialized
   `serde_json::Value`.

## Open Questions

1. **`find_completion_boundary` signature**: the current helper accepts `&[ObservationRecord]`
   but after the source_domain guard, callers have `Vec<&ObservationRecord>`. Two options:
   (a) change helper to `&[&ObservationRecord]`; (b) keep signature and copy-collect.
   Option (a) is preferred but may cascade to other callers. Implementor should check
   all `find_completion_boundary` callsites before deciding.

2. **`DomainPackRegistry::iter_packs()`**: the startup wiring needs to iterate over all
   registered packs to add their categories to `CategoryAllowlist`. The pseudocode adds
   this method. Confirm the method name and signature with the implementation agent for
   `domain-pack-registry`.

3. **`Severity` enum variants**: `rule-dsl-evaluator` maps `"critical"` and `"error"`
   severity strings to `Severity::Critical` or `Severity::Warning`. The actual `Severity`
   enum variants must be verified against `unimatrix-observe/src/types.rs` before
   implementing `parse_severity()`.

4. **check_column_exists helper in migration.rs**: if the existing migration infrastructure
   already has a PRAGMA-based column check utility, use it. If not, the pseudocode in
   `schema-migration.md` provides an implementation pattern.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `observation pipeline detection rules patterns`
  (category: pattern) — found #2843 (HookType blast radius pattern, 59 files), #2907
  (ADR-005 source_domain guard), #2905 (ADR-003 rule evaluator DSL), #882 (best-effort
  optional computation for pipeline extensions). These were used to confirm the
  source_domain guard contract and wave plan in the pseudocode.
- Queried: `/uni-query-patterns` for `col-023 architectural decisions` (category: decision)
  — found #2906 (ADR-004 wave plan), #2903 (ADR-001 HookType generalization), #2908
  (ADR-006 UniversalMetrics canonical representation), #2909 (ADR-007 ingest security).
  All 7 ADRs for col-023 were found in Unimatrix and cross-referenced with the ADR files.
- Deviations from established patterns:
  - `DomainPackRegistry::new()` returns `Result<Self>` rather than infallible `Self` —
    this is a deliberate deviation for startup validation (R-09).
  - `parse_observation_rows()` takes an additional `registry: &DomainPackRegistry`
    parameter — the function signature changes from Wave 3 to Wave 4.
