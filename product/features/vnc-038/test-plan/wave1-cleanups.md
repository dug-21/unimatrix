# Test Plan — Wave-1 Cleanups (#735 carry-items CI-2/CI-3)

> Components: `crates/unimatrix-server/src/http/router.rs`, `http/public_url.rs:19` · Surface: file/grep check · Risks: R-15 (Low — mechanical) · AC-12, AC-13

## Scope
Two mechanical cleanups that fall out of the route-grammar rewrite (NOT separate effort). Track, do not over-weight — no isolation or integrity stake.

## File-Check Expectations

### router.rs line count (R-15 sc.1 / AC-12 / NFR-09)
- `test_router_rs_under_500_lines` — assert `crates/unimatrix-server/src/http/router.rs` is ≤ 500 lines post-rewrite. The extraction is a natural outcome of the default-alias removal + per-slug observe + `parse_project_key` simplification.

### public_url.rs dead_code removal (R-15 sc.2 / AC-13)
- `test_public_url_no_dead_code_allow` — grep/absence check: assert `crates/unimatrix-server/src/http/public_url.rs` retains NO module-level `#![allow(dead_code)]` and NO "until wiring lands" comment (the wiring landed; the allow no longer applies).

## Coverage Requirement
Both mechanical items verified once: `router.rs` ≤500 lines (AC-12); `public_url.rs` stale `dead_code`/comment removed (AC-13). Verification items, not architecture risks.
