# Test Plan: C5 -- PRODUCT-VISION.md W2-1

## Component

Update W2-1 volume description (~lines 448-459) to reflect the single-volume model shipped by nan-014. Update the `[Medium]` security requirement to replace bind-mount guidance with env var injection.

## Risks Covered

- **R-04** (Med): Documentation edit scope creep beyond W2-1 volume section

## Unit Test Expectations

No unit tests apply. PRODUCT-VISION.md is documentation only.

## PR Diff Review

### DR-01: Edit Boundary Enforcement
- **Act**: `git diff product/PRODUCT-VISION.md`
- **Assert**: Changes are ONLY within the W2-1 volume description lines (~448-459). No lines outside this range are modified.
- **Risk**: R-04

### DR-02: Content Correctness
- **Assert**: Single `unimatrix-data` volume described (not two volumes)
- **Assert**: No reference to `unimatrix-shared` for config
- **Assert**: ONNX models described as baked into the image
- **Assert**: Config described as living in the data volume
- **Assert**: Backup = volume snapshot statement preserved
- **Risk**: AC-05

### DR-03: Security Requirement Update
- **Assert**: `[Medium]` security requirement no longer references "config.toml as read-only bind mount from secrets manager"
- **Assert**: Replacement text mentions `UNIMATRIX_CONFIG` env var for sensitive config injection
- **Risk**: AC-05

### DR-04: Correction Annotation
- **Assert**: Text includes annotation referencing nan-014 shipped design (per ADR-004)
- **Risk**: R-04

## Code Review Checklist

- [ ] Changes constrained to W2-1 volume description only (SR-03)
- [ ] No edits to adjacent W2-2 through W2-8 sections
- [ ] No edits to Vision Goals, Architecture Principles, or other sections
- [ ] Single `unimatrix-data` volume replaces multi-volume description
- [ ] `unimatrix-shared` reference removed
- [ ] `[Medium]` security requirement updated per FR-05
- [ ] Correction annotation present per ADR-004
- [ ] Prose reads naturally -- no orphaned references to removed content

## Edge Cases

- **Stale line numbers**: The lines (~448-459) are approximate. If prior edits shifted content, the implementor must locate the W2-1 volume description by content, not line number.
- **Adjacent section contamination**: A W2-1 edit that accidentally extends into the W2-2 section header or content. DR-01 catches this via diff boundary check.
