# vnc-038 Architect Report — vnc-038-agent-1-architect

## Deliverables

- `product/features/vnc-038/architecture/ARCHITECTURE.md`
- ADR-001 .. ADR-007 (one file each, see below)

## ADR files + Unimatrix entries

| ADR file | Title | Unimatrix |
|----------|-------|-----------|
| architecture/ADR-001-dumb-client-invariant.md | The Dumb-Client Invariant — server sole authority on route shape (SR-01) | #5080 |
| architecture/ADR-002-v2-bundle-server-composed-urls.md | v:2 bundle carries server-composed MCP+observe URLs — atomic dual-side (SR-02, RD-2) | #5081 |
| architecture/ADR-003-per-slug-observe-funnel.md | Per-slug observe on the per-request funnel — sole route, no boot fallback (SR-03, RD-3) | #5082 |
| architecture/ADR-004-delete-default-unified-resolver.md | Delete the default — unified resolver, single = N=1 (SR-04, RD-5) | #5083 |
| architecture/ADR-005-reserved-slug-rederivation.md | Reserved-slug re-derivation under the new grammar (SR-05) | #5084 |
| architecture/ADR-006-local-uds-identity-under-unified-resolver.md | Local UDS path-hash identity under the unified resolver (AC-10 vs RD-5) | #5085 |
| architecture/ADR-007-register-writes-routing-intent.md | register writes [[projects]]; restart applies — atomic, re-attach-safe (SR-07, RD-4) | #5086 |

## Supersession (deprecated in Unimatrix)

- #4954 (vnc-034 ADR-001, v:1 slug-free bundle) → superseded by #5081 (vnc-038 ADR-002).
- #4949 (vnc-034 ADR-005, /v1/tools default alias) → superseded by #5083 (vnc-038 ADR-004).

## Key decisions

1. Spine = dumb-client invariant: server sole route authority; client posts bundle URLs verbatim. All 3 client compose sites (init.js:305 slug, init.js:307 default, transport-http.js:84 observe) deleted; byte-for-byte invariant test.
2. v:2 bundle = {v,mcp_url,observe_url,token,fp} — atomic Rust+JS+corpus, hard cut from v:1, reuse #4956 corpus infra.
3. Observe = /v1/{slug}/observe through the SAME resolve_store funnel; boot-bound resolve_store(Default) deleted; N=2 isolation proof (the #4974 guard).
4. Default deleted: DefaultResolver, /v1/tools->Default, _=>Default, MultiProjectRouter's default arm all removed; empty [[projects]] => nothing servable, loud message.
5. Local UDS reconciled (ADR-006): path-hash is a self-registered resolver key at boot, NOT a Default arm — RD-5 and AC-10 both hold.
6. register writes [[projects]] atomically (temp+fsync+rename, idempotent, State B re-attach never genesis); restart applies via existing boot read.
7. Reserved-slug set value retained, derivation re-documented (observe = sub-route segment; tools = conservative keep).

## Open questions (for human / downstream)

1. Local-UDS path-hash key representation — spec-level realization detail (ADR-006).
2. tools un-reservation — kept reserved conservatively; confirm if it should become registerable now (ADR-005).
3. #735 sequencing (SR-06) — same router/boot surface; leader must sequence after #735 or pin a shared branch point.
4. "Zero existing served users" (RD-1) — validate before the AC-09 hard cut.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced vnc-034 ADR set (#4949/#4950/#4951/#4954), #4963 seam wiring, #4974 ceremonial-funnel trap, #4956 codec corpus, #5079 boot-read/observe-binding facts; all applied.
- Stored: entries #5080–#5086 "ADR-001..ADR-007 vnc-038" via context_store (category decision, tags [adr, vnc-038]). Deprecated #4954 and #4949 with supersession reasons pointing to #5081 / #5083. No typed edges asserted — none met the high traversal-necessity bar at authoring; intra-feature spine + Supports links deferred to retro per convention.
