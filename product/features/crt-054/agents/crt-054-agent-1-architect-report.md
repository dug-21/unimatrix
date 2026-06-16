# crt-054 Architect Report — Full Redesign (producer-only)

**Agent**: crt-054-agent-1-architect
**Date**: 2026-06-16
**Task**: FULL REDESIGN of crt-054 architecture + ADRs against the producer-only re-scope; reconcile every prior Unimatrix ADR.

## Deliverables

- **ARCHITECTURE.md**: `product/features/crt-054/architecture/ARCHITECTURE.md` (regenerated, not edited).
- **ADR files** (10, regenerated; 10 stale files deleted):
  - ADR-001-fold-inside-transcript-buffer-both-routes.md (corrected: reload latch removed)
  - ADR-002-signature-catalog-shared-regexset-config.md (corrected: no reread/compaction class, no token field, no role)
  - ADR-003-activity-snapshot-copy-struct.md (corrected: no compaction latch, no in-handler sum)
  - ADR-004-late-bind-attribution-coverage-honesty.md (corrected: no persisted activity_session_count)
  - ADR-005-never-persist-envelope-content-opacity.md (corrected: no token estimate)
  - ADR-006-fold-survives-to-review.md (heavily corrected: four-returns/persist moved to crt-055)
  - ADR-007-compaction-events-table-write-seam.md (NEW — Surface A, SR-01 resolved)
  - ADR-008-schema-version-ownership.md (corrected: was the STALE ADR)
  - ADR-009-believable-zero-regression-guard.md (corrected: asserts activity_snapshot())
  - ADR-010-wave-b-verified-precondition.md (kept: scoped to Surface B)

## Unimatrix Reconciliation Table

| Prior entry | Prior ADR | Action | New entry | New ADR |
|-------------|-----------|--------|-----------|---------|
| #4999 | 001 fold inside buffer | corrected | #5026 | 001 |
| #5000 | 002 cycle_review_index columns | **deprecated** (out of scope) | — | — |
| #5001 | 003 RegexSet catalog | corrected | #5027 | 002 |
| #5002 | 004 activity_snapshot() | corrected | #5028 | 003 |
| #5003 | 005 late-bind attribution | corrected | #5029 | 004 |
| #5004 | 006 never-persist envelope | corrected | #5030 | 005 |
| #5005 | 007 read-before-purge/four-returns | corrected (heavy) | #5031 | 006 |
| #5006 | 008 schema version (STALE) | corrected | #5032 | 008 |
| #5007 | 009 believable-zero guard | corrected | #5033 | 009 |
| #5008 | 010 Wave B precondition | corrected | #5034 | 010 |
| — | 007 compaction_events table | **new** | #5035 | 007 |

Typed edge: #5035 (ADR-007) → `Prerequisite` → #5032 (ADR-008). Justification: the compaction_events table cannot land without the CURRENT_SCHEMA_VERSION bump ADR-008 defines — a future agent must read ADR-008 first to migrate it correctly. Single edge; supersession handled by context_correct (not edges).

## Key Design Decisions

1. **SR-01 resolved favorably (ADR-007).** At `listener.rs:1854` (co-located with `increment_compaction`), no registry/session/buffer lock is held — `increment_compaction` takes+releases internally, and the buffer-tail guard dropped at `:1835`. The compaction_events INSERT acquires only the DB connection, ordered after the registry critical section. No lock nesting → no deadlock. INSERT is on-path but non-blocking-on-error.
2. **Surface B fold both routes by construction (ADR-001).** Accumulator embedded in TranscriptBuffer; registered and held routes fold into the same accumulator with zero new wiring.
3. **Two coverage models, deliberately asymmetric.** Surface B is declaration-gated (ADR-004); Surface A (compaction_events) is declaration-independent — written at the handler regardless, attributed at review. This dissolves the held/registered edge case for compaction.
4. **Hard boundary purge.** All removed scope (cycle_review_index, store_cycle_review, SUMMARY_SCHEMA_VERSION, reload reckoning, token fields) is absent from every new artifact — not re-imported.

## Open Questions (for spec / human / crt-055 alignment)

