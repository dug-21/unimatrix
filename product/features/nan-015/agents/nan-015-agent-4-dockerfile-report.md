# Agent Report: nan-015-agent-4-dockerfile

## Status: COMPLETE

## Files Modified
- `Dockerfile`

## Changes Made

1. **Removed model bake-in from builder stage** (lines 95-101): Deleted `ENV HOME=/data`, `model-download`, `model-download --nli`, and `rm -rf /data/.cache/huggingface` commands.
2. **Consolidated directory setup**: Replaced separate `/data` chown/chmod block with unified `mkdir -p /data /shared/models && chown -R 65532:65532 /data /shared && chmod 0700 /data /shared`.
3. **Removed model COPY from runtime stage**: Deleted `COPY --from=builder --chown=65532:65532 /data/.cache/unimatrix/ /data/.cache/unimatrix/`.
4. **Added /shared COPY to runtime stage**: `COPY --from=builder --chown=65532:65532 /shared /shared` for ownership inheritance in distroless.
5. **Added UNIMATRIX_MODEL_CACHE to runtime ENV**: `UNIMATRIX_MODEL_CACHE=/shared/models` appended to ENV block.
6. **Updated VOLUME directive**: `VOLUME ["/data"]` changed to `VOLUME ["/data", "/shared"]`.
7. **Updated comments**: Stage 2 header, run command, and directory setup comments updated to reflect two-volume model.

## Test Results

Tests: N/A (infrastructure-only, no cargo tests)

Static analysis per test plan:
- Test 1 (no model-download commands): PASS
- Test 3 (/shared directory ownership 65532:65532, permissions 0700): PASS
- Test 4 (VOLUME includes both /data and /shared): PASS
- Test 5 (UNIMATRIX_MODEL_CACHE=/shared/models in runtime ENV): PASS
- Test 7 (HEALTHCHECK unchanged): PASS
- Workspace build: PASS (zero errors)

## Issues
None.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced ADR-001 (env var redirect), ADR-002 (cache precedence), ADR-003 (shared volume RW), nan-014 Dockerfile lesson (#4582), cargo-chef pattern (#4579). Applied distroless ownership propagation pattern (COPY from builder for directory ownership since no shell available in runtime).
- Stored: nothing novel to store -- the distroless ownership propagation via COPY is already documented in #4579 cargo-chef pattern, and the specific changes follow the pseudocode exactly without discovering new gotchas.
