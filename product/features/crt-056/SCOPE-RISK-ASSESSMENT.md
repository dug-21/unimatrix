# Scope Risk Assessment: crt-056

Per-slug intelligence parity — maintained analytics + correct service config on a concurrency-clean per-project tick work-unit. Scoped 2026-06-19. Historical evidence: #4974 (ceremonial seam / N=1 false confidence, vnc-034), #2535 (rayon monopolisation, crt-022), #2543 (rayon panic SIGABRT in tests).

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Singleton analytics handles (`ConfidenceState`/`EffectivenessState`/`TypedGraphState`/`PhaseFreqTable`/`ContradictionScanCache`/`TickMetadata`) are global `Arc<RwLock<_>>`; naive per-slug iteration overwrites each slug's state with the next (slug A's graph replaced by B). | High | High | Mandate per-slug handle SETS owned by the per-slug `ServiceLayer`; tick mutates only its own context's handles. Make this structural, not convention. (SCOPE crux, AC-4) |
| SR-02 | Shared rayon ML pool serves both MCP hot path and per-slug background ticks. A long per-slug tick closure monopolises threads; with pool sized per config but ticks unbounded, MCP latency degrades to tick duration. Evidence #2535. | Med | Med | Serialize rayon across slugs (serial loop gives this free); document the worst-case single-slug tick duration as the monopolisation envelope; do not let the serial loop hold rayon across all N slugs in one closure. |
| SR-03 | Test-default constructor path (NLI off, pool=1, default params) is the EXISTING behavior; a constructor refactor that drops it breaks unit tests / introduces a cloud-only code path the local single-project install never exercises. Evidence: vnc-034 isolation-seam constraint. | High | Med | Constructor refactor must be additive (OQ-4): per-slug passes pre-built `ServiceLayer`, test path preserved. Prefer the form (required param vs `Option`) that least disturbs call sites; the single-project daemon must traverse the same parity path. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Step B leakage — bounded pool, LRU residency/eviction, cadence signals, concurrent rayon are explicitly OUT, but the BackgroundJob seam invites "just a small scheduler." Scope creep inflates the feature and risks shipping half a scheduler. | High | High | Hold the work-unit boundary hard (Constraints): the job touches ONLY its `PerSlugTickContext` + shared read-only resources + rayon. Architect builds the SEAM, not the queue/pool/residency/cadence. Reviewer should reject any scheduler machinery. |
| SR-05 | Parity is under-defined: AC-1 lists config fields but `adapt_service` independence and `session_capabilities` per-slug are open (OQ-5). An incomplete parity list ships a still-degraded slug and a silent C5 gap. | Med | Med | Resolve OQ-5 in design to a closed parity checklist; AC-1 must assert equality with the daemon's RESOLVED config, field by field — not a representative subset. |
| SR-06 | "No new functional analytics" / global-config-only boundary (#785/C6 is separate) can blur — temptation to add per-slug overrides while threading config. | Low | Med | Keep crt-056 to GLOBAL config parity; per-slug custom config is #785. Spec writer should constrain the threaded config to the daemon's single resolved set. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | CEREMONIAL SEAM trap (direct precedent #4974, vnc-034 Wave 1→2): a BackgroundJob seam built ahead of Step B can resolve-then-discard or sit beside a parallel path, passing every N=1 (single-slug) test while the concurrency-clean contract is unproven. The same false-green that hid the `let _store` discard. | High | High | AC-4 (cross-slug corruption guard) MUST run at N=2 with two REAL slugs — A's write absent from B after ticking both. N=1 cannot distinguish funnel from bypass. Make the per-slug handle set the SOLE route the tick mutates; no parallel global-handle write path. |
| SR-08 | Serving/tick handle identity: the tick must rebuild the SAME handles the serving path reads (principle 7, in-memory hot path). If tick and serve hold different handle instances, AC-5 passes structurally but serving reads stale state. OQ-1 ownership ambiguity. | High | Med | Resolve OQ-1: one handle set per slug, shared between serve and tick (the per-slug `ServiceLayer` owns it; the tick references it). Add a behavioral AC-5 proof: search reflects post-tick state, not just "handle exists." |
| SR-09 | Loop-global `tick_counter` fires interval gates (`tick % 4 == 0`, contradiction-scan cadence) synchronously for all slugs (OQ-3). All slugs run heavy interval ops on the same tick → periodic latency spike, and concurrency-readiness assumption breaks if a job reads loop-global counter state. | Med | Med | Pick per-slug counters (preferred for the concurrency-clean contract — no cross-context shared mutable state) over accepted synchronized gating. If synchronized, document the latency-spike envelope and confirm the counter is read-only shared, not mutated cross-context. |
| SR-10 | Rayon test-harness instability: per-slug ticks add background rayon closures; a panic inside one without a `panic_handler` aborts the whole test process (SIGABRT). Evidence #2543. | Low | Med | Ensure the multi-slug tick test harness installs the rayon `panic_handler`; extend the existing Layer-2 harness rather than new scaffolding. |

## Assumptions

- **A1 (Background Research, SCOPE L66-72):** All 9 tick operations take `&Store` explicitly, are idempotent, and reach no global store singleton. If any op closes over a global handle, per-slug calling is unsound — invalidates SR-01's mitigation. Architect should re-verify per op, not assume.
- **A2 (SCOPE L83-86):** Embedding/NLI models and `ConfidenceParams` are safely shared read-only `Arc`. If any "read-only" resource carries interior-mutable cached state, it becomes a cross-slug shared-mutable hazard (SR-01 class). Verify the shared `Arc`s are truly immutable.
- **A3 (SCOPE L93-94):** All config-parity inputs (`config`, `ml_inference_pool`, `nli_handle`, `inference_config`, `confidence_params`, `categories`, `observation_registry`) are in scope at `build_project_server` (main.rs:1084-1091), needing only threading. If lifecycle/ownership differs from the daemon's, threading is more than plumbing.
- **A4 (Non-Goals, SCOPE L50-58):** Serial loop is correct "for modest N." If real OSS deployments register large N slugs, the serial tick falls behind before Step B exists — accepted, but the design must not preclude Step B and the cadence assumption should be stated.

## Design Recommendations

1. **SR-01 + SR-07 + SR-08 are one structural decision.** The per-slug `ServiceLayer` owns one handle set; the tick mutates exactly that set; serving reads exactly that set. Resolve OQ-1 so resolve/maintain/serve are the SAME handles — this collapses three High risks at once.
2. **Prove the contract at N=2, never N=1 (SR-07, direct #4974 precedent).** AC-4 is the load-bearing test and doubles as the concurrency-readiness proof. Apply the verify-the-funnel checklist: grep for discarded handles, ensure no parallel global-handle write path beside the per-slug seam.
3. **Hold the Step B boundary (SR-04).** Build the BackgroundJob seam + register today's ops as jobs; build NO queue/pool/residency/cadence. Serialized rayon comes free from the serial loop (SR-02).
4. **Additive constructor refactor (SR-03).** Preserve the test-default path; single-project daemon and per-slug must share the parity path — one isolation seam (vnc-034 ADR-003), no cloud-only branch.
5. **Close the parity definition (SR-05).** Resolve OQ-5; AC-1 asserts field-by-field equality with the daemon's resolved config.
