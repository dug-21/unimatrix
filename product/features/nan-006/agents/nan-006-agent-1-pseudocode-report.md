# Agent Report: nan-006-agent-1-pseudocode

## Files Produced
- /workspaces/unimatrix/product/features/nan-006/pseudocode/OVERVIEW.md
- /workspaces/unimatrix/product/features/nan-006/pseudocode/rust-env-var.md
- /workspaces/unimatrix/product/features/nan-006/pseudocode/fast-tick-fixture.md
- /workspaces/unimatrix/product/features/nan-006/pseudocode/test-availability.md
- /workspaces/unimatrix/product/features/nan-006/pseudocode/docs-update.md
- /workspaces/unimatrix/product/features/nan-006/pseudocode/mark-registration.md

## Components Covered
- C1: Rust env var (background.rs) — read_tick_interval() function + background_tick_loop modification
- C2: fast_tick_server fixture + UnimatrixClient extra_env parameter
- C3: test_availability.py — all 6 tests with full pseudocode
- C4: USAGE-PROTOCOL.md — Pre-Release Gate section + table + suite reference
- C5: pytest.ini — availability mark registration

## Key Design Decisions
1. UnimatrixClient needs `extra_env: dict[str, str] | None = None` parameter — cleanest way to pass env vars to subprocess without modifying fixture env globals
2. Rust: `fn read_tick_interval() -> u64` is a pure function at module scope — testable without async runtime
3. TICK_INTERVAL_SECS constant is REMOVED entirely — replaced by function call at startup (not a module-level constant)
4. test_cold_start_request_race requires raw UnimatrixClient instantiation (not via fixture) to avoid wait_until_ready()
5. xfail tests use strict=False — prevents XPASS from failing the suite if a bug is incidentally fixed

## Open Questions
- None: all design decisions resolved from source documents and GH#281

## Knowledge Stewardship
- Queried: /uni-query-patterns for unimatrix-server -- patterns present (async wrappers, env var config patterns). Consistent with existing patterns.
- Deviations from established patterns: none — env var fallback pattern is standard Rust
