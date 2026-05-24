# Agent Report: nan-014-agent-7-dockerfile

## Component
Dockerfile (three-stage cargo-chef build)

## Files Created
- `/workspaces/unimatrix/Dockerfile`

## Files Modified
None.

## Implementation Summary

Created production Dockerfile with three stages:

1. **planner** (rust:1.89-slim-bookworm): cargo-chef 0.1.71 (pinned + --locked, ADR-006), copies workspace manifests + patches + .cargo/config.toml, extracts recipe.json.

2. **builder** (rust:1.89-slim-bookworm): Installs ORT 1.20.1 with TARGETARCH-conditional download and SHA-256 verification (ADR-002). Cooks dependencies from recipe, builds release binary, strips it. Runs model-download (embedding + NLI) with HOME=/data. Creates /data with chown 65534:65534 + chmod 0700 (WARN-2).

3. **runtime** (gcr.io/distroless/cc-debian12:nonroot): Copies binary, ORT .so, models, /data. Sets ENV (HOME=/data, LD_LIBRARY_PATH, UNIMATRIX_LOG, UNIMATRIX_CONFIG per ADR-005). VOLUME, HEALTHCHECK (ADR-003), ENTRYPOINT + CMD. No EXPOSE (C-10).

## ORT SHA-256 Hashes (Captured)
- x64: `67db4dc1561f1e3fd42e619575c82c601ef89849afc7ea85a003abbac1a1a105`
- aarch64: `ae4fedbdc8c18d688c01306b4b50c63de3445cdf2dbd720e01a2fa3810b8106a`

## Deviations from Pseudocode
- Pseudocode listed `unimatrix-eval` as a workspace crate; actual crate is `unimatrix-learn`. Used correct name.
- No other deviations.

## Test Results
- cargo build --workspace: PASS (no errors, pre-existing warnings only)
- Dockerfile is a static file; shell-based tests (docker build/run) are Stage 3c tester scope.
- Build validation checklist: all 13 items pass (verified against test-plan/dockerfile.md).

## Issues
None.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced ADR-002 (ORT SHA-256), ADR-006 (cargo-chef pinning), ADR-005 (data path), entry #4274 (ORT verification gap lesson). Applied all.
- Stored: nothing novel to store -- Dockerfile follows established patterns from pseudocode and ADRs. ORT hashes are captured in the Dockerfile ARG values themselves.
