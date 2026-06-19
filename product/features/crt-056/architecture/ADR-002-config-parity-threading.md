## ADR-002: Config-parity threading into `build_project_server` (global config only, params-at-end)

### Context
`build_project_server` (`http_provision.rs:125-204`) builds each per-slug server but takes only
`(base_dir, slug, embed_handle, permissive, instructions)`. It constructs per-slug subsystems with
defaults (`AdaptConfig::default()`, `CategoryAllowlist::new()` at lines 180-181) and then calls
`UnimatrixServer::new` (186-197), which falls into the test-default `ServiceLayer`. The result is
Defect 1: NLI off, pool=1, wrong fusion/PPR/confidence weights, empty allowlist, no operator
domain packs, and an **unloaded per-slug NLI handle** (a *second* model that never loads).

The Background Research (SCOPE L93-94, A3) established that the daemon's config-parity inputs are
all in scope at the per-slug call site's enclosing function: the daemon builds its own
config-driven `ServiceLayer` at `main.rs:880-898` from exactly these values, and the per-slug loop
runs in the same function (per-slug loop at `main.rs:1084`, call site `main.rs:1085-1092`). They are
present; they are simply not threaded
down into `build_project_server`.

Constraint (SR-06): crt-056 is **global** config parity only. Per-slug CUSTOM overrides are #785 /
C6. The threaded config must be the daemon's single *resolved* set, not a per-slug-overridable one.

### Decision
Thread the daemon's resolved config inputs + the **one shared loaded** `nli_handle` into
`build_project_server`, appended at the end of the signature (params-at-end convention, #2552/#2553
— minimizes diff blast radius), and build the config-driven `ServiceLayer` there:

```rust
pub async fn build_project_server(
    base_dir: &Path,
    slug: &ProjectSlug,
    embed_handle: &Arc<EmbedServiceHandle>,
    permissive: bool,
    instructions: Option<String>,
    // crt-056 Wave 1 — global config parity (appended):
    rayon_pool: &Arc<RayonPool>,
    nli_handle: &Arc<NliServiceHandle>,        // the ONE loaded model, shared (AC-2)
    nli_top_k: usize,
    nli_enabled: bool,
    inference_config: &Arc<InferenceConfig>,
    confidence_params: &Arc<ConfidenceParams>,
    categories: &Arc<CategoryAllowlist>,        // operator allowlist + lifecycle policy
    observation_registry: &Arc<DomainPackRegistry>,
) -> Result<ProjectServerInput, ServerError> {
    // ...existing store / vector_index / registry / audit (148-184)...
    let service_layer = ServiceLayer::new(
        Arc::clone(&store), vector_index.clone(), async_vector_store.clone(),
        Arc::clone(&store), Arc::clone(embed_handle), adapt_service.clone(),
        audit.clone(), Arc::new(UsageDedup::new()),
        /* boosted_categories */, Arc::clone(rayon_pool),
        Arc::clone(nli_handle), nli_top_k, nli_enabled,
        Arc::clone(inference_config), Arc::clone(observation_registry),
        Arc::clone(confidence_params), Arc::clone(categories),
    );
    let server = UnimatrixServer::new(/* ...existing... */, instructions, Some(service_layer)); // ADR-001
    Ok(ProjectServerInput { slug: slug.clone(), store, server })
}
```

Caller (the per-slug loop at `main.rs:1084`, call site `main.rs:1085-1092`) passes the SAME `Arc`s the daemon's own `ServiceLayer` consumed at
`main.rs:880-898` — `Arc::clone` of the one loaded `nli_handle`, the one resolved
`inference_config`, `confidence_params`, the operator `categories`, the `observation_registry`, and
the config-sized `ml_inference_pool`. **`Arc::clone`, never rebuild** — that is what guarantees AC-2
(one model in memory, not N) and prevents the per-slug unloaded handle.

Parity is a **closed checklist** (resolves SR-05; see ADR-006 for the full list): the 8 threaded
fields, asserted **field-by-field** against the daemon's resolved config in AC-1 — not a
representative subset.

A2 caveat (flagged, not built around): these shared `Arc`s must be truly read-only. The architect
asserts `nli_handle` is read-only at inference time; AC-2 ("one model, no per-slug mutation") is the
behavioral proof. If any carries interior-mutable cached state, it is an SR-01-class hazard — the
spec/test phase must verify (Open Question 1 in ARCHITECTURE.md §8).

### Consequences
- **Easier:** per-slug servers serve at config parity with one threading change; AC-1/AC-2 follow
  directly. No second model loads.
- **Harder / cost:** the per-slug call site grows by 8 args; `build_project_server`'s own per-slug
  `adapt_service`/`categories` defaults (180-181) are replaced by the threaded operator values.
- **Boundary held (SR-06):** ONLY the daemon's single resolved config is threaded; there is no
  per-slug override parameter. #785 later adds overrides; this ADR must not introduce that seam.
- **Depends on ADR-001** (the `Some(ServiceLayer)` constructor arm). **Feeds ADR-003** (the handle
  set inside the config-driven `ServiceLayer` is what Wave 2 maintains).
