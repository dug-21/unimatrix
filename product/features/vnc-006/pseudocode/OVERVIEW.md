# Pseudocode Overview: vnc-006 — Service Layer + Security Gateway

## Component Interaction

```
UnimatrixServer
  |-- services: ServiceLayer (new field)
  |     |-- search: SearchService
  |     |     |-- gateway: Arc<SecurityGateway>
  |     |     |-- store, vector_store, entry_store, embed_service, adapt_service
  |     |-- store_ops: StoreService
  |     |     |-- gateway: Arc<SecurityGateway>
  |     |     |-- store, vector_index, embed_service, adapt_service
  |     |-- confidence: ConfidenceService
  |           |-- store
  |
  |-- (existing fields: entry_store, vector_store, embed_service, registry, audit, ...)
```

## Data Flow

### Search Path (MCP + UDS)

```
Transport (tools.rs / uds_listener.rs)
  --> Construct AuditContext from transport identity
  --> Convert transport params to ServiceSearchParams
  --> services.search.search(params, audit_ctx)
      --> gateway.validate_search_query(query, k, audit_ctx)  [S1 warn, S3 bounds]
      --> embed query via embed_service
      --> adapt via adapt_service (MicroLoRA)
      --> normalize embedding
      --> HNSW search (filtered or unfiltered)
      --> batch fetch entries
      --> filter quarantined (S4)
      --> re-rank: 0.85*sim + 0.15*conf
      --> provenance boost (+0.02 for lesson-learned)
      --> co-access boost (spawn_blocking)
      --> feature boost (tag match) -- not present in current code, reserved
      --> similarity/confidence floor filtering
      --> truncate to k
      --> gateway.emit_audit(event)  [S5]
      --> return SearchResults { entries, query_embedding }
  <-- Transport formats response, records usage, returns
```

### Write Path (MCP)

```
Transport (tools.rs)
  --> Construct AuditContext::Mcp
  --> Build NewEntry from params
  --> services.store_ops.insert(entry, None, audit_ctx)
      --> gateway.validate_write(title, content, category, tags, audit_ctx)  [S1 reject, S3 bounds]
      --> embed via embed_service + adapt_service
      --> near-duplicate detection via vector_store
      --> insert_in_txn atomically (entry + audit in same txn)
      --> insert HNSW vector
      --> gateway.emit_audit(event)  [S5]
      --> return InsertResult
  --> services.confidence.recompute(&[entry_id])
  <-- Transport formats response
```

### Internal Write Path (UDS auto-outcome)

```
uds_listener.rs
  --> Construct AuditContext::Internal { service: "auto-outcome" }
  --> Build NewEntry
  --> services.store_ops.insert(entry, None, audit_ctx)
      --> gateway.validate_write: skips S1 scan (Internal), applies S3
      --> embed, duplicate check, insert_in_txn
  --> services.confidence.recompute(&[entry_id])
```

## Shared Types (services/mod.rs)

- `AuditContext { source, caller_id, session_id, feature_cycle }`
- `AuditSource { Mcp { agent_id, trust_level }, Uds { uid, pid, session_id }, Internal { service } }`
- `ServiceError { ContentRejected, ValidationFailed, Core, EmbeddingFailed, NotFound }`
- `ServiceLayer { search, store_ops, confidence }`

## Existing Patterns Reused

1. **ContentScanner::global()** -- OnceLock singleton, scan() + scan_title() methods
2. **insert_with_audit** in server.rs -- atomic txn pattern (lines 186-348); insert_in_txn extracts the inner loop
3. **compute_search_boost** from coaccess module -- spawn_blocking call for co-access
4. **rerank_score** from confidence module -- 0.85*sim + 0.15*conf
5. **PROVENANCE_BOOST** constant from confidence module
6. **spawn_blocking_fire_and_forget** pattern used in uds_listener.rs
7. **AuditLog::log_event** -- fire-and-forget audit emission
8. **CategoryAllowlist::validate** -- category checking

## Pattern Deviations

1. **insert_in_txn vs insert_with_audit**: The new `Store::insert_in_txn` method extracts the entry/index write logic from `insert_with_audit` in server.rs. The server.rs method currently does both entry writes AND VECTOR_MAP writes in the same closure. `insert_in_txn` will handle entry+indexes; StoreService will handle VECTOR_MAP and audit in the same transaction.

2. **ConfidenceService batching**: Current code spawns individual tasks. New code batches into a single spawn_blocking. Same compute_confidence function, different scheduling.

3. **SecurityGateway S1 on search queries**: Currently, search queries are NOT scanned. This is NEW behavior (defense-in-depth) that produces ScanWarning but does not reject. The search still proceeds.

## Integration Harness Plan

Existing test infrastructure:
- TestHarness in server.rs tests (creates full UnimatrixServer)
- tempdir-based Store instances in tools.rs tests
- Integration tests in tests/ directory

New integration test needs:
- SearchService comparison test: seed store, run through SearchService, verify identical results
- StoreService atomic audit test: insert via StoreService, verify entry + audit in same read txn
- ConfidenceService batch test: recompute multiple entries, verify all updated
- SecurityGateway integration: scan queries and writes through gateway, verify behavior
- Transport rewiring: verify tools.rs and uds_listener.rs delegate correctly

Applicable existing suites from product/test/infra-001/:
- Smoke tests for basic server functionality
- Any search/store related integration tests
