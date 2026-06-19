# vnc-040 Architect Report — Agent vnc-040-agent-1-architect

## Artifacts
- `product/features/vnc-040/architecture/ARCHITECTURE.md`
- `product/features/vnc-040/architecture/ADR-001-per-slug-overlay-at-call-site.md` (Unimatrix #5197)
- `product/features/vnc-040/architecture/ADR-002-model-invariants-and-fallthrough-by-construction.md` (Unimatrix #5198)
- `product/features/vnc-040/architecture/ADR-003-post-merge-revalidation.md` (Unimatrix #5199)

## Key Decisions
1. New `resolve_slug_config` helper at the call-site module; overlay at `main.rs:1089-1110`, NOT in `load_config`. Reuses `merge_configs`/`load_single_config`/`validate_config`. `build_project_server` signature UNCHANGED.
2. Model invariants by construction: fields 0-2 (embed/pool/NLI) `Arc::clone`d UNCONDITIONALLY outside any merge branch (SR-07). Embedding descriptor lock (SR-03) resolves to: only descriptor is `inference.embedding_model_sha256` (already global-wins #4655) + no new descriptor field; there is NO separate `[embedding]` section today.
3. Post-merge `validate_config(&merged)` mandated inside the helper (SR-01, per #3905) — strengthens AC-08.
4. Fallthrough is structural: no-file arm returns global config and reuses the daemon's existing parity Arcs; no merge runs (SR-04, AC-02 pointer-grade sentinel).
5. ADR-003->dsn-001 #2286 reconciliation restated: C6 is a THIRD precedence layer using the SAME replace discipline; no conflict.

## Open Questions (for spec/delivery)
- OQ-A1: Spec must re-audit the `merge_configs` inline `InferenceConfig {...}` literal (#4070) for the global->per-slug call shape before reuse (SR-02). Not assumed identical to global->project.
- OQ-A2: Spec must record the dependency that the per-slug vector index stays `VectorConfig::default()`; if dims become config-driven later, an explicit `[embedding]` section lock is required (A2/SR-03 re-open).
- OQ-A3: Spec to enumerate EVERY cross-field/sum invariant in `validate_config` for the post-merge AC.
- Helper return type: `Cow<UnimatrixConfig>` proposed for allocation-free fallthrough; spec confirms exact form (invariant: no-file => no merge, no re-derivation).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced crt-056 ADR-002 (#5165 reserved seam), dsn-001 #2286 (ADR-003 reconciliation), #3905 (post-merge re-validation lesson), #4655 (hash-pin global-wins), #5196 (whole-section-lock pattern), #3923 (custom-preset prohibition). All applied directly.
- Stored: entry #5197 "ADR-001..." , #5198 "ADR-002...", #5199 "ADR-003..." via context_store (category decision). Edges: #5198 Supports->#5196, #5197 Prerequisite->#5165, #5199 Supports->#3905, intra-feature Prerequisite spine to #5197. No prior ADR superseded (vnc-040 fills the crt-056-reserved seam and extends #2286).
