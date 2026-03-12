# Agent Report: nan-004-agent-1-pseudocode

## Task
Produce per-component pseudocode files for nan-004 (Versioning & Packaging), covering all 11 components (C1-C11).

## Files Produced

| File | Component |
|------|-----------|
| `pseudocode/OVERVIEW.md` | Cross-cutting: component interaction, data flow, shared types, sequencing |
| `pseudocode/npm-package-structure.md` | C1: npm package.json files, directory layout, skill bundling |
| `pseudocode/js-shim.md` | C2: JS entry point, argv routing, exit code passthrough |
| `pseudocode/binary-resolution.md` | C3: Platform binary resolution via require.resolve + UNIMATRIX_BINARY fallback |
| `pseudocode/init-command.md` | C4: Project wiring (.mcp.json, settings, skills, DB, validation) |
| `pseudocode/settings-merge.md` | C5: Structure-aware settings.json merge with ADR-004 prefix patterns |
| `pseudocode/postinstall.md` | C6: ONNX model pre-download, unconditional exit 0 |
| `pseudocode/binary-rename.md` | C7: Rename binary, add Version/ModelDownload subcommands, update configs |
| `pseudocode/model-download.md` | C8: ensure_model() CLI wrapper + re-export from embed crate |
| `pseudocode/version-sync.md` | C9: Workspace version 0.5.0, version.workspace = true in all 9 crates |
| `pseudocode/release-pipeline.md` | C10: GitHub Actions release.yml (build, package, publish, release) |
| `pseudocode/release-skill.md` | C11: /release skill (bump, changelog, commit, tag, push) |

## Components Covered

All 11: C1 through C11.

## Open Questions

1. **ensure_model() not re-exported**: `crates/unimatrix-embed/src/download.rs` has `pub fn ensure_model()` but the `download` module is private in lib.rs. C8 needs `pub use download::ensure_model;` added to `crates/unimatrix-embed/src/lib.rs`. Flagged in OVERVIEW.md.

2. **Cache directory resolution for model-download**: `EmbedConfig::default()` sets `cache_dir: None`. The embedding pipeline resolves this internally, but `handle_model_download()` must resolve it explicitly using `dirs::cache_dir()`. Need to verify the exact resolution path matches what the embed crate uses at runtime. If they diverge, the postinstall downloads to a different location than the server expects.

3. **handle_version with --project-dir side effect**: The architecture says `unimatrix version --project-dir <root>` triggers DB creation. This means `handle_version()` must conditionally call `ensure_data_directory` + `Store::open`. This is a somewhat surprising side effect for a "version" command. An alternative would be a dedicated `init-db` subcommand, but the architecture explicitly specifies this approach. Pseudocode follows the architecture.

4. **Architecture doc stale reference to UserPromptSubmit tee**: The ARCHITECTURE.md Hook Command Format section still mentions the tee pipeline for UserPromptSubmit. The IMPLEMENTATION-BRIEF.md resolved decision explicitly drops tee: "No special cases -- all hooks use the same format (tee pipeline dropped for distribution)." The ADR-004 also mentions the tee special case. Pseudocode follows the IMPLEMENTATION-BRIEF.md resolved decision (NO tee).

## Self-Check

- [x] Architecture output was read before writing any pseudocode
- [x] No invented interface names -- every name traced to architecture or codebase
- [x] Output is per-component (OVERVIEW.md + one file per component), not monolithic
- [x] Each component file includes function signatures, error handling, and test scenarios
- [x] No TODO, placeholder functions, or TBD sections -- gaps flagged explicitly
- [x] Shared types defined in OVERVIEW.md match usage in component files
- [x] All output files within `product/features/nan-004/pseudocode/`
- [x] Knowledge Stewardship report block included

## Knowledge Stewardship
- Queried: /query-patterns not available in agent context (deferred tool, no server running)
- Deviations from established patterns: The sync CLI subcommand pattern (documented in existing Unimatrix entries #1102, #1104, #1160 per ADR-002) is extended with two new sync-path subcommands (Version, ModelDownload). This follows the established pattern of sync subcommands without tokio runtime.
- Stored: nothing novel to store -- read-only pseudocode agent; all design decisions originate from architect ADRs
