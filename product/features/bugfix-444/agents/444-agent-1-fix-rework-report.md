# Agent Report: 444-agent-1-fix-rework

**Feature**: bugfix-444
**Branch**: bugfix/444-enforce-index-active-set-invariant

## Task

Resolve two blocking security findings on the existing bugfix-444 PR:
- F-1: `heal_pass_batch_size` missing range guard in `InferenceConfig::validate()`
- F-2: Bare integer literals (`status = 0`, `status = 3`) in new SQL queries

## Changes Made

### F-1: `InferenceConfig::validate()` range guard

File: `crates/unimatrix-server/src/infra/config.rs`

Added range check `[1, 1000]` for `heal_pass_batch_size` at the end of `validate()`, just before `Ok(())`. Follows the exact early-return pattern used by every other bounded field (`max_graph_inference_per_tick`, `ppr_max_expand`, etc.) with `ConfigError::NliFieldOutOfRange`. A value of `0` would produce `LIMIT 0` in the heal-pass SQL, silently disabling the heal pass.

Also added 5 new tests:
- `test_heal_pass_batch_size_zero_rejected` — 0 fails
- `test_heal_pass_batch_size_1001_rejected` — 1001 fails
- `test_heal_pass_batch_size_valid_min` — 1 passes
- `test_heal_pass_batch_size_valid_max` — 1000 passes
- `test_heal_pass_batch_size_default_valid` — 20 (default) passes

### F-2: Typed status binding in SQL queries

File: `crates/unimatrix-server/src/services/status.rs`

Added `Status` to the existing `unimatrix_store` import at module level (merged into `use unimatrix_store::{EntryRecord, Status}`).

Three SQL queries updated to use parameterized typed bindings:

1. Unembedded active count (line ~699): `WHERE status = 0` -> `WHERE status = ?1` with `.bind(Status::Active as u8 as i64)`
2. Prune pass quarantined query (line ~937): `WHERE e.status = 3` -> `WHERE e.status = ?1` with `.bind(Status::Quarantined as u8 as i64)`
3. Heal pass unembedded query (line ~986): `WHERE status = 0 ... LIMIT ?1` -> `WHERE status = ?1 ... LIMIT ?2` with bind parameters renumbered to avoid collision

Pattern follows the established codebase convention from `unimatrix-store/src/read.rs`: `.bind(Status::Active as u8 as i64)`.

## Test Results

- Before: 2325 passed, 0 failed (baseline)
- After: 2330 passed, 0 failed (+5 new tests for heal_pass_batch_size validation)

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- not invoked (targeted rework with explicit scope; no architectural ambiguity requiring briefing)
- Stored: nothing novel to store -- the SQL bind pattern and validate() pattern are already established codebase conventions visible in source. No new gotchas discovered.
