# Alignment Report: vnc-025

> Reviewed: 2026-06-05
> Artifacts reviewed:
>   - product/features/vnc-025/architecture/ARCHITECTURE.md (+ ADR-001..008)
>   - product/features/vnc-025/specification/SPECIFICATION.md
>   - product/features/vnc-025/RISK-TEST-STRATEGY.md
> Scope inputs: product/features/vnc-025/SCOPE.md, SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md; goals #4673 (proactive-delivery), #4677 (self-learning), #4710 (personal-cloud)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Advances proactive-delivery (remote PreCompact fidelity) and enables self-learning (crt-052 buffer); principles 2, 3, 5, 6, 7, 8 explicitly honored; 1, 4 N/A (no knowledge entries or edges created) |
| Milestone Fit | PASS | Clean F1→F2→F3 discipline; distillation deferred to crt-052, enterprise held to documented seams; nothing built for a future milestone |
| Scope Gaps | PASS | All 6 SCOPE goals, all 13 ACs, all 11 constraints, and all 3 resolved decisions traced into FR/NFR/AC |
| Scope Additions | WARN | hook.rs extraction-core move (ADR-005) and ADR-008 hardening are additions beyond SCOPE literal text — both justified, both risk-covered (R-14, R-02/R-06) |
| Architecture Consistency | WARN | RISK-TEST-STRATEGY is stale relative to ADR-008; spec carries no explicit no-panic/poison-policy requirement (lives only in ADR-008 + R-02) |
| Risk Completeness | PASS | 15 risks, all 9 SRs dispositioned with traceability; security section covers untrusted u64 offsets, prompt injection (R-13), memory DoS, secrets gates |

Counts: 4 PASS, 2 WARN, 1 VARIANCE (accept-recommended), 0 FAIL.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Simplification | In-content elision marker → metadata-only elision | SCOPE Goal 2 / AC-07 say overflow "records an elision marker + dropped-byte count"; ADR-002 / FR-11 record elision as metadata only (`elided_bytes` + non-zero `base_offset`), never bytes spliced into content. Rationale documented (spec Resolved OQ-3): spliced markers would corrupt offset math and JSONL parsing, and would break the byte-parity SCOPE Goal 5 demands. Sound — the marker requirement and the parity requirement were mutually inconsistent as written; ADR-002 resolved in favor of parity |
| Simplification | AC-02 convergence under overflow → tail-window equivalence | SCOPE AC-02 says deltas "converge to identical buffer content regardless of arrival order"; FR-02/AC-02 (spec) weaken this for cap-crossing sequences to tail-window equivalence (full-content equality holds only below the cap). Flows from human-approved resolved decisions 1+2 (ring-tail; no covered-range replay buffering), but see Variance 1 — the architect explicitly asked for human confirmation |
| Addition | `hook.rs` extraction internals moved to `uds/transcript_block.rs` (ADR-005) | SCOPE proposed "server-side equivalent" and reuse of the constant family; the docs instead refactor the only production-live transcript path into a shared core. Justified: directly closes SR-05/A3 (parity drift) by making parity structural; regression risk owned by R-14 (pre/post test-name inventory, constant pins) |
| Addition | ADR-008: checked offset arithmetic + treat-as-empty poison recovery | Not in SCOPE text. Not scope creep — required hardening for an attacker-controlled u64 on an authenticated-but-untrusted wire (R-02/R-06); preserves the SCOPE Constraint 4 always-Ack contract on the poison path |
| Addition | Config `validate()` floor (`transcript_buffer_max_bytes >= 65_536`), `with_transcript_cap` ctor | Minor implementation surface of Goal 2 / resolved decision 1; no concern |

Not an addition (checked explicitly): ADR-002's hole-range list (capped at 64) is the minimal mechanism AC-02's order-independent merge requires — a naive reset-on-gap scheme fails AC-02. The "full covered-range tracking" rejected at scope review was reader-facing range exposure for crt-052's fallback trigger; ADR-002 keeps the representation encapsulated exactly as resolved decision 2 directed.

## Variances Requiring Approval

