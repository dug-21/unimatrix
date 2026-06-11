# Agent Report — vnc-034-agent-3-slug-router

**Component:** SlugRouter + StoreResolver seam (Wave 1, C4 isolation seam)
**Wave / sub-wave:** Wave 1, Sub-wave 1 (shared types other components depend on)

## Summary

Implemented the C4 isolation seam MINIMAL per ADR-003/004/005 and `pseudocode/slug-router.md`:
route grammar + `StoreResolver` trait + `SlugRouter` layer + the `ProjectSlug` allowlist
parse edge + `RouteError`. The slug RESOLVER logic and `ProjectRouter`-as-`StoreResolver`
are correctly left to Wave 2; `DefaultResolver` is correctly left to its own component;
`main.rs` wiring is correctly left to Sub-wave 3 (not touched).

## Files modified

- `crates/unimatrix-server/src/http/router/seam.rs` (NEW) — the seam: `ProjectKey`,
  `ProjectSlug` (+`TryFrom` allowlist), `StoreResolver` trait, `RouteError`,
  `parse_project_key`, `SlugRouter` layer.
- `crates/unimatrix-server/src/http/router.rs` — `mod seam;` + `pub use` re-export of the
  locked surface; removed the inline seam draft. Existing `PathRouter` / `ProjectRouter` /
  `McpAdapter` preserved unchanged (retained for Wave-2 extension).
- `crates/unimatrix-server/src/http/router/tests.rs` — 15 seam tests appended (cumulative,
  extends the existing module; no isolated scaffolding).

NOT touched (per constraints): `tls.rs`, `public_url.rs`, `main.rs`, `client_bundle.rs`.

## Locked surface (built exactly to spec — no invented variants)

```rust
pub enum ProjectKey { Default, Slug(ProjectSlug) }
pub struct ProjectSlug(String);                       // TryFrom<&str>: ^[a-z0-9][a-z0-9-]{0,62}$
pub trait StoreResolver: Send + Sync + 'static {
    fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError>;
}
pub enum RouteError { UnknownProject, InvalidSlug(String) }
pub struct SlugRouter<ReqBody> { /* resolver: Arc<dyn StoreResolver>, project_router */ }
```

Route grammar (ADR-005): `/v1/tools/...` -> `Default` (matched BEFORE the slug arm so the
reserved literal `tools` can never become a slug); `/v1/{slug}/tools/...` -> `Slug` (parses
in Wave 1, resolver returns `UnknownProject` — slug path inert until Wave 2); non-/v1 paths
-> `Default` (backward-compat). `InvalidSlug` -> 400 JSON; `UnknownProject` -> 404 JSON
(never the default store, R-01 sc.3); no panic, no `.unwrap()`, no path join before validation.

## Tests: 15 added, all pass (81 passed / 0 failed in `http::router`)

- Route grammar: v1/tools->Default, v1/{slug}/tools->Slug, non-v1->Default, reserved `tools`
  never a slug, reserved words (health/observe/v1) parse-as-slug-but-inert.
- Allowlist corpus (R-03): accepts valid + 63-char max; rejects `../`, `..`, `a/../b`,
  `%2e%2e`, `%2f`, `a%2fb`, `%2e`, `/etc`, `/etc/passwd`, `.`, `/`, `\`, `a\b`, `a.b`,
  leading `-`, uppercase, whitespace/tab, empty, 64-char over-length; exact 63/64 boundary;
  traversal slug fails at the parse edge before resolution; InvalidSlug carries input.
- R-01: Slug under default-like resolver -> `UnknownProject` (never default store, never
  panic); resolver-swap proves the Wave1<->Wave2 boundary IS the trait (stub `ProjectRouter`
  injected the same `Arc<dyn StoreResolver>` way, lights up the slug, no callsite change).
- R-10/C6: seam types present + object-safe. RouteError Display does not echo raw rejected
  input.

Validation: `cargo build -p unimatrix-server` clean; `cargo clippy` — zero warnings on my
files (verified `router/seam.rs` count == 0; the one `router.rs` clippy hit is the
pre-existing `OBSERVE_PATH` const, not mine); `cargo fmt` applied.

## Issues / notes for the leader

1. **`router.rs` is 539 lines (> 500).** It was ALREADY ~524 lines before vnc-034 (pre-existing
   PathRouter/ProjectRouter/McpAdapter). I REDUCED the seam footprint in router.rs by extracting
   it to the focused 303-line `router/seam.rs`. Further splitting the pre-existing dispatch code
   is out of my scope (would restructure code I was told to preserve, and the brief said touch
   ONLY router.rs). Flagging for a future cleanup, not a blocker.
2. **`router/tests.rs` is 2059 lines** — the pre-existing cumulative test module. CLAUDE.md
   mandates cumulative test infra (extend, don't fragment), so I appended rather than split.
   Flagging the size; splitting it is a separate refactor decision.
3. **Built-but-unwired warnings handled, not suppressed blindly:** the seam has no production
   caller until Sub-wave 3 wires `SlugRouter` into `main.rs`, so every public item is
   `dead_code` by design (NFR-09 documented-but-degenerate). Added a module-level
   `#![allow(dead_code)]` in seam.rs and `#[allow(unused_imports)]` on the re-export, BOTH with
   doc comments naming Sub-wave 3 as the removal trigger. The next-wave agent should DELETE these
   allows when wiring the listener.
4. **Seam is genuinely exercised (A4/FR-X5), not bypassed:** `SlugRouter::route_mcp` calls
   `resolve_store` on every request before dispatch; the resolver-swap unit test injects a stub
   to prove the trait is the only injection point.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` + `context_search` (category=pattern) -- surfaced
  ADR-003/004/005 (#4950/#4949/#4953), the local path-hash ADR (#80), and the severable
  revert-boundary seam pattern (#4869, adjacent but distinct: that is for no-cross-reference
  rollback gates with defaulted trait methods; this seam is the simpler built-but-unwired case).
- Stored: entry #4957 "Wave-1 built-but-unwired seam in router.rs: extract to a child module +
  targeted allow(dead_code), wiring deferred to the main.rs sub-wave" via /uni-store-pattern.
