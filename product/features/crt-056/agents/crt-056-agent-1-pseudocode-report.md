# Agent Report — crt-056-agent-1-pseudocode (Stage 3a pseudocode)

## Deliverables
Per-component pseudocode under `product/features/crt-056/pseudocode/`:
- `OVERVIEW.md` — component map, sequencing, data flow, shared types, constraint coverage, gaps.
- `unimatrix-server-new.md` (ADR-001) — additive `Option<ServiceLayer>`; `None` arm = byte-for-byte test default.
- `build-project-server.md` (ADR-002/006) — 8-field config-parity threading; `Arc::clone` shared model.
- `daemon-http-boot.md` (ADR-002/003/005) — thread Arcs, collect contexts, retire global-handle wiring (957-961, 968-991).
- `per-slug-tick-context.md` (ADR-003) — borrow bundle; handles are `Arc::clone`s of `ServiceLayer` accessors.
- `background-job-seam.md` (ADR-004) — trait/registry/shared types; 9 jobs delegate to existing ops; Wave-2 gating audit.
- `per-slug-tick-loop.md` (ADR-005) — serial loop, per-slug counter, serialized rayon.
- `multi-slug-harness.md` (NFR-7/C-9) — N=2 Layer-2 fixture + rayon `panic_handler`.

## Components covered
All 7 from the brief's Component Map. Every interface name/signature traced to ARCHITECTURE §6 or live
source (constructor 281-386; build_project_server 125-204; daemon ServiceLayer 880-898; handle extraction
957-961; spawn_background_tick 968-991; run_single_tick ops + ordering invariant 441-804/546-551; per-slug
counter 352-358; panic→restart 286-301). No invented names.

## Honored decisions/constraints
ServiceLayer sole owner, PerSlugTickContext borrows via `*_handle()` (same Arc); additive `None` arm
byte-for-byte; BackgroundJob is SHAPE only (no queue/pool/residency/cadence-signal); serial loop, per-slug
counter, serialized rayon; one shared model via `Arc::clone`; ordering invariant preserved by registry
order (co-access promotion AFTER compaction, BEFORE TypedGraphState::rebuild); session_capabilities OUT.

## Open questions / gaps (flagged, not placeheld)
- **G-1:** A2 interior-immutability of shared `Arc`s is a delivery-time type-level audit (R-04); pseudocode treats them read-only.
- **G-2:** boot loop needs slug `ServiceLayer` + `tick_metadata` readable off `input.server`; add thin accessors if not exposed (no new state).
- **G-3:** `vector_index` for the context — read off `input.server.vector_index` or add to `ProjectServerInput` (additive).
- **boosted_categories:** confirm provenance for AC-1 domain/category parity; may be a 9th threaded arg (flagged in build-project-server.md).
- **spawn_background_tick deletion:** confirm no other caller before removing the fn.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern) + context_get — #1560 (Arc<RwLock<T>> through ServiceLayer, sole writer = tick; grounds ADR-003), #4097 (post-construction write only propagates via Arc::clone; grounds handle-identity invariant), #4974 (ceremonial seam / N=1 false confidence; grounds Wave-2 funnel audit + AC-4 N=2), #2535 (rayon monopolisation envelope), #2543/#3355 (rayon panic_handler / Cancelled probe; grounds harness).
- Deviations from established patterns: none. Pseudocode applies #1560/#4097/#4974/#2535/#2543 directly.
</content>
