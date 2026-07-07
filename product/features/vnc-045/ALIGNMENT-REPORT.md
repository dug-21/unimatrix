# Alignment Report: vnc-045 — `context_tag` (mechanism only)

> Reviewed: 2026-07-07
> Scope status: REDUCED 2026-07-07 — `protected_tags` value-hygiene policy DEFERRED in full; feature ships the `context_tag` MECHANISM only. This report re-runs the alignment check against the reduced source documents.
> Artifacts reviewed:
>   - product/features/vnc-045/architecture/ARCHITECTURE.md (mechanism only; ADR-001/002/004/008/009 active; ADR-003/005/006/007 DEFERRED; §8 Deferred)
>   - product/features/vnc-045/specification/SPECIFICATION.md (FR-01..11, NFR-01..07, AC-01..07; §11 Future Extension)
>   - product/features/vnc-045/RISK-TEST-STRATEGY.md (R-01..R-08, 0 Critical; SR traceability)
>   - product/features/vnc-045/SCOPE.md (SD-1..SD-12; Deferred / Future Extension; Scope Risks Voided)
> Vision source: product/PRODUCT-VISION.md
> Goals consulted: #5518 self-learning, #5517 domain-agnostic, #5474 integrity
> Prior guardian knowledge: #3742 (deferred-branch WARN pattern), #5607 (enterprise-seam-in-OSS PASS-on-inertness), #4974 (ceremonial-seam check)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Advances domain-agnostic (#5517), self-learning (#5518), integrity (#5474). Value-opaque, learning-preserving, audit-as-primary-control. |
| Milestone Fit | PASS | Platform-infrastructure feature; no future-milestone capability pre-built. Deferral tightens milestone discipline. |
| Scope Gaps | PASS | Every SCOPE item (SD-1..12, AC-01..07) addressed in source docs. No gap. |
| Scope Additions | PASS | Only in-scope design-time resolutions (colon-less `replace` → degrade-to-add; A3 carve-out). No new capability beyond SCOPE. |
| Architecture Consistency | PASS | ADR index, audit shape, and metadata field names identical across all three docs. |
| Risk Completeness | PASS | 0 Critical (prior threading Critical VOIDED-BY-DEFERRAL); audit (R-03) and learning-invariance (R-01) both High. Deferred material carries zero test requirement. |
| CRITICAL CHECK (no deferred material in-scope) | PASS | No source doc treats `protected_tags` config, per-slug threading, validator, `min_trust_level`, or cadence guard as in-scope or as a test requirement. |

**Variances requiring human approval: NONE.**

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | — | None. SD-1..SD-12 and AC-01..AC-07 each trace into SPEC (FR/AC) and RISK (R-mapping). |
| Addition | — | None that expand capability. |
| Design-resolution | Colon-less / null-namespace `replace` → degrade-to-`add` | SCOPE Open Questions = none; SPEC §10 leaves it as an architect design-time decision; ARCH §4.3 + ADR-004 resolve it as degrade-to-`add` (pure insert, records `prior_value:null`). Within scope — a resolution of a delegated decision, not a new feature. |
| Design-resolution | A3 tenant-isolation carve-out (ARCH §9) | Documents the metadata-filter-bypass risk (ass-094 OoS) as inert under 1-client:1-project + per-slug DB, with a carry-forward guard. Documented boundary, not a shipped capability. Consistent with memory "architect-for-enterprise-build-for-OSS". |
| Simplification | `protected_tags` policy deferred wholesale | Rationale (SCOPE §"Why defer"): retrofit-HARD contracts (audit shape, gate location, value-opacity seam) are kept; retrofit-CHEAP config plumbing is deferred because inert threading with no live consumer cannot be behaviorally tested and rots (`::default()` trap). Documented, coherent. |

## Variances Requiring Approval

None.

## Detailed Findings

### Vision Alignment — PASS

Confirmed on the four axes requested:

1. **Self-learning preservation (not zeroing the learning vector).** The feature's core rationale is that `context_correct` hard-resets the entire learning vector (`confidence, access_count, last_accessed_at, helpful_count, unhelpful_count` — `write_ext.rs:542-561`) on every tag change, and `context_tag` exists to avoid that churn. FR-06 / NFR-01 / AC-02 require all five learning columns byte-identical pre/post; R-01 makes this a High core-value guard with invariance scenarios. Directly advances goal #5518 (self-learning): the entry's accumulated usage signal survives a volatile status flip. PASS.

