## ADR-002: Internal Store Path via UnimatrixServer::clone() for System-Generated Entries

### Context

col-010b introduced `write_lesson_learned()`, a ~180-line free function that reimplements the entry store pipeline outside of `UnimatrixServer`. It manually orchestrates: `Store::insert` (ENTRIES + 5 indexes), then a separate `Store::put_vector_mapping` transaction, then a separate `Store::get` + `Store::update` round-trip to set `embedding_dim`, then `VectorIndex::insert_hnsw_only`, then separate get/update pairs for the supersede chain, then a separate confidence seed. This is six separate write transactions for what `insert_with_audit` accomplishes atomically in one.

This duplication exists because the fire-and-forget `tokio::spawn` in `context_retrospective` cannot capture `&self` (the `UnimatrixServer` reference). The original implementation worked around this by cloning individual `Arc` fields (`store`, `embed_service`, `categories`, `vector_index`, `adapt_service`) and passing them as arguments to a standalone function that rebuilds the pipeline from raw parts.

**Problems with the current approach:**

1. **Atomicity gap.** `Store::insert` does not write VECTOR_MAP. The separate `put_vector_mapping` call is the exact non-atomic pattern that GH #14 identified and `insert_with_audit` was designed to fix (see vnc-003 ADR-001). A crash between `Store::insert` and `put_vector_mapping` leaves an entry without a vector mapping.

2. **No audit trail.** `write_lesson_learned` produces no audit event. System-generated entries are invisible in the audit log, making debugging and compliance harder.

3. **Drift risk.** Any change to the store pipeline (new indexes, new validation, new side effects like OUTCOME_INDEX writes or adaptation prototype updates) must be manually duplicated in `write_lesson_learned`. The pipeline has already grown twice since vnc-002 (OUTCOME_INDEX in col-001, adaptation prototypes in crt-006), and `write_lesson_learned` missed the adaptation prototype update.

4. **Extra round-trips.** The `embedding_dim` update requires a get-modify-update cycle because `Store::insert` does not accept an embedding. `insert_with_audit` does not need this because it handles the embedding in the same transaction.

5. **Reusability.** Future system-generated entries (col-005 auto-knowledge extraction, cortical auto-corrections, potential auto-pattern discovery) would each need to duplicate this same workaround.

**The `&self` constraint:** `insert_with_audit` is `pub(crate) async fn` on `&self`. A `tokio::spawn` closure requires `'static` — it cannot capture `&self`. However, `UnimatrixServer` derives `Clone`, and every field is `Arc`-wrapped. Cloning the server is a handful of `Arc::clone` calls with no data duplication. This is the standard Tokio pattern for spawning tasks that need access to shared server state.

Three alternatives were evaluated:

1. **Extract a freestanding `internal_insert` function** that takes all subsystem Arcs as arguments. This is essentially what exists today but cleaned up. It avoids the clone but still requires manually passing 8+ Arc parameters and keeping the function signature in sync with pipeline changes.

2. **Wrap `UnimatrixServer` in `Arc<Self>` and change all methods to take `self: &Arc<Self>`**. This is a large refactor that changes every tool handler signature. The rmcp `#[tool]` macro generates handlers on `&self`, and changing to `Arc<Self>` would require either forking the macro or adding an extra indirection layer.

3. **Clone `self` inside the `tokio::spawn` closure** and call `insert_with_audit` on the clone. This uses the existing `Clone` derive, requires no signature changes, and gives the spawned task full access to the same pipeline that `context_store` uses.

### Decision

System-generated entries (starting with lesson-learned auto-persistence) use the existing `insert_with_audit` pipeline by cloning the `UnimatrixServer` into the `tokio::spawn` closure. The `write_lesson_learned` free function is replaced with a method on `UnimatrixServer` or, at minimum, the spawned task calls `server_clone.insert_with_audit(...)` directly.

The pattern:

```rust
// In context_retrospective handler, where self: &UnimatrixServer
let server = self.clone(); // Arc clones only, no data duplication
tokio::spawn(async move {
    // 1. Embed (using server.embed_service, server.adapt_service)
    // 2. Supersede check + deprecation (using server.store)
    // 3. server.insert_with_audit(new_entry, embedding, audit_event)
    //    → atomic ENTRIES + indexes + VECTOR_MAP + audit
    //    → HNSW insert after commit
    // 4. Supersede chain linking (using server.store)
    // 5. Confidence seed (using server.store)
    // 6. Adaptation prototype update (using server.adapt_service)
});
```

Steps 1, 2, 4, 5, and 6 remain as they are (they are domain-specific to lesson-learned supersede logic). Step 3 replaces the manual `Store::insert` + separate `put_vector_mapping` + separate `embedding_dim` update with a single `insert_with_audit` call.

Content scanning (step 6 of `context_store`) is intentionally skipped: lesson-learned content is system-generated from retrospective data, not user-supplied. Near-duplicate detection (step 8 of `context_store`) is intentionally skipped: the supersede-by-topic check handles deduplication for this category. Identity resolution and capability checks are intentionally skipped: this is an internal system operation, not an agent request.

### Consequences

**Easier:**
- Lesson-learned entries get atomic ENTRIES + VECTOR_MAP writes (fixing the GH #14 regression).
- Lesson-learned entries appear in the audit log with `operation: "context_retrospective/lesson-learned"` and `agent_id: "cortical-implant"`.
- `embedding_dim` is set correctly without a separate round-trip (the HNSW data_id allocation in `insert_with_audit` handles the VECTOR_MAP write, and the caller can set `embedding_dim` on the `NewEntry` or the record can be updated once after insert).
- Future pipeline additions (new indexes, new side effects) automatically apply to system-generated entries.
- Future auto-persistence features clone the same pattern: `self.clone()` into `tokio::spawn`, call `insert_with_audit`.
- The `write_lesson_learned` free function shrinks from ~180 lines to ~60 lines (embed + supersede check + one `insert_with_audit` call + supersede chain + confidence seed).

**Harder:**
- Each `tokio::spawn` for lesson-learned writes clones ~12 Arcs. This is negligible (Arc clone is an atomic increment, ~2ns each) but is worth noting for documentation.
- `insert_with_audit` currently does not set `embedding_dim` on the `EntryRecord` it creates (it is hardcoded to 0). The lesson-learned path needs `embedding_dim` set to the actual embedding length. This requires either: (a) adding an `embedding_dim` field to `NewEntry`, or (b) doing a post-insert update for `embedding_dim`. Option (a) is preferred as it keeps the pipeline clean.
- The supersede chain linking (steps 4 in the pattern above) and confidence seeding (step 5) remain as separate transactions after `insert_with_audit`. These could be folded into `insert_with_audit` in the future but are out of scope for this change.
