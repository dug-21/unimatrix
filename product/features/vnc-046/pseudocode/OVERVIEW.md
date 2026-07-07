# vnc-046 Pseudocode — OVERVIEW

Per-slug state isolation for the cloud (HTTPS) observe path. Completes the vnc-038
per-request funnel: the resolver resolves **all** per-slug observe state (registry, pending,
services), `build_project_server` reaches full construction parity (P1+P2+P3), and a real boot
assertion + compile-time field census close the "constructor-default never overwritten" class.

All names/signatures below are traced to the source (`crates/unimatrix-server/src/...`) or to
ARCHITECTURE.md "Integration Surface" / the ADRs. Nothing here is invented; genuine gaps are in
**Open Questions**.

## Components

| Component | File | Source file | Pattern |
|-----------|------|-------------|---------|
| resolution-funnel | `resolution-funnel.md` | `http/router/seam.rs` | Guardrail seam (FR-12) |
| project-resolver | `project-resolver.md` | `http/router/project_resolver.rs` | ADR-001 |
| observe-context | `observe-context.md` | `http/router.rs` + `main.rs` ctor | ADR-001/002, P-vestigial |
| observe-handler | `observe-handler.md` | `http/router/handlers.rs` + `uds/listener.rs` | ADR-001, R-14 |
| project-provisioner | `project-provisioner.md` | `http_provision.rs` + `main.rs` call site | ADR-002 (P1/P2/P3) |
| boot-assertion | `boot-assertion.md` | `main.rs` | ADR-003 |
| isolation-suite | `isolation-suite.md` | `tests/project_routing_integration.rs` | ADR-004 |

## Data Flow

### Write path (observe) — the change
```
POST /v1/{A}/observe
  → route_observe: key = parse_project_key(path)          // ProjectKey::Slug(A) — Step 0, ONCE
  → store    = observe_ctx.resolver.resolve_store(&key)?   // existing (vnc-038)
  → registry = observe_ctx.resolver.registry_for(&key)?    // NEW — A's Arc<SessionRegistry>
  → pending  = observe_ctx.resolver.pending_for(&key)?     // NEW — A's Arc<Mutex<Pending>>
  → services = observe_ctx.resolver.services_for(&key)?    // NEW — A's ServiceLayer
  → dispatch_request(hook, &store, &embed, &store, version, &registry, &pending, &services, caps)
       → session_registry.apply_transcript_delta(...)      // writes into A's registry
```
Post-`resolve_store` `Err` from any `*_for` = boot-wiring contradiction → **500, never 404** (R-14).

### Read path (MCP) — unchanged wiring, correct by construction
```
POST /v1/{A}/tools/cycle_review → adapter_for(&key) → A's McpAdapter → A's UnimatrixServer
  → self.session_registry.take_transcripts_for_feature(fc)  // SAME Arc instance A's write path used
```
Convergence guarantee: `ProjectEntry::from_server` `Arc::clone`s registry/pending/services **off
`server` before it moves into `McpAdapter::new`**, so `resolver.registry_for(&A)` and
`A_server.session_registry` are clones of one `Arc` — `Arc::ptr_eq` holds (pinned by boot assertion).

### Config path (P3) — boot-time, in `build_project_server`
Set 5 config-snapshot fields on the per-slug server from the slug's resolved config
(`resolve_slug_config`, vnc-040), mirroring the daemon `main.rs:978-990`.

### Boot path
Per built slug, after `build_project_server`: `assert_per_slug_isolation` verifies convergence
(`Arc::ptr_eq`), pairing (`has_transcript_hold()`), P3 sentinels, and the global-handle invariants;
`Err` aborts boot. Compile-time census (no `..`) forces every future field to be classified.

## Shared Types (existing — use exactly, do not redefine)

