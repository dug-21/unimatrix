# col-014: Scope Risk Assessment

## SR-01: False Positive Increase in Text Extraction

**Severity**: Medium
**Likelihood**: Low

Relaxing validation from `{alpha}-{digits}` to "any safe string with a hyphen" increases the set of strings that `extract_feature_id_pattern` will match from free text. Hyphenated English words (e.g., "well-known", "re-use") could be extracted as feature IDs.

**Mitigation**: The `extract_feature_id_pattern` function splits on whitespace and delimiter characters, then calls `is_valid_feature_id` on each token. Hyphenated English words would match, but attribution requires the *same* token to appear consistently across records to establish a feature context. A one-off false positive in a single record does not create a partition -- it would need to appear as the dominant signal across a session. The `trim_matches` in `extract_feature_id_pattern` strips non-alphanumeric/non-hyphen characters, which further constrains candidates.

**Residual risk**: Acceptable. Attribution already tolerates noise -- the partition-based approach (FR-04.3 priority ordering: path > text pattern > git checkout) means path-based signals dominate.

## SR-02: Dot Character in Feature IDs

**Severity**: Low
**Likelihood**: Low

Allowing dots (e.g., `v2.1-migration`) means file extensions could partially match. A path like `test-file.rs` contains a hyphen and safe characters.

**Mitigation**: Path extraction (`extract_from_path`) looks for `product/features/` prefix and extracts the next path segment. The segment `test-file.rs` would pass `is_valid_feature_id` but would only match if that exact string appeared as a feature_cycle target in an `attribute_sessions` call. Since callers pass real feature IDs, this is a theoretical concern only.

**Residual risk**: Negligible. The function is private and only called from well-defined extraction pipelines.

## SR-03: Underscore Ambiguity

**Severity**: Low
**Likelihood**: Low

Allowing underscores means tokens like `my_variable` could match if they also contain a hyphen somewhere in context. Since hyphens are required, `my_variable` alone would not match.

**Mitigation**: Hyphen requirement eliminates pure-underscore identifiers. Only `underscore-hyphen` combinations match, which are plausible feature IDs.

**Residual risk**: Acceptable.

## Top 3 Risks for Architecture Attention

1. **SR-01**: False positive increase -- architect should confirm that attribution's partition-based approach tolerates the broader match set
2. **SR-02**: Dot character interaction with file paths -- architect should verify `extract_from_path` isolates path segments correctly
3. **SR-03**: Underscore ambiguity -- minor, no architect action needed
