# crt-036-docs Agent Report

## Agent ID
crt-036-docs

## Task
Update README.md after delivery of crt-036 (Intelligence-Driven Retention Framework, #409).

## Artifacts Read
- `product/features/crt-036/SCOPE.md`
- `product/features/crt-036/specification/SPECIFICATION.md`
- `README.md`

## Sections Modified

### 1. Configuration — Key Config Sections (TOML block)
Added the `[retention]` block with all three operator-configurable parameters:
- `activity_detail_retention_cycles` (default 50, range [1, 10000])
- `max_cycles_per_tick` (default 10, range [1, 1000])
- `audit_log_retention_days` (default 180, range [1, 3650])

Each parameter includes its documentation comment and range, matching FR-11 of SPECIFICATION.md exactly. The block was inserted inside the existing `[inference]` TOML code block (before the closing fence and the `[observation]` block), consistent with the config file ordering.

### 2. Tips for Maximum Value — Tip #11 added
Added tip #11: "Run retrospectives to advance the retention window." Documents the operational constraint that the K-window only advances past reviewed cycles, so skipping `context_cycle_review` causes raw signal data to accumulate without bound. References `context_status` `pending_cycle_reviews` field for visibility. This traces directly to SCOPE.md Goal 6 (gate on `cycle_review_index`) and AC-04 (documented operational constraint).

## Sections Not Modified
- MCP Tool Reference — no new tools were added
- Skills Reference — no new skills were added
- Knowledge Categories — no new categories were added
- CLI Reference — no new subcommands or flags were added
- Core Capabilities — feature is internal GC machinery, not a user-facing capability
- Architecture Overview — no schema migration, no new crates, no storage layout changes

## Commit
Hash: `23b5576`
Message: `docs: update README for crt-036 (#409)`

## Fallback Chain Used
None. Both SCOPE.md and SPECIFICATION.md were present and sufficient.
