# Component: `build_project_server` — config-parity threading

> Wave 1. ADR-002 (#5165) + ADR-006 (#5164). Resolves FR-1..FR-6, FR-9, FR-10. Covers AC-1, AC-2.
> Risks R-05 (partial threading), R-12 (N model copies), R-11 (adapt/session parity).
> Source: `crates/unimatrix-server/src/http_provision.rs:125-204` (defaults at 180-181).

## Purpose

Build each per-slug `UnimatrixServer` with the daemon's **resolved global** config and the **one
shared loaded** `nli_handle`, so per-slug servers reach config parity over the closed 8-field
checklist (ADR-006). Replace the two per-slug defaults (`AdaptConfig::default()` /
`CategoryAllowlist::new()` at 180-181) with the operator's resolved values where the checklist requires
it. `Arc::clone` every shared resource — NEVER rebuild a model (C-3, AC-2).

## Integration surface (exact)

Existing signature (`http_provision.rs:125-131`):
```
async fn build_project_server(
    base_dir: &Path, slug: &ProjectSlug, embed_handle: &Arc<EmbedServiceHandle>,
    permissive: bool, instructions: Option<String>,
) -> Result<ProjectServerInput, ServerError>
```
Wave-1 signature (ADR-002 — 8 params appended at end, params-at-end convention #2552/#2553):
```
async fn build_project_server(
    base_dir: &Path, slug: &ProjectSlug, embed_handle: &Arc<EmbedServiceHandle>,
    permissive: bool, instructions: Option<String>,
    // crt-056 Wave 1 appended:
    rayon_pool: &Arc<RayonPool>,
    nli_handle: &Arc<NliServiceHandle>,        // the ONE loaded model, shared (AC-2)
    nli_top_k: usize,
    nli_enabled: bool,
    inference_config: &Arc<InferenceConfig>,
    confidence_params: &Arc<ConfidenceParams>,
    categories: &Arc<CategoryAllowlist>,
    observation_registry: &Arc<DomainPackRegistry>,
) -> Result<ProjectServerInput, ServerError>
```
`ServiceLayer::new` (existing, `main.rs:880-898` shows the 17-arg call) is the construction contract —
this component matches it field-for-field with the threaded values.

## Modified function: `build_project_server`

```text
async fn build_project_server(base_dir, slug, embed_handle, permissive, instructions,
                              rayon_pool, nli_handle, nli_top_k, nli_enabled,
                              inference_config, confidence_params, categories,
                              observation_registry) -> Result<ProjectServerInput, ServerError>:

    # --- UNCHANGED (http_provision.rs:132-170): path-join, no-auto-create guard, open store,
    #     build/load per-slug vector_index. Keep verbatim. ---
    data_dir   = base_dir.join(slug.as_str())
    db_path    = data_dir.join(PROJECT_DB_NAME)
    if not db_path.exists():
        return Err(ServerError::Config("slug '{slug}' ... not registered ... run `register` first"))
    store         = Arc::new(SqlxStore::open(&db_path, PoolConfig::default()).await?)
    vector_index  = load-or-build over store (existing 157-170)

    # --- UNCHANGED per-slug isolated subsystems (http_provision.rs:172-184) EXCEPT lines 180-181 ---
    registry            = Arc::new(AgentRegistry::new(Arc::clone(&store), permissive, Vec::new())?)
    registry.bootstrap_defaults()?
    audit               = Arc::new(AuditLog::new(Arc::clone(&store)))
    async_vector_store  = Arc::new(AsyncVectorStore::new(Arc::new(VectorAdapter::new(Arc::clone(&vector_index)))))

    # ADR-006: adapt_service stays PER-SLUG INDEPENDENT STATE, same config (AdaptConfig::default()
    # is the resolved value today — NOT threaded; #785 would thread it if AdaptConfig becomes
    # operator-configurable). Keep the existing construction (line 180).
    adapt_service       = Arc::new(AdaptationService::new(AdaptConfig::default()))

    # ADR-002: line 181's `CategoryAllowlist::new()` default is REPLACED — the per-slug ServiceLayer
    # uses the THREADED operator `categories`. Do NOT keep a per-slug empty allowlist.

    # --- ADR-002 CORE CHANGE: build the config-driven ServiceLayer (mirrors main.rs:880-898) ---
    usage_dedup   = Arc::new(UsageDedup::new())     # per-slug, as the daemon builds its own
    service_layer = ServiceLayer::new(
                       Arc::clone(&store),                  # store
                       Arc::clone(&vector_index),           # vector_index
                       Arc::clone(&async_vector_store),     # vector_store
                       Arc::clone(&store),                  # entry_store
                       Arc::clone(embed_handle),            # SHARED stateless embed (already shared today)
                       Arc::clone(&adapt_service),          # per-slug independent (ADR-006)
                       Arc::clone(&audit),
                       Arc::clone(&usage_dedup),
                       default_boosted_categories_set(),    # same resolved boosted set the daemon uses
                       Arc::clone(rayon_pool),              # FR-5: shared config-sized pool, NOT size-1
                       Arc::clone(nli_handle),              # FR-6/AC-2: the ONE loaded model — Arc::clone, NEVER new()
                       nli_top_k,                           # FR-2/FR-3: threaded, not 20-default
                       nli_enabled,                         # FR-2: config value, NEVER hardcoded false
                       Arc::clone(inference_config),        # FR-3: resolved fusion/PPR, not ::default()
                       Arc::clone(observation_registry),    # FR-4: operator domain packs, not builtin-only
                       Arc::clone(confidence_params),       # FR-3: operator weights, not ::default()
                       Arc::clone(categories),              # FR-4: operator allowlist + lifecycle, not ::new()
                   )

    # --- ADR-001: pass the config-driven layer to the constructor (was 186-197) ---
    server = UnimatrixServer::new(
                 Arc::clone(&store), async_vector_store, Arc::clone(embed_handle),
                 registry, audit, Arc::clone(categories),   # constructor `categories` param: pass the threaded one
                 Arc::clone(&store), Arc::clone(&vector_index), adapt_service,
                 instructions,
                 Some(service_layer),                       # NEW (ADR-001)
             )

    return Ok(ProjectServerInput { slug: slug.clone(), store, server })
    # G-3 (OVERVIEW): if PerSlugTickContext needs vector_index off ProjectServerInput, either read
    # input.server.vector_index (server.rs:351) or add vector_index to ProjectServerInput. FLAGGED.
```

### `boosted_categories` note
The daemon passes a resolved `boosted_categories` (`main.rs:889`). The exact source variable must match
the daemon's (it is `default_boosted_categories_set()` per the constructor's `None` arm and the daemon's
resolved set). The implementer threads the SAME value the daemon used so AC-1's "domain pack / category"
parity holds. If `boosted_categories` is operator-resolved at the daemon and not derivable inside
`build_project_server`, add it to the appended params. FLAGGED as a possible 9th thread — confirm against
`main.rs` `boosted_categories` provenance during Wave 1.

## Data flow
- Inputs: per-slug `slug`/`base_dir`/`embed_handle` (existing) + 8 resolved-config `Arc`s/values from
  the daemon's already-resolved set (`main.rs:880-898`).
- Outputs: `ProjectServerInput { slug, store, server }` where `server.services` is the config-driven layer.
- Transformation: `Arc::clone` of shared resources (no copy of `T`); construction of per-slug store,
  vector_index, registry, audit, adapt_service (independent state).

## Error handling
- Returns `Result<_, ServerError>` (unchanged). Existing error paths kept verbatim:
  `ServerError::Config` for unregistered slug (no auto-create, fail-loud — Failure Mode "config field
  missing"), `ServerError::Core(CoreError::Vector(_))` for vector index, `?` on store open / registry.
- **No silent default fallback for the 8 threaded fields** (Failure Mode: "config field missing must fail
  loud, not degrade"). The params are required (not `Option`), so a missing field is a compile error at
  the call site, not a runtime degrade — exactly the guard against re-creating Defect 1.

## Key test scenarios (hints for tester)
- **AC-1 / R-05.1 (field-by-field parity).** After boot, assert per-slug `ServiceLayer` equals the
  daemon's resolved config across all 8 ADR-006 fields: `nli_enabled`, `nli_top_k`, shared `nli_handle`,
  `InferenceConfig`, `ConfidenceParams`, `CategoryAllowlist`, domain-pack set (`observation_registry`),
  effective rayon pool size. NOT a subset. **`session_capabilities` is OUT — do NOT assert it** (ADR-006).
- **R-05.2 (NLI both directions).** Config NLI-on ⇒ per-slug on; config NLI-off ⇒ per-slug off. Proves
  the flag is threaded, not hardcoded.
- **AC-2 / R-12 (one model, shared Arc).** Assert exactly one NLI model + one embedding model loaded in
  process; per-slug `nli_handle`/`embed_handle` are the SAME `Arc` instances as the daemon's
  (`Arc::ptr_eq`). Source audit: no `NliServiceHandle::new()` on the per-slug path.
- **FR-9 / R-05.3 (global-config-only guard).** All per-slug servers resolve to the same global config;
  no per-slug override param exists (keeps #785/C6 out).
- **R-11 (adapt independence).** `adapt_service` is per-slug (drive adaptation on A; B's adaptive state
  unchanged) — adjacent to AC-4.
</content>
