# Component: `UnimatrixServer::new` — additive `Option<ServiceLayer>`

> Wave 1. ADR-001 (#5136). Resolves OQ-4, FR-7, FR-8. Covers AC-6. Risk R-06.
> Source: `crates/unimatrix-server/src/server.rs:281-386` (constructor; test-default body 306-333).

## Purpose

Let callers supply a pre-built, config-driven `ServiceLayer` so per-slug servers serve at config
parity, **without** removing the existing test-default construction unit tests depend on. The change
is purely additive: append one final `Option<ServiceLayer>` param; `Some(s)` ⇒ use `s`; `None` ⇒ the
exact prior body. Daemon and per-slug both pass `Some(...)` — one isolation seam, `None` is
unit-test-only (C-6, NFR-5).

## Integration surface (exact)

Existing signature (`server.rs:281-292`), 10 params ending in `instructions: Option<String>`:
```
UnimatrixServer::new(
    entry_store: Arc<Store>, vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    embed_service: Arc<EmbedServiceHandle>, registry: Arc<AgentRegistry>, audit: Arc<AuditLog>,
    categories: Arc<CategoryAllowlist>, store: Arc<Store>, vector_index: Arc<VectorIndex>,
    adapt_service: Arc<AdaptationService>, instructions: Option<String>,
) -> Self
```
Wave-1 signature (ADR-001 — append final param):
```
UnimatrixServer::new(
    /* ...existing 10 params, unchanged order... */,
    services: Option<ServiceLayer>,        // NEW, LAST
) -> Self
```

## Modified function: `UnimatrixServer::new`

```text
fn new(... existing 10 params ..., services: Option<ServiceLayer>) -> Self:

    # --- UNCHANGED setup (server.rs:293-304) ---
    implementation = Implementation::new(SERVER_NAME, CARGO_PKG_VERSION).with_description(...)
    server_info    = ServerInfo::new(...).with_server_info(implementation)
                       .with_instructions(instructions.unwrap_or(SERVER_INSTRUCTIONS_DEFAULT))
    usage_dedup    = Arc::new(UsageDedup::new())

    # --- ADR-001 CORE CHANGE ---
    # The ENTIRE existing test-default body (server.rs:306-333) moves verbatim into the
    # None arm. Byte-for-byte: test-pool size 1, NliServiceHandle::new() (unloaded),
    # nli_top_k 20, nli_enabled false, InferenceConfig::default(), DomainPackRegistry::
    # with_builtin_claude_code(), ConfidenceParams::default(), CategoryAllowlist::new(),
    # default_boosted_categories_set().
    services = match services:
        Some(s) => s                          # config-driven layer supplied by caller (parity path)
        None    => ServiceLayer::new(         # EXACT prior body, moved unchanged (C-4, AC-6)
                       Arc::clone(&store), Arc::clone(&vector_index), Arc::clone(&vector_store),
                       Arc::clone(&entry_store), Arc::clone(&embed_service), Arc::clone(&adapt_service),
                       Arc::clone(&audit), Arc::clone(&usage_dedup),
                       default_boosted_categories_set(),
                       Arc::new(RayonPool::new(1, "test-pool").expect(...)),   # size-1 test pool
                       NliServiceHandle::new(),                                # unloaded
                       20, false,                                             # nli_top_k, nli_enabled
                       Arc::new(InferenceConfig::default()),
                       Arc::new(DomainPackRegistry::with_builtin_claude_code()),
                       Arc::new(ConfidenceParams::default()),
                       Arc::new(CategoryAllowlist::new()),
                   )

    # --- UNCHANGED from here (server.rs:335-386) ---
    effectiveness_state = services.effectiveness_state_handle()
    tick_metadata       = Arc::new(Mutex::new(TickMetadata::new()))
    UnimatrixServer { ... services, effectiveness_state, tick_metadata, ... }   # all other fields identical
```

### Critical correctness notes
- The `usage_dedup` built at the top is consumed only by the `None` arm's `ServiceLayer::new`. In the
  `Some` arm the caller's `ServiceLayer` already owns its own `usage_dedup`; the locally-built one is
  unused on that arm. Keep the local build (cheap, and the `None` arm needs it). Do NOT thread the
  local `usage_dedup` into the `Some` layer.
- `effectiveness_state = services.effectiveness_state_handle()` MUST run on BOTH arms after `services`
  is resolved (it already does — it reads from whichever layer won). This preserves the existing
  serve-side handle wiring for `Some` layers too (pattern #4097: serving consumers hold `Arc::clone`s).
- Field-for-field, the returned struct is identical to today except `services` may be the supplied one.
  No field is dropped, added, or reordered.

## Data flow
- Input: 10 existing params + `services: Option<ServiceLayer>`.
- Output: a `UnimatrixServer` whose `services` field is the supplied (`Some`) or test-default (`None`)
  layer, with `effectiveness_state`/`tick_metadata` derived as before.
- Transformation: none on `Some`; full test-default construction on `None`.

## Error handling
- Infallible (`-> Self`), unchanged. The only fallible call is the `None` arm's `RayonPool::new(1,...)`,
  which keeps its existing `.expect("test RayonPool construction must succeed")` — test-only path, so
  the panic-on-failure contract is preserved (FR-8 byte-for-byte).
- No new error variant. Config validation stays upstream (`validate_config`), as the existing doc-comment
  at `server.rs:278-280` states.

## Call-site impact (mechanical, params-at-end)
Every `UnimatrixServer::new` call gains a trailing arg (ADR-001 Consequences enumerate via #2552/#2553):
- `server.rs` self / `http_provision.rs:186-197` / daemon `main.rs:919-933` ⇒ pass `Some(...)`.
- `test_support.rs`, `uds/listener.rs::make_services`, `infra/shutdown.rs`, all unit tests ⇒ append `, None`.
This component owns only the signature + body; the `Some(...)` value is built by `build-project-server.md`
(per slug) and `daemon-http-boot.md` (daemon). Test call-site `, None` edits belong to those test files.

## Key test scenarios (hints for tester)
- **AC-6 / R-06.1 (test-default preserved).** An existing unit-test call site appending `, None` compiles
  and produces NLI-off / pool-size-1 / `InferenceConfig::default` / `ConfidenceParams::default` / empty
  `CategoryAllowlist` behavior — byte-for-byte the prior server. Assert on a constructed test server's
  resolved fields.
- **R-06.2 (same-path / no cloud-only branch).** Assert daemon and per-slug both reach `Some(config-driven)`;
  `None` is reachable only from unit tests. Structural: grep that no `if cloud {...} else {...}` parity
  branch exists; the only branch is `Some`/`None` and both converge on the same field assignments.
- **Some-arm wiring.** Construct with `Some(layer)`; assert `server.services` is that layer and
  `server.effectiveness_state` is `Arc::ptr_eq` to `layer.effectiveness_state_handle()` (feeds R-03).
</content>
