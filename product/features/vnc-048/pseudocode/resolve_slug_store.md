# Component 1 — `resolve_slug_store` funnel (`unimatrix-engine/src/projects.rs`)

## Purpose

The single shared slug-store resolution funnel. Holds all three reuse invariants (one base
derivation, one join site, one validation edge) and the pre-open existence gate structurally,
so neither `export` nor `import` re-derives, re-joins, or re-validates (ADR-001/002).
Foundation component (Wave A) — export and import depend on it.

## Visibility raises (bodies unchanged)

Two existing items in `projects.rs` are raised so the funnel and the two command modules
(same crate) can call them. No behavior change.

```
# projects.rs:123 — the ONLY join site (C-2)
- fn per_slug_data_dir(base: &Path, slug: &ProjectSlug) -> PathBuf
+ pub(crate) fn per_slug_data_dir(base: &Path, slug: &ProjectSlug) -> PathBuf
  # body unchanged: base.join(slug.as_str())
  # doc contract retained: "NEVER call with a raw &str; &ProjectSlug is the proof of validation"

# projects.rs:206 — the ONLY validation edge (AC-04); associated fn on impl ProjectRegistry
- fn validate_slug(raw_slug: &str) -> Result<ProjectSlug, ServerError>
+ pub(crate) fn validate_slug(raw_slug: &str) -> Result<ProjectSlug, ServerError>
  # body unchanged: ProjectSlug::try_from(charset) THEN is_reserved_slug (two separate checks)
```

Constants reused (already in `projects.rs`): `PROJECT_DB_NAME` (= "unimatrix.db"),
`PROJECT_VECTOR_DIR` (= "vector"). Do NOT hardcode the literals — use the constants.

> NOTE for implementer: confirm `PROJECT_VECTOR_DIR` is in scope in `projects.rs`. It is
> defined for the per-slug layout (`http_provision`) as "vector"; if not already imported/
> defined in `projects.rs`, add a `const PROJECT_VECTOR_DIR: &str = "vector"` matching the
> `http_provision` value (byte-identical), OR reuse the existing exported constant. Flagged
> as OPEN QUESTION 1 — the implementer must reuse the canonical constant, never a second
> literal (this is itself a "second scheme" trap in miniature).

## New type

```
pub(crate) struct SlugStorePaths {
    pub slug_dir:   PathBuf,   // {base}/<slug>
    pub db_path:    PathBuf,   // slug_dir/PROJECT_DB_NAME
    pub vector_dir: PathBuf,   // slug_dir/PROJECT_VECTOR_DIR
}
```

Derive `Debug` (tests assert on paths). No `Clone` needed unless a caller requires it.

## New function

```
pub(crate) fn resolve_slug_store(
    paths: &ProjectPaths,
    raw_slug: &str,
) -> Result<SlugStorePaths, ServerError>
```

### Body (ordered — the core contract)

