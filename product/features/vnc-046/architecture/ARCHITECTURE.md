# vnc-046 — Architecture: Per-Slug State Isolation for the Cloud (HTTPS) Observe Path

## System Overview

Unimatrix serves N projects from one cloud (HTTPS) process. Each registered slug gets its
own `UnimatrixServer` (its own store, vector index, hash chain, analytics dir, config-driven
`ServiceLayer`), built in `http_provision::build_project_server` and held in the resolver's
`slug → ProjectEntry` map. Two HTTP entry handlers share one funnel (`Arc<dyn StoreResolver>`):

- **MCP** (`/v1/{slug}/tools/...`) → `SlugRouter` → `resolve_store` / `adapter_for` → the
  slug's `McpAdapter` → the per-slug `UnimatrixServer` (serves `cycle_review`,
  `context_briefing`, etc.).
- **Observe** (`/v1/{slug}/observe`) → `route_observe` → `parse_project_key` →
  `resolve_store(&key)` → `dispatch_request`.

vnc-038 (ADR-003, #5082) moved observe onto the per-request store funnel — but resolved
**only the store**. Every other handle in `ObserveContext` (`session_registry`,
`pending_entries_analysis`, `services`) stayed a **daemon-global** clone, and
`build_project_server` overwrote **none** of the ~8 `UnimatrixServer::new` test-default
fields the daemon/stdio paths overwrite (`main.rs:975-994`). Result: a per-slug **split-brain**
— the object that writes observe state (the global handles) and the object that reads it (the
per-slug server) are different instances. #930 is the transcript face of this; the audit
(GH #930) inventoried **9 NEEDS-PER-SLUG-FIX state items across 3 fix patterns**, plus 2
vestigial fields to delete.

This feature **completes the vnc-038 funnel**: the resolver resolves *all* per-slug observe
state, `build_project_server` reaches full construction parity, and a real boot assertion
converts the whole "constructor-default never overwritten" bug class from silent-read-zero to
loud-at-boot. It restores goal #5519's OSS-in-scope invariant — "one cloud serves N projects,
each fully isolated … cross-project contamination structurally impossible via the
`resolve_store(request)` funnel" — on the transcript, knowledge-read, and config paths where
it is **currently not met** on cloud. Governing pattern: #5629 (construction parity + funnel
completeness). Local UDS/stdio paths are correct (they share the one global registry) and are
untouched (NG-4).

## Component Breakdown

| Component | File | Responsibility | Change in vnc-046 |
|-----------|------|----------------|-------------------|
| `StoreResolver` trait | `http/router/seam.rs` | THE single per-request resolution funnel | **Extend**: add `registry_for` / `pending_for` / `services_for` beside `resolve_store` / `adapter_for` (ADR-001) |
| `MultiProjectRouter` / `ProjectEntry` | `http/router/project_resolver.rs` | Holds the `slug → entry` map; implements the funnel | **Extend**: `ProjectEntry` carries the slug's registry/pending/services handles; new methods resolve them O(1) (ADR-001) |
| `ObserveContext` | `http/router.rs` | Handle bundle for `route_observe` | **Reshape**: drop the 3 flat global handles + 2 vestigial fields; keep only `resolver`, `embed_service`, `server_version` (ADR-001, ADR-002) |
| `route_observe` | `http/router/handlers.rs` | Observe entry handler | **Change**: resolve registry/pending/services from the already-parsed `key`, pass to `dispatch_request` (ADR-001) |
| `build_project_server` | `http_provision.rs` | Assembles the per-slug `UnimatrixServer` | **Change**: construct + set the per-slug registry+hold pair, pending, and the 5 config-snapshot fields; thread 3 new params (ADR-002) |
| `assert_wave_b_precondition` → `assert_per_slug_isolation` | `main.rs` | Boot-time wiring guard | **Extend**: real `ServerError` returned per built slug + exhaustive field census (ADR-003) |
| Behavioral isolation suite | `tests/` (extend `project_routing_integration.rs`, reuse #800 fixture) | Prove INV-T/K/C bidirectionally through `/v1/{slug}/...` | **New** (ADR-004) |

## Component Interactions & Data Flow

### Write path (observe), after this feature
```
POST /v1/{A}/observe
  → route_observe: key = parse_project_key(path)            // ProjectKey::Slug("A")
  → store    = resolver.resolve_store(&key)                 // existing (vnc-038)
  → registry = resolver.registry_for(&key)                  // NEW — A's registry (ADR-001)
  → pending  = resolver.pending_for(&key)                   // NEW — A's pending  (ADR-001)
  → services = resolver.services_for(&key)                  // NEW — A's ServiceLayer (ADR-001)
  → dispatch_request(hook, &store, …, registry, pending, services, …)
     → session_registry.apply_transcript_delta(...)         // writes into A's registry
```

### Read path (MCP), unchanged wiring — becomes correct by construction
```
POST /v1/{A}/tools/cycle_review
  → adapter_for(&key) → A's McpAdapter → A's UnimatrixServer
  → self.session_registry.take_transcripts_for_feature(fc)  // A's registry — SAME instance
```
Convergence guarantee: `resolver.registry_for(&A)` returns `Arc::clone` of the **same**
`SessionRegistry` instance `A`'s `UnimatrixServer.session_registry` holds (both are
`Arc::clone`d from the one built in `build_project_server`). Write and read meet on one
instance per slug — the local-mode topology, restored per slug instead of globally.

### Config path (P3), boot-time
`build_project_server` sets `server.observation_registry / inference_config / store_config /
retention_config / transcript_signal_class_names` from the slug's resolved config (mirror
`main.rs:978-990`), so MCP reads of `self.<config field>` (`tools.rs`, `status.rs:815`)
observe the slug's declared config, not builtin/global defaults.

