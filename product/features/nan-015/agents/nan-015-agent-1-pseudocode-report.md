# Agent Report: nan-015-agent-1-pseudocode

## Task

Produce per-component pseudocode files for nan-015 (Shared Model Volume for ONNX Models).

## Deliverables

| File | Component |
|------|-----------|
| `product/features/nan-015/pseudocode/OVERVIEW.md` | Component interaction, data flow, shared types, sequencing |
| `product/features/nan-015/pseudocode/cache-path-resolution.md` | `EmbedConfig::resolve_cache_dir()` env var insertion |
| `product/features/nan-015/pseudocode/dockerfile.md` | Model bake-in removal, `/shared` setup, env var, volume directive |
| `product/features/nan-015/pseudocode/compose-config.md` | `unimatrix-shared` volume definition, security guidance comments |
| `product/features/nan-015/pseudocode/documentation.md` | PRODUCT-VISION.md and WAVE2-ROADMAP.md W2-1 updates |

## Components Covered

1. **cache-path-resolution** -- 1 function modified (`resolve_cache_dir`), 5 test scenarios
2. **dockerfile** -- 7 changes (remove 3 blocks, add 4 blocks), 8 test scenarios
3. **compose-config** -- 4 changes (volume mount, comments, volume definition), 5 test scenarios
4. **documentation** -- 2 files updated (W2-1 sections), 5 test scenarios

## V-01 Resolution

SPECIFICATION C-04 (lines 258-266) already lists the correct precedence ordering (config field > env var > dirs > fallback). No correction needed. V-01 is resolved.

## Open Questions

None. All design decisions are resolved by ADR-001, ADR-002, ADR-003. All interface names traced to architecture and existing codebase.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` -- returned 12 entries. Key: #4650 (ADR-001 env var redirect), #4651 (ADR-002 precedence), #4652 (ADR-003 shared volume RW), #4579 (container build pattern), #70 (cache directory ADR), #4647 (model hash pinning procedure).
- Queried: `mcp__unimatrix__context_search` (pattern: model cache config) -- #4626 (co-locate config with data volume), #3817 (config field default dual-site pattern).
- Queried: `mcp__unimatrix__context_search` (pattern: container dockerfile volume) -- #4579 (cargo-chef Dockerfile pattern), #4626 (container volume design).
- Deviations from established patterns: none. The env var insertion follows the existing `resolve_cache_dir()` pattern. The Dockerfile changes follow the cargo-chef three-stage pattern established in #4579.