| Type / signature | Source |
|---|---|
| `enum ProjectKey { Slug(ProjectSlug) }` | `seam.rs:51` |
| `enum RouteError { UnknownProject, InvalidSlug(String) }` | `seam.rs:155` |
| `trait StoreResolver: Send + Sync + 'static` (`resolve_store`, `adapter_for`) | `seam.rs:124` |
| `Arc<SessionRegistry>` (`.has_transcript_hold()`, `.with_transcript_cap()`, `.with_transcript_hold()`, `.with_signature_scanner()`) | `infra/session.rs` |
| `Arc<Mutex<PendingEntriesAnalysis>>` (`PendingEntriesAnalysis::new()`) | `server.rs:74` |
| `ServiceLayer` (`Clone`; holds Arcs) | `services/` |
| `TranscriptHold::new(max_sessions, Arc<dyn PurgeAuditSink>)`, `AuditLogPurgeSink::new(Arc<AuditLog>)` | `infra/transcript_hold.rs:180,84` |
| `struct ProjectEntry { store, adapter }` (`pub(crate)`) | `project_resolver.rs:50` |
| `ProjectEntry::from_server(store, server, max_body, allowed_origins, allowed_hosts)` | `project_resolver.rs:82` |
| `struct ProjectServerInput { slug, store, server, vector_dir }` | `project_resolver.rs:126` |
| `build_project_server(base_dir, slug, embed_handle, permissive, instructions, rayon_pool, nli_handle, nli_top_k, nli_enabled, inference_config, confidence_params, categories, observation_registry, boosted_categories) -> Result<ProjectServerInput, ServerError>` | `http_provision.rs:136` |
| `dispatch_request(request, store, embed, _vector_store, entry_store, _adapt, version, &SessionRegistry, &Arc<Mutex<Pending>>, &ServiceLayer, caps)` | `uds/listener.rs:773` |
| `assert_wave_b_precondition(&SessionRegistry, &UnimatrixConfig) -> Result<(), ServerError>` | `main.rs:81` |
| `UnimatrixServer.service_layer()` / `.vector_index()` / `.session_registry` / `.pending_entries_analysis` accessors | `server.rs`, used `main.rs:1233-1238` |

## New / Changed Types (this feature)

- **`StoreResolver`** gains 3 methods, **no default impl** (ADR-001, FR-12):
  ```
  fn registry_for(&self, key: &ProjectKey) -> Result<Arc<SessionRegistry>, RouteError>;
  fn pending_for(&self, key: &ProjectKey)  -> Result<Arc<Mutex<PendingEntriesAnalysis>>, RouteError>;
  fn services_for(&self, key: &ProjectKey) -> Result<ServiceLayer, RouteError>;
  ```
- **`ProjectEntry`** gains 3 fields: `session_registry: Arc<SessionRegistry>`,
  `pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>`, `services: ServiceLayer`.
- **`ObserveContext`** reshaped `{ resolver, embed_service, server_version }` — DROP
  `vector_store`, `adapt_service`, `session_registry`, `pending_entries_analysis`, `services`.
- **`build_project_server`** gains 3 params-at-end: `store_config: &Arc<StoreConfig>`,
  `retention_config: &Arc<RetentionConfig>`, `signal_class_names: &Arc<Vec<String>>`.
- **`dispatch_request`** loses the two vestigial params `_vector_store` / `_adapt_service`
  (AC-09; ~100 call sites — see `observe-handler.md` blast-radius note).
- **`assert_per_slug_isolation(...) -> Result<(), ServerError>`** — new (ADR-003).

## The ObserveContext Reshape (central change)

`ObserveContext` today (`router.rs:81-102`) carries 8 fields, five of which are daemon-global
`Arc` clones wired in `main.rs:1268-1277` — the split-brain source. After this feature it carries
only 3 boot-invariant handles; the per-request per-slug state is resolved from `resolver` on each
call. This is the load-bearing shape change; every other component orbits it.

| ObserveContext field | Before | After | Why |
|---|---|---|---|
| `resolver: Arc<dyn StoreResolver>` | kept | **kept** | the one funnel |
| `embed_service: Arc<EmbedServiceHandle>` | kept | **kept** | correctly-global (one ONNX model) |
| `server_version: String` | kept | **kept** | static |
| `vector_store` | present | **DELETE** | vestigial (`_vector_store` unused) — FR-11/AC-09 |
| `adapt_service` | present | **DELETE** | vestigial (`_adapt_service` unused) — FR-11/AC-09 |
| `session_registry` | global clone | **DELETE** → `registry_for(&key)` | P1 split-brain |
| `pending_entries_analysis` | global clone | **DELETE** → `pending_for(&key)` | P1 split-brain |
| `services` | global clone | **DELETE** → `services_for(&key)` | P2 read-leak |

## Field Census Classification (ADR-003 guard 2 — the whole class)

Exhaustive destructure of `UnimatrixServer` (`server.rs:197-289`), **no `..`**. Every field routed
to a class; PER-SLUG fields flow into the boot assertion (guard 1).

