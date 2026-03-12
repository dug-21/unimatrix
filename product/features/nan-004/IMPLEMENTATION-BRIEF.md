# nan-004: Versioning & Packaging — Implementation Brief

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/nan-004/SCOPE.md |
| Scope Risk Assessment | product/features/nan-004/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/nan-004/architecture/ARCHITECTURE.md |
| Specification | product/features/nan-004/specification/SPECIFICATION.md |
| Risk & Test Strategy | product/features/nan-004/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/nan-004/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| C1: npm Package Structure | pseudocode/npm-package-structure.md | test-plan/npm-package-structure.md |
| C2: JS Shim | pseudocode/js-shim.md | test-plan/js-shim.md |
| C3: Binary Resolution | pseudocode/binary-resolution.md | test-plan/binary-resolution.md |
| C4: Init Command | pseudocode/init-command.md | test-plan/init-command.md |
| C5: Settings Merge | pseudocode/settings-merge.md | test-plan/settings-merge.md |
| C6: Postinstall | pseudocode/postinstall.md | test-plan/postinstall.md |
| C7: Binary Rename | pseudocode/binary-rename.md | test-plan/binary-rename.md |
| C8: Model Download Subcommand | pseudocode/model-download.md | test-plan/model-download.md |
| C9: Version Synchronization | pseudocode/version-sync.md | test-plan/version-sync.md |
| C10: Release Pipeline | pseudocode/release-pipeline.md | test-plan/release-pipeline.md |
| C11: Release Skill | pseudocode/release-skill.md | test-plan/release-skill.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Establish npm/npx distribution of the Unimatrix Rust binary so that adopters install via `npm install @dug-21/unimatrix` and wire their project via `npx unimatrix init` without a Rust toolchain. This includes the esbuild/turbo `optionalDependencies` pattern for platform-specific binaries (linux-x64 initially), a GitHub Actions release pipeline triggered by version tags, semantic versioning with lockstep workspace crates at v0.5.0, and a deterministic project wiring command that configures MCP server, hooks, skill files, and database schema.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Hook PATH resolution | Absolute paths to the platform binary in `.claude/settings.json` and `.mcp.json` — no bare names, no PATH shimming. Re-run `npx unimatrix init` after project move or `node_modules` reinstall. | SR-09, ADR-001 | architecture/ADR-001-hook-path-resolution.md |
| Binary rename strategy | Single atomic rename from `unimatrix-server` to `unimatrix` in one commit. Crate name stays `unimatrix-server`. No backward compatibility shim (no external consumers yet). New subcommands: `Version`, `ModelDownload`. | SR-06, ADR-002 | architecture/ADR-002-binary-rename.md |
| Init command runtime | Implemented in JavaScript (`lib/init.js`), not Rust. JS shim intercepts `init` arg and delegates. Rust binary invoked only for DB creation (`unimatrix version --project-dir <root>`) and validation (`unimatrix version`). | OQ-1, ADR-003 | architecture/ADR-003-init-in-javascript.md |
| settings.json merge strategy | Prefix-match identification via regex patterns matching `unimatrix hook` or `unimatrix-server hook` (bare or absolute path). Append-or-update per event. Dedup on re-runs. Malformed JSON = error, do not modify. | SR-08, ADR-004 | architecture/ADR-004-settings-merge-strategy.md |
| Version source of truth | `[workspace.package] version` in root `Cargo.toml`. All 9 crates use `version.workspace = true`. npm `package.json` versions synced by `/release` skill. CI validates match before publish. | ADR-005 | architecture/ADR-005-version-source-of-truth.md |

## Files to Create/Modify

### New Files

| File | Description |
|------|-------------|
| `packages/unimatrix/package.json` | Root npm package: bin, optionalDependencies, postinstall, scripts |
| `packages/unimatrix/bin/unimatrix.js` | JS shim: intercepts `init`, exec's Rust binary for all else |
| `packages/unimatrix/lib/resolve-binary.js` | Resolves platform binary path via `require.resolve` or `UNIMATRIX_BINARY` env |
| `packages/unimatrix/lib/init.js` | Init command: project wiring (.mcp.json, settings, skills, DB) |
| `packages/unimatrix/lib/merge-settings.js` | Isolated settings.json merge logic with prefix-match identification |
| `packages/unimatrix/postinstall.js` | ONNX model pre-download, unconditionally exits 0 |
| `packages/unimatrix/skills/` (13 dirs) | Bundled copies of all skill directories |
| `packages/unimatrix-linux-x64/package.json` | Platform package: os=linux, cpu=x64 |
| `packages/unimatrix-linux-x64/bin/unimatrix` | Pre-compiled stripped Rust binary (populated by CI) |
| `.github/workflows/release.yml` | Release pipeline: build, package, publish on `v*` tags |
| `.claude/skills/release/SKILL.md` | `/release` skill for version bump, changelog, tag, push |
| `CHANGELOG.md` | Generated from conventional commits, grouped by type |

