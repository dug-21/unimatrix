# vnc-038-docs — README Update Report

**Feature:** vnc-038 | **Issue:** #770 | **PR:** #772 | **Branch:** feature/vnc-038
**Commit:** 7f17868422c8d7621ed36c1b060de944050db574
**Source artifacts:** SCOPE.md + SPECIFICATION.md (both present; no fallback needed)

## Sections modified

1. **Container Deployment → HTTPS serving (personal cloud)** — token never printed to stdout/logs (AC-11/NFR-06); first boot serves nothing until `project register <slug>` + restart; `client-bundle <slug>` now takes a slug.
2. **Container Deployment → "Serving multiple projects" → renamed "Serving projects"** — removed the no-slug `/v1/tools/...` default route (hard cutover, AC-01/AC-09); register writes `[[projects]]` routing intent, restart applies (FR-02/FR-03); 1st and Nth project identical command (FR-04); local STDIO/UDS carve-out (AC-10).
3. **Bundle-driven attach (pinned TLS)** — bundle is now `v:2` per-project carrying server-composed MCP + observe URLs (FR-06); removed the stale `init --remote ... --slug <slug>` block (client appends no slug, FR-08); rotation re-runs `client-bundle <slug>`.
4. **Configuration** — first-boot token-to-stdout claim replaced with bundle-only delivery (NFR-06/AC-11); `UNIMATRIX_PUBLIC_URL` consumers reworded to "server-composed endpoint URLs" (x2).
5. **CLI Reference** — `client-bundle <slug>` requires positional slug, emits `v:2` bundle; `project register <slug>` writes routing intent instead of printing config instructions, restart-to-apply, 1st=Nth.
6. **Architecture Overview → MCP Transport (HTTPS)** — routes are per-slug `/v1/{slug}/tools/...` and `/v1/{slug}/observe`; observe on the same per-request funnel (FR-09); no no-slug/default route.
7. **Architecture Overview → Hook Integration + Hook-Driven Invisible Delivery capability** — remote client POSTs to the server-composed observe URL from the bundle (composes no path), replacing the stale `{url}/observe` shape (FR-08/FR-12).
8. **Architecture Overview → Data Layout note** — cloud serves nothing until a slug is registered (N=1); local path-hash install separate and unaffected.

## Deliberately NOT changed
- **Reserved-slug set** (`v1`/`health`/`observe`/`tools`) left as-is: the final set under the new grammar is an unresolved architect open question (OQ-3 / SR-05); no shipped value to trace to.
- **docs/client-setup.md** left untouched: SCOPE explicitly defers stale client-setup remote docs to #768, a pre-committed fast-follow — non-goal for this diff.
- **Security Model → Project Scoping** left as-is: its claims (slug from transport not payload, cannot escape `/data/.unimatrix/{slug}/`) remain true and are only strengthened by mandatory identity.

## Scope adherence
Only README.md modified and committed. No source code read. No `.claude/`, protocol, or per-feature docs touched. Terminology consistent (Unimatrix, SQLite). No aspirational language. Tool count (14) and skill count (10) unchanged and correct.
