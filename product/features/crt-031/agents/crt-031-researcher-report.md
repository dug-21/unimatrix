# crt-031 Researcher Report

## Summary

Explored the problem space for the category lifecycle policy feature. SCOPE.md written to
`product/features/crt-031/SCOPE.md`.

## Key Findings

### CategoryAllowlist (categories.rs, 454 lines)

- Wraps `RwLock<HashSet<String>>` — currently carries only presence/absence, no policy metadata
- `from_categories(Vec<String>)` is the primary constructor; `new()` delegates to it with `INITIAL_CATEGORIES`
- `add_category()` is called at runtime for domain pack categories — will default to `pinned` under the new policy
- Poison recovery pattern: `.unwrap_or_else(|e| e.into_inner())` on every lock access — `is_adaptive()` must follow the same pattern
- Test suite enforces exactly 5 categories and guards specifically against `outcome` (ADR-005); the new field does not change category count

### KnowledgeConfig (config.rs)

- `boosted_categories: Vec<String>` is the direct structural predecessor for `adaptive_categories` — same semantics (parallel list, must be subset of `categories`)
- `validate_config()` already builds a `category_set: HashSet<&str>` for the boosted check; the adaptive check can reuse it at zero cost
- `ConfigError::BoostedCategoryNotInAllowlist` is the exact model for the new `AdaptiveCategoryNotInAllowlist` variant
- Two `main.rs` call sites construct `CategoryAllowlist::from_categories(knowledge_categories)` — both must pass `adaptive_categories`

### maintenance_tick (background.rs)

- `maintenance_tick()` is the correct insertion point for the lifecycle guard stub
- The stub belongs after Step 10 (`run_maintenance`) and before Step 11 (dead-knowledge migration)
- `maintenance_tick` does not currently receive `Arc<CategoryAllowlist>` — this parameter must be threaded through `spawn_background_tick` and `background_tick_loop`

### context_status (status.rs / mcp/response/status.rs)

- `StatusReport` is a large flat struct with `Default` impl — adding `category_lifecycle: Vec<(String, String)>` defaults to empty `Vec`
- `category_distribution` (existing) is the closest field to what lifecycle adds — both are per-category metadata
- `StatusService::compute_report()` is where the new field gets populated
- Both summary text and JSON (`StatusReportJson`) need updating

### ASS-032 ROADMAP context

- Issue #445 is this feature; dependency graph shows it has no blocking deps and can progress in parallel with #409
- #409 (signal-driven entry auto-deprecation) is explicitly downstream of #445 — the guard stub is the integration seam

## Open Questions for Human (in SCOPE.md)

1. Constructor API: `from_categories_with_policy` new constructor vs augmenting `from_categories` signature
2. Status output format: full list vs adaptive-only in summary text
3. Domain pack `add_category` lifecycle: confirm default `pinned` is correct
4. Test count gate requirement for IMPLEMENTATION-BRIEF

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` -- returned 20 entries; entries #3715/#3721 (INITIAL_CATEGORIES lockstep pattern), #2312 (boosted_categories test gotcha), #86 (CategoryAllowlist ADR-003) were most relevant. Entry #178 (ADR-002 maintenance opt-out) confirmed the maintenance tick pattern.
- Stored: entry #3770 "KnowledgeConfig parallel list fields follow the boosted_categories structural pattern" via /uni-store-pattern
