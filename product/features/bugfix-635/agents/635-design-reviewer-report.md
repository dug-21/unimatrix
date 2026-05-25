# Design Review Report — 635-design-reviewer

## Assessment: APPROVED WITH NOTES

## Findings

| # | Finding | Severity |
|---|---------|----------|
| F-1 | Log-level testability must be declared in spec (code-review-only recommended) | Non-blocking |
| F-2 | Effective-config audit should include source trail (global/project/env), not just values | Non-blocking |

## Review Detail

### Hot-Path Risk: CLEAR
`load_config` runs once at startup per entry point. All proposed changes are sub-microsecond tracing calls on a cold path. Item 4's `toml::from_str("")` is also cold-path only.

### Blast Radius: MINIMAL
Pure logging additions. No control flow, error path, or return value changes. Tracing format strings are compile-time checked.

### Architectural Fit: GOOD
- Log level choices (WARN for missing, INFO for present) match existing patterns in main.rs
- Effective-config audit belongs in `load_config` (not main.rs) — runs once regardless of entry point
- Per Unimatrix lesson #4147: spec must declare log-level verification method (recommend code-review-only)

### Missing Constraints: one amendment
- Effective-config audit should log which sources contributed (global/project/env), not just final values. Track via local `sources: Vec<&str>` in `load_config`.

### Security Surface: CLEAR
No new trust boundaries, no format injection risk, no secrets in logged values.

## Knowledge Stewardship

Queried:
- #2328 (dsn-001 config injection pattern) — confirmed load_config is cold-path, non-fatal fallback pattern
- #4147 (log-level AC testability lesson) — applied to F-1 recommendation
- #3463 (design corrections must reach spawn prompt) — noted for coordinator
- #3467 (UDS obs synthetic target ADR) — confirmed these new logs should NOT use synthetic target (they are operational, not diagnostic)
- #3817 (dual-default divergence pattern) — relevant to item 4; pattern already documented
- #2286 (ADR-003 merge semantics) — confirmed merge_configs architecture is stable

Declined to store: no new reusable pattern emerged. All findings reference existing Unimatrix knowledge.
