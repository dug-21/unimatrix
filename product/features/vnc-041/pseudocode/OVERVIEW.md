# vnc-041 Pseudocode — OVERVIEW

> Config seeding (global (a) + per-slug (b)) + seam-level WARN for global-locked keys.
> Capability **C17** (#5214). Feature B of the vnc-040 split. GH **#801**.
> **Crate: `unimatrix-server` (binary crate)** — NOT `unimatrix-engine`. `unimatrix-engine/src/project.rs`
> is a read-only dependency. Sources: ARCHITECTURE.md, ADR-001..005, SPECIFICATION.md, RISK-TEST-STRATEGY.md.

## Components

| # | File | Crate file (modify) | Responsibility |
|---|------|---------------------|----------------|
| C1 | seed-write-primitive.md | `infra/config.rs` | Extract `write_if_absent(path, content)`; `write_default_config_if_absent` delegates to it. The shared `create_new(true)` no-clobber write. |
| C2 | per-slug-seed-renderer.md | `infra/config.rs` | NEW `render_per_slug_seed_toml() -> String`: classification-derived legend + reused `DEFAULT_CONFIG_TOML`. |
| C3 | per-slug-seed-writer.md | `projects.rs` | In `register`, seed file (b) via C1 with C2 body, at State B + State C, after `ensure_project_stanza`. Best-effort. |
| C4 | global-serve-seed.md | `main.rs` | Inside `if config.http.enabled`, seed file (a) via `write_default_config_if_absent`. Container-only by branch placement. |
| C5 | seam-warn.md | `http_provision.rs` | In `resolve_slug_config` file-present arm, WARN once per locked key per boot for keys the file SETS that are `is_per_slug_overlayable == false`. |

## The three config files (canonical disambiguation — read before any code)

This feature touches THREE files; **two are the same physical file**, one is distinct.
Conflating them is the dominant failure mode (SR-05, SR-09, R-05).

| Tag | Path | Writer today | vnc-041 action |
|-----|------|--------------|----------------|
| **(a)** GLOBAL annotated `config.toml` | `paths.data_dir.join("config.toml")` = `{.unimatrix base}/{path-hash}/config.toml` | `write_default_config_if_absent`, called ONLY by `handle_version` (`init`/`version`). `serve` only READS it. | **C4**: container `serve` writes it (skip-if-exists). |
| **(c)** PROJECTS REGISTRY `[[projects]]` stanza | **SAME physical file as (a)** | `ensure_project_stanza` (`config_write.rs`) via `register`, atomic RMW. | **NONE.** UNCHANGED — vnc-041 never writes (a)/(c) from `register`. |
| **(b)** PER-SLUG `config.toml` | `per_slug_data_dir(base_dir, slug).join("config.toml")` = `{base_dir}/{slug}/config.toml`, `base_dir = paths.data_dir.parent()` — SIBLING of the path-hash dir | nothing writes it today; read by `resolve_slug_config`. | **C2/C3/C5**: `register` writes it (skip-if-exists); resolver reads + WARNs on it. |

**Critical invariant (SR-05):** (a)≡(c) is ONE file; (b) is a DIFFERENT file. C3's per-slug seed touches
**only (b)**. The global seed (a) is `serve`-time (C4), never inside `register`, so the two writers
(`ensure_project_stanza` on (a)/(c), the per-slug seed on (b)) target different paths from different
commands — they cannot collide.

## Shared types (CONSUMED from Feature A — UNCHANGED, `infra/config.rs`)

```
const PER_SLUG_CONFIG_CLASSIFICATION: &[ConfigKeyClass]          // :4447
struct ConfigKeyClass { key: &'static str, disposition: OverlayDisposition }  // :4428
enum  OverlayDisposition { PerSlugOverlayable, GlobalLocked }    // :4413
fn    is_per_slug_overlayable(key: &str) -> bool                 // :4552
static DEFAULT_CONFIG_TOML: &str                                 // :4605 (annotated template, NOT serialized)
```

The `match` over `OverlayDisposition` in C2 MUST be exhaustive — no catch-all. A future variant is an
intended compile break (ADR-003 forcing function, R-06).

## New / extended surface (this feature)

```
// C1 — infra/config.rs
fn write_if_absent(path: &Path, content: &str)                   // NEW shared primitive (module-private)
fn write_default_config_if_absent(path: &Path, force: bool)      // EXISTING — force=false delegates to write_if_absent

// C2 — infra/config.rs
pub fn render_per_slug_seed_toml() -> String                     // NEW

// C5 — http_provision.rs (signature UNCHANGED)
pub fn resolve_slug_config<'a>(base_dir: &Path, slug: &ProjectSlug, global: &'a UnimatrixConfig)
    -> Result<Cow<'a, UnimatrixConfig>, ServerError>             // EXTEND: add WARN pass in file-present arm

// REUSE
fn per_slug_data_dir(base: &Path, slug: &ProjectSlug) -> PathBuf // projects.rs:122 — the SINGLE per-slug join site
const PROJECT_CONFIG_NAME: &str = "config.toml"                  // http_provision.rs:272
```

## Data flow

```
serve (tokio_main_daemon, main.rs)
  ensure_data_directory(None, None) -> paths.data_dir
  load_config_and_build_allowlist(&paths.data_dir) -> config
  if config.http.enabled {                                    ◄── C4 structural container gate
     write_default_config_if_absent(&paths.data_dir.join("config.toml"), false)   // (a) via C1
     for slug in projects:
        resolve_slug_config(base_dir, slug, &global)           ◄── C5 WARN pass (file-present arm)
  } else { /* local STDIO/UDS: NO seed call site (AC-06) */ }

register <slug> (ProjectRegistry::register, projects.rs)
  State A: loud error, no write
  State B (re-attach): Store::open -> ensure_project_stanza (a≡c) -> seed (b)   ◄── C3
  State C (genesis):   create dir -> Store::open -> ensure_project_stanza (a≡c) -> seed (b)   ◄── C3
     where seed (b):
        path = per_slug_data_dir(&self.base_dir, &slug).join("config.toml")     // SAME join the resolver uses
        body = render_per_slug_seed_toml()                                       // C2, DERIVES from classification
        write_if_absent(&path, &body)                                           // C1, skip-if-exists, best-effort
```

Cross-component contract: C2 and C5 both bind to the same `infra/config.rs` registry at runtime
(NFR-03 single consumption point). C3 calls C2 (render) + C1 (write). C4 reuses C1's caller
(`write_default_config_if_absent`) with the existing `DEFAULT_CONFIG_TOML`.

## Sequencing constraints (build order)

1. **C1** first — both seed writers depend on the no-clobber primitive; the extraction must keep the
   four existing `write_default_config_if_absent` tests green (R-09).
2. **C2** depends only on the Feature A registry — buildable in parallel with C1, but C3 needs both.
3. **C3** depends on C1 (write) + C2 (body) + `per_slug_data_dir` (reuse).
4. **C4** depends on C1 (via `write_default_config_if_absent`) only.
5. **C5** independent of C1–C4 (consumes only the Feature A registry + `toml` raw parse).

## Cross-cutting constraints (apply to every file)

- **Best-effort seeds (C3, C4):** a seed-write failure logs `tracing::warn` and the command/daemon
  proceeds. No `.unwrap()`, never gate registration's hash-chain steps or daemon boot (C-09, R-10).
- **No-clobber atomic (C1):** `OpenOptions::create_new(true)` only; `AlreadyExists` is a silent no-op;
  NO `path.exists()` precheck (the O_EXCL open IS the guard — no TOCTOU window) (C-02, R-03).
- **A→B one-way (C2, C5):** render FROM `PER_SLUG_CONFIG_CLASSIFICATION` / `is_per_slug_overlayable` at
  runtime; never restate the split (C-01, R-04/R-06).
- **WARN-only (C5):** log line only; resolution output, return type, error paths UNCHANGED (C-03, R-07).
- **Content-free WARN (C5):** name key + slug only — NEVER the operator's set VALUE (C-11, #4749).
- **Workspace rules:** ≤500 lines/file, no stubs/TODO, no `.unwrap()` in non-test, `tracing` only (C-10).
```
