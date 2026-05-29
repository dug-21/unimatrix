# Test Plan: documentation

**Files**: `product/PRODUCT-VISION.md`, `product/WAVE2-ROADMAP.md`

## Two-Volume Description (R-15, Med)

### Test 1: No remaining baked-in model references

```
Name: verify_no_baked_in_references
Method: Grep both files.
Commands:
  grep -i "baked into" product/PRODUCT-VISION.md
  grep -i "baked into" product/WAVE2-ROADMAP.md
Assert:
  1. No line in PRODUCT-VISION.md describes ONNX models as "baked into" the image (in W2-1 context).
  2. No line in WAVE2-ROADMAP.md describes ONNX models as "baked into" the image.
  3. Any "(correct)" annotation next to model bake-in text has been removed or updated.
Risk: R-15
AC: AC-10
```

### Test 2: Two-volume model described

```
Name: verify_two_volume_description
Method: Grep both files.
Commands:
  grep -c "unimatrix-shared" product/PRODUCT-VISION.md
  grep -c "unimatrix-shared" product/WAVE2-ROADMAP.md
Assert:
  1. PRODUCT-VISION.md mentions `unimatrix-shared` at least once in the W2-1 section.
  2. WAVE2-ROADMAP.md mentions `unimatrix-shared` at least once.
  3. Both files describe the two-volume architecture: `unimatrix-data` for integrity-critical data, `unimatrix-shared` for re-downloadable models.
Risk: R-15
AC: AC-10
```

## Security Documentation (R-03, High)

### Test 3: Hash pinning guidance for shared volumes

```
Name: verify_hash_pinning_guidance
Method: Review docker-compose.yml comments and any updated documentation.
Assert:
  1. Documentation states operators should set `nli_model_sha256` config when using shared volumes.
  2. Documentation explicitly acknowledges embedding model SHA-256 enforcement is NOT yet available.
  3. Issue #651 is referenced as the tracking issue for embedding hash enforcement.
Risk: R-03
AC: AC-11
```

### Test 4: Read-only hardening guidance

```
Name: verify_ro_hardening_guidance
Method: Grep docker-compose.yml and documentation.
Assert:
  1. `:ro` mount option is documented as optional hardening after initial model population.
  2. Warning that `:ro` on an empty volume prevents auto-download is present or implied.
Risk: R-03
AC: AC-11
```

### Test 5: Security guidance does not mislead about coverage

```
Name: verify_no_false_security_assurance
Method: Documentation review.
Assert:
  1. No statement implies that ALL models are hash-verified on the shared volume.
  2. The gap between NLI (has hash) and embedding (no hash until #651) is explicit.
  3. An operator reading only the docker-compose.yml comments understands the current security posture.
Risk: R-03 -- "not misled about the embedding hash enforcement gap (SR-04)"
```

## Documentation Review Checklist

| Check | Files | AC |
|-------|-------|-----|
| No "baked into" in W2-1 context | PRODUCT-VISION.md, WAVE2-ROADMAP.md | AC-10 |
| "unimatrix-shared" mentioned | PRODUCT-VISION.md, WAVE2-ROADMAP.md | AC-10 |
| Two-volume separation explained | PRODUCT-VISION.md, WAVE2-ROADMAP.md | AC-10 |
| nli_model_sha256 guidance present | docker-compose.yml | AC-11 |
| #651 gap acknowledged | docker-compose.yml | AC-11 |
| :ro hardening documented | docker-compose.yml | AC-11 |
| No false security assurance | All docs | AC-11 |
