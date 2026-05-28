# Test Plan: C6 -- WAVE2-ROADMAP.md W2-1

## Component

Update W2-1 volume list (~lines 39-43) to match shipped single-volume model. Add correction annotation.

## Risks Covered

- **R-04** (Med): Documentation edit scope creep beyond W2-1 volume list

## Unit Test Expectations

No unit tests apply. WAVE2-ROADMAP.md is documentation only.

## PR Diff Review

### DR-05: Edit Boundary Enforcement
- **Act**: `git diff product/WAVE2-ROADMAP.md`
- **Assert**: Changes are ONLY within the W2-1 volume list lines (~39-43). No lines outside this range are modified.
- **Risk**: R-04

### DR-06: Content Correctness
- **Assert**: Single `unimatrix-data` volume described
- **Assert**: Three named volumes (`unimatrix-knowledge`, `unimatrix-analytics`, `unimatrix-shared`) replaced
- **Assert**: ONNX models noted as baked into image
- **Assert**: Config noted as living in data volume
- **Risk**: AC-06

### DR-07: Correction Annotation
- **Assert**: Text includes annotation such as "Updated to reflect nan-014 shipped design" (per ADR-004)
- **Risk**: R-04

## Code Review Checklist

- [ ] Changes constrained to W2-1 volume list only (SR-04)
- [ ] No edits to W2-1 content outside the volume list (goals, acceptance criteria, etc.)
- [ ] No edits to W2-2 through W2-8 sections
- [ ] Single volume replaces multi-volume list
- [ ] Correction annotation present per ADR-004
- [ ] Prose reads naturally

## Edge Cases

- **Stale line numbers**: Lines (~39-43) are approximate. Implementor must locate by content.
- **Table/list formatting**: If the volume list is a markdown table or bullet list, the replacement must match the surrounding format.
