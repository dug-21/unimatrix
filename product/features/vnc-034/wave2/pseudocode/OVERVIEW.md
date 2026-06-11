# vnc-034 Wave 2 — Pseudocode Overview

> Multi-project routing (#727). Populates the Wave-1 `StoreResolver` seam (merged
> on `main`) with `ProjectRouter`, adds the `[[projects]]` config + slug validation,
> and the `register`/`list`/`delete` lifecycle CLI. Purely additive — no Wave-1
> client re-init (AC-CT-C4). Source of truth: `../specification/SPECIFICATION.md`
> (FR-C1..C7, FR-X1..X5), `../architecture/ARCHITECTURE.md`, ADR-003/004/005, and
> the LOCKED delivery decisions in `../wave2/WAVE2-DELIVERY-BRIEF.md` (D1/D2/D3 + the
> Stage-3a refinements D4/D5/D6 and the seam-funnel honesty record).

## Locked decisions encoded here (the gate will check these)

- **D1 — slug grammar is EXACTLY `^[a-z0-9][a-z0-9-]{0,62}$`** (lowercase alnum +
  hyphen, must start alnum, **1–63 chars, NO underscore, NO 64-char bound**).
  This regex is **already implemented and merged** in Wave 1
  (`http/router/seam.rs` `ProjectSlug::TryFrom`, lines 71–104). Wave 2 **REUSES it
  unchanged** — it does NOT re-implement, widen, or add a second validator. The
  drifted issue-#727 value `^[a-z0-9][a-z0-9_-]{0,63}$` is NOT implemented anywhere.
  Validation lives at the parse edge (`SlugRouter`/`ProjectSlug::try_from`) BEFORE any
  filesystem use; escape from `/data/.unimatrix/{slug}/` is *unrepresentable* (AC-W2-R6).
- **D2 — NO config-overlay.** Not designed. Out of Wave 2.
- **D3 — per-project health is a CLI `list` field only** (operator-side, in
  `projects.rs`), included only if cheap; **NO per-slug HTTP/network health surface**
  (would reopen the ADR-004/OQ-B slug-listing rejection / breach AC-W1-S6).
- **D4 — `delete` = DE-REGISTER only** (preserve the on-disk data dir + hash chain);
  **`--purge` destroys, loudly** (re-type the slug via `--confirm <slug>`); **re-register
  RE-ATTACHES** to the preserved store/chain (de-register→register is a RESTORE, never a
  new chain over old data). Lives in `project-registry-cli.md`.
- **D5 — reserved-slug refusal at `register`** (`v1`/`health`/`observe`/`tools`), a
  SEPARATE check from the D1 charset allowlist (a charset-valid slug like `tools` is still
  rejected — it would shadow the `/v1/tools/...` default alias, ADR-005). Single shared
  `RESERVED_SLUGS` list in `projects-config.md`, reused by the register CLI.
- **D6 — `register` idempotence is TWO-STATE:** already-registered-AND-routing → loud
  error; data-dir-exists-but-de-registered → re-attach (D4), NOT an error. Never collapsed.
- **Funnel (seam record) — the Wave-1 discard path is ELIMINATED:** `adapter_for` is the
  SOLE dispatch route; no residual fixed-adapter fallback. See `project-router.md`.
- Rust: no `unsafe`, no `.unwrap()` in non-test code, max 500 lines/file, no new crates.

## Components and their files

