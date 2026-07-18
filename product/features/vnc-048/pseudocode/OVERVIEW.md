# vnc-048 Pseudocode Overview — Per-Slug Backup/Restore

Feature: add `--slug <name>` to the operator `export`/`import` CLIs so they target the
runtime's literal-slug store (`{base}/<slug>/unimatrix.db`) through ONE shared funnel,
`resolve_slug_store`. Without `--slug`, both commands are unchanged.

Source of truth: ARCHITECTURE.md, ADR-001..006, SPECIFICATION.md (FR-1..16, AC-01..13,
C-1..10), RISK-TEST-STRATEGY.md (R-01..14). Invent no interface names — all names below
trace to the Integration Surface table in ARCHITECTURE.md.

## Components

| # | Component | File (module) | Pseudocode |
|---|-----------|---------------|-----------|
| 1 | Slug-store resolution funnel | `unimatrix-engine/src/projects.rs` | `resolve_slug_store.md` |
| 2 | Export slug branch + stderr summary | `.../export.rs` | `export.md` |
| 3 | Import slug branch + pre-flight gates + vector redirect | `.../import/mod.rs` | `import.md` |
| 4 | CLI wiring (clap `--slug`) | `main.rs` | `main-dispatch.md` |
| 5 | README canonical restore procedure | `README` | `readme.md` |

`infra/pidfile.rs` is REUSED unchanged (no pseudocode) — `read_pid_file`,
`is_process_alive`, `is_unimatrix_process`.

## Build order (waves)

- **Wave A (foundation):** Component 1 lands first. Both commands depend on the funnel and
  the two `pub(crate)` visibility raises.
- **Wave B (parallel):** Components 2 and 3 branch to the funnel independently.
- **Wave C:** Component 4 (clap wiring) depends on both new signatures; Component 5 (README)
  can proceed any time but import help text (Component 4/3) points at it.

## Shared types & contracts

### `SlugStorePaths` (NEW, `projects.rs`, `pub(crate)`)

```
pub(crate) struct SlugStorePaths {
    pub slug_dir:   PathBuf,   // {base}/<slug>
    pub db_path:    PathBuf,   // slug_dir/"unimatrix.db"   (PROJECT_DB_NAME)
    pub vector_dir: PathBuf,   // slug_dir/"vector"         (PROJECT_VECTOR_DIR)
}
```

### `resolve_slug_store` — the single funnel contract (ADR-001/002)

```
pub(crate) fn resolve_slug_store(
    paths: &ProjectPaths,      // path-hash paths from ensure_data_directory
    raw_slug: &str,            // untrusted operator CLI input
) -> Result<SlugStorePaths, ServerError>
```

Ordered, once, in ONE place. Neither command re-derives base, re-joins, or re-validates:

```
1. slug     = ProjectRegistry::validate_slug(raw_slug)?     # charset + reserved; before any FS/DB (AC-04)
2. base     = paths.data_dir.parent()
                   .map(Path::to_path_buf)
                   .unwrap_or_else(|| paths.data_dir.clone())   # C-1, NO unwrap
3. slug_dir = per_slug_data_dir(base, &slug)                # ONLY join site, &ProjectSlug only (C-2)
4. db_path  = slug_dir.join(PROJECT_DB_NAME)                # "unimatrix.db"
   vector_dir = slug_dir.join(PROJECT_VECTOR_DIR)           # "vector"
5. EXISTENCE GATE: if !db_path.exists() -> Err naming the fully-resolved absolute
                   db_path + next action (C-3, AC-03). Creates nothing.
6. Ok(SlugStorePaths { slug_dir, db_path, vector_dir })
```

The funnel does NOT open the DB, check PID, or check audit — those are caller-side
(import-only) gates layered AFTER it. `SqlxStore::open` is reached by the caller ONLY after
the existence gate returns Ok (C-3, R-02).

### `ProjectPaths` (existing, `project.rs:176-186`)

`{ data_dir, db_path, vector_dir, pid_path: PathBuf }` — all path-hash-scoped. In slug mode:
DB + vector come from `SlugStorePaths`; **`pid_path` stays from `ProjectPaths`** (base-scoped
daemon PID, ADR-003/004). These two path sources must NOT be tidied into one.

