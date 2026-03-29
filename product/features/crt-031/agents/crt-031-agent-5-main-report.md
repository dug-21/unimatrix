# Agent Report: crt-031-agent-5-main

**Component**: main.rs startup wiring
**Feature**: crt-031 — Category Lifecycle Policy + boosted_categories De-hardcoding
**Date**: 2026-03-29

## Summary

Updated `crates/unimatrix-server/src/main.rs` with all 6 changes across both startup paths.

## Changes Made

### Files Modified
- `crates/unimatrix-server/src/main.rs`

### Changes by Type

**CategoryAllowlist construction (2 sites — changes 1+2):**
- Line 550: Path 1 (project config) — added `adaptive_categories` extraction from `config.knowledge.adaptive_categories`; changed `CategoryAllowlist::from_categories(knowledge_categories)` to `CategoryAllowlist::from_categories_with_policy(knowledge_categories, adaptive_categories)`
- Line 946: Path 2 (global config) — identical change

**ServiceLayer::new call sites (2 sites — changes 3+4):**
- Line 659: Path 1 — added `Arc::clone(&categories)` as new final argument
- Line 1054: Path 2 — identical change

**spawn_background_tick call sites (2 sites — changes 5+6):**
- Line 734: Path 1 — added `Arc::clone(&categories)` as new final argument
- Line 1133: Path 2 — identical change

**UnimatrixServer::new call sites (bonus fix, not in pseudocode spec):**
- Lines 687 and 1082: Changed `categories` (move) to `Arc::clone(&categories)`. Required to prevent a use-after-move compile error: `UnimatrixServer::new` previously consumed `categories` by value, but `spawn_background_tick` (called afterward) now also needs an `Arc::clone(&categories)`. The pseudocode did not flag this because it showed the ordering correctly but did not note that the pre-existing move would become invalid.

## Build Status

Build with `cargo build -p unimatrix-server` produces exactly 3 errors, all inter-wave dependencies as expected:

| Error | File | Root Cause | Expected |
|-------|------|------------|----------|
| `E0061`: 10 args but 9 given | `background.rs:452` | `StatusService::new` new param — background agent (Wave 2) | Yes |
| `E0063`: missing field `category_lifecycle` | `services/status.rs:498` | `StatusReport` new field — status/response agent (Wave 2) | Yes |
| `E0061`: 10 args but 9 given | `services/mod.rs:461` | `StatusService::new` new param — status/mod agent (Wave 2) | Yes |

Zero errors originating from `main.rs`. No unexpected errors.

## Verification

```
grep -n "from_categories_with_policy\|adaptive_categories\|Arc::clone(&categories)" main.rs
```

Output shows all 12 expected hits across both startup paths (2 adaptive_categories extractions, 2 from_categories_with_policy calls, 8 Arc::clone(&categories) uses — 2 for ServiceLayer, 2 for UnimatrixServer, 2 for spawn_background_tick, and 2 internal to from_categories_with_policy call syntax).

## Issues / Deviations from Pseudocode

**One deviation from pseudocode spec (necessary fix):** The pseudocode showed `categories` being passed as a move to `UnimatrixServer::new` and then `Arc::clone(&categories)` in `spawn_background_tick` afterward. This is a use-after-move that would not compile. Fixed by converting the `UnimatrixServer::new` argument from a move to `Arc::clone(&categories)`. This matches how all other `Arc` handles in the same call sites are passed. No behavior change — both produce an additional strong reference to the same `Arc<CategoryAllowlist>`.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — found entry #3213 (threading new Arc to background tick has a SECOND invisible construction site) and entry #3775 (crt-031 ADR-001 summary). Entry #3213 was directly relevant — it warned about the silent second construction site, confirming the R-02 risk was real.
- Stored: entry #3779 "When threading a new Arc into spawn_background_tick, check for pre-existing moves of that Arc into UnimatrixServer::new between construction and the tick call" via /uni-store-pattern
