# Test Plan: C7 -- DEFAULT_CONFIG_TOML Header

## Component

Update the `DEFAULT_CONFIG_TOML` header comment (~lines 3130-3138 in config.rs) to emphasize per-project as canonical and global as defaults.

## Risks Covered

- **R-07** (High): Header comment edit corrupts TOML template content

## Unit Test Expectations

### Existing Tests (Must Pass Unchanged)

The config parsing tests in config.rs exercise `DEFAULT_CONFIG_TOML` by parsing it through the TOML parser. If the header edit introduces a syntax error (missing `#` prefix, stray characters entering the template body), these tests will fail.

**Specific test expectations**:
- All config parsing tests that call `toml::from_str(DEFAULT_CONFIG_TOML)` or equivalent must pass
- All `write_default_config_if_absent` tests must pass (they write the template then parse it back)
- All `load_config` tests that exercise the default config path must pass

**Expected result**: `cargo test --workspace` passes with zero test file changes. R-07 is fully covered by existing automated tests.

## Code Review Checklist

- [ ] All changed lines are `#`-prefixed TOML comments in the header block
- [ ] No changes to template body content (field definitions, values, inline comments)
- [ ] Per-project described as "canonical" or "primary" configuration
- [ ] Global described as "defaults" or "optional cross-project defaults layer"
- [ ] Replace semantics for list fields still documented
- [ ] No stray characters outside comment lines
- [ ] Header block remains valid TOML when the `#` prefixes are considered comments

## Content Assertions

- [ ] String `canonical` or `primary` appears in the header referencing per-project config (AC-07)
- [ ] String `defaults` appears in the header referencing global config (AC-07)
- [ ] String `~/.unimatrix/{hash}/config.toml` or `per-project` path notation present
- [ ] String `~/.unimatrix/config.toml` or `global` path notation present for the defaults layer

## Manual Verification (AC-07)

### MV-04: Generated Config Header
- **Arrange**: Run `unimatrix config` (or equivalent) to generate a default config file
- **Act**: Inspect the generated file header
- **Assert**: Header contains canonical/defaults language matching the updated `DEFAULT_CONFIG_TOML`

## Edge Cases

- **Missing `#` on a comment line**: Would introduce a TOML parse error. Caught by existing config parsing tests (cargo test).
- **Edit extends past header into template body**: Would change config field defaults. Caught by existing config parsing tests.
- **Trailing whitespace or BOM**: Unlikely to cause issues in TOML but could affect byte-for-byte comparison tests if any exist. Existing tests parse, not byte-compare.
- **Multi-line comment with continuation**: If the header uses `#` on each line (standard TOML), each line is independent. No continuation syntax to break.