### Visibility raises (Wave A, bodies unchanged)

- `per_slug_data_dir` (`projects.rs:123`): `fn` → `pub(crate) fn`. Only join site.
- `ProjectRegistry::validate_slug` (`projects.rs:206`): `fn` → `pub(crate) fn`. Only
  validation edge. Called as `ProjectRegistry::validate_slug(raw)` (associated fn, no `self`).

### `ServerError` conversion

`ServerError: std::error::Error`, so `resolve_slug_store`'s `Err` converts via `?` into
export/import's `Box<dyn std::error::Error>`. Callers use `.map_err(|e| ...)?` only where
they need to add "next action" context not already in the funnel's message.

## Data flow across boundaries

```
main.rs (clap) --slug Option<&str>
   └─> run_export(..., slug) / run_import(..., slug)   [+ _with_base variants]
         └─> ensure_data_directory(project_dir, base_dir) -> ProjectPaths   (C-6: still creates hash dir)
         └─> if let Some(raw) = slug:
               SlugStorePaths = resolve_slug_store(&paths, raw)?      # funnel (Component 1)
               open_target = SlugStorePaths.db_path                   # not paths.db_path
               vector_target (import) = SlugStorePaths.vector_dir
             else:
               open_target = paths.db_path                            # unchanged path-hash flow
               vector_target (import) = paths.vector_dir
         └─> (import only, slug mode) live-PID gate (paths.pid_path) then non-empty-audit gate
         └─> SqlxStore::open(open_target)                             # only after existence gate
```

## Deploy-shape base derivation (C-1 — four shapes, one derivation)

`base = data_dir.parent()` must be the `.unimatrix` base by construction. Same idiom in all:

| Shape | `ensure_data_directory` base input | `data_dir.parent()` | Outcome |
|---|---|---|---|
| In-container (`HOME=/data`) | `None` → `/data/.unimatrix` | `/data/.unimatrix` | correct (the destination) |
| Local dev | `None` → `~/.unimatrix` | `~/.unimatrix` | correct |
| `*_with_base(X)` test hook | `Some(X)` → base verbatim `X` | `X` | correct; AC-09 seeds `X/<slug>` |
| Host bind-mount | `None` → host `$HOME/.unimatrix` | host base | MISS → existence gate fails loud naming the host path (SR-11/C-7) |

The `_with_base` wrinkle is load-bearing for AC-09: `Some(X)` sets base verbatim to `X`, so
`data_dir.parent() == X` and the funnel joins `X/<slug>` — the same path the `http_provision`
literal-slug layout writes. That lets the seam test seed via runtime layout and read via the
CLI resolver.

## Fail-loud inventory (every accept-but-inert path names the resolved absolute path)

| Failure | Component | AC |
|---|---|---|
| Missing store at resolved db_path | 1 (funnel existence gate) | AC-03 |
| Charset-invalid / reserved slug | 1 (validate_slug, before FS/DB) | AC-04 |
| Host base miss (bind-mount) | 1 (surfaces as AC-03 naming host path) | SR-11/C-7 |
| Live daemon PID present (import) | 3 | AC-13 |
| Non-empty `audit_log` target (import) | 3 | AC-10/FR-13 |
| Sparse/empty export (0 entries) | 2 (stderr count summary self-diagnoses) | AC-06 |

## Invariants no component may violate

- One base derivation (Component 1 only). No `--base` flag/env, no second scheme.
- One join site: `per_slug_data_dir`, `&ProjectSlug` only. Traversal closed structurally at
  `ProjectSlug::try_from` — a raw `&str` reaching the join is a defect (R-08).
- One validation edge: `validate_slug`.
- Existence check strictly BEFORE `SqlxStore::open` (C-3).
- Live-PID-only import refusal (ADR-003); `[[projects]]` half dropped.
- Vector rebuild into `slug_dir/vector`; PID stays `paths.pid_path` (ADR-004).
- No `.unwrap()`/`.expect()` in non-test code; `fmt`/`clippy -D warnings` clean; ≤500 lines/file.
