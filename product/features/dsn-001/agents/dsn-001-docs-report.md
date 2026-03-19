# Agent Report: dsn-001-docs

Agent ID: dsn-001-docs
Feature: dsn-001 — Config Externalization (W0-3)
Issue: #306

## Sections Modified

### 1. New section added: "Configuration" (between "Tips for Maximum Value" and "MCP Tool Reference")

Added a new top-level section covering:
- Two-level TOML file locations (`~/.unimatrix/config.toml` global, `~/.unimatrix/{hash}/config.toml` per-project)
- Merge semantics (per-project shadows global, list fields replace not append)
- Restart-required note and abort-on-violation startup behavior
- Profile preset table: collaborative / authoritative / operational / empirical / custom with best-for and freshness half-life columns
- Key config sections ([knowledge], [server], [agents]) with representative TOML and inline comments
- Security note on world-writable abort, group-writable warning, and instructions injection scan

### 2. Architecture Overview — Data Layout block updated

Added `~/.unimatrix/` parent block with `config.toml` (global, optional) and `config.toml` under `{project-hash}/` (per-project override, optional). Previously neither path appeared in the data layout.

### 3. Knowledge Categories — closing sentence updated

Replaced "The category allowlist is runtime-extensible via `add_category()`, but the 8 built-in categories cover the primary use cases." with a sentence documenting that the default list is replaceable via `[knowledge] categories` in `config.toml`, and that the 8 built-in categories cover software delivery while other domains can supply domain-appropriate lists. The prior sentence implied the only path to new categories was via `add_category()` at runtime.

### 4. MCP Tool Reference — context_cycle_review verified correct

The tool-rename agent had already updated line 218 to `context_cycle_review`. Verified: the table row reads `context_cycle_review` and no reference to `context_retrospective` is present in the MCP Tool Reference section. No edit required.

## Commit

Hash: 0254eda
Message: docs: update README for dsn-001 config externalization (#306)

## Self-Check

- [x] Read SCOPE.md before making any edits
- [x] Read SPECIFICATION.md (used as preferred source for interface details)
- [x] Read current README.md to identify affected sections
- [x] All edits trace to specific claims in SCOPE.md / SPECIFICATION.md
- [x] No source code was read
- [x] Only README.md was modified
- [x] Commit message uses `docs:` prefix
- [x] No aspirational language added
- [x] Terminology consistent: Unimatrix, context_search, context_cycle_review, SQLite
- [x] Preset weight table values match SPECIFICATION.md AC-23 table exactly
- [x] Knowledge Stewardship: Exempt (README documentation agent, no storage/query expected)
