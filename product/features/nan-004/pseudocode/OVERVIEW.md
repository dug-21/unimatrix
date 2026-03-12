# nan-004: Versioning & Packaging — Pseudocode Overview

## Components Involved

| ID | Component | Language | Wave | Why |
|----|-----------|----------|------|-----|
| C1 | npm Package Structure | JSON/config | 2 | Directory layout, package.json files, skill bundling |
| C2 | JS Shim | JavaScript | 2 | Entry point: route `init` to JS, everything else to Rust binary |
| C3 | Binary Resolution | JavaScript | 2 | Resolve platform binary path from optionalDependencies |
| C4 | Init Command | JavaScript | 3 | Deterministic project wiring (.mcp.json, settings, skills, DB) |
| C5 | Settings Merge | JavaScript | 3 | Structure-aware merge of hooks into .claude/settings.json |
| C6 | Postinstall | JavaScript | 3 | ONNX model pre-download, unconditionally exits 0 |
| C7 | Binary Rename | Rust | 1 | Rename unimatrix-server to unimatrix, update CLI + config files |
| C8 | Model Download Subcommand | Rust | 2 | Expose ensure_model() as CLI subcommand for postinstall |
| C9 | Version Synchronization | TOML/config | 1 | Workspace version 0.5.0, version.workspace = true in all crates |
| C10 | Release Pipeline | YAML | 4 | GitHub Actions: build, package, publish on v* tags |
| C11 | Release Skill | Markdown | 4 | /release skill: bump, changelog, tag, push |

## Data Flow

```
npm install @dug-21/unimatrix
  -> npm resolves optionalDependencies (C1)
  -> @dug-21/unimatrix-linux-x64 installed (binary)
  -> postinstall.js (C6)
     -> resolve-binary.js (C3) -> binary path
     -> execFileSync: unimatrix model-download (C8)
     -> ONNX model cached to ~/.cache/unimatrix-embed/

npx unimatrix init
  -> bin/unimatrix.js (C2) intercepts "init"
  -> lib/init.js (C4):
     -> detect project root (.git walk)
     -> resolve-binary.js (C3) -> binary absolute path
     -> write/merge .mcp.json (unimatrix server entry)
     -> merge-settings.js (C5) -> merge hooks into .claude/settings.json
     -> copy skills/ -> .claude/skills/ (13 dirs)
     -> execFileSync: unimatrix version --project-dir <root> (triggers DB creation)
     -> execFileSync: unimatrix version (validation)
     -> print summary

npx unimatrix hook SessionStart  (or any subcommand)
  -> bin/unimatrix.js (C2) exec's Rust binary with forwarded args

Hook fires (Claude Code shell):
  -> .claude/settings.json has absolute path (ADR-001)
  -> Shell executes: /abs/path/.../unimatrix hook SessionStart
  -> Rust binary (C7) handles hook directly

/release (maintainer workflow):
  -> Release skill (C11) bumps version in Cargo.toml
  -> Syncs npm package.json versions
  -> Generates CHANGELOG.md
  -> Commits, tags, pushes
  -> GitHub Actions (C10) builds, packages, publishes
```

## Shared Types and Constants

### JavaScript Constants (shared across C2, C3, C4, C5, C6)

```
PLATFORMS = { "linux-x64": "@dug-21/unimatrix-linux-x64" }

HOOK_EVENTS = [
  "SessionStart", "Stop", "UserPromptSubmit",
  "PreToolUse", "PostToolUse", "SubagentStart", "SubagentStop"
]

EVENT_MATCHERS = {
  "SessionStart": "",  "Stop": "",  "UserPromptSubmit": "",
  "PreToolUse": "*",   "PostToolUse": "*",
  "SubagentStart": "*", "SubagentStop": "*"
}

UNIMATRIX_PATTERNS = [
  /^unimatrix\s+hook\s/,
  /^unimatrix-server\s+hook\s/,
  /\/unimatrix\s+hook\s/,
  /\/unimatrix-server\s+hook\s/
]

SKILL_DIRS = [13 skill directory names from .claude/skills/]
```

### Rust Types (modified, not new)

```
Cli struct: adds --project-dir (existing), --verbose (existing)
Command enum: adds Version, ModelDownload variants
```

## Sequencing Constraints

1. **Wave 1 (parallel)**: C7 (binary rename) and C9 (version sync) have no dependencies.
2. **Wave 2 (depends on Wave 1)**: C1, C2, C3, C8 need the renamed binary and versioned workspace.
3. **Wave 3 (depends on Wave 2)**: C4, C5, C6 need the package structure, shim, and binary resolution.
4. **Wave 4 (depends on Waves 1-3)**: C10, C11 need everything in place to build and publish.

## Gap: ensure_model() Not Re-exported

`crates/unimatrix-embed/src/download.rs` contains `pub fn ensure_model()` but the `download` module is private (`mod download;` in lib.rs). C8 (ModelDownload subcommand) needs this function. The implementation must either:
- Add `pub mod download;` or `pub use download::ensure_model;` to `crates/unimatrix-embed/src/lib.rs`
- Or add a public wrapper function in the embed crate's public API

The architecture lists `EmbedConfig::default()` then `ensure_model()` as the integration surface. The pseudocode for C8 assumes `ensure_model` will be re-exported.
