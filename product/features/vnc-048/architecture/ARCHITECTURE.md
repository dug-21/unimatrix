# vnc-048 Architecture — Per-Slug Backup/Restore for Personal-Cloud

Source: `SCOPE.md` (approved; OQ-1..OQ-4 RESOLVED), `SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-11).
GH Issue #953.

## System Overview

Unimatrix ships two store-address resolvers over one `.unimatrix` base (verified line-by-line in SCOPE):

- **Path-hash** — `data_dir = base.join(sha256(project_root)[..16])` (`unimatrix-engine/src/project.rs:163`). Every operator CLI subcommand resolves here, including `export`/`import`.
- **Literal-slug** — `base.join(slug.as_str())` (`http_provision.rs:172-174`, `projects.rs:123-125`). The runtime (per-slug routing, `project register/list/delete`) writes here.

Both dirs are siblings under the same base (pattern #4972). A 16-hex hash segment is a charset-valid slug, so a wrong resolve returns a *real* store, not an error — the two resolvers only disagree when a test seeds through one and reads through the other (lesson #5507). This is why `export`/`import` silently target the near-empty path-hash store in a personal-cloud deployment and report success.

This feature adds `--slug <name>` to `export` and `import`. With `--slug`, the target store is resolved through the **runtime's** literal-slug scheme via a single shared funnel; without `--slug`, both commands are byte-for-byte unchanged. The design's entire discipline is **reuse one scheme, invent no second** (SCOPE Non-Goal: "A second configuration scheme for the same value is the single thing this design most refuses").

## Component Breakdown

| Component | Responsibility | Change |
|---|---|---|
| `projects.rs` (slug resolution) | Own the single slug-store resolution funnel: validate → derive base → join → existence-gate. | New `resolve_slug_store` helper; raise `per_slug_data_dir` + `validate_slug` to `pub(crate)`. |
| `export.rs` | Export a store's corpus to JSONL; stderr count summary. | Add `slug: Option<&str>` to `run_export`/`run_export_with_base`; branch on it to the funnel; add AC-06 summary. |
| `import/mod.rs` | Restore JSONL into a store; rebuild HNSW; pre-flight gates. | Add `slug: Option<&str>`; branch to the funnel; live-PID gate; non-empty-audit gate; redirect vector rebuild to `slug_dir/vector`. |
| `infra/pidfile.rs` | PID liveness primitives (existing). | Reused, unchanged. |
| `main.rs` | CLI dispatch of `Export`/`Import`. | Thread `--slug` clap arg into the `run_*` calls (`main.rs:556-567`). |
| README | Canonical `register → stop → import → start` restore procedure (OQ-3). | New section; help text points to it. |

No shared runtime/HTTP path is modified (C-9). Both commands stay sync pre-tokio subcommands (C-8, procedure #1192 / pattern #4577); import keeps its multi-thread runtime (`block_in_place` in `embed_reconstruct` panics on `current_thread`, GH#554).

## Component Interactions & Data Flow

### Slug-mode resolution (single funnel — the core contract)

```
run_export/run_import (slug = Some(raw))
  └─ paths = ensure_data_directory(project_dir, base_dir)      # path-hash paths; also creates+chmods hash dir (C-6, accepted)
  └─ SlugStorePaths = resolve_slug_store(&paths, raw):
       1. slug   = validate_slug(raw)         # ProjectSlug::try_from + is_reserved_slug — CLI edge, before any FS/DB (AC-04)
       2. base   = paths.data_dir.parent() → unwrap_or_else(|| data_dir.clone())   # C-1, NO unwrap
       3. slug_dir  = per_slug_data_dir(base, &slug)           # C-2, ONLY join site, &ProjectSlug only
       4. db_path   = slug_dir/"unimatrix.db"; vector_dir = slug_dir/"vector"
       5. EXISTENCE GATE: db_path.exists()? else fail loud naming the fully-resolved absolute path (C-3, AC-03)
  └─ (import only) live-PID gate  (ADR-003) using paths.pid_path  — base-scoped daemon PID
  └─ (import only) non-empty-audit gate (ADR-005)
  └─ SqlxStore::open(db_path)     # reached ONLY after the existence gate passes — open never the gate (SR-02)
```

Without `--slug` (`slug = None`) the funnel is not entered; `paths.db_path` (path-hash) flows exactly as today (AC-05, AC-11).

### Import restore-sequence data flow (the product outcome, AC-12)

```
project register <slug>   # creates {base}/<slug>/{unimatrix.db, vector}, writes [[projects]]; prints "Restart to apply"
stop                      # daemon releases every per-slug store; live-PID gate now clears
import --slug <slug> -i dump.jsonl
      # DB restore into {base}/<slug>/unimatrix.db  +  HNSW rebuilt into {base}/<slug>/vector (ADR-004)