## Technology Decisions (ADR index)

| ADR | Title | Resolves |
|-----|-------|----------|
| ADR-001 | Per-slug resolution funnel on `Arc<dyn StoreResolver>` (no side-map) | SR-01; funnel completeness (#5629); Decision 1 |
| ADR-002 | Full per-slug construction parity in `build_project_server` (P1+P2+P3, delete vestigial) | SR-03, SR-04, SR-07; Decision 2 |
| ADR-003 | Real boot assertion + exhaustive field census guarding the whole default-never-overwritten class | SR-02, SR-06; Decision 3 |
| ADR-004 | Bidirectional behavioral isolation suite through `/v1/{slug}/...` as the primary gate | SR-05, SR-06, SR-08; Decision 4 |
| ADR-005 | #925 is NOT subsumed — retained as an independent metrics-plane fix | SR-09; Decision 5 |

## Integration Points

- **vnc-038 (#5082)** — the per-request store funnel this completes. New methods live beside
  `resolve_store`; no new funnel, no parallel dispatch (the #4974 guard).
- **vnc-034 (ADR-007 / #4951, #5135)** — the per-slug isolation seam and slug identity this
  realizes on the transcript/knowledge/config planes.
- **vnc-040 (#5209/#5217)** — `resolve_slug_config` is the source of the P3 config-snapshots
  and the per-slug `[retention]` cap now wired into buffers.
- **crt-056** — the per-slug config-driven `ServiceLayer` already built in
  `build_project_server`; P2 only needs the resolver to hand it back per-request.
- **crt-054 / ADR-010** — `assert_wave_b_precondition`, extended by ADR-003.
- **#800 (infra-001)** — the multi-slug HTTP fixture the INV-C proof reuses (SR-08, ADR-004).
- **#925** — orthogonal metrics-plane defect; retained (ADR-005).

## Integration Surface

Exact names/types so downstream agents do not invent them.

### Existing (do not change signatures except as noted)

| Integration Point | Type / Signature | Source |
|---|---|---|
| `StoreResolver::resolve_store` | `fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError>` | `http/router/seam.rs:128` |
| `StoreResolver::adapter_for` | `fn adapter_for(&self, key: &ProjectKey) -> Option<&McpAdapter>` | `seam.rs:150` |
| `ProjectKey` | `enum ProjectKey { Slug(ProjectSlug) }` | `seam.rs` |
| `RouteError` | `enum RouteError { UnknownProject, InvalidSlug(String) }` | `seam.rs:154` |
| `ProjectEntry` | `pub(crate) struct { store: Arc<Store>, adapter: McpAdapter }` | `project_resolver.rs:50` |
| `ProjectEntry::from_server` | `(store, server, max_body, allowed_origins, allowed_hosts) -> Self` | `project_resolver.rs:82` |
| `SessionRegistry` | `Arc<SessionRegistry>` | `infra/session.rs` |
| `PendingEntriesAnalysis` | `Arc<Mutex<PendingEntriesAnalysis>>` | `server.rs` |
| `ServiceLayer` | `ServiceLayer` (`Clone`; holds Arcs) | `services/` |
| `dispatch_request` | `(request, store, embed, _vector_store, entry_store, _adapt, version, session_registry: &SessionRegistry, pending: &Arc<Mutex<PendingEntriesAnalysis>>, services: &ServiceLayer, caps)` | `uds/listener.rs:773` |
| `take_transcripts_for_feature` | `fn(&self, feature_cycle: &str) -> Vec<(String, TranscriptSnapshot)>`; folds on `SessionState.feature == feature_cycle` ∪ held-buffer scan | `infra/session.rs:473-497` |
| `assert_wave_b_precondition` | `fn(&SessionRegistry, &UnimatrixConfig) -> Result<(), ServerError>` | `main.rs:81` |
| `build_project_server` | `async fn(base_dir, slug, embed, permissive, instructions, rayon_pool, nli_handle, nli_top_k, nli_enabled, inference_config, confidence_params, categories, observation_registry, boosted_categories) -> Result<ProjectServerInput, ServerError>` | `http_provision.rs:136` |

### New / changed interfaces introduced by this feature

| Integration Point | Type / Signature | ADR |
|---|---|---|
| `StoreResolver::registry_for` | `fn registry_for(&self, key: &ProjectKey) -> Result<Arc<SessionRegistry>, RouteError>` | ADR-001 |
| `StoreResolver::pending_for` | `fn pending_for(&self, key: &ProjectKey) -> Result<Arc<Mutex<PendingEntriesAnalysis>>, RouteError>` | ADR-001 |
| `StoreResolver::services_for` | `fn services_for(&self, key: &ProjectKey) -> Result<ServiceLayer, RouteError>` | ADR-001 |
| `ProjectEntry` (extended) | add `session_registry: Arc<SessionRegistry>`, `pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>`, `services: ServiceLayer` — `Arc::clone`d from `server` in `from_server` before it moves into `McpAdapter` | ADR-001 |
| `ObserveContext` (reshaped) | `{ resolver: Arc<dyn StoreResolver>, embed_service: Arc<EmbedServiceHandle>, server_version: String }` — DROP `session_registry`, `pending_entries_analysis`, `services`, `vector_store`, `adapt_service` | ADR-001, ADR-002 |
| `build_project_server` (3 new params) | append `store_config: &Arc<StoreConfig>`, `retention_config: &Arc<RetentionConfig>`, `signal_class_names: &Arc<Vec<String>>` (params-at-end, crt-056 idiom) | ADR-002 |
| `assert_per_slug_isolation` | `fn(input: &ProjectServerInput, resolver: &dyn StoreResolver, config: &UnimatrixConfig) -> Result<(), ServerError>` — called per built slug at boot | ADR-003 |

**Error boundary note.** The three new resolver methods return `RouteError` on the same domain
as `resolve_store` (`UnknownProject`), but in `route_observe` the key has already resolved a
store, so an `Err` from any of them is a *boot-wiring contradiction*, not a client error —
ADR-003's boot assertion makes that state unreachable, so the handler maps a defensive `Err` to
`500`, never `404`. The vestigial `_vector_store` / `_adapt_service` `dispatch_request` params
are already `_`-unused; deleting the `ObserveContext` fields removes the last live reference
(AC-09).

**Hot-path cost (SR-01).** `registry_for` / `pending_for` / `services_for` are each one
`HashMap` lookup + `Arc::clone` (`ServiceLayer` clone = a handful of `Arc::clone`s) — no I/O,
no lock, no DB, same cost class as the `resolve_store(&key)` the observe path already runs. The
key is parsed once (Step 0) and reused. No side-map is introduced in `main.rs` (SR-01, the
vnc-034 #4974 guard).

## Open Questions

1. **P3 in-scope confirmed by uni-zero + researcher** — this architecture takes P3 in-scope
   (ADR-002). If speed forces a cut, P1+P2 are the floor and the P3 gap ships with a PR risk
   note; the human owns filing the ADR-007-seam follow-up (SCOPE OQ-5). No architectural
   blocker to landing all three.
2. **INV-C observability (SCOPE OQ-3).** `retention_config`'s purge behavior and
   `inference_config`'s briefing-blend weights may lack a clean public observation surface. Per
   uni-zero's answer, any such field relies on the ADR-003 boot assertion + a wiring-pin unit
   as a **documented AC-06 exception** — not deferred. The tester enumerates which INV-C fields
   are covered behaviorally vs white-box (ADR-004). Confirm the public surfaces:
   `signal_class_names → signal_class_counts` (cycle_review), observation categories → status,
   store byte-limit → store-cap behavior, retention → purge behavior.
3. **`#800` fixture ownership (SR-08).** ADR-004 reuses the #800 multi-slug HTTP fixture rather
   than forking. Confirm with the tester / #800 owner before building INV-C fixtures so
   config-parity is proven once (also C6's path to proven).
