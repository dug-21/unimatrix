## ADR-001: README as Single File with Capability-First Section Order

### Context

nan-005 must rewrite the README to serve external users — evaluators, first-time adopters, and operators. Two structural questions arise: (1) should documentation be split across multiple files in a `docs/` directory, or kept as a single README.md, and (2) how should sections be ordered?

The existing README is developer-focused and internally-ordered (architecture before getting started, features listed as checkboxes). This ordering serves contributors but fails evaluators who need to assess value before committing to setup.

The risk assessment (SR-03) flagged that 11 sections covering 11 tools + 14 skills + categories + CLI + architecture might exceed practical single-file size. Estimated line count: 450–650 lines. GitHub renders markdown with heading anchors, making a single file navigable at this size.

### Decision

README.md remains a single file. Section order is capability-first:

1. Hero (what it is)
2. Why Unimatrix (problem + differentiators)
3. Core Capabilities (what users can do)
4. Getting Started (how to install and configure)
5. Tips for Maximum Value (operational guidance)
6. MCP Tool Reference (11 tools)
7. Skills Reference (14 skills)
8. Knowledge Categories (8 categories)
9. CLI Reference (5 subcommands)
10. Architecture Overview (high-level only)
11. Security Model (user-facing summary)

No `docs/` directory is created. The documentation agent (Component 2) updates this single file incrementally — keeping maintenance tractable for a single agent reading two input artifacts.

### Consequences

- Single file is the maintenance target. The documentation agent never needs to route edits across multiple files.
- Navigability via GitHub heading anchors. Users can link to `#mcp-tool-reference` directly.
- Architecture and security sections are intentionally minimal — they exist to satisfy evaluator questions, not to document internals. Internal architecture details stay in feature docs.
- If the README grows beyond ~800 lines due to future features, a future feature (not nan-005) may split it. At current surface area (11 tools + 14 skills + 8 categories + 5 CLI subcommands), 650 lines is the upper bound.
