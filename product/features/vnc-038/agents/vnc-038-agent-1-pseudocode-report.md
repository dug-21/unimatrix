# Agent Report — vnc-038-agent-1-pseudocode (Stage 3a)

## Deliverables

13 files under `product/features/vnc-038/pseudocode/`:
- `OVERVIEW.md` — component map, data flow, shared types, the Rust↔JS v:2 bundle contract, sequencing, error boundaries, open questions.
- 12 per-component files (one per architecture component / file-surface).

Every interface name traces to existing code (file:line cited inline) or to an ADR (#5080–5088). No invented names.

## Components Covered

1. Bundle codec (Rust) — `client_bundle.rs` → `bundle-codec-rust.md`
2. Bundle decoder (JS) — `bundle.js` → `bundle-decoder-js.md`
3. Client attach (JS) — `init.js` → `client-attach-js.md`
4. Hook transport (JS) — `transport-http.js` → `hook-transport-js.md`
5. Route grammar + resolver — `seam.rs`/`project_resolver.rs` → `route-grammar-resolver.md`
6. Observe route + handler — `router.rs`/`main.rs` → `observe-route.md`
7. Boot wiring — `main.rs` → `boot-wiring.md`
8. register CLI — `projects.rs` → `register-cli.md`
9. Reserved slugs — `config.rs` → `reserved-slugs.md`
10. First-boot token (CI-1) — `token.rs` → `token-redaction.md`
11. Local STDIO/UDS direct-binding guard (C-13) — `main.rs:859`/`:1158` → `local-binding-guard.md`
12. Wave-1 cleanups (CI-2/CI-3) — `router.rs`/`public_url.rs` → `wave1-cleanups.md`

## Key Design Positions Honored

- ADR-001 dumb-client: all 3 client compose sites (init.js:305 slug-append, :307 default-append, transport-http.js:84 /observe-append) designed out; client stores/posts verbatim; closed set empty.
- ADR-002 v:2 bundle {v, mcp_url, observe_url, token, fp}; single `compose_route_urls` owner; atomic Rust+JS+corpus; guard ordering preserved.
- ADR-003 per-slug observe on the funnel; ObserveContext holds `Arc<dyn StoreResolver>` (no boot-bound store); top-level /observe deleted; N=2 isolation.
- ADR-004 delete the default; ProjectKey collapses to Slug-only; MultiProjectRouter drops `default`; DefaultResolver deleted; N=1.
- ADR-006 local kept on its DIRECT path-hash binding; Component 11 is a guard-only (no production change) component; empty-projects loud message gated to cloud/HTTP path so local isn't caught.
- ADR-007 register writes [[projects]] atomically (temp+fsync+rename), idempotent, re-attach-safe.
- ADR-008 token via bundle only.
- ADR-005 reserved set value retained, derivation re-documented.

## Open Questions / Gaps (flagged)

- **OQ-A (Component 3/4):** settings.local.json moves from a single `unimatrix.remote.url` to `{mcp_url, observe_url}`. Pseudocode stores both verbatim and feeds the hook transport `observe_url`. Whether MCP requests are issued by this Node client or only by Claude Code's MCP layer determines if `mcp_url` is also consumed by transport — impl confirms the MCP consumer. Dumb-client holds either way (verbatim, no derivation).
- **OQ-B (Component 10 / CI-1):** The architecture cites a "token print at token.rs:101", but the LIVE code already emits only a token-free notice (`render_first_boot_notice`, token.rs:171/205) and a path-only debug log (token.rs:248). CI-1 is largely already satisfied. Component 10 specifies the ASSERTION + regression guard + a defensive sweep, NOT removal of a leaking print (none exists). This matches the brief's "verify/confirm, don't assume a print exists to remove."
- **OQ-C (`tools` reservation):** Kept reserved per ADR-005; un-reserve is a one-line follow-up. The existing `validate_slug` error message claiming `tools` "would shadow the default-project alias" must be updated (the alias is gone) — noted in `reserved-slugs.md`.
- **Legacy `{remote,token}` init path:** not the #766 surface; kept working with a single local observe derivation on the legacy branch only. If the human wants it retired, that's a one-line deletion — flagged, not assumed.

## Self-Check

- Architecture read before any pseudocode: yes (all 4 source docs + 8 ADRs + cited source files).
- No invented interface names: yes (every name cited to code or ADR).
- Per-component output (OVERVIEW + 12 files), not monolithic: yes.
- Each component file has signatures, error handling, test scenarios: yes.
- No TODO/placeholder/TBD; gaps flagged explicitly: yes.
- Shared types in OVERVIEW match component usage: yes.
- All output within `product/features/vnc-038/pseudocode/`: yes.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search ('http routing slug resolver bundle', category pattern) — surfaced #5079 (boot-bound observe / register-prints-not-writes), #4963 (seam constructor injection), #4974 (ceremonial-seam: prove the funnel at N=2). context_get on ADR-001..008 (#5080,5081,5082,5083,5084,5087,5086,5088) — all applied.
- Deviations from established patterns: none. Pseudocode applies existing patterns (single-funnel seam #4963, ceremonial-funnel N=2 proof #4974, parity-corpus mechanics #4956, atomic temp+rename). Read-only tier — nothing stored.
