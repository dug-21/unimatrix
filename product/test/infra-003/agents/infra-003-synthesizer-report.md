# Agent Report — infra-003-synthesizer

**Role:** Synthesizer (Session 1 → implementation deliverables)
**Status:** COMPLETE (regenerated for the read-as-barrier + per-session revision)

## Produced (regenerated from updated sources)

- `product/test/infra-003/IMPLEMENTATION-BRIEF.md`
- `product/test/infra-003/ACCEPTANCE-MAP.md` — AC-01…AC-15 (authoritative tracker) + Delivery-Coordination Actions table (R-15, R-16)
- GH Issue #853 — UPDATED (not duplicated); design-outcome section replaced; original synced scope preserved; labels untouched.

## Key changes since prior synthesis

- **Durability barrier reworked → read-as-barrier.** Aggregate `store_size` ("store grew") barrier removed as unsound (satisfied by the first of a store's two writes; races the second under `tokio::spawn` + `synchronous=NORMAL`). Positive-control marker-keyed read IS the barrier: strictly sequential per-store writes + bounded retry-until-present. Own-store timeout → INFRA (never RED); mis-route → RED at the cross-store cell. `store_size` demoted to liveness-only.
- **NEW AC-15 — MCP per-session isolation:** each `/v1/{slug}/mcp` probe uses its own `Mcp-Session-Id`; no cross-route reuse; INFRA-vs-RED both ways. Acceptance map now AC-01…AC-15 (authoritative; SCOPE's AC-01…14 is known non-blocking drift — carried the full 15).
- **Non-substring markers:** `infra003-{obs,mcp}-{a,b}-<run>`, required mutually non-substring (MCP read is `content LIKE '%marker%'`).
- **Risk register now 18** (4 Critical, 8 High, 4 Med, 2 Low): R-05 reclassified Med (barrier resolved-by-design), R-17 (crossed session) + R-18 (substring collision) added.
- **ADR Unimatrix IDs:** ADR-001 #5335, ADR-002 #5342, ADR-003 #5343, ADR-004 #5344.

## Delivery actions surfaced (HIGH, concrete linkage)

- **R-15 (#815):** invariant update ships in the SAME delivery PR as the new script, cross-linked on #815; closes #815's intent.
- **R-16 (#788):** durable adoption comment on #788 — N5/#788 must wire the gate into the recurring lane so N3 advances to "maintained."

## Non-blocking notes

- **ALIGNMENT-REPORT.md is stale** vs this revision (old single-direction AC-01…15 framing, `eval-baseline`, slug-B WARN). The variance is resolved by `isolation-b`. Regenerating it is tidy-up, not a gate blocker.
- ARCHITECTURE C6 illustrative `count(*)`/`LIKE` queries — SPEC AC-03/AC-07 authoritative.

## Post-synthesis correction (own-store timeout disposition)

Fixed an internal contradiction in ACCEPTANCE-MAP: the header said "own-store timeout = INFRA (never RED)" while AC-03/AC-05/AC-07/AC-09/AC-10 said "at-deadline = RED". Aligned all five to the authoritative position — own-store positive-control not-PRESENT-by-deadline = **INFRA** (durability not established), never RED; RED reserved for the cross-store cell (marker found in the wrong store). Source authority: ADR-002 (#5342), ARCHITECTURE C5/C7, RISK-TEST-STRATEGY R-05. Also fixed the same stray wording in IMPLEMENTATION-BRIEF C-08.

**Flag — SPEC source drift (for the spec owner, not a synthesis call):** SPECIFICATION FR-06.2, the ubiquitous-language "Marker-keyed retry-until-present" entry, and the SPEC AC-03/AC-07 tables still say "Marker absent **at** the deadline is RED" — contradicting ADR-002 / ARCHITECTURE / RISK R-05 (which say INFRA). The decision record + risk strategy govern, so the deliverables follow INFRA; the SPEC prose should be reconciled to match (own-store at-deadline = INFRA, cross-store presence = RED).

## Open items

None requiring a human decision. N3 stays partial.
