## ADR-001: Combined Audit Transaction on UnimatrixServer

### Context

vnc-001's `AuditLog::log_event()` opens its own redb write transaction for each audit event. redb serializes all write transactions -- only one can be active at a time across the entire database. For mutating tools like `context_store`, the current pattern would require two serial write transactions per call: one for the entry insert (with vector mapping and secondary indexes), and one for the audit event. This doubles write latency on the critical path for mutations. GH issue #11 identified this as a throughput bottleneck.

Three approaches were considered:
1. **Extend `AsyncEntryStore` with a generic `insert_with_audit` method** -- keeps the generic wrapper but leaks server-specific audit concerns into the core crate
2. **Expose `AuditLog::write_in_txn` and have tool handlers manage transactions directly** -- clean separation but moves complex transaction management into tool handler code
3. **Add `insert_with_audit` as a method on `UnimatrixServer`** -- keeps transaction coordination at the server level where all subsystems are available

### Decision

Add the combined operation as a method on `UnimatrixServer`, not on `AsyncEntryStore`. The method `UnimatrixServer::insert_with_audit(entry, embedding, audit_event)` coordinates the combined write transaction by:
1. Calling `spawn_blocking` with access to `Arc<Store>` and `Arc<AuditLog>`
2. Inside the blocking closure: opening one write transaction, inserting the entry (via Store directly), writing the audit event (via `AuditLog::write_in_txn`), committing once
3. After the write transaction: inserting the embedding into the HNSW index (which is a separate data structure, not in redb)

`AuditLog` gets a new method `write_in_txn(&self, txn: &WriteTransaction, event: AuditEvent) -> Result<u64, ServerError>` that writes an audit event into an existing transaction without committing it. This method reuses the same COUNTERS-based monotonic ID logic but operates on the caller's transaction.

`UnimatrixServer` gains an `Arc<Store>` field for direct transaction access.

### Consequences

**Easier:**
- Mutating tools perform one write transaction instead of two -- halves write latency for context_store
- Future mutating tools (vnc-003's `context_correct`) reuse the same pattern
- `AsyncEntryStore` remains generic and server-agnostic
- Audit event monotonic IDs and cross-session continuity are preserved (same COUNTERS key)

**Harder:**
- `UnimatrixServer` holds both `Arc<Store>` (raw) and `Arc<AsyncEntryStore>` (wrapped) -- two references to the same underlying store
- The `insert_with_audit` method bypasses the `EntryStore` trait for the insert operation, calling `Store` directly inside `spawn_blocking`
- Read-only tools still use the separate `audit.log_event()` pattern (unchanged) -- two different audit write paths exist
