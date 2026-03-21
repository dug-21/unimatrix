## ADR-006: UniversalMetrics Typed Struct as Canonical Representation (Option A)

### Context

SR-02 (scope risk assessment) flagged a representation ambiguity in the metrics layer:
SCOPE.md Phase 4 described `MetricVector.universal` becoming a `HashMap<String, f64>` "at
the logical level" while also claiming `UniversalMetrics` (the typed struct) stays as the
storage representation under Option B. Having two live representations of the same data
with independent serialization paths is a regression surface.

The existing `UniversalMetrics` struct in `unimatrix-store/src/metrics.rs` has 21 typed
fields (e.g., `total_tool_calls: u64`, `search_miss_rate: f64`, `bash_for_search_count: u64`).
The `UNIVERSAL_METRICS_FIELDS: &[&str]` const in the same file enumerates these 21 field
names and is used by the structural test (R-03/C-06) in `sqlite_parity.rs` to verify that
the SQL `OBSERVATION_METRICS` table columns exactly match the Rust struct fields.

Two options were evaluated for how extension domain metrics are stored:

**Option A: UniversalMetrics typed struct is canonical; extension via nullable JSON column**
- `UniversalMetrics` struct is unchanged — it remains the source of truth for claude-code metrics
- `OBSERVATION_METRICS` gains one new nullable column: `domain_metrics_json TEXT NULL`
- Extension domain metrics are stored as JSON in this column only
- `MetricVector.universal` remains `UniversalMetrics` — no HashMap at the logical level
- `UNIVERSAL_METRICS_FIELDS` test continues to verify exactly 21 + 1 = 22 columns (21 typed
  + 1 extension JSON column), with the extension column excluded from the field alignment check

**Option B: HashMap<String, f64> at logical level with typed struct as storage**
- `MetricVector.universal` becomes `HashMap<String, f64>`
- `UniversalMetrics` becomes a serialization adapter only
- All 21 typed accessors in `baseline.rs` must be rewritten as string lookups
- Risk: serialization round-trip correctness across v13→v14 boundary requires new aliases
- Risk: `UNIVERSAL_METRICS_FIELDS` structural test requires complete redesign

**Option A is chosen** (lower risk, preserves R-03/C-06 alignment, no baseline rewrite).

### Decision

`UniversalMetrics` typed struct remains the canonical representation for claude-code
metrics. There is exactly one representation of claude-code metric data — the typed struct.

Schema v14 adds a single nullable column to `OBSERVATION_METRICS`:
```sql
ALTER TABLE OBSERVATION_METRICS ADD COLUMN domain_metrics_json TEXT NULL;
```

Extension domain metrics (from non-claude-code domain packs) are stored as JSON objects
in `domain_metrics_json`. The format is a flat `{"key": value}` map where values are
numeric (f64). Nested structures are not supported in v1.

`MetricVector` in `unimatrix-store/src/metrics.rs` gains one new field:
```rust
pub struct MetricVector {
    pub computed_at: u64,
    pub universal: UniversalMetrics,     // unchanged
    pub phases: BTreeMap<String, PhaseMetrics>,  // unchanged
    pub domain_metrics: HashMap<String, f64>,    // NEW: empty for claude-code sessions
}
```

`domain_metrics` is empty (`HashMap::new()`) for all `"claude-code"` sessions, preserving
complete backward compatibility. Existing rows (schema v13 and below) read back with `NULL`
for `domain_metrics_json`, which deserializes as `HashMap::new()` via `#[serde(default)]`.

The `UNIVERSAL_METRICS_FIELDS` structural test is updated to:
1. Count: `UNIVERSAL_METRICS_FIELDS.len() + 1` SQL columns expected (the `+1` is
   `domain_metrics_json`)
2. Field alignment check: the 21 `UNIVERSAL_METRICS_FIELDS` names must match the first 21
   columns in declaration order; the 22nd column `domain_metrics_json` is verified
   separately by name (not included in field-name-to-column alignment)

`OUTCOME_INDEX` / `BaselineSet.universal` is already `HashMap<String, BaselineEntry>` —
it is string-keyed today and does not change. Existing serialized baseline data in
OUTCOME_INDEX naturally deserializes without migration.

### Consequences

**Easier:**
- No baseline.rs rewrite — all 21 typed accessors continue to work unchanged
- `UNIVERSAL_METRICS_FIELDS` structural test requires only a minor update (count + one
  extra column check), not a redesign
- Backward compatibility is guaranteed — existing MetricVector rows deserialize cleanly
- The v13→v14 migration is a single `ALTER TABLE ADD COLUMN` with NULL default — the
  simplest possible migration
- Claude-code sessions produce zero byte overhead in `domain_metrics_json` (NULL column)

**Harder:**
- Extension domain metrics have no typed accessor — callers must use the HashMap by string
  key; this is acceptable since external domains are not known at compile time
- The compute path for non-claude-code domain metrics must populate `domain_metrics` via a
  new computation hook in the domain pack (not via `compute_universal()` which remains
  claude-code specific)
- Future domain metrics that need typed accessors require adding a new optional struct
  field to `MetricVector` (not using the JSON blob) — acceptable as a future design
  decision when a domain warrants it
