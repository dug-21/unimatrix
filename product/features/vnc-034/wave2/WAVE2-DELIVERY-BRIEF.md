# vnc-034 Wave 2 — Delivery Routing Brief

> **Coordination artifact only.** The technical ground truth is the shared vnc-034 design:
> `../specification/SPECIFICATION.md`, `../architecture/ARCHITECTURE.md`, `../architecture/ADR-001..007`,
> `../RISK-TEST-STRATEGY.md`, `../ACCEPTANCE-MAP.md`, `../IMPLEMENTATION-BRIEF.md`.
> This file routes the **Wave 2** delivery (issue **#727**) and records the human-locked delivery
> decisions of 2026-06-11. It does **not** redesign anything. Where this file and a source document
> conflict on technical substance, the source document wins — EXCEPT the three locked decisions below,
> which the human set at delivery time and which supersede any drift.

## Wave boundary (recap)

Wave 1 (merged, #733 → #725 + #726) built the `StoreResolver` seam on `main`:
`trait StoreResolver`, `SlugRouter` (per-request MCP funnel), `ProjectKey {Default, Slug}`,
`ProjectSlug`, `RouteError`, and `DefaultResolver` (maps `/v1/tools/...` → the single store,
returns `UnknownProject` for any `/v1/{slug}/...`). The slug route grammar already **parses**;
the resolver is inert for slugs. **Wave 2 = one trait-impl swap behind that seam, plus the
registry/config that backs it.** Purely additive — no Wave-1 client re-init (AC-CT-C4).

## LOCKED DELIVERY DECISIONS (human, 2026-06-11) — read before writing any code

### D1 — Slug allowlist regex is `^[a-z0-9][a-z0-9-]{0,62}$` (NOT the issue-body value)

This is the **SR-09 trust boundary** (fix-before-merge). The canonical grammar is **`^[a-z0-9][a-z0-9-]{0,62}$`**
— lowercase ASCII alphanumeric + hyphen, must start alphanumeric, **1–63 chars** (DNS label limit;
slugs feed hostname/SAN-adjacent contexts, C3). Source: ADR-004 §Decision (line 18), FR-C5, IMPLEMENTATION-BRIEF.

> ⚠️ The **issue #727 body** drifted to `^[a-z0-9][a-z0-9_-]{0,63}$` — TWO silent changes:
> (1) underscore added to the charset, (2) max length widened to 64. **Both are wrong.** Do NOT
> implement the issue-body version. There is no ADR-004 amendment authorizing underscore; the 63-char
> bound is deliberate (DNS label). Implement EXACTLY `^[a-z0-9][a-z0-9-]{0,62}$`. The gate and PR review
> MUST assert this exact value — it cannot pass as "matches the spec" against the drifted issue.

Forbidden and rejected at the parse edge in `SlugRouter`, before any filesystem use: `.`, `/`, `\`,
`%`, whitespace, uppercase, and every path separator / encoding thereof (`../`, encoded `/`, `%2e`,
`%2f`, over-length, uppercase, empty). Escape from `/data/.unimatrix/{slug}/` must be *unrepresentable*,
not merely rejected (AC-W2-R6).

### D2 — Config-overlay is SPLIT to a follow-up (NOT in this PR)

Per-project config-overlay (ass-060 discovery #2) is **out of Wave 2**. Config precedence/merge semantics
balloon (dsn-001 territory) and are not load-bearing for the routing+isolation promise Wave 2 exists to
prove. If a config-overlay need surfaces, file a follow-up issue — do not implement here.

### D3 — Per-project health is registry/CLI-side ONLY; NO per-slug network endpoint

Include per-slug store-open status **as a field on the `list` CLI output** (operator-side, in `projects.rs`)
*only if* it falls out cheaply. **Do NOT add any per-slug HTTP/network health surface.** A per-slug
network health/topology surface is the slug-listing surface already rejected in **ADR-004 / OQ-B**
(unauthenticated → leaks project topology to anyone who reaches the port) and would breach **AC-W1-S6**
("no unauthenticated endpoint beyond `GET /health`"). Any over-the-wire per-slug health gets the same
out-of-band/authenticated discipline as slug-listing and is **split**, not drifted in.

## Wave 2 Component Map → output paths (this wave's Stage 3a writes here)

| Component | Target source | Pseudocode (write here) | Test Plan (write here) |
|-----------|---------------|-------------------------|------------------------|
| ProjectRouter (`StoreResolver` impl; slug → per-slug store; per-slug hot caches; drop-in swap at `SlugRouter` call site) | `crates/unimatrix-server/src/http/router.rs` | `wave2/pseudocode/project-router.md` | `wave2/test-plan/project-router.md` |
| ProjectRegistry + lifecycle CLI (`register`/`list`/`delete`; creates `/data/.unimatrix/{slug}/` own DB+vector+hash-chain+analytics; D3 list-status field) | `crates/unimatrix-server/src/projects.rs` *(new)* | `wave2/pseudocode/project-registry-cli.md` | `wave2/test-plan/project-registry-cli.md` |
| `[[projects]]` config + slug validation (D1 regex) | `crates/unimatrix-server/src/config.rs` | `wave2/pseudocode/projects-config.md` | `wave2/test-plan/projects-config.md` |

Cross-cutting: `wave2/pseudocode/OVERVIEW.md`, `wave2/test-plan/OVERVIEW.md`.
Gate reports → `wave2/reports/`; risk coverage → `wave2/testing/`; agent reports → `wave2/agents/`.

**Standard-location note:** vnc-034's `../pseudocode/` and `../test-plan/` dirs already hold **Wave 1**
files. Wave 2 deliberately writes under `wave2/` for clean segmentation — every agent spawn names the
exact `wave2/...` paths. Do not write Wave 2 output into the Wave-1 dirs.

## Acceptance (from ../ACCEPTANCE-MAP.md — Wave 2 + cross-wave)

- **AC-W2-R1** `/v1/{slug}/…` routes to the per-slug store.
- **AC-W2-R2** `[[projects]]`-absent ⇒ `/v1/tools/…` unchanged (single-project backward-compat; OQ-C).
- **AC-W2-R3** Per-slug isolation: no cross-project read or write.
- **AC-W2-R4** Register / list / delete lifecycle works.
- **AC-W2-R5** N clients : 1 slug, attributed by `session_id`; each client stays bound to one slug.
- **AC-W2-R6** Slug allowlist (D1) rejects path-traversal / encoded separators; no filesystem escape. *(SR-09, fix-before-merge)*
- **AC-CT-C4** Store access funnels through `resolve_store`; Wave 2 adds no bypass and re-points no Wave-1 client (additive seam swap).
- **AC-CT-C6** Token authorizes / slug scopes / cert secures — three concerns never collapsed; `BearerValidator` / `TlsConfig` / slug seams intact for enterprise.

## Out of scope (Wave 2)

Config-overlay (D2, split); any per-slug network health/listing surface (D3, split); cross-project
knowledge sharing / owner store (enterprise); OAuth/JWT/RBAC per-slug authz (enterprise; slug is the
seam); local→cloud `migrate` (separate, nxs-012); multi-tenant; one-client-multiplexing-multiple-projects
(permanent OSS boundary, A1).

## Delivery sequencing

Shared source docs (ARCHITECTURE/SPECIFICATION/RISK-TEST-STRATEGY) already cover Wave 2 — **no new
design session**. Run delivery Stages 3a→3c on the existing design. Cycle topic: **`vnc-034-wave-2`**.
PR **closes #727**; umbrella **#733** closes when Wave 2 merges.