1. MAX_SIGNAL_CLASSES value + default error/refusal catalog — product judgment, align jointly with crt-055.
2. Schema-version number (29/30) — merge-order-dependent, SM coordination at merge.
3. compacted_at precision — Unix seconds; confirm seam clock granularity matches the PostToolUse ts crt-055 gates against.
4. Surface A INSERT transaction shape — confirm services.store_ops helper exists / thin one needed; no contention with the briefing write path on the handler.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_get #4999–#5008 — retrieved all 10 prior crt-054 ADRs and confirmed the STALE #5006 (SUMMARY_SCHEMA_VERSION/cycle_review_index ownership) per SCOPE line 93.
- Stored: entry #5035 "ADR-007 crt-054: Durable compaction_events Table" via context_store (the one genuinely new decision). Reconciled #4999/#5001/#5002/#5003/#5004/#5005/#5006/#5007/#5008 via context_correct → #5026–#5034; deprecated #5000 (cycle_review_index persistence, entirely out of scope) via context_deprecate.

---

# REWORK — open-question resolutions (crt-055 producer contract now FINAL)

**Date**: 2026-06-16
**Trigger**: crt-055 design complete; the four crt-054 open questions are resolved. Encode into ADR-002, ADR-007, ARCHITECTURE.md, and the matching Unimatrix entries. Touch nothing outside these resolutions.

## Resolutions encoded
**ADR-002 (file + #5027 → #5049):**
- `MAX_SIGNAL_CLASSES = 16`, PINNED — exactly equal, not `≤16`, not "decided jointly"; a shared compile-time constant crossing the producer/consumer boundary (the `class_counts` array width) that must equal crt-055's exactly. Open Q2 resolved.
- Default catalog = `error` + `refusal` only; added content-opacity rationale — FP rate can never be audited post-ship, so defaults must be calibrated against real transcripts during delivery before locking; counts are directional, not precise.

**ADR-007 (file + #5035 → #5050):**
- Q4 confirmed: single autocommit INSERT helper on `store_ops`, no explicit transaction, no lock held, non-blocking; on DB error log ids/counts and proceed (lock-ordering decision unchanged).
- Q3 added: `compacted_at` is Unix SECONDS (server wall clock, `now_secs()/.as_secs()`), documented explicitly in the DDL comment. The gate-side `ts/1000` normalization is crt-055's at the gate (crt-055 Binding constraint 8), not crt-054's.
- Observability added: a named failure counter (`compaction_events_insert_failed`) on INSERT failure — detect systematic loss + enable crt-055 row-vs-`increment_compaction` drift check.

**ADR-008 (file + #5032):** verified already correct (next `CURRENT_SCHEMA_VERSION` bump for `compaction_events` only; 29-vs-30 by merge order = SM coordination; `SUMMARY_SCHEMA_VERSION` 4→5 is crt-055's). No edit; #5032 untouched.

**ARCHITECTURE.md:** §3 data flow + error boundaries, §6 Integration Surface (`=16` pin, DDL seconds comment, new INSERT-helper row), §8 Open Questions (four producer-contract questions marked resolved; only merge-order version number + delivery-time pattern calibration remain).

## Scope-creep guard — confirmed NOT absorbed into any crt-054 ADR
`context_reload_pct` basis-points-vs-REAL, the `ts/1000` normalization itself, and the `compaction_reread` reread/boundary-selection all remain crt-055's. crt-054 supplies seconds + rows; crt-055 does all reckoning. ADR-007 states this explicitly.

## Files changed
- `architecture/ADR-002-signature-catalog-shared-regexset-config.md`
- `architecture/ADR-007-compaction-events-table-write-seam.md`
- `architecture/ARCHITECTURE.md`
- (ADR-008 file unchanged — already correct)

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- returned the crt-054 ADR set + crt-055 siblings; confirmed #5027=ADR-002, #5035=ADR-007 via context_get before correcting.
- Stored: #5027 → **#5049** (ADR-002) and #5035 → **#5050** (ADR-007) via context_correct (required update method; never deprecate+store). #5032 (ADR-008) untouched — already correct. No typed edges asserted (none meet the traversal-necessity bar; supersession via context_correct, never an edge). Nothing novel beyond the two corrections — rework to encode finalized contract values, not new architecture.
