# Agent Report — vnc-047-agent-1-architect

Task: architecture + ADRs for vnc-047 (`context_cycle` set-once opaque tags in a new `cycle_tags`
junction, surfaced in `context_cycle_review`). GH #940.

## Deliverables (rev2 — folds in the three human decisions of 2026-07-09)
- `product/features/vnc-047/architecture/ARCHITECTURE.md`
- ADR-001 `cycle-tags-junction-and-schema-v31.md` — store #5651
- ADR-002 `set-once-write-on-hook-cycle-start-transaction.md` — **corrected #5652 → #5658** (whole-set-once; edge Prerequisite → #5599 carried forward)
- ADR-003 `fire-and-forget-durability-envelope-absent-session.md` — store #5653
- ADR-004 `retrospectivereport-tags-surface-and-summary-v6.md` — store #5654
- ADR-005 `gc-protection-by-omission.md` — store #5655
- ADR-006 `deferred-mutation-home-on-context-tag.md` — store #5656
- ADR-007 `ack-echo-best-effort.md` — store #5659 (NEW)

## Rev2 changes (human decisions)
1. **Whole-set-once** replaces per-row accumulate: write path now does a set-level EXISTS guard inside
   a BEGIN IMMEDIATE txn — first tag-bearing start freezes the whole set; later same/subset/superset/
   different sets are wholesale no-ops; a tagless start never burns the one-shot. Preserves
   value-opacity (no namespace parsing — the reason per-key write-once was rejected). ADR-002
   rewritten + `#5652` context_corrected → `#5658`.
2. **Run-identity labels** reframe (not workflow-only): ARCHITECTURE.md + ADR-002 reworded; examples
   now workflow version / run mode / confidence-required / arm.
3. **Ack echo (ADR-007, feasible — nothing dropped):** (a) best-effort tag phrase in the EXISTING
   context_cycle ack (Start→accepted-for-recording; non-start→ignored), built from `cycle_type` +
   `params.tags`, no `phase` read, no new interface; (b) frozen-skip vs wrote-set is listener-only →
   surfaced as `tracing`, not returned to the caller.

ADR-001/003/004/005 checked for "accumulate"/"first-write-wins-per-row" phrasing — none present; no
correction needed. GC-by-omission and no-per-tag-audit-event notes retained.

## Key decisions
- Two version cascades modeled distinctly (SR-01): ADR-001 = `CURRENT_SCHEMA_VERSION` 30→31 (real DB
  migration, 3 paths); ADR-004 = `SUMMARY_SCHEMA_VERSION` 5→6 (fidelity stamp, no DB migration).
- `cycle_tags(feature_cycle, tag)` junction = source of truth; no FK (free-text cycle id).
- Write rides the hook cycle_start path in ONE new transactional primitive
  `insert_cycle_start_with_tags`; `insert_cycle_event` (15 call sites) untouched; `ON CONFLICT DO
  NOTHING` idempotency (SR-05 first-write-wins). No second persistence route (SR-03).
- SR-07: tags persist iff payload `feature_cycle` non-empty — independent of registry presence; #519
  pre-register covers evicted sessions.
- SR-09 correction: GC protection is by OMISSION from retention DELETE paths (no "protected set"
  exists) + a regression-test extension.
- `RetrospectiveReport.tags` is a REQUIRED `Vec<String>` (`#[serde(default)]`) so the compiler enforces
  every construction site — turns the SR-01 miss into a build error.

## Integration points confirmed / corrected
- Confirmed: hook goal path hook.rs:769/:839-880; listener handle_cycle_event :2848, step-5 spawn
  :3062; insert_cycle_event db.rs:320; get_cycle_start_goal db.rs:371; RetrospectiveReport.goal
  types.rs:436; review goal-set tools.rs:3409-3428; build_cycle_review_record tools.rs:4554;
  render_goal_section retrospective.rs:203/:49; CURRENT_SCHEMA_VERSION migration.rs:26 (=30);
  SUMMARY_SCHEMA_VERSION cycle_review_index.rs:54 (=5), pinned test :709-716; migration version-step
  pattern `if current_version < N` (latest <30 at :1474); fresh-create create_tables_if_needed db.rs:534.
- Corrected vs SCOPE: (1) retention has no "protected-table set" to register in — protection is by
  omission from the DELETE lists (gc_cycle_activity :116, gc_unattributed_activity :202); enforced only
  by test_gc_protected_tables_regression :521. (2) `insert_cycle_event` is NOT transactional today
  (single acquired conn) — the same-txn requirement needs a NEW method, not a param on the existing one
  (avoids 15-call-site churn). (3) tags travel via `tool_input["tags"]`/`payload["tags"]`, not through
  the non-persisting MCP `CycleParams` — the param only declares the interface.

## Open questions for the human
None blocking. Restated decided risks to expect at demo: SR-05 (re-issued start with changed tags
silently no-ops) and SR-10 (no back-fill; pre-v6 reviews show `## Tags` empty forever).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_lookup (vnc-045, col-025) + context_get on #3396
  (goal on cycle_events start row), #5599 (vnc-045 add_tag primitive), #3399 (col-025 degrade-to-None
  read contract) — applied all three as the ported precedents.
- Stored: entries #5651–#5656 "ADR-001..ADR-006 vnc-047" via context_store; #5659 "ADR-007 vnc-047"
  (ack echo). Corrected #5652 → #5658 (ADR-002 whole-set-once) via context_correct with agent_id
  (owning agent), edge carried forward. One typed edge: ADR-002 --Prerequisite--> #5599 (cycle_tags
  ports the entry-tag junction model; diverging forks the tag model, SR-06). No supersession of prior
  ADRs (additive to vnc-045/col-025). Intra-feature Prerequisite spine + Supports links left for retro
  per convention (sibling ADR ids exist but outcomes do not).
