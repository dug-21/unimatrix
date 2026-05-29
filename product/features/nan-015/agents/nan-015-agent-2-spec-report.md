# Agent Report: nan-015-agent-2-spec

## Status: COMPLETE

## Output
- `/workspaces/unimatrix/product/features/nan-015/specification/SPECIFICATION.md`

## Key Decisions

1. **Env var precedence is the top-level resolution step**: `UNIMATRIX_MODEL_CACHE` checks before `EmbedConfig.cache_dir` and `dirs::cache_dir()`. This is the cleanest slot -- explicit, self-documenting, no side effects on other `dirs::` calls.

2. **SR-07 partial file corruption handled by existing retry state machine**: Rather than requiring atomic write (temp file + rename), the specification relies on the existing `Loading -> Failed -> Retrying` state machine with exponential backoff. ONNX session load failure on a corrupt file triggers the retry path. Atomic write is listed as out of scope and can be addressed separately.

3. **SR-04 (embedding hash gap) addressed in AC-11 documentation**: The specification requires AC-11 documentation to explicitly acknowledge that embedding model SHA-256 enforcement is tracked as #651, so operators are not misled about the enforcement gap.

4. **SR-08 (accidental env var in developer shell) documented as intended behavior**: If a developer sets `UNIMATRIX_MODEL_CACHE` outside a container, the redirection is the env var working as designed, not a bug.

## Scope Risk Coverage

| Risk | Coverage |
|------|----------|
| SR-01 | Constraint C-04: explicit precedence chain documented |
| SR-02 | Constraint C-08: verify-then-load ordering preserved |
| SR-03 | NFR-04: existing retry/backoff policy, graceful degradation |
| SR-04 | AC-11 + FR-13: documentation acknowledges #651 gap |
| SR-05 | Constraint C-07: CI/CD audit required |
| SR-06 | FR-02 + C-04: single resolution function enforced |
| SR-07 | Constraint C-09: existing retry state machine handles corrupt files |
| SR-08 | Constraint C-05: documented as container-internal env var |

## Open Questions

None -- all scope questions were resolved in SCOPE.md.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- 13 entries returned, key relevant: #4642 (verify-then-load lesson), #4636 (volume description ADR), #4570 (ORT supply chain ADR), #4579 (container build pattern)
