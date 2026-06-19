# crt-056 — Architect Agent Report

**Agent:** crt-056-agent-1-architect | **Task:** architecture + ADRs for per-slug intelligence parity.

## Deliverables
- `product/features/crt-056/architecture/ARCHITECTURE.md`
- `architecture/ADR-001-additive-constructor.md` (Unimatrix #5136)
- `architecture/ADR-002-config-parity-threading.md` (#5137)
- `architecture/ADR-003-serviceLayer-owns-handle-set.md` (#5138)
- `architecture/ADR-004-backgroundjob-seam.md` (#5139)
- `architecture/ADR-005-serial-loop-per-slug-counter.md` (#5140)
- `architecture/ADR-006-parity-definition.md` (#5141)

## Key decisions
- ServiceLayer is the SOLE per-slug handle-set owner; PerSlugTickContext borrows its `*_handle()` accessors (same Arc<RwLock>). One handle set per slug, shared serve+tick. Resolves OQ-1; collapses SR-01/SR-07/SR-08.
- Additive `UnimatrixServer::new(..., services: Option<ServiceLayer>)`; daemon + per-slug share the `Some` parity path, `None` is test-only (SR-03, single isolation seam).
- 8 config fields threaded into `build_project_server` params-at-end; Arc::clone the ONE loaded nli_handle (AC-2); closed field-by-field parity checklist (SR-05).
- BackgroundJob seam = trait + static registry only; `ResourceClass` is a declaration-only forward hook; NO queue/pool/residency/cadence (SR-04). Registry order preserves the co-access→rebuild ordering invariant.
- Serial loop + per-slug `tick_counter` (free from per-slug TickMetadata) + serialized rayon (free from serial loop) (SR-02, SR-09). Step B not precluded, not built.

## OQ resolutions
OQ-1 → per-slug ServiceLayer owns, tick borrows. OQ-2 → minimal `run/cadence/resource_class`. OQ-3 → per-slug counters. OQ-4 → `Option` (additive). OQ-5 → adapt_service per-slug (same config, independent state); session_capabilities OUT (recommend, flagged).

## Open questions for human
1. Confirm `session_capabilities` out of parity scope (recommend out).
2. A2 re-verify shared Arcs (esp. nli_handle) are truly read-only / no interior-mutable cache — spec/test obligation.
3. Confirm OSS N envelope for the serial loop (documented, accepted per Non-Goals).

## SR coverage
SR-01/07/08 → ADR-003; SR-02/09 → ADR-005; SR-03 → ADR-001; SR-04/07 → ADR-004; SR-05/06 → ADR-002+006; SR-10 → ADR-005 (harness panic_handler). A1 re-verified (tick ops take &Store, no global singleton). A2 flagged.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced #1560 (background-tick Arc<RwLock> through ServiceLayer, sole-writer pattern) and #2552/#2553 (params-at-end constructor convention), both load-bearing and applied; context_search(decision) found no conflicting prior ADR (vnc-038 #5083 N=1-resolver and col-031 #3689 handle-pattern are adjacent/reinforcing, not superseded).
- Stored: entries #5136–#5141 "crt-056 ADR-001..006" via /uni-store-adr (decision, topic crt-056). Two high-bar Prerequisite edges asserted (#5138→#5137, #5140→#5139). No prior ADR superseded — crt-056 is net-new atop vnc-034's completed routing.
