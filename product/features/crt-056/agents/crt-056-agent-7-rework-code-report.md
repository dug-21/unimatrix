# crt-056 Gate 3c Rework (code side) — Agent Report

Agent: crt-056-agent-7-rework-code
Commit: 9ccde2a9 on `feature/crt-056`

## Scope
CODE side of Gate 3c REWORKABLE FAIL: (1) correct inaccurate "sole tick path /
no longer wired" claims; (2) make AC-1/AC-2 testable via thin additive accessors.
Did NOT write AC-1/AC-2 tests (uni-tester owns those) and did NOT touch
RISK-COVERAGE-REPORT.md or wave2-gating-audit.md.

## Task 1 — corrected the inaccurate "sole path" claims (WARN, NFR-5/C-6)

Decision applied: do NOT re-wire stdio; correct the claims to be accurate.

- `crates/unimatrix-server/src/background/tick_loop.rs`
  - Module doc: replaced "This is the SOLE tick path now ... legacy
    `spawn_background_tick` global-handle path is no longer wired from main.rs"
    with scoped wording — the global-handle tick is RETIRED on the multi-project
    HTTP daemon path (which drives the per-slug serial loop); documented the
    stdio single-store path (N=1, no per-slug servers) as an accepted carve-out
    that retains the legacy single-store tick.
  - `spawn_per_slug_tick` doc: "replaces `spawn_background_tick`" → "replaces it
    ON THE DAEMON PATH — the stdio single-store path retains the legacy tick".
- `crates/unimatrix-server/src/main.rs`
  - HTTP daemon comment (~1196): "the SOLE tick path" → "the SOLE tick path ON
    THIS multi-project HTTP daemon path ... stdio keeps its own legacy tick".
  - stdio `spawn_background_tick` call site (~1596): added a carve-out comment
    documenting why stdio (N=1) deliberately retains the legacy global-handle tick.

No behavioral change. Verified no other code comments carry the inaccurate claim
(grep of `crates/` for "sole tick path" / "no longer wired", excluding the two
tester-owned report files).

## Task 2 — made AC-1 / AC-2 testable (thin additive read-only accessors)

All getters are additive, read-only, introduce no new state and no behavioral
change. The resolved-config values live on the sub-services exactly as the daemon
built them; the new `ServiceLayer` getters surface them for assertion. A test
builds a per-slug server via `build_project_server` and reads `server.service_layer()`.

### New `pub` accessors on `ServiceLayer` (`services/mod.rs`) — the test surface
```rust
pub fn nli_handle(&self) -> &Arc<NliServiceHandle>            // AC-2: Arc::ptr_eq vs daemon's
pub fn nli_enabled(&self) -> bool                             // AC-1 (both directions)
pub fn nli_top_k(&self) -> usize                              // AC-1
pub fn fusion_weights(&self) -> FusionWeights                 // AC-1 (InferenceConfig-derived parity)
pub fn boosted_categories(&self) -> &std::collections::HashSet<String>  // AC-1
pub fn confidence_params(&self) -> &Arc<unimatrix_engine::confidence::ConfidenceParams> // AC-1
pub fn observation_registry(&self) -> &Arc<DomainPackRegistry>  // AC-1 (domain packs)
pub fn category_allowlist(&self) -> &Arc<CategoryAllowlist>     // AC-1
pub fn ml_inference_pool(&self) -> &Arc<RayonPool>             // AC-1: .pool_size() for effective pool size
```

### Supporting `pub(crate)` getters (delegated to by the above)
- `services/search.rs` (`impl SearchService`): `nli_handle()`, `nli_top_k()`,
  `nli_enabled()`, `fusion_weights()`, `boosted_categories()`.
- `services/status.rs` (`impl StatusService`): `confidence_params()`,
  `observation_registry()`, `category_allowlist()`.

### Type changes (additive, needed for the boundary)
- `FusionWeights`: added `PartialEq` derive (clean field-by-field equality in the
  test) and changed `pub(crate) struct` → `pub struct`; re-exported
  `pub use search::FusionWeights;` from `services/mod.rs` so the binary-crate test
  can name the return type of `ServiceLayer::fusion_weights()`.

### Notes for the tester (AC-1 / AC-2 assertion guidance)
- 8-field parity checklist is now all readable through `service_layer()`:
  `nli_enabled()`, `nli_top_k()`, `nli_handle()`, the inference-derived
  `fusion_weights()`, `confidence_params()`, `category_allowlist()`,
  `observation_registry()` (domain packs), and `ml_inference_pool().pool_size()`
  (effective rayon pool size). `boosted_categories()` is also exposed.
- AC-2: `build_project_server` threads `Arc::clone(nli_handle)` (never
  `NliServiceHandle::new()`), so `Arc::ptr_eq(per_slug.service_layer().nli_handle(),
  daemon_nli_handle)` holds, and across N=2 slugs all `nli_handle()` results are the
  same Arc. The daemon's resolved Arcs (`inference_config`, `confidence_params`,
  `category_allowlist`, `observation_registry`, `rayon_pool`) are `Arc::clone`d into
  each per-slug ServiceLayer, so those parity fields can be asserted by value
  (PartialEq) or by `Arc::ptr_eq` where the same Arc is threaded.

## Build status
`cargo build -p unimatrix-server` — green (lib + bins). Warnings unchanged from
baseline (24, one fewer than before — removed a redundant getter). No new clippy
findings from the added code (the `too_many_arguments` notes are pre-existing on
the `new` constructors). Did not run/modify integration tests per spawn directive.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced #5165 (ADR-002 params-at-end
  threading), #2552/#2938/#3248 (Arc<T> ServiceLayer threading patterns), #5171
  (R-04 interior-immutability Step-B scope), #3216 (threaded-to-tick-but-not-ServiceLayer
  bug class). Applied: kept accessors additive/delegating, no new Arc state.
- Stored: entry #5174 "Scope absolute 'sole path / retired' claims to the path they
  actually hold on" via context_store (lesson-learned).