start                     # boots, loads the rebuilt index → vector search served (SR-10 proven from start, not disk)
```

## Technology Decisions (ADR index)

| ADR | Title | Risks / ACs |
|---|---|---|
| ADR-001 | The reuse triad: one base derivation, one join site, one validation edge — no second scheme | SR-01, C-1, C-2, C-6; AC-01, AC-04 |
| ADR-002 | Pre-open existence gate; the gate is file-existence, not registration | SR-02, SR-04, C-3; AC-03, AC-05, AC-11 |
| ADR-003 | Live-PID-only import refusal makes the shutdown vector-clobber structurally unreachable | SR-03, C-4, OQ-1; AC-13 |
| ADR-004 | Import rebuilds HNSW into `slug_dir/vector`; PID path stays base-scoped path-hash | SR-10; AC-02, AC-12 |
| ADR-005 | Non-empty-`audit_log` pre-flight refusal; supported target is a freshly-registered slug | SR-05, C-5, OQ-2; AC-10 |
| ADR-006 | Close the silence: fail-loud naming the fully-resolved absolute path + export stderr count summary | SR-08, SR-11, C-7; AC-03, AC-06 |

## Integration Surface

Downstream agents MUST use these exact names/signatures — invent none.

| Integration Point | Type / Signature | Source |
|---|---|---|
| `ensure_data_directory` | `fn(override_dir: Option<&Path>, base_dir: Option<&Path>) -> io::Result<ProjectPaths>` | `unimatrix-engine/src/project.rs:146` |
| `ProjectPaths` fields | `data_dir, db_path, vector_dir, pid_path: PathBuf` (all path-hash-scoped) | `project.rs:176-186` |
| Base derivation idiom (C-1) | `paths.data_dir.parent().map(Path::to_path_buf).unwrap_or_else(\|\| paths.data_dir.clone())` | existing at `projects.rs:181-185`, `main.rs:1287` |
| `per_slug_data_dir` | `fn(base: &Path, slug: &ProjectSlug) -> PathBuf` — **raise to `pub(crate)`**; only join site (C-2) | `projects.rs:123` |
| `ProjectRegistry::validate_slug` | `fn(raw_slug: &str) -> Result<ProjectSlug, ServerError>` — **raise to `pub(crate)`**; only validation edge | `projects.rs:206` |
| `ProjectSlug::try_from` | `TryFrom<&str>`, charset `^[a-z0-9][a-z0-9-]{0,62}$` (1..=63 ASCII bytes) | `http/router/seam.rs:96-118` |
| `is_reserved_slug` | `fn(slug: &ProjectSlug) -> bool` (`v1, health, observe, tools`) | `infra/config.rs:2498` |
| `read_pid_file` | `fn(path: &Path) -> Option<u32>` | `infra/pidfile.rs:93` |
| `is_process_alive` | `fn(pid: u32) -> bool` (`kill -0`) | `infra/pidfile.rs:113` |
| `is_unimatrix_process` | `fn(pid: u32) -> bool` (`/proc/{pid}/cmdline` on Linux) | `infra/pidfile.rs:141` |
| `run_export` / `run_export_with_base` | add `slug: Option<&str>` param | `export.rs:32,46` |
| `run_import` / `run_import_with_base` | add `slug: Option<&str>` param | `import/mod.rs:54,68` |
| `SqlxStore::open` | `async fn(db_path, PoolConfig) -> Result<SqlxStore>` — **auto-creates + migrates (a write)** | `store/src/db.rs:61,82` |
| import vector rebuild target | `reconstruct_embeddings(&paths.vector_dir, ...)` → redirect to `slug_vector_dir` | `import/mod.rs:226`, `embed_reconstruct.rs:110` |
| import PID pre-flight (existing, strengthened) | warning-only `paths.pid_path.exists()` → live-PID hard-error in slug mode | `import/mod.rs:268-273` |
| `drop_all_data` | cannot clear `audit_log` (append-only triggers, schema v25) | `import/mod.rs:286`, ADR-005 vnc-014 (#4359) |

### New interface introduced

```rust
// projects.rs — the single slug-store resolution funnel (pub(crate))
pub(crate) struct SlugStorePaths {
    pub slug_dir: PathBuf,
    pub db_path: PathBuf,     // slug_dir/"unimatrix.db"
    pub vector_dir: PathBuf,  // slug_dir/"vector"
}

