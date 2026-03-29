# Agent Report: crt-031-agent-1-pseudocode

## Task
Produce per-component pseudocode files for crt-031 (Category Lifecycle Policy + boosted_categories
De-hardcoding) covering all 7 components in the Component Map.

## Output Files

| File | Component |
|------|-----------|
| `product/features/crt-031/pseudocode/OVERVIEW.md` | Component interaction, data flow, shared types, wave dependencies |
| `product/features/crt-031/pseudocode/categories.md` | infra/categories module split + new field + new methods |
| `product/features/crt-031/pseudocode/config.md` | infra/config KnowledgeConfig extension + validate + merge + helper |
| `product/features/crt-031/pseudocode/main.md` | main.rs 6 wiring changes (2 CategoryAllowlist + 2 ServiceLayer + 2 spawn_background_tick) |
| `product/features/crt-031/pseudocode/status.md` | services/status.rs + services/mod.rs + mcp/response/status.rs |
| `product/features/crt-031/pseudocode/background.md` | background.rs 4 functions + Step 10b stub |
| `product/features/crt-031/pseudocode/eval-layer.md` | eval/profile/layer.rs + 6 test infrastructure literal removal sites |

## Components Covered

1. `infra/categories` — module split (categories.rs → infra/categories/mod.rs + lifecycle.rs),
   new `adaptive: RwLock<HashSet<String>>` field, `from_categories_with_policy`, `is_adaptive`,
   `list_adaptive`, all tests including poison recovery for new lock.

2. `infra/config` — `adaptive_categories` field on `KnowledgeConfig`, `Default` change for
   `boosted_categories` (vec![] not vec!["lesson-learned"]), both serde default fns,
   `ConfigError::AdaptiveCategoryNotInAllowlist`, `validate_config` extension,
   `merge_configs` extension, `default_boosted_categories_set()` public helper.

3. `main.rs` — Both `CategoryAllowlist` construction sites updated to `from_categories_with_policy`,
   both `ServiceLayer::new()` sites gain `Arc::clone(&categories)`, both `spawn_background_tick`
   sites gain `Arc::clone(&categories)`.

4. `services/status` + `services/mod.rs` + `mcp/response/status.rs` — `StatusService` gains
   `category_allowlist: Arc<CategoryAllowlist>` field, all 4 `StatusService::new()` sites
   documented (including the critical `run_single_tick` site), `StatusReport` gains
   `category_lifecycle: Vec<(String, String)>`, summary/JSON formatter asymmetry documented.

5. `background.rs` — `maintenance_tick`, `background_tick_loop`, `spawn_background_tick`, and
   `run_single_tick` all gain the new parameter. Step 10b lifecycle guard stub specified with
   exact code. Critical R-02 constraint on `run_single_tick` documented.

6. `eval/profile/layer.rs` + 6 test infrastructure sites — literal removal pseudocode for all
   7 sites, verification grep commands, sequencing constraint noted.

## Open Questions / Gaps

None. All prior open questions (OQ-1 through OQ-6) were resolved in ARCHITECTURE.md and
ADR-001. No new gaps found during pseudocode production.

One clarification for implementation agent regarding `merge_configs` semantics (documented in
config.md): after the `Default` change, `default.knowledge.adaptive_categories` is `[]`.
Serde-deserialized project configs that omit `adaptive_categories` produce `["lesson-learned"]`
(not `[]`). The comparison `["lesson-learned"] != []` is `true`, so the project value "wins"
even when both configs use the serde default. This is identical behavior to `boosted_categories`
and is correct per the spec. The implementation agent must verify the test scenario in R-07
covers both the project-wins and global-fallback paths against the actual comparison logic.

## Self-Check

- [x] Architecture output was read before writing any pseudocode
- [x] No invented interface names — every name traced to ARCHITECTURE.md, ADR-001, or codebase
- [x] Output is per-component (OVERVIEW.md + 6 component files), not monolithic
- [x] Each component file includes function signatures, error handling, and test scenarios
- [x] No TODO or placeholder functions — gaps flagged explicitly
- [x] Shared types defined in OVERVIEW.md match usage in component files
- [x] All output files within `product/features/crt-031/pseudocode/`
- [x] Knowledge Stewardship report block included below

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_search` (pattern, CategoryAllowlist RwLock poison recovery) —
  returned #734 (graceful RwLock fallback), #3721 (INITIAL_CATEGORIES grep pattern), #3771
  (parallel list collision trap — directly applied in config.md SR-03 section), #2731 (NLI mutex
  poison pattern), #3700 (lock-before-loop snapshot — confirmed list_adaptive approach in Step 10b).
- Queried: `mcp__unimatrix__context_search` (decision, crt-031) — no results (ADR stored in files,
  not Unimatrix entries yet).
- Queried: `mcp__unimatrix__context_briefing` — returned #3775 (crt-031 ADR), #3770 (parallel
  category list pattern), #86 (ADR-003 RwLock HashSet). All applied.
- Deviations from established patterns: none. All design choices follow established patterns:
  - Two independent RwLocks (ADR-003, entry #86)
  - serde default fn + changed Default (entry #3774 pattern)
  - list_adaptive once per tick, not per-category is_adaptive loop (entry #3700)
  - StatusService new field follows observation_registry pattern (col-023)
  - ConfigError variant pattern follows BoostedCategoryNotInAllowlist exactly
