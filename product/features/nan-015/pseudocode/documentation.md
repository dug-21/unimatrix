# documentation -- Pseudocode

## Purpose

Update PRODUCT-VISION.md and WAVE2-ROADMAP.md W2-1 sections to describe the two-volume model (`unimatrix-data` + `unimatrix-shared`). Remove outdated "ONNX baked into image" references. This addresses AC-10 and R-15.

## Files

- `product/PRODUCT-VISION.md`
- `product/WAVE2-ROADMAP.md`

## Changes

### 1. PRODUCT-VISION.md -- W2-1 Section (lines 446-459)

**Current text** (lines 447-453):

```markdown
### W2-1: Container Packaging
**Business outcome**: Knowledge survives infrastructure changes — production-grade deployment with clean backup, recovery, and standard container lifecycle.

**What**: Dockerfile + docker-compose with a single named volume.
*(Updated to reflect nan-014 shipped design.)*
- `unimatrix-data` — databases, vector indexes, config, and logs (back up frequently; integrity-critical). ONNX models baked into image.

Container is stateless except the volumes. Backup = volume snapshot of `unimatrix-data`. `HEALTHCHECK` verifies daemon liveness and schema version currency.
```

**New text**:

```markdown
### W2-1: Container Packaging
**Business outcome**: Knowledge survives infrastructure changes — production-grade deployment with clean backup, recovery, and standard container lifecycle.

**What**: Dockerfile + docker-compose with two named volumes.
*(Updated to reflect nan-015 shipped design.)*
- `unimatrix-data` — databases, vector indexes, config, and logs (integrity-critical, back up frequently).
- `unimatrix-shared` — ONNX models (~166 MB, re-downloadable). Auto-populated on first start. Backup optional.

Container is stateless except the volumes. Backup = volume snapshot of `unimatrix-data`. `unimatrix-shared` can be reconstructed from HuggingFace Hub. `HEALTHCHECK` verifies daemon liveness and schema version currency.
```

Key changes:
- "single named volume" -> "two named volumes"
- "nan-014" -> "nan-015"
- Split volume description into two bullet points
- Remove "ONNX models baked into image" -- replaced with unimatrix-shared description
- Add note that unimatrix-shared is reconstructible

### 2. WAVE2-ROADMAP.md -- W2-1 Section (lines 36-43)

**Current text** (lines 36-43):

```markdown
### W2-1: Container Packaging (ASS-043)
**Goal**: Single-image personal cloud deployment. Containerized daemon with ONNX runtime. Air-gap deployable — no runtime internet dependencies.

Named volume *(updated to reflect nan-014 shipped design)*:
- `unimatrix-data` — databases, vector indexes, config, and logs (integrity-critical, back up frequently). ONNX models baked into image.

Non-root container user. HEALTHCHECK on daemon liveness + schema version.
```

**New text**:

```markdown
### W2-1: Container Packaging (ASS-043)
**Goal**: Single-image personal cloud deployment. Containerized daemon with ONNX runtime. Air-gap deployable via volume pre-population.

Named volumes *(updated to reflect nan-015 shipped design)*:
- `unimatrix-data` — databases, vector indexes, config, and logs (integrity-critical, back up frequently).
- `unimatrix-shared` — ONNX models (~166 MB, re-downloadable from HuggingFace Hub). Auto-populated on first start. Backup optional.

Non-root container user. HEALTHCHECK on daemon liveness + schema version.
```

Key changes:
- "nan-014" -> "nan-015"
- "Named volume" -> "Named volumes" (plural)
- "no runtime internet dependencies" -> "Air-gap deployable via volume pre-population" (more accurate -- first run does need internet unless pre-populated)
- Split volume description into two bullet points
- Remove "ONNX models baked into image"

## Error Handling

N/A -- documentation-only changes. No runtime behavior.

## V-01 Alignment Warning Resolution

The Implementation Brief flags V-01: "SPECIFICATION C-04 may need ordering correction to match architecture (config field > env var > dirs > fallback)."

**Finding**: SPECIFICATION C-04 (lines 258-266) already lists the correct ordering:
1. `EmbedConfig.cache_dir` config field (highest priority)
2. `UNIMATRIX_MODEL_CACHE` env var
3. `dirs::cache_dir()` platform default
4. `.unimatrix/models` final fallback

The Domain Models section (line 171) also shows the correct ordering in the `resolve_cache_dir()` description.

**Conclusion**: V-01 is resolved. No specification correction needed. The specification text matches the architecture.

## Key Test Scenarios

### T-01: No "baked into image" references remain (R-15)

```
grep -i "baked into" product/PRODUCT-VISION.md product/WAVE2-ROADMAP.md
# Must return no matches.
```

### T-02: Two-volume description present in both files (AC-10)

```
grep "unimatrix-shared" product/PRODUCT-VISION.md product/WAVE2-ROADMAP.md
# Both files must mention unimatrix-shared.
```

### T-03: "nan-015" annotation replaces "nan-014" in W2-1

```
grep "nan-015" product/PRODUCT-VISION.md product/WAVE2-ROADMAP.md
# Both files must reference nan-015 in W2-1 section.
grep "nan-014 shipped design" product/PRODUCT-VISION.md product/WAVE2-ROADMAP.md
# Must return no matches in W2-1 context (nan-014 may still appear elsewhere).
```

### T-04: Volume descriptions consistent across all three files

```
# docker-compose.yml, PRODUCT-VISION.md, and WAVE2-ROADMAP.md must all describe:
#   unimatrix-data: integrity-critical (databases, indexes, config, logs)
#   unimatrix-shared: re-downloadable ONNX models
# No file should contradict the others.
```

### T-05: Air-gap language accurate

```
# WAVE2-ROADMAP.md must NOT say "no runtime internet dependencies"
# (first run requires internet unless volume is pre-populated).
# Must say "air-gap deployable via volume pre-population" or equivalent.
```