### Modified Files

| File | Change |
|------|--------|
| `Cargo.toml` (root) | Add `version = "0.5.0"` to `[workspace.package]` |
| `crates/unimatrix-store/Cargo.toml` | `version.workspace = true` (replace `0.1.0`) |
| `crates/unimatrix-vector/Cargo.toml` | `version.workspace = true` |
| `crates/unimatrix-embed/Cargo.toml` | `version.workspace = true` |
| `crates/unimatrix-core/Cargo.toml` | `version.workspace = true` |
| `crates/unimatrix-server/Cargo.toml` | `version.workspace = true`, `[[bin]] name = "unimatrix"`, workspace edition/rust-version |
| All other crate `Cargo.toml` files | `version.workspace = true` |
| `crates/unimatrix-server/src/main.rs` | Rename CLI to `unimatrix`, add `Version` and `ModelDownload` subcommands, add `--project-dir` flag |
| `.mcp.json` | Update binary path from `unimatrix-server` to `unimatrix` |
| `.claude/settings.json` | Update all 7 hook commands from `unimatrix-server` to `unimatrix` |

## Data Structures

### Rust CLI (modified)

```rust
#[derive(Parser)]
#[command(name = "unimatrix", about = "Unimatrix knowledge engine")]
struct Cli {
    #[arg(long)]
    project_dir: Option<PathBuf>,

    #[arg(long, short)]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Hook { event: String },
    Export { #[arg(short, long)] output: Option<PathBuf> },
    Import { #[arg(short, long)] input: PathBuf, ... },
    Version,
    ModelDownload,
}
```

### npm Platform Map (JS)

```javascript
const PLATFORMS = {
  "linux-x64": "@dug-21/unimatrix-linux-x64"
};
```

### Unimatrix Hook Identification Patterns (JS)

```javascript
const UNIMATRIX_PATTERNS = [
  /^unimatrix\s+hook\s/,
  /^unimatrix-server\s+hook\s/,
  /\/unimatrix\s+hook\s/,
  /\/unimatrix-server\s+hook\s/,
];
```

### Hook Events (7 events wired by init)

```
SessionStart, Stop, UserPromptSubmit, PreToolUse, PostToolUse, SubagentStart, SubagentStop
```

### Hook Command Format (absolute path)

```json
{
  "type": "command",
  "command": "/abs/path/to/node_modules/@dug-21/unimatrix-linux-x64/bin/unimatrix hook SessionStart"
}
```

Special case: `UserPromptSubmit` appends `| tee -a ~/.unimatrix/injections/hooks.log`.

## Function Signatures

### JavaScript

```javascript
// resolve-binary.js
function resolveBinary() -> string  // Returns absolute path to platform binary

// init.js
async function init(options: { dryRun: boolean }) -> void

// merge-settings.js
function mergeSettings(filePath: string, binaryPath: string, options: { dryRun: boolean })
    -> { actions: string[], content: object }
```

### Rust (new/modified)

```rust
// main.rs — new subcommands (sync, no tokio)
fn handle_version() -> Result<()>         // Prints "unimatrix {version}"
fn handle_model_download() -> Result<()>  // Calls ensure_model(), prints progress

// unimatrix-embed (if not already public)
pub fn ensure_model() -> Result<PathBuf>  // Downloads ONNX model to cache
```

### Integration Surface (existing, consumed by init)

```rust
fn detect_project_root(override_dir: Option<&Path>) -> io::Result<PathBuf>
fn compute_project_hash(project_root: &Path) -> String
fn ensure_data_directory(override_dir, base_dir) -> io::Result<ProjectPaths>
fn Store::open(db_path) -> Result<Store>  // + migrate_if_needed()
```

## Constraints

