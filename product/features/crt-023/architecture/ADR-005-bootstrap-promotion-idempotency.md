## ADR-005: Bootstrap Promotion Idempotency via COUNTERS Table String Key

### Context

FR-24 specifies a durable completion marker in the `COUNTERS` table with key `bootstrap_nli_promotion_done` to ensure the bootstrap edge promotion task is idempotent. SR-07 flags this as needing explicit architecture confirmation.

The `COUNTERS` table schema (from `counters.rs`):

```sql
CREATE TABLE counters (name TEXT PRIMARY KEY, value INTEGER NOT NULL)
```

The table uses `TEXT` primary key for arbitrary counter names. The `read_counter` / `set_counter` / `increment_counter` helpers in `crate::counters` operate on `&str` names and `u64` values.

A boolean "done" flag fits naturally as `set_counter(conn, "bootstrap_nli_promotion_done", 1)` (0 = not done, 1 = done). The `read_counter` helper returns 0 for a missing row, so the check is `read_counter(pool, "bootstrap_nli_promotion_done").await? == 0`.

**Timing of the promotion task:**

The product vision says "first background tick after startup." The background tick in `background.rs` runs after the server is fully initialized (HTTP listener up, stores open, vector index warm). This is the correct time to run the bootstrap promotion:

- HNSW is warm (loaded from disk) — neighbor lookups work correctly.
- `NliServiceHandle` may not yet be `Ready` on the very first tick (model loading takes ~5–30s depending on hardware and cache state). FR-25 specifies: if NLI is not ready at first tick, defer to the next tick where NLI IS ready.

The deferral mechanism: the promotion task checks `nli_handle.get_provider()`. If it returns `Err(NliNotReady)`, the task logs `tracing::info!("bootstrap NLI promotion deferred: NLI not ready")` and exits without setting the completion marker. On the next tick, it checks the completion marker first, finds it absent, and re-attempts.

**Zero-row case:**

On current production databases (crt-021 confirmed zero `bootstrap_only=1` rows due to unresolved AC-08 in W1-1), the promotion task:
1. Checks completion marker → absent (0)
2. Queries `GRAPH_EDGES WHERE bootstrap_only = 1 AND relation_type = 'Contradicts'`
3. Gets 0 rows back
4. Sets completion marker to 1 via `set_counter`
5. Done

This is a correct successful run. The marker prevents re-running on subsequent restarts.

**Transaction scope:**

The promotion task processes all bootstrap rows in a single operation: for each row, DELETE + conditionally INSERT. The completion marker is set in the same transaction as the final batch of edge operations. This ensures atomicity: if the transaction fails midway, neither the edge changes nor the marker are committed.

For large bootstrap sets (future-proofing), the task may batch rows. The completion marker is set only after all batches complete. If a batch fails, the marker is not set and the task re-runs on the next tick from the beginning (re-processing rows that were already deleted in previous batches is safe due to `INSERT OR IGNORE` idempotency, but re-processing DELETE-only rows against absent source rows is also safe).

### Decision

**`COUNTERS` table string key `bootstrap_nli_promotion_done`** with value `1` when complete, `0` / absent when not.

**Promotion task entry logic:**

```rust
async fn maybe_run_bootstrap_promotion(
    store: &Store,
    nli_handle: &NliServiceHandle,
    rayon_pool: &RayonPool,
    config: &InferenceConfig,
) {
    // Fast path: already done.
    let pool = store.write_pool_server();
    let done = counters::read_counter(pool, "bootstrap_nli_promotion_done")
        .await
        .unwrap_or(0);
    if done != 0 {
        return;
    }

    // Require NLI to be ready (FR-25).
    let provider = match nli_handle.get_provider().await {
        Ok(p) => p,
        Err(_) => {
            tracing::info!("bootstrap NLI promotion deferred: NLI not ready");
            return;
        }
    };

    run_bootstrap_promotion(store, provider, rayon_pool, config).await;
    // Marker set inside run_bootstrap_promotion on success.
}
```

**Completion marker set inside a write transaction** covering the last batch of edge modifications. If any error occurs before the transaction commits, the marker is not set and the next tick re-runs.

**`set_counter` is the correct primitive** — it uses `INSERT OR REPLACE` so calling it multiple times is idempotent (setting 1 when already 1 is a no-op at the SQLite level since the value does not change, and INSERT OR REPLACE handles the conflict).

### Consequences

**Easier:**
- No new schema required. The `COUNTERS` table already exists with the right design.
- The `read_counter` helper returning 0 for missing rows means the "first run" case is handled automatically without special NULL handling.
- Deferral (FR-25) is a natural early-return; the marker absence causes the task to re-run on the next tick.

**Harder:**
- The zero-row case and the non-zero-row case both set the same marker. If an operator wants to re-run the bootstrap promotion (e.g., after manually inserting `bootstrap_only=1` rows for testing), they must DELETE the counter row directly in SQLite. This is an admin-only operation; no CLI command is needed.
- The promotion task must acquire a write pool connection. On write pool contention (write pool is at most 2 connections), the task may wait. The background tick should handle this gracefully — a timeout or error in the promotion task leaves the marker absent and triggers a retry on the next tick.
