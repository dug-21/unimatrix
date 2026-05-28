# C7: DEFAULT_CONFIG_TOML Header

## Purpose

Update the `DEFAULT_CONFIG_TOML` header comment to emphasize per-project as the canonical, primary configuration and global as the optional defaults layer.

## Target File

`crates/unimatrix-server/src/infra/config.rs`, `DEFAULT_CONFIG_TOML` static string, lines 3130-3138.

## Current State

```rust
pub static DEFAULT_CONFIG_TOML: &str = r#"# Unimatrix configuration file.
# All values shown are the compiled defaults. Uncomment and edit to override.
# File: ~/.unimatrix/{project-hash}/config.toml
#
# Two-level config hierarchy:
#   ~/.unimatrix/config.toml           -- global (applies to all projects)
#   ~/.unimatrix/{hash}/config.toml    -- per-project (overrides global)
# Per-project values replace global values field-by-field (replace semantics).
# List fields replace entirely — no append.
```

## Pseudocode

```
REPLACE the header comment block (lines 3130-3138, the content inside the raw string 
before the first "# ---" separator) WITH:

  # Unimatrix configuration file.
  # All values shown are the compiled defaults. Uncomment and edit to override.
  # File: ~/.unimatrix/{project-hash}/config.toml
  #
  # This is the PRIMARY (per-project) configuration. It is the canonical config
  # location, written automatically on first run.
  #
  # Two-level config hierarchy:
  #   ~/.unimatrix/{hash}/config.toml    -- primary (per-project, this file)
  #   ~/.unimatrix/config.toml           -- defaults (global, optional)
  # Per-project values replace global values field-by-field (replace semantics).
  # List fields replace entirely — no append.
```

Key changes:
- ADD two lines after "# File:" establishing this file as PRIMARY and canonical.
- SWAP the order of the hierarchy listing: per-project first, global second.
- RELABEL per-project as "primary (per-project, this file)".
- RELABEL global as "defaults (global, optional)".
- PRESERVE existing explanation of replace semantics and list field behavior.
- PRESERVE the "# File:" line unchanged.

## What Does NOT Change

- The `pub static DEFAULT_CONFIG_TOML: &str = r#"` declaration.
- Any TOML content after the header (section comments, field comments, field values).
- The replace semantics explanation.
- The `r#"..."#` raw string delimiters.

## Constraints

- C-05: `ConfigLoadResult`, `ConfigProvenance`, `SourceStatus` types NOT modified.
- R-07: Changes MUST be limited to `#`-prefixed comment lines in the header. No TOML template body content is modified. A stray character entering the template body would corrupt config parsing.
- Every line in the header block MUST start with `#` (TOML comment prefix).

## Error Handling

Not applicable (comment-only change). Risk R-07 (template corruption) mitigated by:
1. All changed lines begin with `#`.
2. Existing config parsing tests exercise `DEFAULT_CONFIG_TOML` and will fail if TOML is corrupted.

## Key Test Scenarios

1. `cargo test --workspace` passes -- confirms `DEFAULT_CONFIG_TOML` still parses correctly.
2. Code review: all changes are in `#`-prefixed comment lines.
3. Code review: no changes to TOML template body below the header.
4. Per-project listed first in hierarchy, labeled "primary".
5. Global listed second in hierarchy, labeled "defaults (global, optional)".
6. `unimatrix config` generates a file with the updated header.
