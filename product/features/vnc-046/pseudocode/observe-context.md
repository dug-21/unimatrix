# Component: observe-context (`ObserveContext` reshape)

**Source:** `crates/unimatrix-server/src/http/router.rs:81-102` + construction in `main.rs:1268-1277`
**ADR:** ADR-001, ADR-002 · **FR:** FR-5, FR-11 · **AC:** AC-09 · **Risks:** R-09

## Purpose

Collapse `ObserveContext` from 8 fields (5 of them daemon-global split-brain handles) to the 3
boot-invariant handles that are genuinely global to the observe edge. Per-request per-slug state now
comes from `resolver` (see `observe-handler.md`), not from flat fields.

## Reshaped Struct (`router.rs:81`)

```
#[derive(Clone)]
pub struct ObserveContext {
    pub resolver: Arc<dyn StoreResolver>,       // KEEP — the one funnel
    pub embed_service: Arc<EmbedServiceHandle>, // KEEP — correctly-global (one ONNX model, NFR-5)
    pub server_version: String,                 // KEEP — static
    // DELETED: vector_store, adapt_service (vestigial, FR-11/AC-09),
    //          session_registry, pending_entries_analysis, services (per-request now, FR-5)
}
```

Remove now-unused imports from `router.rs` if no other item needs them: `AsyncVectorStore`,
`VectorAdapter`, `AdaptationService`, `SessionRegistry`, `Mutex`, `PendingEntriesAnalysis`,
`ServiceLayer`. (Verify each is unused elsewhere in the file before deleting the `use`.)

## Construction Site Update (`main.rs:1268-1277`)

```
observe_ctx = ObserveContext {
    resolver: Arc::clone(resolver),                              // KEEP
    embed_service: Arc::clone(embed_handle),                    // KEEP
    server_version: env!("CARGO_PKG_VERSION").to_string(),      // KEEP
    // DELETE the 5 removed field initializers:
    //   vector_store, adapt_service, session_registry, pending_entries_analysis, services
}
```
This is the same object used by both the daemon HTTP path and any other `ObserveContext` construction
site — grep `ObserveContext {` to confirm the single construction at `main.rs:1268` and update it.
The daemon-global `session_registry`/`pending_entries_analysis`/`services`/`async_vector_store`/
`adapt_service` locals stay alive for their *other* consumers (UDS listener, daemon server, tick) —
only the `ObserveContext` initializers drop.

## Why Each Deletion Is Safe

| Deleted field | Justification |
|---|---|
| `vector_store` | `dispatch_request`'s `_vector_store` is `_`-unused (`listener.rs:777`); FR-11/AC-09 removes the field and (see observe-handler) the param. |
| `adapt_service` | `dispatch_request`'s `_adapt_service` is `_`-unused (`listener.rs:779`); FR-11/AC-09. |
| `session_registry` | replaced by `resolver.registry_for(&key)` per request (FR-5) — the #930 split-brain source. |
| `pending_entries_analysis` | replaced by `resolver.pending_for(&key)` per request (FR-4/FR-5). |
| `services` | replaced by `resolver.services_for(&key)` per request (FR-6) — the P2 read-leak source (R-09). |

## Data Flow

- **In (construction, boot):** `resolver`, `embed_handle`, version — all available at `main.rs:1268`.
- **Out (per request):** `route_observe` reads `resolver` to resolve store/registry/pending/services;
  reads `embed_service` and `server_version` directly.

## Error Handling

None here (pure struct + construction). Resolution errors surface in `observe-handler.md`.

## Key Test Scenarios (hints)

- `ObserveContext` has exactly 3 fields; `vector_store`/`adapt_service`/`session_registry`/
  `pending_entries_analysis`/`services` are **absent** — compile + diff review (AC-09).
- No remaining reference to the deleted fields anywhere (compile is the gate).
- `#[derive(Clone)]` still holds (all 3 remaining fields are Clone).
