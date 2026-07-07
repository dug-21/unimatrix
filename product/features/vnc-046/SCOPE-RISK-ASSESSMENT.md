# Scope Risk Assessment: vnc-046

Per-slug state isolation for the cloud (HTTPS) observe path. Product/scope-level risks surfaced BEFORE
architecture. IDs traced forward in RISK-TEST-STRATEGY.md.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Reshaping `ObserveContext` from flat global handles to per-request resolution (registry, pending, services) touches the observe hot path — every `/v1/{slug}/observe` call and every per-slug tick context (`main.rs:1237`). A mis-shaped funnel or a lock/DB touch on the resolve path is a latency regression on the busiest surface. | Med | Low | Resolve methods must be O(1) Arc-clone map lookups on `Arc<dyn StoreResolver>` beside `resolve_store` (audit; pattern #5629) — no I/O, no lock, no side-map (#4974 guard). Architect must state the hot-path cost class explicitly. |
| SR-02 | Inventory incompleteness — the known set grew 2→9 ("the more you look, the more you find"). Latent per-slug fields outside the 9 (e.g. per-slug `ExtractionContext`/neural enhancer #5170; `client_type_map`) could ship still-global, recreating the split-brain on a new field. | High | Med | AC-08 boot assertion must guard the whole "constructor-default never overwritten" CLASS, not just the 9 enumerated fields — enumerate every `UnimatrixServer::new` test-default and classify each as per-slug / correctly-global / correctly-per-instance at boot. |
| SR-03 | `transcript_hold` wired without its paired `session_registry` splits the purge gate → held buffers never purge → unbounded memory growth (design-reviewer F1). A silent OOM on long-lived cloud instances. | High | Low | Constrain the pair to move together and land BEFORE the tick-context loop (`main.rs:1229`). Architect treats them as one unit; boot assertion checks the registry carries a wired hold. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | P3 config-family (OQ-1) cut for speed ships working transcript+knowledge isolation beside config still reading builtin/global defaults — the exact inconsistent half-isolation this feature exists to kill. C6 stays partial. | Med | Med | Take P3 IN-SCOPE (uni-zero + researcher concur; one `build_project_server` pass, 3 available params). If cut, P1+P2 are the floor; human files the ADR-007-seam follow-up with a PR risk note. |
| SR-05 | Behavioral suite gives FALSE CONFIDENCE if an invariant is not observable through the public `/v1/{slug}/…` interface. OQ-3 config fields with no clean public surface, if dropped from the suite, leave that invariant unproven behaviorally while the suite still reads all-green. | High | Med | Any in-scope config field lacking a public observation point gets a white-box guard (AC-08 boot assertion + wiring-pin unit) as a DOCUMENTED AC-06 exception — never silently omitted. The suite must enumerate which invariants it does/doesn't cover behaviorally, so a gap is visible, not implied. |
| SR-06 | The isolation suite passing at N≥2 is NOT sufficient — a one-directional probe (write A, assert A-present/B-empty) false-GREENs the symmetric failure (B's route mis-resolving INTO A's store): the victim's own store stays correctly empty and a mis-routed handler still returns non-404. Lesson #5348 — two independent reviews missed this. | High | Med | Every INV-T*/K*/C* must be BIDIRECTIONAL: drive each slug's write through its OWN route and assert present-in-own AND absent-in-every-other in BOTH directions (pattern #5347). Do not lean on a `debug_assert!` for the un-probed direction — it is compiled out of release, giving zero behavioral coverage. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | **P2 cross-project knowledge-READ leak (privacy).** Observe-path briefing/search/compact read the GLOBAL `ServiceLayer`, so a per-slug agent reads the WRONG project's persisted knowledge store (`listener.rs:1417/1498/1534/1587`). vnc-038 isolated only the store WRITE funnel. Blast radius: a co-hosted tenant's knowledge surfaces in another project's briefings — persistable via distillation, not transient. | High | High (present on cloud today) | Confirm P2 non-negotiable in-scope (OQ-2). Regression vector to guard: any future `ObserveContext` field added as a flat global handle, or the resolver bypassed by a side-map. AC-08 must cover `services`; INV-K1/K2 must be bidirectional (SR-06). |
| SR-08 | Overlap with **open #800** (infra-001 multi-slug HTTP fixture) — the vehicle for the INV-C1/C2 config-parity proof. Duplicated or diverging fixtures risk proving config-parity twice, or a fixture built here that #800 must then reconcile. | Med | Med | Coordinate before building INV-C fixtures: extend the #800 multi-slug HTTP fixture (cumulative test infra) rather than fork one. This is also C6's single path to proven. Architect/tester confirm the fixture owner. |
| SR-09 | Overlap with **open #925** (cycle-review foreign-session sweep) — same family as INV-T2 (cross-slug transcript fold under an identical `{phase}-{NNN}` name). Per-slug registries may SUBSUME #925, or #925 may be a needed defense-in-depth layer atop structural isolation. | Med | Med | Architect must reconcile in the ADR: does per-slug-registry-by-construction make #925's sweep redundant, or is the sweep retained as belt-and-suspenders? Avoid shipping two overlapping mechanisms without a stated relationship. Human owns any #925 close/keep call. |

## Assumptions

- **NG-5 / Problem Statement** — the slug is on the URL (`/v1/{slug}/observe`) and parsed to `ProjectKey::Slug`
  in `route_observe` BEFORE any registry write. If false, the entire no-wire-change premise collapses. Confirmed
  by the #930 investigator addendum (PER-SLUG-ROUTING-VIABLE) — treat as validated, but the architect owns re-confirming.
- **Background Research (crt-056)** — the per-slug `ServiceLayer` is already config-correct, so P2 only needs
  `ObserveContext` to stop reading the global one. If crt-056's per-slug ServiceLayer is not actually config-driven,
  P2 is a deeper fix than "resolve-per-request." Architect verifies before scoping P2 as mechanical.
- **F2 / Isolation Invariants** — `{phase}-{NNN}` feature-cycle names collide across co-hosted slugs in practice
  (shared vocabulary). INV-T2 depends on this being a realistic case, not exotic — the whole cross-project fold
  leak severity rests on it. Confirmed plausible by two reviewers; do not downgrade to one-tenant-per-server.

## Design Recommendations

1. **SR-02 + SR-07**: Make AC-08's boot assertion the structural backstop for the whole per-slug field class —
   it catches unwired fields the behavioral enumeration misses. Required complement, not substitute (OQ-4).
2. **SR-05 + SR-06**: The behavioral suite is the primary gate but is only trustworthy if bidirectional and if it
   declares its own coverage gaps. Instruct the tester: bidirectional per direction, N≥2, no `debug_assert` reliance
   for the un-probed direction, and an explicit list of any invariant covered only white-box.
3. **SR-04**: Land P3 in-scope; if speed forces a cut, P1+P2 is the floor and the config gap ships with a PR risk note.
4. **SR-08 + SR-09**: Reconcile with #800 (reuse the fixture) and #925 (state the subsume-or-defense-in-depth
   relationship) in the ADR before delivery, not after.

## Knowledge Stewardship
- **Queried:** `context_search` for per-slug isolation risk patterns and behavioral-test false-confidence — surfaced
  #5629 (governing construction-parity pattern), #5348 (bidirectional isolation test lesson — a one-directional probe
  false-GREENs a reverse mis-route; `debug_assert` compiled out = zero release coverage), #5347 (bidirectional N×M
  gate shape), #5175 (config-parity tests drive the provisioner's public assembly from the external crate), #5170
  (per-slug ExtractionContext/neural enhancer — a per-slug field OUTSIDE the ADR-003 handle bundle, evidence for
  SR-02 inventory-incompleteness). Read all four #930 GH comments (investigator + addendum, design review, uni-zero
  product review, architect audit + 9-item inventory) and the uni-zero scope review.
- **Stored:** nothing novel — the governing invariant is #5629 and the bidirectional-test lesson is #5348, both
  already captured; feature-specific risks live in this document, and no cross-feature (2+) risk pattern emerged that
  is not already stored.
- **Declined:** storing #930 defect specifics (bugs are GH issues, not lessons — standing rule).
