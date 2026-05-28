# C5: PRODUCT-VISION.md W2-1

## Purpose

Update the W2-1 section to reflect the shipped single-volume model (nan-014 reality). Per ADR-004: correct factual errors, constrain to W2-1 volume description only, add annotation.

## Target File

`product/PRODUCT-VISION.md`, lines 448-458 (W2-1 volume description and security requirements).

## Current State

```markdown
**What**: Dockerfile + docker-compose with two named volumes:
- `unimatrix-data` — single SQLite database (back up frequently; integrity-critical)
- `unimatrix-shared` — ONNX models (re-downloadable), `config.toml` (mount as read-only bind)

Container is stateless except the volumes. Backup = volume snapshot of `unimatrix-data`. `HEALTHCHECK` verifies daemon liveness and schema version currency.

**Security requirements:**
- [High] Named volumes owner-only at container build time (`chmod 0700`)
- [Medium] `config.toml` as read-only bind mount from secrets manager, not in data volume
- [Low] Container runs as non-root (`USER unimatrix`)
```

## Pseudocode

```
REPLACE lines 448-451 (the "What" paragraph and volume list) WITH:

  **What**: Dockerfile + docker-compose with a single named volume.
  *(Updated to reflect nan-014 shipped design.)*
  - `unimatrix-data` — databases, vector indexes, config, and logs (back up frequently; integrity-critical). ONNX models baked into the image.

KEEP line 452 unchanged:
  Container is stateless except the volumes. Backup = volume snapshot of `unimatrix-data`. `HEALTHCHECK` verifies daemon liveness and schema version currency.

REPLACE line 456 ([Medium] security requirement) WITH:

  - [Medium] Sensitive config values injected via `UNIMATRIX_CONFIG` env var pointing to a secrets-manager-provided path

KEEP lines 455 and 457 ([High] and [Low] requirements) unchanged.
```

Key changes:
- Two-volume description replaced with single `unimatrix-data` volume.
- `unimatrix-shared` reference removed entirely.
- ONNX models noted as baked into image.
- Contents list updated: "databases, vector indexes, config, and logs".
- nan-014 annotation added per ADR-004.
- [Medium] security requirement updated from bind-mount guidance to env var injection.

## Constraints

- C-06: Edits constrained to W2-1 volume description only (lines 448-458).
- SR-03: No broad PRODUCT-VISION.md revision.
- ADR-004: Include "Updated to reflect nan-014 shipped design" annotation.

## Error Handling

Not applicable (prose edit).

## Key Test Scenarios

1. Single `unimatrix-data` volume described (no `unimatrix-shared`).
2. ONNX models described as baked into image.
3. Config described as living in the data volume.
4. nan-014 annotation present.
5. [Medium] security requirement references `UNIMATRIX_CONFIG` env var, not bind mount.
6. [High] and [Low] security requirements unchanged.
7. PR diff shows changes only within lines 448-458.
