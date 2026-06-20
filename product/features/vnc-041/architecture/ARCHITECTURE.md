# vnc-041 Architecture — Config Seeding + Seam-Level WARN (C17 / vnc-040 Feature B)

> Companion to `product/features/vnc-041/SCOPE.md` and `SCOPE-RISK-ASSESSMENT.md`.
> Feature B of the vnc-040 split. Feature A (#799) shipped per-slug config *resolution*
> (`resolve_slug_config`, the `PER_SLUG_CONFIG_CLASSIFICATION` registry). This feature
> ships the *provisioning* half — files that seed the config in place — plus the R-13
> seam-level WARN that makes a hand-edited global-locked key visible.

## 1. System Overview

vnc-041 adds three behaviors, all on the **`unimatrix-server` binary crate**, layered
strictly *on top of* Feature A's resolver. It introduces **no new merge, no new
validation, no new serializer, and no new config knob** (SCOPE Non-Goals).

```
                       unimatrix (binary crate)
  ┌─────────────────────────────────────────────────────────────────────┐
  │ serve (tokio_main_daemon)            register <slug>                   │
  │   │                                    │ (run_project_command)         │
  │   ▼                                    ▼                               │
  │ if config.http.enabled  ◄── Goal 1   ProjectRegistry::register         │
  │   (container/HTTP path)   STRUCTURAL   │  State B re-attach / State C   │
  │   │                       GATE         │  genesis                       │
  │   │ global seed (file a)               │ ◄── Goal 2 per-slug seed (b)   │
  │   ▼                                    ▼                               │
  │ per-slug loop                        ensure_project_stanza (file a≡c)  │
  │   └─ resolve_slug_config ◄── Goal 4   (vnc-038, UNCHANGED)             │
  │        WARN locked keys                                                 │
  └─────────────────────────────────────────────────────────────────────┘
                  │ DERIVES annotations + WARN surface FROM ▼
        infra/config.rs : PER_SLUG_CONFIG_CLASSIFICATION / is_per_slug_overlayable
                          (Feature A, the single source of truth — ADR-004 #5217)
```

The one-way **A → B contract** is the spine: Feature A *owns*
`PER_SLUG_CONFIG_CLASSIFICATION`; Feature B *consumes* it at runtime for both the
per-slug seed annotations (Goal 3) and the WARN surface (Goal 4). B restates nothing.

## 2. The three config files (settled — read before any code)

This is the single most-conflated area. Confirmed against live code:

| File | Path | Writer (today) | vnc-041 action |
|------|------|----------------|----------------|
| **(a) GLOBAL annotated** `config.toml` | `{base}/.unimatrix/{path-hash}/config.toml` = `paths.data_dir.join("config.toml")` | `write_default_config_if_absent` — called **only** by `handle_version` (`init`/`version`), main.rs:1958. `serve` only READS it. | **Goal 1**: `serve` writes it (skip-if-exists), container path only. |
| **(c) PROJECTS REGISTRY** `[[projects]]` stanza | **SAME physical file as (a)** — `config_data_dir.join("config.toml")`, `config_data_dir == paths.data_dir` | `ensure_project_stanza` (`config_write.rs:35`), called by `register` (vnc-038 ADR-007). Atomic RMW, preserves all sections. | **None.** vnc-041 does NOT write (a)/(c) from `register`. |
| **(b) PER-SLUG** `config.toml` | `{base_dir}/{slug}/config.toml`, `base_dir = paths.data_dir.parent()` — a **SIBLING** of the path-hash dir | **Nothing writes it today.** Read by `resolve_slug_config` (`http_provision.rs:317`). | **Goal 2**: `register` writes it (skip-if-exists). **Goals 3/4** act on its content + resolution. |

The critical invariant (SR-05): **(a)≡(c) is one file; (b) is a different file.** B's
per-slug seed touches **only (b)**. The global seed (a) is `serve`-time and never runs
inside `register`, so the two writers (`ensure_project_stanza` on (a)/(c), and the
per-slug seed on (b)) never collide — they target different paths from different commands.

## 3. Component Breakdown

| # | Component | New/changed | Crate / file | Responsibility |
|---|-----------|-------------|--------------|----------------|
| C1 | **Seed-write primitive** | reuse (existing) | `infra/config.rs` `write_default_config_if_absent` | The no-clobber `create_new(true)` write. Already exists; (a) reuses verbatim. See ADR-001. |
| C2 | **Per-slug seed renderer** | new | `infra/config.rs` (new fn `render_per_slug_seed_toml`) | Render the per-slug seed body: classification-derived annotation legend + reused `DEFAULT_CONFIG_TOML` knob template. ADR-003. |
| C3 | **Per-slug seed writer** | new | `projects.rs` (in `register`, State B + State C) | After store open/genesis, write (b) via the C1 no-clobber primitive using the C2-rendered body. ADR-002. |
| C4 | **Global serve-time seed** | new | `main.rs` `tokio_main_daemon`, inside `if config.http.enabled` | Write (a) via C1 (reusing `DEFAULT_CONFIG_TOML`) when absent. Container-only by the `http.enabled` structural gate. ADR-004. |
| C5 | **Locked-key seam WARN** | new | `http_provision.rs` `resolve_slug_config` | When the per-slug file *sets* a key whose `is_per_slug_overlayable` is false, emit one `tracing::warn` per locked key per boot, naming key + slug. ADR-005. |

## 4. Component Interactions / Data Flow

### Goal 1 — global seed (serve)
```
tokio_main_daemon
  → ensure_data_directory(None, None) → paths.data_dir   (HOME=/data in container)
  → load_config_and_build_allowlist(&paths.data_dir)
  → if config.http.enabled {                              ◄── STRUCTURAL container gate
        write_default_config_if_absent(&paths.data_dir.join("config.toml"), force=false)
        ... existing per-slug loop ...
    }  else { /* local UDS/STDIO: NO seed write — AC-06 sentinel */ }
```

### Goal 2/3 — per-slug seed (register)
```
ProjectRegistry::register(slug)
  State C: create dir → Store::open (genesis) → ensure_project_stanza(a≡c)
                                              → write_per_slug_seed(b)   ◄── NEW
  State B: Store::open (re-attach)            → ensure_project_stanza(a≡c)
                                              → write_per_slug_seed(b)   ◄── NEW
  where write_per_slug_seed(b):
     path  = per_slug_data_dir(base_dir, slug).join("config.toml")   (SAME join the resolver uses)
     body  = render_per_slug_seed_toml()                              (DERIVES from classification)
     C1 no-clobber create_new write(path, body)                      (skip-if-exists)
```

### Goal 4 — seam WARN (resolve_slug_config, file-present arm only)
```
resolve_slug_config(base_dir, slug, &global)
  file present:
     raw  = toml::from_str::<toml::Value>(text)     ◄── parse RAW table to see PRESENT keys
     for each present (section.key) in raw:
        if !is_per_slug_overlayable(key) { tracing::warn!(key, slug, "ignored — managed globally") }
     ... existing load_single_config → validate → merge → validate (UNCHANGED) ...
```

## 5. Integration Points

- **Feature A registry** (`PER_SLUG_CONFIG_CLASSIFICATION`, `is_per_slug_overlayable`) —
  consumed by C2 (render) and C5 (WARN). Already `pub` and in-crate (config.rs). No
  visibility change needed: both `register` and `resolve_slug_config` live in the same
  `unimatrix-server` crate as the registry.
- **vnc-038 `register`** (`projects.rs`, `config_write.rs`) — C3 hooks two new call sites
  inside `register` (State B, State C), after `ensure_project_stanza`. `ensure_project_stanza`
  itself is UNCHANGED.
- **vnc-040 `resolve_slug_config`** (`http_provision.rs`) — C5 adds a WARN pass in the
  file-present arm; the load/merge/validate flow is UNCHANGED.
- **`ensure_data_directory`** (`unimatrix-engine/src/project.rs`) — read-only dependency;
  supplies `paths.data_dir` (and via `.parent()`, `base_dir`). UNCHANGED.

## 6. Container-vs-local discriminator — CORRECTION to the risk assessment

SR-04 (and SCOPE AC-01) assume container-vs-local is decided by `ensure_data_directory`'s
`base_dir = Some(/data)` argument. **The live `serve` path does not work that way.** Every
`ensure_data_directory` call in `main.rs` passes `base_dir = None` (lines 599, 1347, 1779,
529, 546); container-vs-local is decided by `dirs::home_dir()` (HOME=`/data` in the
container — see `health.rs:21`). So the global seed cannot key off the `base_dir` argument
at the serve call site — it is always `None` there.

**The correct structural gate is `if config.http.enabled`** (main.rs:1011). HTTP is the
container/multi-project transport; local STDIO/UDS runs in the `else` branch. The
`require_http_for_projects` invariant (main.rs:670) already ties `[[projects]]` to
`http.enabled`. Placing the global seed *inside* the `config.http.enabled` block makes
"container only" a compile-time branch fact, not a runtime flag — and the AC-06 sentinel
asserts the local (`http.enabled == false`) path writes zero new files. See ADR-004.
This is the structural confinement SR-04 requires, expressed against the real seam.

## 7. Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| `write_default_config_if_absent` | `pub fn write_default_config_if_absent(path: &std::path::Path, force: bool)` — `create_new(true)` no-clobber; warn-and-continue, never errors | `infra/config.rs:4836` (REUSE — C1, ADR-001) |
| `DEFAULT_CONFIG_TOML` | `pub static DEFAULT_CONFIG_TOML: &str` — annotated knob template (a static string, NOT serialized from the struct) | `infra/config.rs:4605` (REUSE in C2/C4) |
| `PER_SLUG_CONFIG_CLASSIFICATION` | `pub const PER_SLUG_CONFIG_CLASSIFICATION: &[ConfigKeyClass]` | `infra/config.rs:4447` (CONSUME — C2, C5) |
| `ConfigKeyClass` | `pub struct ConfigKeyClass { pub key: &'static str, pub disposition: OverlayDisposition }` | `infra/config.rs:4428` |
| `OverlayDisposition` | `pub enum OverlayDisposition { PerSlugOverlayable, GlobalLocked }` | `infra/config.rs:4413` |
| `is_per_slug_overlayable` | `pub fn is_per_slug_overlayable(key: &str) -> bool` | `infra/config.rs:4552` (CONSUME — C5; iterate the registry directly for C2/render) |
| `render_per_slug_seed_toml` | **NEW** `pub fn render_per_slug_seed_toml() -> String` (or `&'static`-backed `String`) — body = classification legend + `DEFAULT_CONFIG_TOML` | `infra/config.rs` (NEW — C2, ADR-003) |
| `resolve_slug_config` | `pub fn resolve_slug_config<'a>(base_dir: &Path, slug: &ProjectSlug, global: &'a UnimatrixConfig) -> Result<Cow<'a, UnimatrixConfig>, ServerError>` | `http_provision.rs:310` (EXTEND — C5 adds WARN pass in file-present arm; signature UNCHANGED) |
| `PROJECT_CONFIG_NAME` | `const PROJECT_CONFIG_NAME: &str = "config.toml"` (module-private) | `http_provision.rs:272` |
| `per_slug_data_dir` | `fn per_slug_data_dir(base: &Path, slug: &ProjectSlug) -> PathBuf` — `base.join(slug.as_str())` — the SINGLE per-slug join site | `projects.rs:122` (REUSE for C3 path — SR-09) |
| `ProjectRegistry::register` | `fn register(&self, raw_slug: &str) -> Result<(), ServerError>` — State A error / B re-attach / C genesis; `self.base_dir`, `self.config_data_dir` in scope | `projects.rs:267` (EXTEND — C3) |
| `ensure_project_stanza` | `config_write::ensure_project_stanza(config_data_dir, slug)` — writes (a)/(c), atomic RMW | `projects.rs:354` / `config_write.rs:35` (UNCHANGED) |
| `ensure_data_directory` | `pub fn ensure_data_directory(override_dir: Option<&Path>, base_dir: Option<&Path>) -> io::Result<ProjectPaths>` | `unimatrix-engine/src/project.rs:146` (READ-ONLY dep) |
| `config.http.enabled` | resolved `bool` (env override already applied in `load_config`) | `main.rs:1011` (STRUCTURAL gate for C4) |
| `handle_version` | `fn handle_version(project_dir: Option<PathBuf>, force: bool) -> Result<(), Box<dyn Error>>` | `main.rs:1938` (UNCHANGED — existing (a) writer for init/version) |

### Error boundaries
- **Seed writes (C3, C4)**: best-effort, MUST NOT fail the command. `write_default_config_if_absent`
  already logs-and-continues. C3's per-slug variant follows the same posture — a seed-write
  failure logs a `tracing::warn` and `register`/`serve` proceed (the resolver tolerates an
  absent (b) via its no-file arm). Provisioning is convenience; it never gates the daemon or
  the registration's hash-chain-critical steps.
- **WARN (C5)**: pure observation. The raw-table parse for WARN detection must not introduce
  a new error path — if the raw parse fails, the existing `load_single_config` in the same
  arm will already surface a loud, slug-named `ServerError::Config`. The WARN pass either
  reuses the parsed value or degrades silently (no WARN) on a parse it cannot inspect; it
  NEVER turns a parseable file into an error (SR-06: WARN-only, no rejection).

## 8. Cross-cutting decisions (ADR index)

| ADR | Title | Drives | Risks addressed |
|-----|-------|--------|-----------------|
| ADR-001 | The seed-write primitive is the existing `create_new` no-clobber path; never `File::create`/`fs::write`/`atomic_write` | AC-05 | SR-01, SR-05 |
| ADR-002 | Per-slug seed is eager inside `register` (State B + C), writes ONLY file (b) via the shared no-clobber primitive | AC-02, AC-05 | SR-05, SR-09 |
| ADR-003 | Per-slug seed annotations RENDER from `PER_SLUG_CONFIG_CLASSIFICATION` at runtime — a derived legend, not a hand-list; reuse `DEFAULT_CONFIG_TOML`, no new serializer | AC-03 | SR-02, SR-03, SR-07 |
| ADR-004 | Global serve-time seed is gated structurally by `if config.http.enabled` (the real container seam), not the `base_dir` arg; local path writes zero files | AC-01, AC-06 | SR-04 |
| ADR-005 | Seam WARN derives its locked surface from `is_per_slug_overlayable`==false over the keys the per-slug file actually SETS (raw-table inspection); one warn per locked key per boot, WARN-only | AC-04 | SR-02, SR-06, SR-07 |

## 9. Open Questions

- **OQ-A (spec/dev)**: Render granularity for ADR-003. The classification keys are
  *dotted section paths* (`knowledge.categories`, `inference.w_sim`, `tls`, `permissive`).
  `DEFAULT_CONFIG_TOML` is organized by `[section]` with commented field lines. The simplest
  AC-03-satisfying shape is a **header legend block** rendered from the registry (one
  commented line per classified key tagged `editable here` / `managed globally — value
  ignored`) PREPENDED to the reused `DEFAULT_CONFIG_TOML` body. Whether AC-03's "global-locked
  keys INCLUDED but commented-out + marked 'managed globally'" is satisfied by the legend, or
  demands per-key inline tags woven into the body template, is a spec-writer call. Architect
  recommendation: legend block (keeps the proven static template intact, no new serializer,
  and a classification flip provably changes the rendered legend — the AC-03 test target).
- **OQ-B (dev)**: `permissive` and `tls`/`http` have no editable per-slug field (they are
  daemon-process / transport, not in the per-slug overlay surface). ADR-003 must render them
  in the legend as "managed globally" but MUST NOT emit an editable knob for them. ADR-005's
  WARN, symmetrically, fires if a per-slug file *sets* `permissive`/`tls`/`http`/`*_sha256`/
  `rayon_pool_size`. The classification already lists exactly these as `GlobalLocked`, so both
  follow the registry — no special-casing in B.
- **OQ-C (tester)**: The AC-03 "proven, not restated" test (flip a classification entry,
  assert the rendered seed annotation flips) and the SR-08 ripple audit (the four
  `write_default_config_if_absent` tests at config.rs:11262–11346, the `Command::Version`
  match in main_tests.rs:33, and any `register` call-site tests) are the regression spine.
  C3/C4 add call sites, not signature changes, so the `matches!`/`Command` arms should not
  break — confirm during 3a test planning.