| Component | Source file (actual path) | Pseudocode |
|-----------|---------------------------|------------|
| **ProjectRouter** — Wave-2 `StoreResolver` impl: `ProjectKey::Slug(s)` → per-slug `Arc<Store>` + per-slug `McpAdapter`; drop-in swap at the `SlugRouter` call site | `crates/unimatrix-server/src/http/router.rs` (extend existing `ProjectRouter`) | `project-router.md` |
| **ProjectRegistry + lifecycle CLI** — `register`/`list`/`delete`; creates `/data/.unimatrix/{slug}/` (own DB+vector+hash-chain+analytics); pre-tokio sync subcommand (C-10) | `crates/unimatrix-server/src/projects.rs` *(new)* | `project-registry-cli.md` |
| **`[[projects]]` config + slug validation** | `crates/unimatrix-server/src/infra/config.rs` (NOTE: actual path is `infra/config.rs`, the brief's `config.rs` is shorthand) | `projects-config.md` |

Sequencing constraint: **projects-config** types (`ProjectEntry`, `ProjectsConfig`,
`ProjectSlug` reuse) are the shared vocabulary — build first. **ProjectRouter** depends
on those types + the per-slug `UnimatrixServer`/`McpAdapter` builder. **ProjectRegistry
CLI** depends on the per-slug data-dir layout (shared with ProjectRouter) and on
`ProjectSlug` validation. ProjectRouter and the CLI both consume the same
`per_slug_data_dir(base, &ProjectSlug)` helper (single source of the path layout).

## Wave-1 seam being extended (already on `main` — do NOT re-derive)

The Wave-1 seam (`http/router/seam.rs`, `http/router/default_resolver.rs`) is merged
and inert for slugs. Wave 2 swaps the injected resolver at **one** call site:

```
main.rs L898 (Wave 1, today):
  let resolver: Arc<dyn StoreResolver> =
      Arc::new(DefaultResolver::new(Arc::clone(&store)));

main.rs (Wave 2, after this change):
  let resolver: Arc<dyn StoreResolver> =
      Arc::new(ProjectRouter::from_registry(default_store, slug_entries)?);
```

`PathRouter::new(resolver, project_router, observe_ctx)` and `SlugRouter::new(...)` are
**untouched**. The route grammar (`parse_project_key`), `ProjectKey`, `ProjectSlug`,
and `RouteError` are **untouched** (ADR-003: Wave 2 is one trait-impl swap, no
interface re-cut).

### The dispatch-threading correction + funnel elimination (load-bearing — read carefully)

Today `SlugRouter::route_mcp` resolves the store into `let _store` (**discarded**) and
dispatches through the **HTTP** `ProjectRouter<ReqBody>` which holds a SINGLE fixed
`default_server: McpAdapter` (`router.rs:342`). With only `ProjectKey::Default` that is
harmless — but it means **AC-W1-X1 proved the seam's SHAPE, not the funnel**; the resolved
handle was ceremonial (seam-funnel honesty record, BRIEF). **Wave 2 is where the
single-funnel invariant becomes genuinely load-bearing.** For per-slug isolation
(AC-W2-R3) the request must dispatch through the per-slug `McpAdapter` — and the discard
path must be **eliminated entirely**, not supplemented: there must be NO residual
fixed-adapter fallback that can still bypass per-slug dispatch. `adapter_for(&key)` becomes
the SOLE dispatch route (Default keys included).

ADR-003 is explicit: *"per-slug hot-path routing lives INSIDE the seam method, not in a
new edge."* Two name collisions must be kept distinct in the design:

- **HTTP `ProjectRouter<ReqBody>`** (`router.rs:336`, existing) — the tower MCP
  dispatcher PathRouter/SlugRouter call today. Holds `McpAdapter`(s). NOT a `StoreResolver`.
  After Wave 2 it is **no longer the seam's dispatcher** (the seam dispatches via
  `adapter_for`); it survives only for any non-seam HTTP wiring, or is dropped (OQ-PR-9).
- **resolver `ProjectRouter`** (the Wave-2 `StoreResolver` impl, ARCHITECTURE §7 /
  IMPLEMENTATION-BRIEF data structures) — `{ default: Option<ProjectEntry>, slugs:
  HashMap<ProjectSlug, ProjectEntry> }`.

`project-router.md` resolves this collision by making the **single Wave-2 type** own
BOTH responsibilities behind the seam (see that file, "Type-collision resolution"): it
implements `StoreResolver` (so `resolve_store` is the funnel that proves identity and
returns the `Arc<Store>`) AND exposes `adapter_for(&key)` returning the matching per-key
adapter. The trait's `adapter_for` has **no default impl** — every resolver (including the
Wave-1 `DefaultResolver`, minimally extended to hold its adapter) must answer through the
same map its `resolve_store` reads, so resolution and dispatch can never diverge. The
`SlugRouter` call site and grammar are unchanged; the `let _store` discard and the
`self.project_router.route_mcp` fallback are **removed**; per-slug selection lives inside
the seam, exactly as ADR-003 requires.

## Shared types (defined once, referenced by all three component files)

