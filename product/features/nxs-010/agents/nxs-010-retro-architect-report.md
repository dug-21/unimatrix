# nxs-010 Retrospective Architect Report

Agent: nxs-010-retro-architect
Date: 2026-03-10
Feature: nxs-010 (Activity Schema Evolution)
Mode: Retrospective (post-ship knowledge extraction)

## 1. Patterns

### New Entries

| ID | Title | Rationale |
|----|-------|-----------|
| #837 | Store CRUD Module Structure for New Tables | Codifies the module layout used by injection_log, sessions, topic_deliveries, and query_log (4 modules across 2 features). Covers file structure, row helpers, column constants, Store impl block, test scaffolding. |
| #838 | Shared Constructor for Dual-Transport Record Construction | New pattern from nxs-010. When UDS and MCP paths must produce identical records, a shared constructor on the record type (in the store crate) prevents field divergence. Validated by QueryLogRecord::new(). |

### Updated Entries

| Original ID | New ID | What Changed |
|-------------|--------|-------------|
| #390 | #836 | Corrected migration guard from `==` to `<`. Added: version update happens once at end (not per block), backfill guidance (INSERT OR IGNORE), transaction scope guidance (additive = main tx, destructive = separate tx). Validated with nxs-010 evidence. |

### Skipped (No Action Needed)

| Pattern Area | Reason |
|-------------|--------|
| #731 (Batched fire-and-forget) | Still accurate. nxs-010's separate spawn_blocking for query_log is within the pattern's scope -- the pattern warns against N tasks x M concurrent requests, and both UDS/MCP are sequential. Architecture explicitly justified this. |
| #375 (Database init ordering) | Still accurate. nxs-010 followed it exactly (migrate_if_needed before create_tables). |
| #619 (How to add new fields) | Not applicable to nxs-010 (new tables, not new columns). Content is accurate. |
| #681 (Create-new-then-swap migration) | Not applicable to nxs-010 (additive migration, no destructive DDL). Content is accurate. |
| #620 (Idempotent ALTER TABLE guard) | Not applicable to nxs-010. Content is accurate. |

## 2. Procedures

### Updated

| Original ID | New ID | What Changed |
|-------------|--------|-------------|
| #390 | #836 | See Patterns section above. This is categorized as a procedure in Unimatrix. |

### No New Procedures

No new build/test/integration procedures emerged. The schema migration steps were already documented (now corrected). No new techniques beyond what existing procedures cover.

## 3. ADR Validation

All three nxs-010 ADRs were validated by successful implementation with zero rework.

| ADR ID | Title | Status | Evidence |
|--------|-------|--------|----------|
| #818 | AUTOINCREMENT for query_log PK | Validated | query_log uses INTEGER PRIMARY KEY AUTOINCREMENT in both migration.rs and db.rs. test_insert_query_log_autoincrement confirms monotonic allocation. No counter added. Decision boundary (AUTOINCREMENT for telemetry, counters for entities) holds. |
| #819 | Fire-and-forget for query_log writes | Validated | Both UDS (spawn_blocking_fire_and_forget) and MCP (spawn_blocking with dropped handle) implement fire-and-forget with warn-level logging. UDS guards on empty session_id; MCP always writes. Zero latency impact confirmed by gate-3c. |
| #820 | Backfill in main migration transaction | Validated | Backfill INSERT OR IGNORE runs within the main BEGIN IMMEDIATE transaction. test_migration_v10_to_v11_basic confirms correct aggregates. test_migration_v10_to_v11_idempotent confirms re-run safety. No separate transaction needed. |

No ADRs flagged for supersession. All decisions held through implementation.

## 4. Lessons

No lessons extracted. Zero gate failures, zero rework commits, zero scope failures. The 12 Bash permission retries in the retrospective data were operational friction (sandbox environment), not architectural. No architectural lessons to store.

## 5. Observations

### Clean execution signal
3 sessions, 858 tool calls, zero gate failures, zero rework. 5 components built by 5 parallel agents with no merge conflicts or interface mismatches. The design-first approach with explicit Integration Surface tables prevented the kind of interface drift that normally requires rework in parallel builds.

### Procedure #390 had a latent error
The migration guard was documented as `==` instead of `<`. This has been wrong since col-012 (when #390 was created) but did not cause implementation failures because agents also consulted the codebase directly. The correction (#836) now matches actual code semantics. This is a reminder that procedure entries derived from a single feature may contain implementation-specific assumptions that generalize incorrectly.

### Pattern #731 (batched fire-and-forget) scope is important
nxs-010 deliberately deviated from #731's "one spawn_blocking per logical operation" guidance by adding a separate spawn_blocking for query_log writes. The architecture documented why this was acceptable (sequential processing, bounded concurrency). The pattern's constraint is about concurrent load, not absolute task count. Future features should read #731 carefully -- the key variable is "N tasks x M concurrent requests," not "N tasks per request."

### Store module structure is now a four-instance pattern
With nxs-010 adding topic_deliveries and query_log, there are now 4 modules following the same layout (injection_log, sessions, topic_deliveries, query_log). Pattern #837 captures this for future table additions. The structure is stable enough to be a template.
