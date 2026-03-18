# Agent Report: dsn-001-agent-2-spec

## Output

- `/workspaces/unimatrix/product/features/dsn-001/specification/SPECIFICATION.md`

## Key Decisions Made

**D-01: List fields replace, never append (SR-06 resolution)**
FR-003 specifies that list fields (`categories`, `boosted_categories`, `session_capabilities`) are replaced entirely by a per-project value, never appended. This is the least-surprising operator behaviour: a per-project categories list is always authoritative for that project. The intermediate deserialization uses `Option<Vec<T>>` to distinguish "not specified" from "specified as default".

**D-02: `[confidence]` and `[cycle]` sections reserved as empty stubs (SR-04 resolution)**
FR-008 and FR-009 specify that `ConfidenceConfig` and `CycleConfig` are empty structs with `Default` impls, present in `UnimatrixConfig` but consuming no fields. This is the 10-line hedge that prevents a config format break when W3-1 adds `[confidence] weights`. The TOML sections are parseable now; operators can write empty `[confidence]` blocks in config without error.

**D-03: AC-13 grep verification is the authoritative completeness gate for the rename**
The rename checklist in the SR-05 section enumerates 22 specific locations across Rust, Python, Markdown, and skill files. The post-rename `grep -r "context_retrospective"` check is required before the PR is considered complete. Historical feature docs (archival records) are acceptable exceptions.

**D-04: `validate_config()` is a single function, not per-section functions**
All post-parse validation runs in one `validate_config()` call immediately after deserialization, consistent with the `validate_*` naming convention observed in `infra/validation.rs`. This ensures the ContentScanner initialization ordering concern (OQ-02) is localized to one call site.

**D-05: `CategoryAllowlist::new()` delegates to `new_from_config(&Default::default())`**
FR-013 requires this to prevent test/production divergence (SR-07). Tests calling `new()` exercise the same code path as production config-driven construction.

## Open Questions for Architect (copied from spec)

- **OQ-01 (High)**: `ConfidenceParams` struct vs. bare `f64` parameter for `freshness_score()`. Struct recommended to absorb W3-1 without further API churn.
- **OQ-02 (Med)**: `ContentScanner::global()` initialization ordering vs. config load. Must be confirmed before implementation.
- **OQ-03 (Med)**: Config type crate placement — `unimatrix-server/src/infra/config.rs` vs. thin shared crate. Passing plain `Vec<Capability>` across crate boundary avoids dependency issue.
- **OQ-04 (Low, resolved in spec)**: List merge semantics confirmed as replace, not append. Architect should validate this matches intended operator UX.

## Knowledge Stewardship

Queried: `/uni-query-patterns` for config externalization, TOML startup validation — no results (Unimatrix MCP server not callable from this agent context; codebase read directly as secondary evidence).
