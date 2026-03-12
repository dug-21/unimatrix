# Agent Report: nan-004-agent-1-architect

## Task
Design architecture for nan-004 (Versioning & Packaging) — npm/npx distribution of the Unimatrix Rust binary.

## Artifacts Produced

### ARCHITECTURE.md
- `/workspaces/unimatrix/product/features/nan-004/architecture/ARCHITECTURE.md`

### ADR Files
1. `/workspaces/unimatrix/product/features/nan-004/architecture/ADR-001-hook-path-resolution.md`
2. `/workspaces/unimatrix/product/features/nan-004/architecture/ADR-002-binary-rename.md`
3. `/workspaces/unimatrix/product/features/nan-004/architecture/ADR-003-init-in-javascript.md`
4. `/workspaces/unimatrix/product/features/nan-004/architecture/ADR-004-settings-merge-strategy.md`
5. `/workspaces/unimatrix/product/features/nan-004/architecture/ADR-005-version-source-of-truth.md`

## Key Design Decisions

1. **SR-09 resolved: Absolute paths for hooks (ADR-001).** Hook commands in `.claude/settings.json` use the full absolute path to the platform binary inside `node_modules`. No PATH dependency, no shell profile modification, no JS shim overhead on every hook invocation. Trade-off: paths invalidate on project move or `node_modules` rebuild — re-run `npx unimatrix init` to repair.

2. **Binary rename is atomic (ADR-002).** Single commit renames `unimatrix-server` to `unimatrix`, updates all repo configs. No backward compat shim needed — no external consumers exist before first publish.

3. **Init command lives in JavaScript (ADR-003).** The JS shim intercepts `init` and runs JS logic for JSON merge, skill copy, and path resolution. Delegates to the Rust binary for database creation and validation. Rationale: JS can `require.resolve` its own package location; Rust cannot.

4. **settings.json merge uses prefix-match identification (ADR-004).** Regex patterns match both old (`unimatrix-server hook`) and new (`unimatrix hook`) command prefixes, handling the rename transparently. Isolated in `merge-settings.js` for testability.

5. **Cargo.toml workspace version is the single source of truth (ADR-005).** All 9 crates inherit version. npm synced by `/release` skill. Initial version 0.5.0.

## Architecture Highlights

- 11 components (C1-C11) organized in 4 delivery waves.
- Wave 1 (foundation): binary rename + version sync.
- Wave 2 (packages): npm structure, JS shim, binary resolution, model download subcommand.
- Wave 3 (init): init command, settings merge, postinstall.
- Wave 4 (release): GitHub Actions workflow, `/release` skill.
- Integration surface table with 14 integration points documented.
- Hook command format uses absolute path to platform binary directly (not JS shim).

## Open Questions

1. ONNX shared library bundling — static or dynamic? Validate with `ldd` on CI build.
2. Binary size with ONNX — may exceed 20 MB estimate. Document if >50 MB.
3. npm registry authentication — CI needs `NPM_TOKEN` secret.
4. `unimatrix version` output format — recommend human-readable default, `--json` for structured.

## Unimatrix Storage

All 5 ADR store attempts failed with `-32003: Agent lacks Write capability`. ADRs exist as files only. The Design Leader must store these in Unimatrix using a privileged agent or direct invocation.

## Self-Check

- [x] ARCHITECTURE.md contains System Overview, Component Breakdown, Interactions, Integration Surface
- [x] All ADRs follow format: `## ADR-NNN: Title` / `### Context` / `### Decision` / `### Consequences`
- [x] Each ADR is a separate file in `architecture/` with correct naming
- [x] ADR file paths included in return
- [x] Integration Surface table included (14 integration points)
- [x] No placeholder or TBD sections — unknowns flagged as open questions
- [x] All output files within `product/features/nan-004/architecture/`
- [x] Searched Unimatrix for existing ADRs in the affected domain before designing
- [ ] Every ADR stored in Unimatrix via `/store-adr` — BLOCKED (agent lacks Write capability)
- [x] No superseded ADRs identified (nan-004 is a new domain)
