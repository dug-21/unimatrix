# Test Plan — Local STDIO/UDS Direct-Binding Guard

> Components: `crates/unimatrix-server/src/main.rs` STDIO (`:1158`) + UDS (`:859`) boot paths · Surface: existing local UDS/STDIO fixtures + structure/grep guard · Risks: R-13 (Crit — the load-bearing GATE-2 guard) · AC-10 (C-13)

## Scope
Local STDIO/UDS keeps its DIRECT path-hash store binding and is NOT a resolver key (ADR-006 tightening). It opens `~/.unimatrix/{hash}/unimatrix.db` directly at boot and threads `Arc<Store>` straight to its handlers — never routed through the unified resolver. This is the concrete GATE-2 confirmation guard: it FAILS the instant delivery routes local through the resolver or makes local a resolver key.

## Unit / Structure Test Expectations

### Direct-binding assertion (R-13 sc.1 — the load-bearing guard)
- `test_local_stdio_opens_path_hash_store_directly` — assert local STDIO (`main.rs:1158`) opens `~/.unimatrix/{hash}/unimatrix.db` directly at boot and threads the `Arc<Store>` straight to its handler, with NO slug supplied — behavior unchanged from ADR-004.
- `test_local_uds_opens_path_hash_store_directly` — same for local UDS (`main.rs:859`).

### Resolver-bypass assertion (R-13 sc.2 — structure/grep guard)
- `test_local_boot_never_invokes_parse_project_key` — grep/structure guard: assert the local STDIO/UDS boot paths NEVER call `parse_project_key`, NEVER construct the HTTP resolver (`DefaultResolver`/`MultiProjectRouter`), NEVER reference `ProjectKey::Default`, and NEVER touch a bundle. A guard that FAILS if a future edit threads local through the resolver or adds a local resolver-map key.

### No-resolver-key assertion (R-13 sc.3 — ADR-006 tightening)
- `test_local_not_a_resolver_key` — assert local is NOT self-registered as a resolver key; the unified resolver's key space is `ProjectKey::Slug` only — there is no derived path-hash key in the slug map.

### HTTP-only-deletion cross-check (R-13 sc.4, with R-07)
- `test_default_deletions_confined_to_http` — assert the ADR-004 deletions (`DefaultResolver`, `/v1/tools→Default`, `_ => Default`) are confined to HTTP code and do NOT reach the local STDIO/UDS boot paths.

## Edge Cases
- Local STDIO/UDS boot with NO `[[projects]]` and NO slug → resolves its path-hash store directly, NOT a loud-first-boot failure. The loud-first-boot rule (AC-09) is CLOUD-ONLY; local must NOT be caught by the empty-config failure (cross-ref boot-wiring.md).
- `token.rs:101` shared between cloud first-boot and the local path → redaction gated by deployment context, not unconditionally removed (cross-ref token-redaction.md R-14 ∩ R-13).

## Integration
- Existing local-UDS/STDIO fixture: assert the path-hash store still resolves directly over UDS/STDIO with no slug supplied and NOT through the unified resolver; assert delivery did NOT add a resolver-key path for local. (Local is NOT an infra-001 MCP/HTTP-harness surface — no new harness test; structure/grep guard + existing fixture only.)

## Coverage Requirement
Local is provably the pre-existing direct-binding path, untouched by the resolver — the "local unaffected" guarantee is structural (no migration, no operator action), proven by a guard that FAILS the instant local is routed through the resolver or made a resolver key (ADR-006 tightening / C-13). This is the load-bearing GATE-2 guard.
