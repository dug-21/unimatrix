## ADR-002: SQL Aggregation in `unimatrix-store` — `query_log.rs` Extension

### Context

`PhaseFreqTable::rebuild` needs to execute a SQL aggregation over `query_log` joined
with `entries`. There are two placement options:

**Option A — SQL in `services/phase_freq_table.rs`:** Execute the query directly in
the service module using `sqlx::query` with a `Store` reference (the store's
`read_pool()` is accessible).

**Option B — SQL in `unimatrix-store/src/query_log.rs`:** Add a new method
`query_phase_freq_table(&self, retention_cycles: u32) -> Result<Vec<PhaseFreqRow>>`
to the `SqlxStore` impl in the existing `query_log.rs` module.

The existing pattern in the codebase is consistent: SQL queries live in
`unimatrix-store`; `services/` modules call store methods and process the results.
`typed_graph.rs` calls `store.query_all_entries()` and `store.query_graph_edges()`.
`EffectivenessState` calls store methods from `background.rs`. All `query_log`
operations (`insert_query_log`, `scan_query_log_by_sessions`) live in `query_log.rs`.

Placing SQL in the service layer would cross the established crate boundary,
duplicate the store's read-pool access pattern, and make the query untestable via
the store's `TestDb` fixture (which `query_log.rs` tests use today).

The `500-line file limit` constraint is also relevant: if the SQL aggregation is
complex (multi-way JOIN with `json_each`, `feature_cycle` subquery, GROUP BY), it
adds ~30-50 lines. Placing it in the store keeps `phase_freq_table.rs` well under
the 500-line limit.

### Decision

Add `Store::query_phase_freq_table(retention_cycles: u32) -> Result<Vec<PhaseFreqRow>>`
as a new method on `SqlxStore` in `crates/unimatrix-store/src/query_log.rs`.

Define `PhaseFreqRow` as a plain struct in the same file:
```rust
pub struct PhaseFreqRow {
    pub phase: String,
    pub category: String,
    pub entry_id: u64,
    pub freq: i64,
}
```

`PhaseFreqTable::rebuild` in `services/phase_freq_table.rs` calls
`store.query_phase_freq_table(retention_cycles).await?` and converts the
`Vec<PhaseFreqRow>` to the internal `HashMap`.

### Consequences

**Easier:**
- SQL is testable via the store's `TestDb` fixture without spinning up a full
  `ServiceLayer` (AC-08 integration test runs in `unimatrix-store` tests).
- `phase_freq_table.rs` stays well under 500 lines.
- Follows existing codebase conventions; no new access pattern to audit.

**Harder:**
- `PhaseFreqRow` must be exported from `unimatrix-store` and imported in
  `unimatrix-server`. This adds one struct to the store's public API surface.
- `query_log.rs` grows by ~50-70 lines (struct + impl method + row mapping).
  It will remain well within 500 lines.
