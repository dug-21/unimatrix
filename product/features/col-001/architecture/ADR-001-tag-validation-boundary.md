## ADR-001: Tag Validation Boundary

### Context

col-001 introduces structured outcome tags (`key:value` format with recognized keys). The question is where tag validation logic should live: in the store crate (which manages all data) or in the server crate (which handles MCP tool semantics).

The store crate is domain-agnostic by design (CLAUDE.md constraint). It treats tags as opaque `Vec<String>` and provides no semantic interpretation. The category allowlist, content scanning, and input validation already live in the server crate.

### Decision

All structured tag parsing and validation for outcome entries lives in a new server crate module (`outcome_tags.rs`). The store crate remains tag-agnostic.

The validation module:
- Defines `OutcomeTagKey` enum (`Type`, `Gate`, `Phase`, `Result`, `Agent`, `Wave`)
- Defines `WorkflowType` enum (`Feature`, `Bugfix`, `Incident`, `Process`)
- Defines `OutcomeResult` enum (`Pass`, `Fail`, `Rework`, `Skip`)
- Provides `validate_outcome_tags(tags: &[String]) -> Result<(), ServerError>`
- Is called from `context_store` only when `category == "outcome"`

### Consequences

- **Easier**: Store crate stays simple and domain-agnostic. Tag validation is testable in isolation. Adding new outcome tag keys requires only server crate changes.
- **Harder**: The OUTCOME_INDEX table lives in the store crate but its population logic lives in the server crate. This split requires the server crate to open the OUTCOME_INDEX table directly in `insert_with_audit`. This is consistent with how FEATURE_ENTRIES is managed (defined in store, populated by server).
