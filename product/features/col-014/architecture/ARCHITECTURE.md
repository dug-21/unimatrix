# col-014: Architecture

## Overview

Single-function fix in `crates/unimatrix-observe/src/attribution.rs`. No new modules, no cross-crate changes, no schema changes.

## Current State

```
is_valid_feature_id(s) -> bool
  Split on first hyphen
  Validate: prefix is all-alpha, suffix is all-digits
  Used by: extract_from_path, extract_feature_id_pattern, extract_from_git_checkout
```

## Target State

```
is_valid_feature_id(s) -> bool
  Check: non-empty
  Check: length <= 128
  Check: contains at least one hyphen
  Check: all chars are ASCII alphanumeric, hyphen, underscore, or dot
  Used by: extract_from_path, extract_feature_id_pattern, extract_from_git_checkout (unchanged)
```

## ADR-001: Permissive Feature ID Validation

**Context**: `is_valid_feature_id` enforces `{alpha}-{digits}` format. Unimatrix is domain-agnostic (ASS-009). The function is private to `attribution.rs` and used only for extracting feature signals from free text and file paths.

**Decision**: Replace structural format validation with permissive character/length gating. Retain hyphen requirement as the minimal structural constraint to distinguish feature-ID-like tokens from plain words in free text extraction.

**Allowed characters**: ASCII alphanumeric (`a-z`, `A-Z`, `0-9`), hyphen (`-`), underscore (`_`), dot (`.`).

**Max length**: 128 characters, consistent with `MAX_FEATURE_CYCLE_LEN` in `crates/unimatrix-server/src/infra/validation.rs`.

**Consequences**:
- Broader set of strings accepted as feature IDs
- False positive risk in text extraction mitigated by attribution's partition-based approach (SR-01)
- All previously-valid IDs remain valid
- Domain-agnostic: no project-specific conventions encoded

## Integration Surface

No integration changes. The function is private (`fn`, not `pub fn`). All callers are within `attribution.rs`. No API changes, no schema changes, no configuration changes.

## Interaction with SR Risks

- **SR-01 (false positives)**: The hyphen requirement is the key mitigation. Without it, every single word in free text would match. With it, only hyphenated tokens match, which is a reasonable proxy for structured identifiers.
- **SR-02 (dots in paths)**: `extract_from_path` extracts the path segment after `product/features/` up to the next `/`. File extensions like `.rs` won't appear in that segment since they're in deeper path components. The dot allowance is safe here.
