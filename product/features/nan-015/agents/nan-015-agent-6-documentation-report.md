# Agent Report: nan-015-agent-6-documentation

## Task
Update PRODUCT-VISION.md and WAVE2-ROADMAP.md W2-1 sections to replace "ONNX baked into image" with two-volume architecture (unimatrix-data + unimatrix-shared).

## Files Modified
- `product/PRODUCT-VISION.md` — W2-1 section (lines 445-453)
- `product/WAVE2-ROADMAP.md` — W2-1 section (lines 36-43)

## Changes Applied

### PRODUCT-VISION.md
- "single named volume" -> "two named volumes"
- "nan-014 shipped design" -> "nan-015 shipped design"
- Removed "ONNX models baked into the image" from unimatrix-data bullet
- Added unimatrix-shared bullet: ONNX models (~166 MB, re-downloadable), auto-populated, backup optional
- Added note that unimatrix-shared can be reconstructed from HuggingFace Hub

### WAVE2-ROADMAP.md
- "nan-014 shipped design" -> "nan-015 shipped design"
- "Named volume" -> "Named volumes" (plural)
- "no runtime internet dependencies" -> "Air-gap deployable via volume pre-population"
- Removed "ONNX models baked into image" from unimatrix-data bullet
- Added unimatrix-shared bullet with HuggingFace Hub source reference

## Test Results

| Test | Result |
|------|--------|
| T-01: No "baked into" references in either file | PASS |
| T-02: unimatrix-shared mentioned in both files | PASS (PRODUCT-VISION: 2, WAVE2-ROADMAP: 1) |
| T-03: nan-015 annotation replaces nan-014 | PASS |
| T-04: Volume descriptions consistent | PASS (manual review) |
| T-05: Air-gap language accurate (no "no runtime internet dependencies") | PASS |

## Tests
N/A -- documentation-only changes. No cargo tests.

## Issues
None.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced #4653 (nxs-013 prior correction to these same files), #4650 (ADR-001 env var redirect), #4652/#4651 (ADR-002/003). Applied: followed the same documentation update pattern from nxs-013.
- Stored: nothing novel to store -- documentation edit following established pattern from nxs-013 (#4653). No new gotchas or conventions discovered.
