# crt-058 Retrospective — Architect

Feature: crt-058 — Eager Agent-Authored Edge Cleanup at `context_deprecate`. SHIPPED CLEAN (0 rework, gates 3a/3b/3c PASS first pass). Retrospective steward: uni-architect.

## 1. Patterns

- **UPDATED #3910 → #5472** "Multi-pass cleanup: all passes on the same reference table must apply identical status filters." Generalized (additive, original preserved) to cover the crt-058 case: a second pass may DELIBERATELY key differently from the tick (eager pass pulled forward to the source event, keyed on entry-id + provenance), where the invariant is not identical filters but a proven SUBSET relation with the tick unchanged as backstop. Added the executable-enforcement technique — one test running BOTH real cleanup functions over parallel identical fixtures asserting R ⊆ T — as the durable upgrade over the original's manual-grep checklist. Cross-refs crt-058 ADR-003 (#5460) and the mechanical test gotchas in #5467/#5473.
- **UPDATED #5467 → #5473** "Eager subset-delete of graph_edges…" Folded in the F-6 post-merge hardening: marshal RETURNING rows with `try_get`, not `get`, on a non-fatal path — `row.get()` panics on a type/null mismatch and a panic post-commit would defeat the non-fatal contract; `try_get` routes marshaling failures to the swallow-and-warn branch. Placed at the exact marshaling locus rather than as a standalone entry.
- **No new pattern stored.** The candidate reusable structure (eager-at-source + tick-backstop, subset enforced by a both-real-functions test) is now fully captured by the #3910→#5472 generalization plus the feature-specific #5467→#5473 mechanics. A new entry would duplicate.

## 2. Procedures

None changed. Feature reused existing build/test/audit paths. The one genuinely novel how-to (forcing a non-unit-constructible `#[tool]` handler's swallowed-failure branch via a SQLite `BEFORE DELETE … RAISE(ABORT)` trigger from a second WAL connection) is already stored by the tester as pattern **#5470** — not duplicated.

## 3. ADR status — all VALIDATED, none flagged

| ADR | Entry | Validation |
|-----|-------|-----------|
| ADR-001 eager-delete-at-source, tick backstop, non-fatal | #5458 | Gates 3a/3b/3c PASS; AC-06 injected-failure test (real handler) confirms warn+success+advisory-omitted+edges-remain+tick-backstops. Non-fatal contract holds. |
| ADR-002 audit removed tuples via DELETE…RETURNING | #5459 | AC-03/AC-11 pass; metadata is set-equal to seeded tuples, never the `{}` sentinel on non-empty. |
| ADR-003 eager⊆tick as executable invariant | #5460 | Keystone. Unit subset test runs both real fns (R⊆T AND R==exactly the 2 agent edges); integration chokepoint-exclusion via real `context_correct`. The invariant shipped exactly as designed. |
| ADR-004 `edges_removed: Option<u64>` plumbing | #5461 | Per-format behavioral matrix passes (`Some(n)`/`Some(0)`→literal 0/`None`→omitted); backward-compat byte-identity for quarantine/restore. |

Note: the F-6 `try_get` hardening REINFORCES ADR-001's non-fatal guarantee (closes a latent post-commit-panic hole); it contradicts no ADR. No supersession sought or needed.

## 4. Lessons

- **F-04/F-05 interruption → anonymous-write fallback:** report-only, no new lesson. The fix (re-issue `context_cycle(type:start)` after an interruption to recreate the client tracker) is already documented in the delivery/design protocols' "Resuming an interrupted session" note. A lesson would duplicate existing protocol guidance. No non-obvious un-captured angle found — root cause and remedy are both known.
- **F-6 non-fatal marshaling (`try_get`):** captured by enriching #5467 → #5473 rather than a standalone lesson (it was human-directed optional hardening, not a gate miss or failure). The generalizable point — a DB path documented non-fatal that marshals with `row.get()` still panics on type/null mismatch, defeating its own contract; use `try_get` — now lives at the exact marshaling locus.

## 5. Retrospective findings (hotspot-derived)

- **F-04/F-05/F-01 (interruption, timeout gap, cold restart):** single root cause — a mid-cycle `/login` killed the design-session cycle-tracker; phase-end/stop Write calls fell back to anonymous and failed until the delivery session re-issued `context_cycle(type:start)`. Remedy already in-protocol. Recommendation: no action beyond ensuring leaders apply the "Resuming an interrupted session" note promptly. Not a knowledge gap.
- **F-10 sleep workaround (1 instance):** the audit read-back needed a ~50ms settle because `audit_fire_and_forget` drops the JoinHandle. Retro recommendation "use run_in_background + TaskOutput instead of sleep polling" does not apply cleanly to an in-test fire-and-forget settle; the constraint is documented in #5468. No action.
- **F-02/F-03/F-07 (66 files, 50 mutated, 37 design artifacts):** expected breadth for a full design+delivery cycle in one session. Not anomalous.
- **F-09 adr_count 4 (threshold 3):** four ADRs is proportionate — one decision each (eager-at-source, audit-tuples, subset-invariant, response plumbing), no splitting or padding. No action.

## 6. Relationship edges

**None — bar not met.** The #3910→#5472 generalization now references ADR-003 (#5460) and #5467/#5473 in prose, and ADR-003 already cites #3910 in its own prose. Traversal-necessity is satisfied by prose cross-refs; a typed edge would add no path a future agent MUST follow. Supersession (F-6 hardening) was handled as `context_correct` on #5467, not an edge, per convention.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search 'crt-058 eager edge cleanup deprecate' (k=20) + context_get #3910/#5461/#5470 — reviewed all 7 crt-058 entries (#5458–5461 ADRs, #5467/#5468/#5470 patterns); all structurally sound and category-correct, no corrections to their content needed.
- Stored: nothing net-new. Enriched two existing entries via context_correct — #3910→#5472 (multi-pass-cleanup generalization + executable subset-invariant enforcement) and #5467→#5473 (try_get non-fatal marshaling hardening). New patterns deliberately not created (would duplicate the generalized #5472 + feature-specific #5473).
