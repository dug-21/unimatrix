## ADR-003: Direct serde Derives on RetrospectiveReport — No DTO Shim

### Context

SR-01 raised the risk that `RetrospectiveReport` or its nested types might lack
`Serialize + Deserialize` derives, requiring a dedicated serializable DTO to bridge the gap.

The architecture audit (see ARCHITECTURE.md Serde Audit Result section) verified every
field type in `RetrospectiveReport` against `crates/unimatrix-observe/src/types.rs`:

All 23 types in the call tree carry `#[derive(Serialize, Deserialize)]`:
`RetrospectiveReport`, `MetricVector`, `UniversalMetrics`, `PhaseMetrics`, `HotspotFinding`,
`HotspotCategory`, `Severity`, `EvidenceRecord`, `BaselineComparison`, `BaselineStatus`,
`EntryAnalysis`, `HotspotNarrative`, `EvidenceCluster`, `Recommendation`, `SessionSummary`,
`FeatureKnowledgeReuse`, `AttributionMetadata`, `PhaseNarrative`, `PhaseCategoryComparison`,
`PhaseStats`, `ToolDistribution`, `GateResult`, `EntryRef`.

`MetricVector`, `UniversalMetrics`, and `PhaseMetrics` are defined in `unimatrix-store/src/
metrics.rs` and re-exported by `unimatrix-observe`. They have matching `serde` derives.

Existing round-trip tests in `unimatrix-observe/src/report.rs` validate the JSON
serialization path (`test_backward_compat_deserialization`, `test_entries_analysis_roundtrip`).
The `report.rs` test `test_backward_compat_deserialization` explicitly deserializes a JSON
blob without optional fields, confirming `#[serde(default)]` guards are correctly applied.

Two options were evaluated:

**Option A: DTO shim (`CycleReviewSummaryDto`)**
A separate struct mirroring `RetrospectiveReport` with only `Serialize + Deserialize` and no
domain logic. Would add ~60 fields of boilerplate, a mapping function, and maintenance burden
whenever `RetrospectiveReport` gains a new field. Appropriate only if the domain type is
not fully serializable.

**Option B: direct serde — serialize/deserialize RetrospectiveReport directly**
No mapping layer. `serde_json::to_string(&report)` at step 8a; `serde_json::from_str
::<RetrospectiveReport>(&record.summary_json)` at step 2.5 memoization hit. AC-16
(compile-time verification) is satisfied by the existing derives.

### Decision

Use direct serde on `RetrospectiveReport`. No DTO.

Write site (step 8a):
```rust
let summary_json = serde_json::to_string(&report)
    .map_err(|e| StoreError::Serialization(e.to_string()))?;
```

Read site (step 2.5 hit path):
```rust
let report: RetrospectiveReport = serde_json::from_str(&record.summary_json)
    .map_err(|e| StoreError::Deserialization(e.to_string()))?;
```

Deserialization failure at the read site is non-fatal: if `summary_json` cannot be
deserialized (e.g., a schema bump was applied but the stored record predates it and the
JSON is structurally incompatible), fall through to full pipeline recomputation with a
tracing warning. This is a defense-in-depth measure for the case where `schema_version` was
not incremented despite a breaking field change.

AC-16 compile-time enforcement: the write/read call sites themselves serve as the compile-
time check — they will fail to compile if `RetrospectiveReport` loses a `Serialize` or
`Deserialize` bound.

### Consequences

**Easier**:
- No maintenance burden from a parallel DTO struct.
- New fields added to `RetrospectiveReport` with `#[serde(default)]` automatically survive
  JSON round-trips without handler changes.
- Existing round-trip test coverage in `report.rs` validates the path.

**Harder**:
- `RetrospectiveReport` must maintain its serde derives permanently. Any future decision to
  remove serde from the type requires bumping `SUMMARY_SCHEMA_VERSION` and migrating stored
  records (or accepting that stored records become unreadable until `force=true`).
- If a non-serializable field is added to `RetrospectiveReport` in the future without a
  `#[serde(skip)]` annotation, the write site will fail to compile. This is a feature, not
  a bug — it prevents silent data loss. The rule: all new fields on `RetrospectiveReport`
  must be serde-compatible or explicitly skipped.
