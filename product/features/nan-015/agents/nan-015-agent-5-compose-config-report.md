# Agent Report: nan-015-agent-5-compose-config

## Task

Implement docker-compose.yml changes for nan-015: add `unimatrix-shared` named volume, mount to unimatrix service, add documentation comments for volume separation, backup guidance, `:ro` hardening, and security notes.

## Files Modified

- `docker-compose.yml`

## Changes

1. Added `unimatrix-shared:/shared` mount to the unimatrix service's volumes section
2. Added `:ro` hardening comment (optional, after initial model download)
3. Added volume separation explanation (unimatrix-data = integrity-critical, unimatrix-shared = re-downloadable)
4. Preserved existing config.toml editing guidance
5. Updated `unimatrix-data` volume comment: removed stale "model cache" reference, updated to "Integrity-critical data"
6. Added `unimatrix-shared` volume definition with:
   - Description of contents (ONNX models, ~166 MB)
   - Auto-population note (requires internet on first start)
   - Backup optional guidance
   - Air-gap pre-population note
   - Security notes: `:ro` hardening, `nli_model_sha256` pinning, `#651` embedding hash gap

## Test Results

N/A -- infrastructure-only component (no cargo tests). Manual verification against test plan:

| Test | Status |
|------|--------|
| T-01: Both volumes defined | PASS |
| T-02: Volume mount points correct | PASS |
| T-03: Security guidance (AC-11) | PASS |
| T-04: Backup guidance | PASS |
| T-05: No stale "model cache" in unimatrix-data | PASS |
| T-06: Multi-container compatible | PASS |

## Issues

None.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- surfaced ADR-001 (#4650), ADR-002 (#4651), ADR-003 (#4652), nxs-013 ADR-003 (#4635), cargo-chef Dockerfile pattern (#4579). ADR-003 was directly relevant for `:rw` default and security comment wording.
- Stored: nothing novel to store -- straightforward YAML configuration change following validated pseudocode exactly. No runtime gotchas or non-obvious patterns discovered.
