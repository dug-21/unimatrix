# nan-004 Researcher Report

## Agent ID: nan-004-researcher

## Summary

Explored the full problem space for npm/npx distribution of the Unimatrix Rust binary. Read all relevant source files (binary entry point, CLI structure, MCP config, hook scripts, skill files, schema migration, ONNX download pipeline, project root detection), analyzed the nan-003 boundary, and checked for existing CI/release infrastructure (none exists).

## Key Findings

1. **Release binary is 20 MB** (linux aarch64, stripped). Well within npm distribution norms.
2. **ONNX Runtime (`ort = "=2.0.0-rc.9"`) is the hardest cross-compilation constraint**. It links native C libraries. The `tokenizers` crate with `onig` feature adds another native dependency (Oniguruma).
3. **Schema migration is already production-ready**. `migrate_if_needed()` runs on every `Store::open()` with idempotent guards. No new migration infrastructure needed for version upgrades.
4. **ONNX model is downloaded lazily** via `hf-hub` crate from HuggingFace. ~90 MB. Cache location: `~/.cache/unimatrix-embed/`. Postinstall can pre-download this.
5. **settings.json merge is the trickiest init logic**. The file has nested hook structures with matchers, and users may have existing permissions and non-unimatrix hooks that must be preserved.
6. **No CI/CD infrastructure exists** — no `.github/workflows/`, no release scripts, no npm package files. Everything must be created from scratch.
7. **13 skill files exist** that need bundling. 16 agent files exist but are Unimatrix-specific (should NOT be bundled for target projects).
8. **Hook commands use bare `unimatrix-server` name** — this requires the binary to be on PATH or use absolute paths. Open question about renaming to `unimatrix`.
9. **Rust toolchain is 1.89 (edition 2024)** — newer than default GitHub Actions runners provide. CI must install explicitly.
10. **Local patch on `anndists`** — must be included in CI builds (it's in the repo, so cargo will pick it up).

## Scope Boundaries Rationale

- **In scope**: npm package structure, postinstall ONNX download, `npx unimatrix init` (mechanical wiring), GitHub Actions release pipeline, version synchronization.
- **Out of scope**: Interactive onboarding (nan-003), public publishing, Windows, macOS notarization, library crate distribution, CLAUDE.md content authoring.
- **Boundary with nan-003**: nan-004 places files and configs mechanically. nan-003 skills handle interactive knowledge setup. nan-004 installs the skill files that nan-003 authored.

## Risks

1. **ONNX Runtime cross-compilation**: May require Docker-based `cross` or platform-native CI runners. Needs validation spike.
2. **settings.json merge correctness**: Complex JSON merge logic with many edge cases. Needs thorough testing.
3. **Binary path stability**: Absolute paths in `.mcp.json` break when projects move. This is inherent to the approach and needs documentation.
4. **npm private scope auth**: Users must `npm login` with the correct scope before installing. Friction for early adopters.

## Knowledge Stewardship

- Queried: /query-patterns for npm packaging, distribution, versioning, release pipeline -- no directly relevant results (closest were migration patterns and build tooling)
- Stored: nothing novel to store -- findings are feature-specific (npm packaging approach, cross-compilation constraints) and don't generalize beyond nan-004. The schema migration pattern (#370, #681) is already well-documented.
