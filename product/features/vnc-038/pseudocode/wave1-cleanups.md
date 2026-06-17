# Component 12 — Wave-1 Cleanups (CI-2 / CI-3)

**Files:** `crates/unimatrix-server/src/http/router.rs` (CI-2), `crates/unimatrix-server/src/http/public_url.rs` (CI-3)
**AC:** AC-12, AC-13 · **Risk:** R-15 (Low, mechanical) · **NFR:** NFR-09

## Purpose

Two mechanical cleanups that fall out of the route-grammar rewrite — not separate effort.

## CI-2 — `router.rs` ≤ 500 lines (AC-12)

```
router.rs is ~562 lines today (> the 500-line guideline). The route-grammar rewrite naturally shrinks it:
  - The large inline (POST, "/observe") block (router.rs:188-~280) is EXTRACTED to route_observe
    (Component 6, lives in observe.rs or a router submodule) — removes ~90 lines from router.rs.
  - The top-level /observe match arm collapses to a small /v1/.../observe dispatch (Component 6.B).
ACTION:
  - Confirm router.rs is at/under 500 lines AFTER Component 6 lands.
  - If still over, extract the helper response builders (json_error_response, payload_too_large_response,
    internal_error_response, map_health_response) or the PathRouter Service impl into a sibling submodule.
    Prefer moving COHESIVE units (the observe handler already moved; group the error-response helpers next).
  - No behavior change — pure relocation; re-export through the module so call sites are unaffected.
```

> This is an OUTCOME of Components 5/6, verified here — not independent work. The extraction target is the observe handler body, which Component 6 already relocates into `route_observe`.

## CI-3 — remove stale `dead_code` allow (AC-13)

```
public_url.rs:19 has a module-level `#![allow(dead_code)]` and a doc paragraph (lines ~14-18) saying
"Until that wiring lands this module is referenced only by its own tests, so dead_code is allowed".
The wiring HAS landed (derive_public_url is consumed by client_bundle.rs:133 / Component 1).
ACTION:
  - DELETE the `#![allow(dead_code)]` at public_url.rs:19.
  - DELETE the "until that wiring lands ... dead_code is allowed crate-internally" sentence(s) in the
    module doc-comment (the "until wiring lands" comment, AC-13).
  - Build the crate; if any genuinely-unused item now warns, that is a real dead-code finding — remove
    the item or wire it, do NOT re-add the blanket allow (the point is to surface real dead code).
```

## Error Handling

- None (mechanical, compile-time). A new dead-code warning after removing the allow is a real finding to resolve, not to suppress.

## Key Test Scenarios (hints)

1. R-15 sc.1 (AC-12): line-count check — `http/router.rs` ≤ 500 lines post-rewrite.
2. R-15 sc.2 (AC-13): absence check (grep) — `public_url.rs` retains NO `#![allow(dead_code)]` and NO "until wiring lands" comment.
3. `cargo clippy -- -D warnings` clean after the allow is removed (no new dead-code warning, or the flagged item is genuinely wired/removed).