2. **Audit-as-primary-control (ADR-009 complete generic shape).** SD-7 / FR-10 / AC-04 / ARCH §5 spec the complete generic event (`operation="context_tag"`, `target_ids`, `agent_id`, `capability_used`, timestamp, `metadata {action, namespace, tag, prior_value, new_value}`) and emit it in full now, `prior_value` mandatory on remove/replace, because the audit log is append-only and is the one genuinely retrofit-hard piece. This satisfies Architectural Principle #2 (audit append-only and complete, full attribution) and advances goal #5474 (integrity). RISK R-03 treats audit incompleteness as a High, primary-control failure (sentinel `"{}"`, lost/double event, mis-serialized enum, fire-and-forget settle). Accountability-not-prevention is the correct posture given declarative attribution (`credential_type="none"`) — consistent with memory "avoid-overstating-defensive-structure". PASS.

3. **Value-opacity / domain-agnostic north star.** SD-8 / FR-07 / AC-05 require the handler to write any tag without interpreting it — `delivery:proven`, `delivery:anythingelse`, and free-form `foo` all succeed on bare `Capability::Write`; no allow-list, no vocabulary, no `Capability::Tag`, no add/remove/replace capability split. SPEC §1 and Ubiquitous Language state plainly that `delivery:` is "merely an illustrative example" and the engine "assigns it no domain meaning". This is exactly goal #5517 (domain-agnostic: the engine writes a tag it never interprets; any domain mutates volatile tags with no hard-coded vocabulary). `namespace` is derived-and-recorded, NEVER validated — value-opacity holds even in the audit path. PASS.

