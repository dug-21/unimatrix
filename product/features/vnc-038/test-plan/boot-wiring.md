# Test Plan — Boot Wiring (`main.rs`)

> Component: `crates/unimatrix-server/src/main.rs` (boot swap `:1004`, observe bind `:1045-1052`) · Surface: `tests/project_routing_integration.rs` first-boot fixture + infra-001 harness · Risks: R-10 (High), R-02 (Crit, cross-ref) · AC-01, AC-09

## Scope
Boot builds the unified resolver from `project_slugs` only. The `if project_slugs.is_empty() { DefaultResolver } else { MultiProjectRouter }` swap (main.rs:1004) collapses to a single unified-resolver build; empty `[[projects]]` ⇒ nothing servable + loud "register a project to begin" (no auto-serve, no DefaultResolver). The boot-bound observe `resolve_store(Default)` is deleted (see observe-route.md). **Local STDIO (`:1158`) / UDS (`:859`) boot paths are UNTOUCHED — see local-binding-guard.md.**

## Unit / Integration Test Expectations

### Loud-first-boot, no silent default (R-10 / AC-09)
- `test_empty_projects_nothing_servable` — boot with empty `[[projects]]`; assert NO servable store exists; every MCP and observe request fails loud with the actionable "register a project to begin" substance.
- `test_no_default_resolver_built` — assert the `DefaultResolver`-branch at main.rs:1004 is gone; the only resolver built is the unified `MultiProjectRouter` from `project_slugs` (empty ⇒ servable nothing, not a Default).
- `test_no_adopt_or_path_hash_migration_on_served_model` — assert no adopt/derive/path-hash-migration code path runs on the served-project model (AC-09).

### Write→restart→resolve loop (R-06 cross-ref, AC-02/03/04)
- `test_registered_slug_routable_after_boot_reread` — after `register <slug>` writes `[[projects]]`, a fresh boot's `load_config_and_build_allowlist` includes the slug in `project_slugs` and the resolver maps it. (Full loop; pairs with register-cli.md.)
- `test_n2_both_slugs_routable_after_restart` — register A then B; assert both appear in `project_slugs` and both resolve after restart (AC-04, N=2 boot).

### Observe per-request wiring (R-02 cross-ref)
- `test_observe_handler_built_with_resolver_not_store` — assert the observe handler is constructed with `Arc<dyn StoreResolver>`, NOT a pre-resolved store; the boot-bound `resolve_store(&ProjectKey::Default)` at main.rs:1045 is absent.

## Edge Cases
- Empty `[[projects]]` at boot → nothing servable + loud (cloud-only; local must NOT be caught by this — cross-ref local-binding-guard.md edge).
- `[[projects]]` with N stanzas → all N routable after boot.

## Integration (infra-001)
- Gap #2: `test_no_slug_first_boot_fails_loud` — empty `[[projects]]` boot through the live binary; MCP + observe fail loud, never 200/default (R-10/AC-09).

## Coverage Requirement
Empty config = nothing servable + loud actionable message; no silent default-store landing on any path; the unified resolver is built from `project_slugs` only; the boot→register→restart→resolve loop is proven end-to-end. The cutover does NOT reach the local STDIO/UDS boot paths (cross-checked in local-binding-guard.md).