```rust
// REUSED UNCHANGED from Wave 1 (http/router/seam.rs) — NOT redefined in Wave 2:
//   enum ProjectKey { Default, Slug(ProjectSlug) }
//   struct ProjectSlug(String)            // TryFrom<&str> = ^[a-z0-9][a-z0-9-]{0,62}$
//   trait StoreResolver { fn resolve_store(&self, &ProjectKey) -> Result<Arc<Store>, RouteError>; }
//   enum RouteError { UnknownProject, InvalidSlug(String) }

// NEW (projects-config.md) — the [[projects]] config stanza + the reserved-slug list:
struct ProjectsConfig { projects: Vec<ProjectConfigEntry> }      // [[projects]] in TOML
struct ProjectConfigEntry { slug: String }                       // validated to ProjectSlug at load
const RESERVED_SLUGS: [&str; 4] = ["v1", "health", "observe", "tools"];  // D5 — single source;
                                  // checked SEPARATELY from the D1 charset allowlist. `tools`
                                  // shadows the /v1/tools/... default alias (ADR-005). Reused
                                  // by config validation AND the register CLI (no 2nd list).

// NEW (project-router.md) — per-slug runtime entry the resolver/router holds:
struct ProjectEntry {
    store: Arc<Store>,        // the per-slug store handle (sole write capability, FR-X3)
    adapter: McpAdapter,      // the per-slug MCP dispatcher (own UnimatrixServer subsystem)
}

// NEW (project-registry-cli.md) — operator-facing lifecycle, sync/pre-tokio:
struct ProjectRegistry { base_dir: PathBuf }                     // /data/.unimatrix
struct ProjectStatus { slug: ProjectSlug, store_open: Option<bool> }  // D3 list field (cheap-only)

// SHARED path helper — the ONLY place the per-slug layout is spelled (used by
// ProjectRouter construction AND the register/delete CLI):
fn per_slug_data_dir(base: &Path, slug: &ProjectSlug) -> PathBuf
    // = base.join(slug.as_str())  — slug already allowlist-validated, so join is safe;
    //   escape is unrepresentable (AC-W2-R6). NEVER join a raw &str.
```

## Per-slug isolation invariant (AC-W2-R3, the reason Wave 2 exists)

A request for slug A can never read/write slug B's store. Enforced structurally:
1. Identity is transport-derived (`parse_project_key` → `ProjectKey::Slug`), never
   from payload (FR-X2). Unchanged Wave-1 grammar.
2. `resolve_store(Slug(a))` returns ONLY `slugs[a].store` or `UnknownProject` — never a
   fallback to default, never another slug (mirrors `DefaultResolver`'s no-fallthrough).
3. Each slug's `McpAdapter` wraps a `UnimatrixServer` built over `slugs[a].store` and
   that slug's own vector index / hash chain / analytics dir — so even tool dispatch has
   no handle to B. Dispatch is selected SOLELY via `adapter_for(&key)` from the same map
   `resolve_store` reads (no fixed-adapter fallback), so a request can never be served by
   a different slug's adapter. The resolved `Arc<Store>` is the sole write capability (FR-X3).
4. `per_slug_data_dir` is the single path-join site; the allowlist (D1) makes `..`/
   encoded separators unrepresentable, so A's dir cannot resolve into B's (AC-W2-R6).

## Data flow (per request, Wave 2)

```
POST /v1/{slug}/tools/...   (after StaticTokenAuth, PathRouter splits /health//observe)
   │
SlugRouter::route_mcp        (Wave-1 layer; discard path REMOVED in Wave 2)
   │  parse_project_key(path) -> ProjectKey::Slug(ProjectSlug)   [D1 grammar, parse edge]
   │  resolver.resolve_store(&key)  ── the single funnel (FR-X1) ──┐  store is USED, not discarded
   │     ProjectRouter::resolve_store: slugs.get(&slug)            │ Err(UnknownProject)
   │        -> Ok(entry.store)  |  None -> Err(UnknownProject)     │   -> 404 JSON, no panic
   │  resolver.adapter_for(&key) -> entry.adapter  ── SOLE dispatch route ─┘  (no fixed-adapter fallback)
   │     (None only when key is unknown -> 404; NEVER a default-adapter bypass)
   ▼
per-slug McpAdapter -> per-slug UnimatrixServer tool dispatch over slug A's store ONLY
```

Backward-compat (AC-W2-R2 / FR-C6): `[[projects]]` absent ⇒ `slugs` empty, `default`
present ⇒ `/v1/tools/...` resolves `ProjectKey::Default` to the one store AND dispatches
through `adapter_for(&Default)` → the default adapter — byte-identical to Wave 1 (the
difference is structural: one dispatch path instead of resolve-then-discard-then-fixed).
Any `/v1/{slug}/...` with no registered slug ⇒ `UnknownProject` (404). So a server with
zero `[[projects]]` is behaviorally indistinguishable from Wave 1 (no Wave-1 client
re-init, AC-CT-C4) — even though the discard path is gone.

## Open questions / gaps flagged

See each component file's "Open questions" section. The single cross-cutting design
choice that the synthesizer/human should confirm is recorded in `project-router.md`
("Type-collision resolution") — whether the per-slug `McpAdapter` map lives on the
resolver `ProjectaRouter` (recommended, keeps one funnel) vs. on the HTTP
`ProjectRouter<ReqBody>` with the resolver feeding it. The pseudocode commits to the
former (ADR-003 "inside the seam") and flags it explicitly.
