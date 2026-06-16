# Scope Risk Assessment: crt-054

**Scope**: producer-only (re-scoped 2026-06-16). `cycle_review_index` ownership moved to crt-055; this assessment OVERWRITES the prior wider-scope version (2026-06-14). Two producer surfaces only: Surface A (`compaction_events` table) + Surface B (`activity_snapshot()` in-memory fold).

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Lock ordering at `handle_compact_payload`: the `compaction_events` INSERT runs at a seam already holding registry/session locks (`uds/listener.rs:1737` + `increment_compaction`). A new DB acquisition under those locks can deadlock or stall the hot drain path. | High | Med | Architect maps the exact locks held at the seam and orders the INSERT against them; decide on-path vs deferred write so the compaction ACK is not blocked. (Open Q1.) |
| SR-02 | Held-route believable-zero trap: if the fold runs only on the registered route and misses the held-delta route (`session.rs:388-401`), drained-session bytes silently read as zero — the exact #750 failure class (lessons #4998/#5025, ADR-009 #5007). | High | Med | Fold MUST run on both routes; mandate a held-route regression guard asserting a non-empty source (Constraint 2). A no-op/registered-only test gives false confidence (pattern #3624). |
| SR-03 | Integer-width truncation across the `activity_snapshot()` boundary: in-memory `bytes_total` u64 / `delta_count` u32 vs the i64 columns crt-055 lands them in. Silent wraparound corrupts the signal. | Med | Low | Confirm widths and checked/saturating conversion at the producer→consumer seam (Open Q3); add a boundary-value test near u32/i64 limits. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Schema-version sequencing collision with crt-055: both take the next `CURRENT_SCHEMA_VERSION` bump (28→29/30) on different tables. Parallel merge can collide on the same version number or leave a gap (lesson #4095; ADR-017-003 #760 precedent). | High | Med | Treat version assignment as merge-order-dependent: first-merged is 29, second retroactively becomes 30. The in-flight feature MUST update its migration block + artifacts at merge (SM coordination point, #4095). |
| SR-05 | Stale-knowledge residue from the prior wider scope: ADR-008 (#5006) and ARCHITECTURE/ADR-003 residuals still claim crt-054 owns `cycle_review_index`/`SUMMARY_SCHEMA_VERSION`/v4/v29 and the `reread`/`compaction` regex classes + `token_bytes_per_unit`. Designing against these re-introduces removed scope. | Med | High | Architect `context_correct`s #5006 per SCOPE line 93; regenerate (not edit) ADR-003 residuals against the new SCOPE. Prior ADR-009 (#5007) and ADR-001 (#4999) remain valid — reuse them. |
| SR-06 | Token/cost exclusion drift: `bytes` is the binding honest unit (RQ-8). Any token-named field (`token_bytes_per_unit`) re-imported from prior artifacts violates the bytes-only resolution and re-opens the crt-054↔crt-055 contradiction. | Med | Med | Hard scope edge: no token estimate, no token-named field. Spec writer asserts bytes-only as a constraint; reject any token surface in review. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | crt-055 producer-contract coupling / interface drift: crt-055 §"Producer contract" is binding for every field crt-054 writes; the two design in parallel. Independent field/type/default-catalog changes diverge producer and consumer. | High | Med | Any field/width/catalog change is negotiated in crt-055's contract FIRST. Align the default `error`/`refusal` catalog + `MAX_SIGNAL_CLASSES` jointly with crt-055's design session (Open Q2). Treat the contract as a single source, not a copy. |
| SR-08 | Survival-to-review on the crt-052 hold: the in-memory fold must stay accurate and readable until crt-055 reads it, riding the crt-052 hold; if crt-054 zeroes/drops the counter, or the read moves after `purge_cycle_transcripts`, every counter reads zero (ADR-009 #5007 read-before-purge). | High | Low | Producer obligation: never zero/drop before the hold purge. crt-054 depends on crt-052 Wave B staying ON-by-default/non-disableable — flag this hard dependency; a config that re-enables purge-before-read breaks the fold. |
| SR-09 | Cycle-declaration coverage gap: undeclared sessions purge at drain and the fold dies (correct fail-loud). Risk is crt-054 fabricating a zero instead of signalling absence; crt-055 surfaces a `raw_signals_available`-style flag. | Med | Med | crt-054 must never emit a fabricated zero for an undeclared/purged session; absence is signalled, not counted (Constraint 4). Confirm the `compaction_events` row (session-keyed at handler, written regardless of declaration) is independent of the fold path. |
| SR-10 | vnc-036 shelving / `high_water` now-populated: `high_water` is populated on every row but reserved (no wire change). Future precise byte-boundary gating may find the server-captured `high_water` semantically insufficient vs a wire boundary, forcing re-design. | Low | Low | Accept for v1: populating now avoids a second migration. Document `high_water` semantics (server-captured at handler, not wire-precise) so crt-055/future gating doesn't over-trust it. Reopen vnc-036 only on measured need. |

## Assumptions

- **A1 (SCOPE §Re-scope note, §Dependencies):** crt-055 absorbs all `cycle_review_index`/`SUMMARY_SCHEMA_VERSION` ownership and its producer contract is authoritative. If crt-055 slips or changes the contract, crt-054's two surfaces have no consumer and field definitions float — drives SR-07.
- **A2 (SCOPE §Dependencies, crt-052 Wave B):** the transcript hold is ON by default, unconditional, non-disableable. If this regresses or becomes disableable, fold survival-to-review (SR-08) breaks silently.
- **A3 (SCOPE In-scope §1, Constraint 5):** a durable INSERT can co-locate at `handle_compact_payload` without disturbing the compaction hot path / lock graph — unvalidated until design (SR-01).
- **A4 (SCOPE §Migration):** crt-054 and crt-055 migrate distinct tables so merge order is free — true only if version-number assignment is coordinated at merge (SR-04).

## Design Recommendations

1. **SR-01 / SR-08 first**: resolve lock ordering at the write seam and counter survival-to-review before field-level design — these are the hot-path/sequencing seams where the #750 class lives (ADR-009 #5007).
2. **SR-04 / SR-07**: establish the crt-055 contract as the single source for fields AND the schema-version merge-order rule before either feature locks its migration block (#4095).
3. **SR-02 mandatory guard**: require the held-route non-empty-source regression test as an acceptance gate; do not accept a registered-only or no-op-path test (pattern #3624).
4. **SR-05 / SR-06**: correct #5006 and purge token-named / `reread` / `compaction`-class residue from prior artifacts before design proceeds.