```
FUNCTION resolve_slug_store(paths, raw_slug):

    # ── Step 1: validation edge — before ANY filesystem or DB access (AC-04, C-2) ──
    slug = ProjectRegistry::validate_slug(raw_slug)?
        # ProjectSlug::try_from enforces ^[a-z0-9][a-z0-9-]{0,62}$, ASCII, 1..=63 bytes.
        # is_reserved_slug rejects v1/health/observe/tools.
        # On Err, propagate ServerError::Config unchanged — its message already names the
        # rejected raw slug + the charset rule. No FS touched (R-08, R-14).

    # ── Step 2: base derivation — the ONE derivation, NO unwrap (C-1) ──
    base = paths.data_dir.parent()
              .map(Path::to_path_buf)
              .unwrap_or_else(|| paths.data_dir.clone())
        # Exact idiom from projects.rs:181-185 / main.rs:1287.
        # parent() is the .unimatrix base by construction in all four deploy shapes
        # (ensure_data_directory builds data_dir = base.join(hash)).
        # The None branch (data_dir at fs root) falls back to data_dir itself; the
        # subsequent existence gate then fails loud on the resulting path — never unwrap.

    # ── Step 3: join site — the ONE join, &ProjectSlug only (C-2) ──
    slug_dir = per_slug_data_dir(&base, &slug)      # base.join(slug.as_str())

    # ── Step 4: derive store paths ──
    db_path    = slug_dir.join(PROJECT_DB_NAME)     # {base}/<slug>/unimatrix.db
    vector_dir = slug_dir.join(PROJECT_VECTOR_DIR)  # {base}/<slug>/vector

    # ── Step 5: EXISTENCE GATE — strictly before any SqlxStore::open (C-3, AC-03) ──
    IF NOT db_path.exists():
        RETURN Err(ServerError::Config(format!(
            "no store found for slug '{slug}' at {abs}: expected the runtime's per-slug \
             store. Run `project register {slug}` and restart, or check --project-dir.",
            slug = slug,
            abs  = db_path.display()      # fully-resolved absolute path (SR-11/C-7)
        )))
        # exists() is a pure read: no store, no dir, no WAL, no output file created (R-02, R-14).
        # This is also where the host-bind-mount base miss surfaces — the printed (host) path
        # is what distinguishes a base miss from a typo (ADR-006).

    # ── Step 6: success ──
    RETURN Ok(SlugStorePaths { slug_dir, db_path, vector_dir })
```

## State machine

None. Pure resolution + one read-only existence probe. No lifecycle state.

## Data flow

- **Input:** `&ProjectPaths` (path-hash paths; `.data_dir` is the derivation source,
  `.pid_path` is NOT consulted here — the import PID gate reads it separately), `raw_slug: &str`.
- **Output:** `SlugStorePaths` (db + vector targets for the caller) or `ServerError`.
- **Transformations:** `&str` → `ProjectSlug` (validated newtype) → `PathBuf` join. The `&str`
  → `ProjectSlug` transition is the traversal-closure boundary; a raw `&str` must never reach
  `per_slug_data_dir`.

## Error handling

All errors are `ServerError` (converts to `Box<dyn Error>` upstream). Two error sources:

| Condition | Error | Names resolved path? | AC |
|---|---|---|---|
| Charset-invalid / reserved slug | `ServerError::Config` from `validate_slug` | N/A (pre-FS; names raw slug) | AC-04 |
| `db_path` does not exist | `ServerError::Config` (Step 5) | YES — absolute `db_path` | AC-03 |

No `SqlxStore::open` call, no `unwrap`/`expect`, no panic path. The funnel never mutates the
filesystem.

## Key test scenarios (hints for tester)

- **Resolve-correct (AC-01/R-05 S1):** with `_with_base(X)` so `data_dir.parent() == X`, and
  a real `X/<slug>/unimatrix.db` present, assert returned `db_path == X/<slug>/unimatrix.db`
  and `vector_dir == X/<slug>/vector`.
- **Missing store (AC-03/R-02):** slug dir absent (or `vector/` present but `unimatrix.db`
  absent) → `Err` whose message contains the absolute `db_path`; assert filesystem unchanged
  (no dir/db/WAL created).
- **Validation reject (AC-04/R-08):** parameterized `Foo!`, `a_b`, 64+ chars, leading `-`,
  uppercase, `v1`, `health`, `observe`, `tools`, and traversal (`../x`, `%2e%2e`, `/abs`,
  `a/b`, embedded NUL) → `Err` with zero filesystem side effects; a `&ProjectSlug` never
  formed for these.
- **`None` parent fallback (NFR-4):** a `ProjectPaths` whose `data_dir.parent()` is `None`
  resolves via the fallback without `unwrap` and fails loud on the existence gate.
- **No second join site:** grep-style structural check that `per_slug_data_dir` has exactly
  one caller in slug mode (this funnel).