- **C-01**: `patches/anndists` must be present at build time; CI asserts before `cargo build`.
- **C-02**: Rust 1.89+ required; CI uses `dtolnay/rust-toolchain` with explicit pin.
- **C-03**: ONNX runtime (`ort =2.0.0-rc.9`) — CI validates binary is self-contained via `ldd` check.
- **C-04**: Oniguruma C dependency — binary must target glibc >= 2.35 (Ubuntu 22.04 LTS baseline).
- **C-05**: Absolute paths in hooks break on project move; user must re-run `npx unimatrix init`.
- **C-06**: settings.json merge must be structure-aware; malformed JSON = error, never overwrite.
- **C-07**: Private npm scope `@dug-21` — requires authentication; `NPM_TOKEN` secret in CI.
- **C-08**: No existing CI infrastructure — all workflow files created from scratch.
- **C-09**: Binary rename is breaking — `.mcp.json` and `.claude/settings.json` updated atomically.
- **C-10**: Schema version 11 — init pre-creates DB at current schema; future upgrades via `migrate_if_needed()`.
- **C-11**: Postinstall must never cause `npm install` to fail; all errors = warn + exit 0.
- **C-12**: npm publish order — platform package published before root package to avoid broken installs.

## Dependencies

### Rust Crates (existing, no new crates)

- `clap` — CLI parsing (new subcommands: `Version`, `ModelDownload`)
- `unimatrix-store` — `Store::open()` + `migrate_if_needed()` for DB pre-creation
- `unimatrix-embed` — `ensure_model()` for ONNX model download
- `unimatrix-engine` — `detect_project_root`, `compute_project_hash`, `ensure_data_directory`

### npm Packages (new, created by this feature)

- `@dug-21/unimatrix` — root distribution package
- `@dug-21/unimatrix-linux-x64` — linux x64 platform binary package

### External Services

- **npm registry** — private scope, package publishing
- **GitHub Actions** — CI/CD for release pipeline
- **Hugging Face Hub** — ONNX model download (`sentence-transformers/all-MiniLM-L6-v2`, ~90 MB)

## NOT in Scope

- Interactive onboarding or knowledge seeding (nan-003 handles that)
- Public npm registry publishing (private scope initially)
- Windows support
- macOS platform packages (darwin-arm64/x64 deferred; package structure supports it)
- macOS notarization or code signing
- Automatic updates or self-updating binaries
- npm packages for library crates (only the server binary)
- CLAUDE.md content generation (`/unimatrix-init` skill handles that)
- Agent definition copying (project-specific, not distributed)
- Cross-compilation (linux-x64 builds natively on CI runner)
- Bundling ONNX model in npm package (~90 MB not viable)
- Shell profile modification for PATH (absolute paths eliminate the need)

## Alignment Status

All vision alignment checks **PASS**. One **WARN** on scope additions (minor implementation-necessary additions: `model-download` subcommand, `UNIMATRIX_BINARY` env var override, `--project-dir` flag, `--verbose` flag). These are reasonable and fall within the spirit of the scope. No variances requiring approval.

The architect overrode the scope's original "PATH-based via `node_modules/.bin/`" approach (Resolved Q5) with absolute paths (ADR-001), correctly addressing SR-09 (the top risk). This is a well-reasoned deviation validated by the vision guardian.

## Delivery Phasing

| Wave | Components | Dependencies |
|------|-----------|-------------|
| Wave 1 — Foundation | C7 (Binary rename), C9 (Version sync) | None (parallel) |
| Wave 2 — Package Structure | C1 (npm packages), C2 (JS shim), C3 (Binary resolution), C8 (Model download) | Wave 1 |
| Wave 3 — Init Command | C4 (Init), C5 (Settings merge), C6 (Postinstall) | Wave 2 |
| Wave 4 — Release Infrastructure | C10 (CI pipeline), C11 (Release skill) | Waves 1-3 |

## Open Questions (for delivery)

1. **ONNX shared library bundling**: Is `ort` statically linked into the binary or does it produce a separate `.so`? Validate with `ldd target/release/unimatrix` on CI build. If separate, platform package must bundle both files.
2. **Binary size**: SCOPE estimates ~20 MB. ONNX runtime may inflate this. If >50 MB, document as known trade-off.
3. **npm registry authentication**: CI needs `NPM_TOKEN` secret. Scoped to publish only. Use `npm publish --access restricted`.
4. **`unimatrix version` output format**: Architecture recommends `unimatrix 0.5.0` (human-readable). Consider `--json` for structured output.
5. **UserPromptSubmit tee command**: The existing hook pipes through `tee -a ~/.unimatrix/injections/hooks.log`. Architecture retains this. Confirm this is production behavior, not a debug artifact.
