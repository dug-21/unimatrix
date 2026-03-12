# Agent Report: nan-004-agent-2-spec

## Task
Write SPECIFICATION.md for nan-004 (Versioning & Packaging).

## Artifacts Produced
- `/workspaces/unimatrix/product/features/nan-004/specification/SPECIFICATION.md`

## Key Decisions

1. **Absolute paths over bare names for hooks (SR-09 mitigation)** — FR-12 and C-05 specify that hook commands use absolute paths to the binary, not bare `unimatrix` names. This directly addresses SR-09 (highest-risk item per the risk assessment). The trade-off is that users must re-run `npx unimatrix init` after moving their project or reinstalling node_modules.

2. **Init implemented as Rust subcommand** — FR-04 specifies init as a Rust subcommand (matching hook/export/import sync paths), not a pure Node.js script. This reuses `project::ensure_data_directory` and `Store::open` for project hash computation and database creation. The JS shim dispatches to it. Flagged as open question for architect confirmation.

3. **settings.json merge identifies unimatrix hooks by binary name in command string** — FR-13 defines the identification heuristic. This avoids needing marker fields or comments in the JSON.

4. **Malformed JSON in settings.json is an error, not overwritten** — C-06 explicitly states that malformed JSON produces an error diagnostic. The init command does not silently replace a corrupt file.

5. **Binary rename treated as atomic with settings update** — C-09 requires updating this repository's settings and mcp.json at the same time as the binary rename to avoid a broken state.

## Open Questions for Architect
1. Init subcommand: Rust vs Node.js implementation (recommendation: Rust)
2. ONNX runtime linking strategy: static vs dynamic, and impact on package size
3. UserPromptSubmit tee pipeline: production requirement or debug artifact?
4. npm authentication mechanism for CI publishing

## Self-Check
- [x] SPECIFICATION.md covers all 17 acceptance criteria from SCOPE.md (AC-01 through AC-17)
- [x] Every functional requirement is testable
- [x] Non-functional requirements include measurable targets
- [x] Domain Models section defines key terms
- [x] NOT in scope section is explicit (12 exclusions)
- [x] Output file is in `product/features/nan-004/specification/` only
- [x] No placeholder or TBD sections
- [x] Knowledge Stewardship report block included

## Knowledge Stewardship
- Queried: /query-patterns for packaging, npm distribution -- not available in agent context (deferred tool). Specification based on SCOPE.md, risk assessment, and codebase inspection.
