# Agent Report: crt-031-agent-3-categories

**Component:** infra/categories (module split + lifecycle policy)
**Feature:** crt-031
**Date:** 2026-03-29

## Work Completed

### Files Created
- `crates/unimatrix-server/src/infra/categories/mod.rs` — struct definition, all impl methods (131 lines)
- `crates/unimatrix-server/src/infra/categories/lifecycle.rs` — reserved stub for #409 (6 lines)
- `crates/unimatrix-server/src/infra/categories/tests.rs` — pre-existing test suite, poison helpers (388 lines)
- `crates/unimatrix-server/src/infra/categories/lifecycle_tests.rs` — crt-031 lifecycle policy tests (252 lines)

### Files Deleted
- `crates/unimatrix-server/src/infra/categories.rs` — replaced by directory module

### Changes Made

1. **Module split** — moved all content from `categories.rs` into `categories/mod.rs`, confirmed zero import regressions via `cargo check` before adding any new code.

2. **Struct extension** — added `adaptive: RwLock<HashSet<String>>` as a second independent lock field alongside the existing `categories` field (ADR-001 decision 3).

3. **Constructor hierarchy** — implemented `from_categories_with_policy(cats, adaptive) -> Self` as the canonical constructor. Updated `from_categories` to delegate with `vec!["lesson-learned"]` as default adaptive. `new()` signature and behavior unchanged.

4. **New methods** — `is_adaptive(&self, category: &str) -> bool` and `list_adaptive(&self) -> Vec<String>` both use `.unwrap_or_else(|e| e.into_inner())` poison recovery on the `adaptive` lock.

5. **Doc comment on add_category** — added invariant statement: domain pack categories registered at runtime are always pinned; adaptive policy is config-only and frozen after startup.

6. **Test suite** — 51 tests total (20 pre-existing + 31 new). Split across two files to stay under the 500-line ceiling:
   - `tests.rs` — all 20 pre-existing tests + shared helpers (`poison_allowlist`, `poison_adaptive_lock`)
   - `lifecycle_tests.rs` — 31 new crt-031 tests covering AC-05 through AC-08, AC-13, R-03, R-06, R-09, E-01 through E-04, E-06

## Test Results

```
test result: ok. 51 passed; 0 failed; 0 ignored
```

All 20 pre-existing tests pass without modification (AC-12 satisfied).
All 31 new lifecycle policy tests pass.

## Self-Check

- [x] `cargo build --workspace` passes (zero errors in my component)
- [x] `cargo test` for `infra::categories` — 51 passed, 0 failed
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or `HACK` in non-test code
- [x] All modified files are within the scope defined in the brief
- [x] Error handling uses `.unwrap_or_else(|e| e.into_inner())` poison recovery — no `.unwrap()` in non-test code
- [x] New struct field has `RwLock<HashSet<String>>` (Debug derives on the parent struct)
- [x] Code follows validated pseudocode — no silent deviations
- [x] Test cases match component test plan expectations
- [x] All source files under 500 lines (mod.rs: 131, tests.rs: 388, lifecycle_tests.rs: 252, lifecycle.rs: 6)
- [x] Knowledge Stewardship report block included

## Issues

**Pre-existing compile issue in config.rs** (not my component): Another agent's test `test_adaptive_categories_serde_round_trip` at config.rs:3147 calls `toml::to_string(&original)` which requires `KnowledgeConfig: Serialize`, but the struct only derives `Deserialize`. This is a blocker for the full test suite but does not affect the categories component. The categories tests run and pass independently.

The one failing test across the crate is `infra::config::tests::test_adaptive_categories_serde_default_when_omitted` — also in config.rs, not categories.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced entries #3775 (ADR-001 crt-031), #3770, #3771 (parallel category list patterns). Applied: confirmed two-lock design, poison recovery pattern, serde default fn convention.
- Queried: `context_search("CategoryAllowlist RwLock poison recovery patterns")` — entry #734 (graceful RwLock fallback) confirmed `.unwrap_or_else(|e| e.into_inner())` is the established pattern.
- Stored: entry #3778 "When a test file split from a module still exceeds 500 lines, split tests.rs into tests.rs (pre-existing) + feature_tests.rs (new), sharing helpers via pub(super)" via /uni-store-pattern
