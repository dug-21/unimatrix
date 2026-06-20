# vnc-041 — Implementation Brief

> Config seeding (global (a) + per-slug (b)) plus seam-level WARN for global-locked keys.
> Capability **C17 — Installation provisions the proper config in place** (Unimatrix #5214).
> Feature B of the vnc-040 split. Pairs with C6 (resolution, shipped Feature A / #799). GH **#801**.
>
> **Crate is `unimatrix-server` (the binary crate) — NOT `unimatrix-engine`.** All new sites
> (`main.rs`, `projects.rs`, `infra/config.rs`, `http_provision.rs`) are in the server binary crate;
> `unimatrix-engine/src/project.rs` is a read-only dependency.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-041/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-041/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/vnc-041/specification/SPECIFICATION.md |
| Architecture | product/features/vnc-041/architecture/ARCHITECTURE.md |
| ADR-001 (seed-write primitive) | product/features/vnc-041/architecture/ADR-001-seed-write-primitive.md |
| ADR-002 (per-slug seed in register) | product/features/vnc-041/architecture/ADR-002-per-slug-seed-in-register.md |
| ADR-003 (annotations render from classification) | product/features/vnc-041/architecture/ADR-003-annotations-render-from-classification.md |
| ADR-004 (global seed http.enabled gate) | product/features/vnc-041/architecture/ADR-004-global-seed-http-enabled-gate.md |
| ADR-005 (seam WARN for locked keys) | product/features/vnc-041/architecture/ADR-005-seam-warn-locked-keys.md |
| Risk Strategy | product/features/vnc-041/RISK-TEST-STRATEGY.md |
| Acceptance Map | product/features/vnc-041/ACCEPTANCE-MAP.md |
| Alignment Report | product/features/vnc-041/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| C1 — Seed-write primitive (`write_if_absent`, `write_default_config_if_absent` delegate) | pseudocode/seed-write-primitive.md | test-plan/seed-write-primitive.md |
| C2 — Per-slug seed renderer (`render_per_slug_seed_toml`) | pseudocode/per-slug-seed-renderer.md | test-plan/per-slug-seed-renderer.md |
| C3 — Per-slug seed writer (in `register`, State B + State C) | pseudocode/per-slug-seed-writer.md | test-plan/per-slug-seed-writer.md |
| C4 — Global serve-time seed (in `if config.http.enabled`) | pseudocode/global-serve-seed.md | test-plan/global-serve-seed.md |
| C5 — Locked-key seam WARN (`resolve_slug_config` file-present arm) | pseudocode/seam-warn.md | test-plan/seam-warn.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode and test-plan files are produced in Session 2 Stage 3a. The Component Map lists the
five components the architecture enumerates (ARCHITECTURE §3); actual file paths are filled during
delivery.

## Goal

Per-slug and cloud-global config are **resolvable** today (Feature A / vnc-040) but never
**provisioned** — no code writes the annotated global `config.toml` on a container `serve`, and
`register <slug>` never writes the per-slug `config.toml` the resolver reads. vnc-041 seeds both files
(skip-if-exists, never clobbering operator-authored config), renders the per-slug seed's per-key
annotations from Feature A's canonical classification, and adds a seam-level `tracing::warn` (R-13) so
a hand-edited global-locked key no longer vanishes silently — all while leaving local STDIO /
single-project behavior byte-for-byte unchanged.

## The Three Config Files (canonical disambiguation — read before any code)

This feature touches THREE files; **two are the same physical file**, one is distinct. Conflating them
is the dominant failure mode (SR-05, SR-09).

| Tag | Name | Path | Writer (today) | vnc-041 action |
|-----|------|------|----------------|----------------|
| **(a)** | GLOBAL annotated `config.toml` | `{.unimatrix base}/{path-hash}/config.toml` = `paths.data_dir.join("config.toml")` | `write_default_config_if_absent` (`infra/config.rs:4836`), called ONLY by `handle_version` (`init`/`version`). `serve` only READS it. | **Goal 1 / C4**: `serve` writes it (skip-if-exists), container/HTTP path only. |
| **(c)** | PROJECTS REGISTRY `[[projects]]` stanza | **SAME physical file as (a)** | `ensure_project_stanza` (`projects/config_write.rs:35`) via `register` (vnc-038, atomic RMW) | **NONE.** vnc-041 does NOT write (a)/(c) from `register`. UNCHANGED. |
| **(b)** | PER-SLUG `config.toml` | `{base_dir}/{slug}/config.toml`, `base_dir = paths.data_dir.parent()` — a **SIBLING** of the path-hash dir | Nothing writes it today. Read by `resolve_slug_config` (`http_provision.rs:317`). | **Goal 2/3/4 / C2,C3,C5**: `register` writes it (skip-if-exists); resolver reads + WARNs on it. |

Critical invariant (SR-05): **(a)≡(c) is one file; (b) is a different file.** B's per-slug seed touches
**only (b)**. The global seed (a) is `serve`-time and never inside `register`, so the two writers never
collide.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Seed-write primitive | Shared `create_new(true)` no-clobber `write_if_absent`; `write_default_config_if_absent` delegates to it. NEVER `fs::write`/`File::create`/`atomic_write` rename for a seed. | SR-01, AC-05 | architecture/ADR-001-seed-write-primitive.md |
| Per-slug seed placement | Eager inside `ProjectRegistry::register` at BOTH success branches (State B re-attach + State C genesis); writes ONLY file (b); best-effort (warn-and-continue). | OQ-2, AC-02, SR-05/SR-09 | architecture/ADR-002-per-slug-seed-in-register.md |
| Per-slug seed annotations | RENDER from `PER_SLUG_CONFIG_CLASSIFICATION` at runtime via a classification-derived legend block + reused `DEFAULT_CONFIG_TOML`; no new serializer; no hand-list. | OQ-1, AC-03, SR-02/03/07 | architecture/ADR-003-annotations-render-from-classification.md |
| Global seed gate | `if config.http.enabled` (the real container seam), NOT the `base_dir` argument. Local `else` branch has zero seed call. | OQ-4, AC-01/AC-06, SR-04 | architecture/ADR-004-global-seed-http-enabled-gate.md |
| Seam WARN | Derives locked surface from `is_per_slug_overlayable == false` over keys the per-slug file actually SETS (raw-table inspection); one warn per locked key per boot; WARN-only, no rejection/behavior change. | OQ-3, R-13, AC-04, SR-02/06/07 | architecture/ADR-005-seam-warn-locked-keys.md |

### Spec open questions — resolved by the ADRs (do NOT re-litigate)

The specification carries OQ-A/OQ-B/OQ-C routed to the architect. They are **already answered**; merged
here so delivery proceeds without reopening them:

- **OQ-A — single classification consumption point.** `is_per_slug_overlayable` /
  `PER_SLUG_CONFIG_CLASSIFICATION` are `pub` and in-crate (`infra/config.rs`); both the `register` seed
  site (C2 render) and the `resolve_slug_config` WARN site (C5) live in the same `unimatrix-server`
  crate. No re-export, no restatement of the split. (ADR-003 + ADR-005)
- **OQ-B — field-less locks render in the legend; WARN still fires.** `permissive` (no `UnimatrixConfig`
  field), `tls`/`http` (transport, never read at the seam), and the `*_sha256` descriptors render in the
  legend as "managed globally; value ignored" with **no editable knob** — keyed on the registry's `key`
  string + disposition, never a struct field (so no panic/no bogus knob). Symmetrically, C5's WARN fires
  if a per-slug file SETS any of them, because the classification already lists them `GlobalLocked`. No
  special-casing in B. (ADR-003 + ADR-005)
- **OQ-C — WARN dedup state: once per locked key per boot.** The resolver runs once per slug at boot
  (per-slug loop, `main.rs:1089`), so once-per-resolution IS once-per-boot. Any dedup state must be
  scoped per (slug, key) per boot, must NOT persist across boots, and must NOT suppress one slug's WARN
  by another's. (ADR-005)

## Files to Create / Modify

All paths under the `unimatrix-server` binary crate.

| File | Change | Summary |
|------|--------|---------|
| `infra/config.rs` | modify | Extract `write_if_absent(path, content)` from `write_default_config_if_absent` (delegate); add `render_per_slug_seed_toml() -> String` (C2). |
| `projects.rs` | modify | In `ProjectRegistry::register`, add per-slug seed call at State B + State C success branches (C3), after `ensure_project_stanza`. Reuse `per_slug_data_dir`. Best-effort. |
| `main.rs` | modify | Inside `tokio_main_daemon`'s `if config.http.enabled` block, call `write_default_config_if_absent(&paths.data_dir.join("config.toml"), false)` before the per-slug loop (C4). |
| `http_provision.rs` | modify | Add a WARN pass to `resolve_slug_config`'s file-present arm (C5): raw `toml::Value` parse → for each present key, `is_per_slug_overlayable(key) == false` ⇒ one `tracing::warn`. |

No new files; no signature changes to `register` / `handle_version` / `Command` (additive call sites
only — SR-08 / R-09). `ensure_project_stanza` and the no-file arm of `resolve_slug_config` are UNCHANGED.

## Data Structures (consumed from Feature A — UNCHANGED)

```rust
// infra/config.rs (Feature A, the single source of truth — ADR-004 #5217)
pub const PER_SLUG_CONFIG_CLASSIFICATION: &[ConfigKeyClass]; // :4447
pub struct ConfigKeyClass { pub key: &'static str, pub disposition: OverlayDisposition } // :4428
pub enum OverlayDisposition { PerSlugOverlayable, GlobalLocked } // :4413
```

The render `match` over `OverlayDisposition` MUST be exhaustive (no catch-all): a future variant is an
intended compile break (ADR-003 forcing function, R-06).

## Function Signatures

```rust
// C1 — REUSE existing; extract shared primitive (ADR-001)
pub fn write_default_config_if_absent(path: &std::path::Path, force: bool); // infra/config.rs:4836 — delegates to write_if_absent
// NEW shared helper: parent-create + OpenOptions::create_new(true) + AlreadyExists-is-noop + warn-and-continue
fn write_if_absent(path: &std::path::Path, content: &str);

// C2 — NEW renderer (ADR-003): legend (from registry) + reused DEFAULT_CONFIG_TOML
pub fn render_per_slug_seed_toml() -> String; // infra/config.rs

// C5 — EXTEND (signature UNCHANGED, ADR-005)
pub fn resolve_slug_config<'a>(
    base_dir: &Path, slug: &ProjectSlug, global: &'a UnimatrixConfig,
) -> Result<Cow<'a, UnimatrixConfig>, ServerError>; // http_provision.rs:310

// CONSUME (Feature A)
pub fn is_per_slug_overlayable(key: &str) -> bool; // infra/config.rs:4552

// REUSE for the (b) path — the single per-slug join site (SR-09)
fn per_slug_data_dir(base: &Path, slug: &ProjectSlug) -> PathBuf; // projects.rs:122 — base.join(slug.as_str())

// EXTEND (signature UNCHANGED — additive call sites)
fn register(&self, raw_slug: &str) -> Result<(), ServerError>; // projects.rs:267

// UNCHANGED dependencies
pub fn ensure_data_directory(override_dir: Option<&Path>, base_dir: Option<&Path>) -> io::Result<ProjectPaths>; // engine/project.rs:146 (read-only dep)
fn handle_version(project_dir: Option<PathBuf>, force: bool) -> Result<(), Box<dyn Error>>; // main.rs:1938 (existing (a) writer)
config_write::ensure_project_stanza(config_data_dir, slug); // config_write.rs:35 (UNCHANGED)
```

`DEFAULT_CONFIG_TOML` (`infra/config.rs:4605`) is a hand-curated annotated static string, NOT serialized
from the struct (structs are `Deserialize`-only). vnc-041 must NOT add a struct→TOML serializer.

`PROJECT_CONFIG_NAME = "config.toml"` (`http_provision.rs:272`, module-private).

## Constraints

- **C-01 — A→B one-way contract.** Annotations (C2) and WARN surface (C5) render FROM
  `PER_SLUG_CONFIG_CLASSIFICATION` / `is_per_slug_overlayable` at runtime; B never restates the split.
- **C-02 — Skip-if-exists / atomic no-clobber.** Use `OpenOptions::create_new`; never
  `fs::write`/`File::create`/`atomic_write` rename on a seed write. No `path.exists()` precheck (the
  O_EXCL open IS the guard — no TOCTOU window).
- **C-03 — WARN, not error.** R-13 adds a log line only; no new rejection path, no resolution change.
- **C-04 — B stays off the shared (a)≡(c) file.** B writes file (b) only; the global seed (Goal 1) is
  `serve`-time, never inside `register`.
- **C-05 — Container-only is structural.** Global seed gated by `if config.http.enabled` (NOT `base_dir`).
  The local `else` branch has NO seed call site — AC-06 holds by branch placement.
- **C-06 — Reuse existing machinery.** Existing config-write / `DEFAULT_CONFIG_TOML` template; no new
  serializer, no new merge logic.
- **C-07 — Restart-applies (vnc-038 ADR-007).** Seeding writes a file; overlay applies on restart. No
  hot-reload.
- **C-08 — `register` is the sole per-slug provisioning point.** The eager seed assumes no slug is
  created by any path other than `register`.
- **C-09 — Best-effort seeds.** Seed-write failure logs a `tracing::warn` and the command/daemon
  proceeds; never gates registration's hash-chain-critical steps or daemon boot. No `.unwrap()`.
- **C-10 — Workspace rules.** ≤500 lines/file, no stubs (`todo!()`/`unimplemented!()`/TODO), no
  `.unwrap()` in non-test code, `tracing` for logs (no `println!`/`eprintln!`).
- **C-11 — Content-free WARN logging.** The WARN names key + slug (bounded identifiers) only — NEVER the
  operator's set VALUE (#4749 content-free-logging; principle #8).

## Dependencies

- **Feature A (vnc-040 / #799)** — `PER_SLUG_CONFIG_CLASSIFICATION`, `is_per_slug_overlayable`,
  `resolve_slug_config` (`http_provision.rs`), `merge_configs` (`infra/config.rs`). The A→B contract
  source. Post-merge F1 fix `5e80febf` derives `[knowledge]`-section exhaustiveness from the type.
- **vnc-038** — `register <slug>` flow (`projects.rs`, `projects/config_write.rs`),
  `ensure_project_stanza`, ADR-007 (restart-applies); `ProjectSlug` validated newtype (path-traversal
  rejection inherited).
- **Existing config machinery** — `write_default_config_if_absent` (the no-clobber `create_new`
  primitive to reuse), `DEFAULT_CONFIG_TOML`, `handle_version`, `project::ensure_data_directory`
  (read-only), `load_config_and_build_allowlist` (`main.rs:1806`), `PROJECT_CONFIG_NAME`,
  `per_slug_data_dir`.
- **Crates** — `toml` (raw `toml::Value` parse in C5), `tracing` (WARN level, mirroring the existing
  `*_sha256` global-wins WARN precedent). No new crate dependency.

## NOT in Scope

- **Resolution / overlay merge logic** — shipped in Feature A (#799). B writes files + adds a WARN.
- **New config sections / new tunable knobs** — B seeds the EXISTING `UnimatrixConfig` surface only.
- **Hot-reload** — overlay applies on restart (vnc-038 ADR-007).
- **Overwriting operator-authored config** — skip-if-exists; never clobber a hand-placed file.
- **Rejecting a global-locked override** — R-13 is WARN, not error.
- **Writing the shared (a)/(c) path-hash file from `register`** — the `[[projects]]` stanza write stays
  vnc-038's; B writes only (b).
- **Multi-slug HTTP end-to-end harness** — infra-001 (#800), separate.
- **A global seed on the local single-project `serve` path** — excluded by OQ-4 / AC-06.

## Alignment Status

Vision guardian (ALIGNMENT-REPORT.md, 2026-06-20): **no VARIANCE, no FAIL.** Vision Alignment, Milestone
Fit, Scope Gaps, Scope Additions, Risk Completeness all PASS. Directly advances personal-cloud goal
#4946 and the standing multi-project independent-config goal.

**One WARN (human awareness only — does not block):** SCOPE.md (AC-01, Background §(a)), SCOPE-RISK
(SR-04, Assumptions), and parts of SPEC (FR-02/AC-01/AC-06) still narrate the superseded
`base_dir = Some(/data)` container discriminator. The architecture (§6, ADR-004) corrected this to
`if config.http.enabled` — every live `serve` call passes `base_dir = None`, so a `base_dir`-keyed gate
would never fire. **This brief and the test strategy carry the corrected `http.enabled` gate as ground
truth.** SCOPE/SCOPE-RISK already carry inline reconciliation notes citing ADR-004 #5238. Delivery MUST
gate the global seed on `config.http.enabled`; the WARN closes when SCOPE/SCOPE-RISK fully reflect the
corrected gate.
