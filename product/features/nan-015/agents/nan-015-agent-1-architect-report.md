# Agent Report: nan-015-agent-1-architect

## Task
Design architecture for nan-015: shared model volume for ONNX models.

## Artifacts Produced

| Artifact | Path |
|----------|------|
| ARCHITECTURE.md | `product/features/nan-015/architecture/ARCHITECTURE.md` |
| ADR-001 | `product/features/nan-015/architecture/ADR-001-env-var-cache-redirect.md` |
| ADR-002 | `product/features/nan-015/architecture/ADR-002-cache-path-precedence.md` |
| ADR-003 | `product/features/nan-015/architecture/ADR-003-shared-volume-default-rw.md` |

## ADR Summary

| ADR | Title | Unimatrix ID | Addresses |
|-----|-------|--------------|-----------|
| ADR-001 | Environment Variable for Container Model Cache Redirect | #4650 | SR-06 |
| ADR-002 | Cache Path Precedence Chain | #4651 | SR-01, SR-06 |
| ADR-003 | Shared Volume Default Read-Write with Optional Read-Only Hardening | #4652 | SR-02 |
| ADR-004 | Two-Volume Documentation Update (supersedes nxs-013 ADR-004 #4636) | #4653 | AC-10 |

## Key Decisions

1. `UNIMATRIX_MODEL_CACHE` env var inserted into `resolve_cache_dir()` as priority 2 (after explicit field, before dirs::cache_dir()). All seven non-test call sites redirected without call-site changes.
2. Four-level precedence chain: field > env var > dirs > fallback. Empty-string guard on env var.
3. Default `:rw` mount for zero-config startup. `:ro` documented as optional hardening.
4. Superseded nxs-013 ADR-004 (#4636 -> #4653) since "baked into image" is no longer accurate.

## Scope Risks Addressed

- **SR-01** (cache path precedence): Resolved by ADR-002 — documented four-level chain with unit test requirements.
- **SR-02** (supply-chain attack surface): Resolved by ADR-003 — default :rw with :ro hardening guidance, verify-then-load ordering preserved.
- **SR-06** (three call sites): Resolved by ADR-001 — actually seven non-test call sites, all use EmbedConfig::default() so all captured by single env var check.

## Open Questions

1. Eval harness in-container: if eval ever runs inside a container, it needs the shared volume mounted. Low risk (eval runs natively today).
2. GGUF future path: `/shared/models/` leaves room for GGUF subdirectory but layout not defined by nan-015.

## Knowledge Stewardship
- Queried: context_briefing at session start — 13 entries returned, key: #4647 (model hash pinning), #4642 (hash verification ordering lesson), #4636 (ADR-004 volume descriptions), #69/#70 (hf-hub and cache directory ADRs), #4570 (ORT supply chain verification ADR), #4579 (container build pattern)
- Stored: ADR-001 env var cache redirect (#4650)
- Stored: ADR-002 cache path precedence (#4651)
- Stored: ADR-003 shared volume default RW (#4652)
- Stored: ADR-004 two-volume documentation update (#4653, supersedes nxs-013 ADR-004 #4636)

## Status
Complete.
