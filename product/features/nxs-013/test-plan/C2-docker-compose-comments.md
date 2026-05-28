# Test Plan: C2 -- docker-compose.yml Comments

## Component

Replace the config bind-mount comment block with per-project config explanation, commented UNIMATRIX_CONFIG env var example, and backup guidance.

## Risks Covered

- **R-08** (Med): docker-compose.yml YAML syntax error

## Unit Test Expectations

No unit tests apply. docker-compose.yml is not exercised by `cargo test`.

## Container Verification

### CV-05: YAML Syntax Validation
- **Arrange**: Modified docker-compose.yml with new comment block
- **Act**: `docker compose -f docker-compose.yml config`
- **Assert**: Command succeeds (exit code 0), valid YAML output produced
- **Risk**: R-08

### CV-06: Commented UNIMATRIX_CONFIG Example Is Valid When Uncommented
- **Arrange**: Manually uncomment the UNIMATRIX_CONFIG environment variable example
- **Act**: `docker compose -f docker-compose.yml config`
- **Assert**: YAML still validates with the env var uncommented. The `environment:` block is syntactically correct.
- **Risk**: R-08 (edge case)

## Code Review Checklist

- [ ] No reference to `/etc/unimatrix/config.toml` bind mount remains
- [ ] Comments explain per-project config lives in the data volume
- [ ] Comments mention `write_default_config_if_absent` auto-creates config on first run
- [ ] Commented UNIMATRIX_CONFIG env var example present for advanced use
- [ ] Backup guidance present: backup = snapshot unimatrix-data volume
- [ ] All changes are comment-only (lines starting with `#`) or in the `environment:` section
- [ ] No structural YAML modifications (services, volumes, networks unchanged)
- [ ] Indentation consistent with rest of file
- [ ] Comments target new users (ADR-003: explain correct pattern, not migration path)

## Content Assertions

- [ ] String `/etc/unimatrix/` does NOT appear anywhere in the file (AC-02)
- [ ] String `bind` or `bind-mount` does NOT appear in config-related comments
- [ ] String `unimatrix-data` appears in backup guidance (AC-08)
- [ ] String `UNIMATRIX_CONFIG` appears in a commented environment example

## Edge Cases

- **Indentation mismatch**: YAML is whitespace-sensitive. Comment indentation does not affect parsing, but the commented env var example must have correct indentation if uncommented.
- **Stray unquoted colon in comments**: YAML comments (lines starting with `#`) are safe from parsing. Only risk is if a `#` is accidentally omitted.