| Field | Class | Wired by |
|---|---|---|
| `store`, `entry_store` | PER-SLUG | ctor (per-slug store) |
| `vector_store`, `vector_index` | PER-SLUG | ctor |
| `registry` (AgentRegistry), `audit`, `usage_dedup` | PER-SLUG | ctor |
| `adapt_service` | PER-SLUG (independent, ADR-006) | ctor |
| `services`, `effectiveness_state` | PER-SLUG (config-driven) | ctor `Some(service_layer)` |
| `session_registry` | PER-SLUG | **P1 — new set in `build_project_server`** |
| `transcript_hold` | PER-SLUG (paired w/ registry) | **P1 — new** |
| `pending_entries_analysis` | PER-SLUG | **P1 — new** |
| `observation_registry`, `inference_config`, `store_config`, `retention_config`, `transcript_signal_class_names` | PER-SLUG (config snapshot) | **P3 — new** |
| `tick_metadata` | CORRECTLY-PER-INSTANCE | ctor (own per server) |
| `embed_service` | CORRECTLY-GLOBAL (one ONNX model) | `Arc::clone` of the one handle |
| `categories` | see Open Question 3 | ctor param (threaded `slug_categories` today) |
| `client_type_map` | CORRECTLY-PER-INSTANCE (runtime `initialize`) | runtime |
| `tool_router`, `server_info` | CORRECTLY-PER-INSTANCE | ctor (macro / static) |

## Sequencing Constraints (build order — load-bearing)

1. **F1/SR-03 pairing:** in `build_project_server`, construct the `SessionRegistry`+`TranscriptHold`
   pair together and set both on the server **before** returning `ProjectServerInput`. They land
   inside `build_project_server`, hence **before** the `main.rs:1229` tick-context loop clones
   `input.server.session_registry`. Never wire registry without hold.
2. **Clone-before-move:** `from_server` clones registry/pending/services off `server` **before**
   `McpAdapter::new(server, ...)` consumes it (project-resolver).
3. **Boot assertion after router built:** the `Arc::ptr_eq` convergence check needs both the
   resolver-returned handle and the server-held handle; `from_servers` consumes the inputs, so the
   server-side handles must be captured before the move and asserted after the router exists — see
   **Open Question 1** and `boot-assertion.md`.

## Open Questions / Gaps

1. **Boot-assertion vs `from_servers` move (sequencing).** ADR-003 signature is
   `assert_per_slug_isolation(input: &ProjectServerInput, resolver: &dyn StoreResolver, config)`,
   but `MultiProjectRouter::from_servers(slug_servers: Vec<ProjectServerInput>, ...)` **consumes**
   the inputs, and the resolver does not exist until after that consume. The `Arc::ptr_eq`
   convergence check needs both the resolver's returned handle and the server-held handle
   simultaneously. Recommended resolution (see `boot-assertion.md`): capture a per-slug
   `IsolationProbe { slug, session_registry, pending, services }` (Arc clones) in the existing
   pre-move loop at `main.rs:1229`, build the router, then assert per slug against the probe. This
   refines the ADR-003 param from `&ProjectServerInput` to `&IsolationProbe`. **Flag for architect
   sign-off** — it is a param-shape refinement, not a behavior change.
2. **Per-slug signature scanner — RESOLVED (ADR-002, Gate 3a OQ-2).** Confirmed: FR-9's
   `signal_class_counts` are produced by the registry's `SignatureScanner` at delta-apply
   (`session.rs:350`), which defaults to `SignatureScanner::empty()` — so the class *names* (P3) with
   an empty scanner give all-**zero** counts (hollow FR-9) and an AC-07 HTTPS≠UDS parity break on any
   signal-bearing transcript. ADR-002 now ratifies building the scanner **per-slug** (NOT the daemon's
   shared `Arc<SignatureScanner>`) from the slug's already-resolved `r.transcript_signals`, fallible,
   chained into the registry as the third member of the cap+hold+scanner triple. `project-provisioner.md`
   P1 implements it: `.with_signature_scanner(Arc::new(SignatureScanner::compile(&r.transcript_signals.enabled_patterns())?))`
   (map_err → `ServerError::Config`). No longer an open question.
3. **`categories` classification.** NFR-5 calls the operator `categories` allowlist
   CORRECTLY-GLOBAL, but `build_project_server` already threads a per-slug `slug_categories`
   (`main.rs:1183`, from `r.knowledge.categories`). The census must classify `categories`
   consistently with the code (per-slug config-driven today), not with NFR-5's prose. **Flag** for
   the census author.
4. **`#800` fixture ownership (SR-08).** The N≥2 behavioral suite + INV-C proof reuse the #800
   multi-slug HTTP fixture (OPEN, owner unconfirmed). `isolation-suite.md` assumes the fixture
   shape; confirm owner before building INV-C fixtures.
