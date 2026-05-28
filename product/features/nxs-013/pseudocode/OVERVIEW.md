# nxs-013 Pseudocode Overview

## Components

| ID | File | Target | Change Type |
|----|------|--------|-------------|
| C1 | C1-dockerfile-env.md | Dockerfile:128-131 | Line removal |
| C2 | C2-docker-compose-comments.md | docker-compose.yml:14-17 | Comment rewrite |
| C3 | C3-provenance-labels.md | main.rs:1347-1375 | String literal update |
| C4 | C4-readme-config.md | README.md:62, 238-243 | Prose update |
| C5 | C5-product-vision-w2-1.md | PRODUCT-VISION.md:448-458 | Prose update |
| C6 | C6-wave2-roadmap-w2-1.md | WAVE2-ROADMAP.md:39-44 | Prose update |
| C7 | C7-default-config-header.md | config.rs:3130-3138 | Comment update |

## Component Interactions

None. All 7 components are independent. No component produces output consumed by another. All can be implemented in parallel with no sequencing constraints.

## Shared Types

No new types introduced. No existing types modified.

Existing types consumed (read-only reference for C3):
- `ConfigProvenance { global: SourceStatus, project: SourceStatus, env_override: SourceStatus }` -- config.rs:2078
- `SourceStatus { Loaded { path }, NotFound { path }, NotApplicable }` -- config.rs:2067

## Data Flow

No data flows between components. Each component modifies a single file in isolation.

The only runtime data flow relevant to nxs-013 is the existing (unchanged) path:
```
load_config() -> ConfigLoadResult { config, provenance }
                                          |
                                          v
                              log_config_provenance(provenance)
                                          |
                                          v
                              tracing::info!/warn! output
```

C1 (Dockerfile) affects which `load_config` step activates at runtime by removing the `UNIMATRIX_CONFIG` ENV default. C3 (main.rs) changes the string literals emitted by `log_config_provenance`. Neither modifies the data flow itself.

## Build Order

No ordering required. All components can be delivered in a single commit.

## Verification Strategy

1. `cargo test --workspace` -- zero test file changes (covers C3, C7)
2. Docker build succeeds (covers C1)
3. `docker inspect` confirms ENV correctness (covers C1)
4. `docker compose config` validates YAML (covers C2)
5. PR diff review confirms edit boundaries (covers C4, C5, C6)
6. Code review confirms string-literal-only changes in `log_config_provenance` (covers C3)
7. Code review confirms comment-only changes in `DEFAULT_CONFIG_TOML` (covers C7)