1. **What**: AC-02's convergence guarantee is weakened under overflow. SCOPE AC-02 promises identical buffer content regardless of arrival order; the spec (FR-02, AC-02) delivers full-content equality only below the cap, and tail-window equivalence once ring-tail elision has advanced the buffer floor (a late head-fill is a defined no-op). ARCHITECTURE.md OQ-1 explicitly said: "flag to human if full-content convergence under overflow is required." The spec resolved it via ADR-002 without a recorded human sign-off — this report is that flag.
   **Why it matters**: User intent is authoritative — SCOPE AC text is the contract the human approved. The weakening is a direct consequence of two decisions the human *did* approve at scope review (4 MiB ring-tail; no covered-range replay buffering — rejected as speculative), so the deviation is derived, not invented. But the derivation deserves one explicit confirmation, because crt-052's distillation will inherit these semantics.
   **Recommendation**: **Accept.** Full-content convergence under overflow would force the covered-range replay buffering scope review already rejected; the only vnc-025 reader (PreCompact, 12 KB tail) is fully served by tail-window equivalence; R-03 tests pin the guarantee. If accepted, no document changes needed — SCOPE.md may optionally be annotated.

## Detailed Findings

### Vision Alignment

- **Goal #4673 (proactive-delivery)**: the feature's centerpiece — closing the remote PreCompact-fidelity gap (#4676) so server-side delivery matches what the local hook injects today — is squarely a proactive-delivery outcome (SCOPE Goal 5, FR-17/AC-11 golden-parity gate).
- **Goal #4677 (self-learning)**: the buffer is the prerequisite for crt-052 transcript distillation ("the conversational narrative … is never captured"); vnc-025 builds the substrate and deliberately nothing more, with the crt-052 seam shaped (FR-15, SR-04).
- **Goal #4710 (personal-cloud)**: HTTP `/observe` transport parity (FR-08, AC-06) and the no-global-cap memory envelope reasoned explicitly from the personal-cloud single-container posture (SCOPE Constraint 11, NFR-04).
- **Principle 2 (append-only audit)**: `transcript_session_purged` rides the existing `AuditLog`/`gc_audit_log` (FR-13); no new retention machinery.
- **Principle 3 (capability checks at service layer)**: deltas inherit `SessionWrite` + bearer gating; FR-08 states "no new auth surface"; R-12.3 tests rejection before dispatch.
- **Principle 5 (graceful degradation)**: crash loses in-flight transcript by design, degrading to crt-052 reconstruction (NFR-05); ADR-008's treat-as-empty poison recovery means one poisoning event never bricks a session.
- **Principle 6 (zero required infrastructure)**: no new runtime dependency (NFR-06/AC-13); resolved decision 3 cites principle 6 for rejecting the registry re-key.
- **Principle 7 (in-memory hot path)**: SR-01 was correctly forced as the *first* architecture decision (ADR-001 `Arc<Mutex<TranscriptBuffer>>`); `get_state()` clone cost is one Arc clone; AC-10 guards it.
- **Principle 8 (no secrets in any database)**: the strongest alignment in the feature — in-memory + purge IS the guarantee (#4721), enforced as hard gates not advisories: NFR-01, AC-04/AC-12, ADR-002 content-opacity by construction, R-05 sentinel tests + static grep gate, R-04 zero-rows preservation. The documents cite "principle 8" for the crash/loss posture, which reads correctly: data loss on crash is the accepted consequence of never persisting.
- **Principles 1 and 4 (hash chain, typed graph)**: N/A — the feature creates no knowledge entries and no edges; transcript bytes are deliberately opaque and never enter knowledge.db.

### Milestone Fit

vnc-025 is F2 of the F1 (vnc-024, shipped) → F2 → F3 (TS client) sequence, with distillation split to crt-052. Discipline is consistently good:

- Eight SCOPE Non-Goals are restated verbatim in the spec's NOT-in-Scope section "to block scope creep," with one carefully argued carve-out: the mechanical JSONL→exchange-turn formatting in the shared extraction core is what the local hook already does and is *required* by like-for-like parity — correctly distinguished from the excluded interpretive parsing.
- Enterprise is held to documented seams only: `session_key()` constructor (FR-20, ADR-007, no re-key), exhaustive `TranscriptRetention` match (FR-16), config knob beside `transcript_retention`. Matches resolved decision 3.
- The 4 MiB default's "generous headroom for crt-052" is a config default decided and human-approved at scope review — not future-milestone construction.
- Forward shaping for crt-052 is limited to one method signature (`clear_transcripts_for_feature`, counts-only today, take-shaped later) — the minimum SR-04 demanded.

### Architecture Review

Internally consistent and faithful to the scope's constraints: tee-before-untouched-filter (ADR-003) preserves the load-bearing vnc-024 non-persistence filter; lock discipline (registry → buffer, memcpy outside the registry lock) honors Constraint 3; SR dispositions cover all nine scope risks including the two human-accepted ones (SR-06 with a documented evidence trigger — >32 sessions or >256 MiB; SR-09 with the A2 invariant recorded as an F3 contract obligation).

Two consistency notes (the WARN):

1. **RISK-TEST-STRATEGY is stale relative to ADR-008.** Its inputs line reads "ADR-001..007" and its closing line states "the poisoned-mutex policy (R-06.2) is the one design decision the tests will force that no ADR currently pins" — ADR-008 now pins exactly that policy (treat-as-empty recovery + clear-on-poison, drop-whole on overflow) and was written in response to R-06.2. The loop closed correctly; the strategy text just doesn't say so. Recommendation: one-line update to the risk strategy (inputs + closing line), or accept as an ordering artifact since R-06.2's scenarios remain valid against ADR-008's chosen policy.
2. **The no-panic contract is absent from the spec.** "No input reachable from the wire can panic inside `TranscriptBuffer`" is load-bearing (R-02 coverage requirement, ADR-008 layer-1 contract, referenced in the architecture's Integration Surface) but appears in no FR/NFR. The spec is the requirements source of truth downstream agents implement from. Recommendation: add a one-line NFR (or extend NFR-03) carrying the no-panic + poison-recovery requirement; low effort, prevents the requirement living only in an ADR.

Pattern check (#3337 — diagram strings diverging from spec, asserted by testers): not reproduced here; the AC-11 golden test derives expectations from `extract_transcript_block(path)` output on a fixture, never from document text (#3426, #2984 both cited).

### Specification Review

- Full traceability: every SCOPE goal maps to FRs (Goal 1 → FR-01..09; Goal 2 → FR-10/11; Goal 3 → FR-12/15/16; Goal 4 → FR-13/14; Goal 5 → FR-17..19; Goal 6 → FR-16/20); all 13 AC-IDs carried verbatim with bound verification methods; all 11 constraints restated as binding; SR table complete.
- The three architecture open questions are all resolved and reflected back into FRs/ACs/Domain Models (OQ-1 → ADR-005 shared core; OQ-2 → tail-window equivalence; OQ-3 → no visible elision marker). OQ-2's resolution is the item escalated as Variance 1 above.
- The accepted lifecycle hazard (sweep-before-review transcript loss, SR-09) is stated explicitly in the Workflows section rather than buried — exactly what the scope risk assessment asked for.
- Domain Models pins the delta-content contract (raw JSONL file bytes, file byte offsets) as load-bearing for F3 — good forward-contract hygiene.

### Risk Strategy Review

- Coverage is proportional to where the danger is: the Critical risk (R-01 merge correctness) gets the densest harness; the two vision-critical posture risks (R-04 delta-bytes-to-disk, R-05 content leak) are hard gates with both dynamic sentinel tests and static review gates — "Both, not either."
- R-13 (prompt injection via streamed transcript into `BriefingContent`) is a genuinely additive security finding beyond the scope risk assessment, with an honest document-and-accept disposition (exposure identical to today's local hook reading a local file).
- Historical evidence is well-used: #4379 audit-pool cluster shapes R-07, #4140 silent-eviction shapes R-08's mandatory named case, #2984 guards against hand-copied expectations.
- All nine SRs traced; SR-06 correctly carries no test (ops-review posture per human acceptance).
- Sole gap is the ADR-008 staleness noted under Architecture Review.

## Knowledge Stewardship

- Queried: /uni-query-patterns for vision alignment patterns — #2298 (config/doc semantic divergence), #3337 (architecture-diagram strings diverging from spec, checked and not reproduced here), #4617 (low relevance).
- Stored: nothing novel to store — the findings (a spec-level refinement of a scope AC flowing from approved scope decisions, and a risk-strategy/ADR ordering staleness) are feature-specific to vnc-025's document sequence and do not yet generalize across features.
