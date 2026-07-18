## ADR-001: The reuse triad — one base derivation, one join site, one validation edge; no second scheme

### Context
`export`/`import` must resolve a per-slug store under the runtime's literal-slug scheme (`{base}/<slug>/unimatrix.db`), while today they resolve only the path-hash scheme (`{base}/<hash>/unimatrix.db`). Both live under one `.unimatrix` base (pattern #4972). The root risk (SR-01, lesson #5507) is that a second address-derivation scheme for the same value would silently disagree with the runtime's — and a hash dir name is itself a charset-valid slug, so a wrong resolve returns a *real* store, not an error. SCOPE names "a second configuration scheme for the same value" as the single thing this design most refuses. There is no `--base` flag and no env var (Non-Goal).

### Decision
Slug resolution reuses exactly three existing seams, wrapped in one shared funnel `resolve_slug_store(paths, raw_slug) -> SlugStorePaths` (in `projects.rs`, `pub(crate)`). Both commands call it; neither re-implements any part.

1. **One base derivation (C-1).** `base = paths.data_dir.parent().map(Path::to_path_buf).unwrap_or_else(|| paths.data_dir.clone())` — the exact idiom already at `projects.rs:181-185` and `main.rs:1287`. **No `unwrap`/`expect`.** `ensure_data_directory` builds `data_dir = unimatrix_base.join(hash)`, so `parent()` is the `.unimatrix` base by construction in all four deploy shapes.
2. **One join site (C-2).** `per_slug_data_dir(base, &slug)` (`projects.rs:123`, raised to `pub(crate)`). Its doc contract: the `&ProjectSlug` type *is* the proof of validation — "NEVER call this with a raw `&str`." A second join site breaks that contract. Traversal is closed **structurally** at the type edge, not at runtime.
3. **One validation edge (AC-04).** `ProjectRegistry::validate_slug(raw) -> Result<ProjectSlug, ServerError>` (`projects.rs:206`, raised to `pub(crate)`) — `ProjectSlug::try_from` (charset) + `is_reserved_slug` (reserved) as two separate checks, body unchanged. This is the smaller diff over lifting to a free function while keeping the single-edge property. It rejects charset-invalid (`Foo!`, `a_b`, 64+ chars) and reserved (`v1`, `health`, `observe`, `tools`) slugs at the CLI edge, before any filesystem or DB access.

`ensure_data_directory` still creates and chmods the path-hash `data_dir` + `vector/` before its `db_path` is discarded in slug mode (C-6). Accepted — `ProjectRegistry::resolve` already does exactly this; it is the price of one base derivation and must not be "optimized" away by deriving the base another way.

### Consequences
Easier: the CLI resolver and the runtime provably agree because they share `per_slug_data_dir` with a `&ProjectSlug` — the AC-09 seam test can make them disagree only by seeding a *different* store, never by a derivation mismatch. Path traversal (`..`, `%2e`, `/`, absolute paths) is unrepresentable because `ProjectSlug::try_from` forbids the bytes (C-2), so the guard is structural. Blast radius is bounded to `projects.rs` (two visibility raises + one helper) plus the two command entry points.

Harder: two `pub(crate)` visibility raises cross the `projects.rs` module boundary — acceptable, same crate. The path-hash `data_dir` is created and immediately discarded in slug mode (C-6) — a wasted `mkdir`/`chmod`, accepted as the cost of a single base derivation. Agents must resist adding a `--base` flag or a sibling-dir scan; both re-introduce a second scheme this ADR forbids.