4. **"Architect for enterprise, build for OSS" line drawn correctly.** The retrofit-HARD seams are kept in scope precisely because they are painful to retrofit onto immutable/append-only substrate:
   - audit-event SHAPE (SD-7, ADR-009) — shipped in full now;
   - `Capability::Write` gate LOCATION (SD-9, ADR-008; ARCH §7 #2) — reused verbatim, the point where enterprise trust-elevation later attaches;
   - value-opacity pre-write interception point (SD-8, ADR-008; ARCH §7 #1) — one marked point where a future `evaluate(tag)` validator drops in.
   The retrofit-CHEAP plumbing (`ProtectedTagsConfig` type, five-site per-slug threading, allow-list validator, `single_value` config, `min_trust_level`, cadence guard, `PerSlugOverlayable`) is deferred, not pre-built. This matches memory "architect-for-enterprise-build-for-OSS" and the human's stated reasoning. It also refines prior guardian pattern #5607: that pattern was recorded against the *earlier* vnc-045 revision, which shipped `min_trust_level` as an inert field tested for inertness (R-10/AC-09b). The reduced scope goes one step further — the field is deferred entirely, leaving only the gate LOCATION as the forward contract. Both are valid build-for-OSS positions; the reduction is the stricter one and removes even the inert-field-rots surface. PASS. (See Knowledge Stewardship: #5607's cited instance IDs are now stale.)

### Milestone Fit — PASS

The feature is platform infrastructure (a parallel fast path on the generic tag lane). No future-milestone capability is built ahead of need: the entire `protected_tags` policy — the only forward-looking surface — is deferred with an explicit rationale that pre-building inert plumbing is strictly worse than deferring. Milestone discipline is tighter after the reduction than before it.

### Architecture Review — PASS

- ADR index (ARCH §10) is internally consistent: ADR-001/002/004(revised)/008(revised)/009(new) Active; ADR-003/005/006/007 DEFERRED with reason pointing to the future feature. Matches the ADRs named in the task.
- ARCH §2 "Explicitly NOT in vnc-045" and §8 Deferred enumerate every deferred surface; §7 describes the two preserved seams as marked notes "no stub, NO empty config, NO call".
- The §1 ASCII request-lifecycle block is illustrative, but the load-bearing strings match spec verbatim: `operation="context_tag"` and `metadata {action, namespace, tag, prior_value, new_value}` are identical to SPEC §3.2 / FR-10 and to the RISK read-back assertions. No divergence of the kind pattern #3337 warns about (informal diagram strings drifting from canonical spec output). Low concern; no action.
- §3 derived-state blast radius is complete and asserts all tag reads are live SQL (ADR-002) — the basis for "no invalidation" (SD-4) and read-freshness (NFR-04).

### Specification Review — PASS

- FR-01..FR-11 each map to AC-01..AC-07 and trace to SDs. FR-02 (gate location), FR-06 (learning preservation), FR-07 (value-opacity), FR-10 (audit shape) carry the four vision axes.
- §11 Future Extension and the "Explicit Non-Goals (do NOT introduce)" block list every deferred item as out of scope with "no requirements or test obligations". FR-07 explicitly forbids shipping a stub / empty `ProtectedTagsConfig` / config type.
- No FR imposes a validator or trust-check requirement. The two seams are described as marked code/ADR notes only (§11, Ubiquitous Language "The two retrofit seams").

### Risk Strategy Review — PASS

- 0 Critical: the prior Critical (five-site per-slug threading → `::default()`) is VOIDED-BY-DEFERRAL. High = R-01 (forbidden-surface / learning-vector invariance), R-02 (atomic replace), R-03 (audit completeness — primary control).
- SR traceability table marks SR-03/04/06/08/09/10 VOIDED-BY-DEFERRAL, each "Not tested." R-04 residual sliver retained only as a *negative* proof (no validator shipped, `validate_outcome_tags` not conflated) with an explicit instruction: "Do NOT write a test that requires a rejection path — none ships."
- Security § and Residual Risk #4 state the two preserved seams are "seams only — no behavior to test" and instruct: do NOT write a test requiring an `evaluate(tag)` rejection path or a `min_trust_level` accept/reject difference. Their correctness is covered negatively (R-04 no-validator-shipped + authorization-blast-radius no-trust-consulted).
- Coverage summary aligns risk counts to scenario counts; no scenario depends on deferred material.

### CRITICAL CHECK — PASS (no WARN)

Swept all four source documents for deferred `protected_tags` material treated as in-scope with test requirements:

| Deferred item | SCOPE | ARCHITECTURE | SPECIFICATION | RISK-TEST |
|---------------|-------|--------------|---------------|-----------|
| `ProtectedTagsConfig` / config type | Non-Goal 1, Deferred § | §2 "Explicitly NOT", §8 | FR-07, §11, Non-Goals | R-04 "no config type shipped"; not tested |
| Five-site per-slug threading | Non-Goal 8, Voided § | §2, §8 | §11, Non-Goals | SR-06/SR-09 VOIDED, not tested |
| Value-hygiene / allow-list validator | Non-Goal 1 | §7 #1 "no stub, no call" | FR-07 "no stub" | R-04 "do NOT write a rejection-path test" |
| `min_trust_level` | Non-Goal 3, Voided § | §7 #2 "not shipped, no gap" | FR-02, §11 | Security §, Residual #4 "do NOT test trust diff" |
| Cadence guard | Non-Goal 8 | §6, §8 (ADR-007 deferred) | §11, Non-Goals | SR-08 VOIDED, not tested |

No document imposes a test on any deferred item. The two preserved seams are consistently described as marked seams only — no stub, no config, no validator call, no trust check — and the risk doc explicitly forbids tests that would require a validator/trust behavior that does not ship. This satisfies pattern #3742 Option B (all three source docs consistently defer with zero test scenarios) and pattern #5607 (inert/absent seam → PASS, not variance). No WARN.

## Knowledge Stewardship
- Queried: `/uni-query-patterns` (context_search, topic vision) for alignment patterns — surfaced #3742 (optional-future-branch must match scope deferral; WARN only if a deferred branch still carries test requirements), #5607 (enterprise seam in build-for-OSS is PASS when only inertness is tested; created today against the *earlier* vnc-045 revision), #4974 (ceremonial-seam check), #3337 (arch-diagram strings diverging from spec). Applied #3742/#5607 as the classification basis for the CRITICAL CHECK and the build-for-OSS axis; confirmed #3337 does not bite (diagram strings match spec verbatim).
- Stored: nothing novel. The governing pattern (#3742 + #5607) already captures this exact scenario for this exact feature; a further vnc-045 entry would be near-duplicate noise (respecting memory "avoid-overstating-defensive-structure"). One reconciliation flagged for the retro instead of a new store: **#5607 cites a now-stale clean instance ("vnc-045 min_trust_level R-10/AC-09b test inertness")** — the reduced scope defers `min_trust_level` entirely, so R-10/AC-09b no longer exist (registers are R-01..R-08, AC-01..AC-07). Refined lesson worth folding into #5607 at retro: *deferring inert plumbing entirely is also PASS — and is preferred over shipping it inert when the plumbing has no live consumer and would rot.* Recommend the retro update #5607 via `context_correct` (owner action; guardian is not the owner).