/// Resolve a per-slug store under the runtime's literal-slug scheme.
/// Validates (charset + reserved), derives base = data_dir.parent() (no unwrap),
/// joins via per_slug_data_dir(&ProjectSlug), and asserts db_path EXISTS —
/// creating nothing. Errors name the fully-resolved absolute path.
pub(crate) fn resolve_slug_store(
    paths: &ProjectPaths,
    raw_slug: &str,
) -> Result<SlugStorePaths, ServerError>;
```

`ServerError: std::error::Error`, so it converts into export/import's `Box<dyn Error>`.

## Deploy-Shape Coverage Axis (C-1 — four shapes, not one representative)

Base = `data_dir.parent()` must be the `.unimatrix` base by construction in each; each shape must either resolve correctly or fail loud with the resolved path (SR-11, Assumptions).

| Shape | `ensure_data_directory` base input | `data_dir.parent()` resolves to | Outcome |
|---|---|---|---|
| In-container (`HOME=/data`, Dockerfile:132) | `None` → `/data/.unimatrix` | `/data/.unimatrix` | Correct — the personal-cloud destination |
| Local dev | `None` → `~/.unimatrix` | `~/.unimatrix` | Correct (single-project shape) |
| `*_with_base` test hook | `Some(X)` → base **verbatim** `X` (NOT `X/.unimatrix`) — #5507 wrinkle | `X` | Correct; AC-09 seam seeds slug store at `X/<slug>` |
| Host bind-mount (outside container) | `None` → host `$HOME/.unimatrix` ≠ container base | host base | **Miss → fail loud naming the resolved absolute path** (SR-11/C-7, ADR-006) |

The `_with_base` wrinkle is load-bearing for AC-09: because `ensure_data_directory(_, Some(X))` sets `unimatrix_base = X` verbatim, `data_dir.parent() == X` and `resolve_slug_store` joins `X/<slug>` — the *same* path the `http_provision` literal-slug layout writes. The seam test therefore seeds `X/<slug>/unimatrix.db` via the runtime layout, seeds `X/<hash>/unimatrix.db` differently, and proves `export --slug foo` emits the slug's rows and **none** of the hash store's (AC-09/SR-09). An N=1 same-path test is ceremonial (#4974) and does not satisfy AC-09.

## Fail-Loud Requirements (every accept-but-inert path, SR-02/05/11)

Each names the fully-resolved absolute path and the next action; none is a silent no-op:

- Missing store at resolved path → AC-03 (both commands, before `open`).
- Charset-invalid / reserved slug → AC-04 (CLI edge, before any FS/DB).
- Host base miss (bind-mount) → SR-11/C-7 (surfaces as AC-03 missing-store naming the *host* path, which distinguishes a base miss from a typo).
- Live daemon PID present (import) → AC-13 (names PID path + `stop → import → start`).
- Non-empty `audit_log` target (import) → AC-10 ("register a fresh slug", never a raw SQLite UNIQUE error).
- Export summary "exported 0 entries" self-diagnoses a sparse export (AC-06/SR-08), including the correctly-retained `--skip-quarantined`/`audit_log` asymmetry (Non-Goal).

## Key Design Decisions Summary

1. One shared funnel (`resolve_slug_store`) holds all three reuse invariants and the existence gate structurally — no command re-derives base, re-joins, or re-validates (ADR-001/002).
2. Import's live-PID hard-error makes the shutdown vector-clobber *unreachable*, not discipline-avoided (ADR-003/SR-03).
3. Vector rebuild redirects to `slug_dir/vector`; PID stays base-scoped (it is the one daemon's PID) (ADR-004).
4. Supported restore target is a freshly-registered (audit-empty) slug; non-empty audit fails pre-flight (ADR-005).
5. Silence is closed by fail-loud-with-resolved-path + an export count summary, never by a heuristic sibling-dir scan (ADR-006).

## Open Questions

- None blocking. OQ-1..OQ-4 are RESOLVED in SCOPE and honored above. OQ-5 (`#5586`/`#5691` `delivery:proven → partial` retag on AC-09/AC-10 evidence) is explicitly a vision-session call, not this feature's — flagged for the human, not filed here.
- Non-Goal boundary for the vision session (not architecture): whether slug-awareness for the other six CLIs (`verify`, `snapshot`, `eval`, `health`, `stop`, `client-bundle`) becomes one tracked item — human call (SR-06). This feature establishes the `--slug` + `resolve_slug_store` pattern they will copy.
