# C6: WAVE2-ROADMAP.md W2-1

## Purpose

Update the W2-1 volume list to reflect the shipped single-volume design. Per ADR-004: correct factual errors, constrain to volume list only, add annotation.

## Target File

`product/WAVE2-ROADMAP.md`, lines 39-44 (volume list and container user line).

## Current State

```markdown
Named volumes:
- `unimatrix-knowledge` — per-repo knowledge DBs (integrity-critical, back up frequently)
- `unimatrix-analytics` — per-repo analytics DBs (self-healing)
- `unimatrix-shared` — ONNX models + `config.toml` as read-only bind

Non-root container user. HEALTHCHECK on daemon liveness + schema version.
```

## Pseudocode

```
REPLACE lines 39-43 (the "Named volumes:" paragraph and three-item list) WITH:

  Named volume *(updated to reflect nan-014 shipped design)*:
  - `unimatrix-data` — databases, vector indexes, config, and logs (integrity-critical, back up frequently). ONNX models baked into image.

KEEP line 44 unchanged:
  Non-root container user. HEALTHCHECK on daemon liveness + schema version.
```

Key changes:
- Three named volumes replaced with single `unimatrix-data`.
- `unimatrix-knowledge`, `unimatrix-analytics`, `unimatrix-shared` all removed.
- Contents list: "databases, vector indexes, config, and logs".
- ONNX models noted as baked into image.
- nan-014 annotation added per ADR-004.
- "Named volumes:" (plural) changed to "Named volume" (singular).

## Constraints

- C-06: Edits constrained to W2-1 volume list only (lines 39-43).
- SR-04: Correct with annotation, do not rewrite other W2-1 content.
- ADR-004: Include "updated to reflect nan-014 shipped design" annotation.

## Error Handling

Not applicable (prose edit).

## Key Test Scenarios

1. Single `unimatrix-data` volume described.
2. No references to `unimatrix-knowledge`, `unimatrix-analytics`, or `unimatrix-shared`.
3. ONNX models described as baked into image.
4. Config described as living in the data volume.
5. nan-014 annotation present.
6. "Non-root container user" line unchanged.
7. PR diff shows changes only within lines 39-43.
