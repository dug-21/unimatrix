# Risk-Based Test Strategy: vnc-046 — Per-Slug State Isolation for the Cloud (HTTPS) Observe Path

Source docs: `SCOPE.md`, `SCOPE-RISK-ASSESSMENT.md` (SR-01…SR-09), `architecture/ARCHITECTURE.md`,
`architecture/ADR-001…005`, `specification/SPECIFICATION.md` (FR-1…14, NFR-1…7, AC-01…10).
Diagnosis of record: GH #930. Governing patterns: #5629 (construction parity + funnel completeness +
`Arc::ptr_eq` boot guard), #5172 (N=2 model-free isolation), #5348/#5347 (bidirectional test shape),
#5427 (source-assertion tests blind to argument threading), #5285 (cloud parity must derive over the wire).

The durable guardrail is the **bidirectional N≥2 behavioral suite** (ADR-004). This strategy risk-assesses
where that suite can still **false-GREEN**, maps every INV-T/K/C invariant to concrete scenarios and coverage
requirements, and pins the two white-box-only config fields, the field-census guard, the hot path, and the
#800/#925 coordination planes.

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Isolation suite proves only one direction (write A / assert A-present + B-empty); the symmetric reverse mis-route (B's route resolving INTO A's state) false-GREENs — victim's own store stays correctly empty, mis-routed handler returns non-404 (#5348) | High | Med | **Critical** |
| R-02 | Behavioral test hand-passes a registry/services handle into `dispatch_request` (or seeds the server field) instead of driving `route_observe` → that slug's `McpAdapter`, structurally hiding the instance-split it exists to catch (#5285; N=1 blindness #4974) | High | Med | **Critical** |
| R-03 | Field-census guard (ADR-003 §2) false-passes: a field is *classified* PER-SLUG in the exhaustive destructure and *set* on the server, but the write path (`dispatch_request` / observe read) still touches a global for it — the census is a source-assertion, blind to argument threading (#5427) | High | Med | **Critical** |
| R-04 | `store_config` + `inference_config` carry a white-box AC-06 exception (no clean public surface). If the wiring-pin unit is weak, value-only (not instance-pinned), or silently omitted, config isolation for these two fields ships unproven while the suite reads all-green (SR-05) | High | Med | **High** |
| R-05 | `transcript_hold` wired without/after its paired `session_registry` (or paired but landed after the `main.rs:1229` tick-loop clone) → purge gate splits → held buffers never purge → unbounded memory on long-lived cloud instances (SR-03, FR-2/FR-3) | High | Low | **High** |
| R-06 | `ObserveContext` reshape blast radius: the 3 new no-default `StoreResolver` methods force ~4 test doubles (`tests.rs:1982/2004/2472/2651`) to implement them; a double that returns a fresh/global registry re-admits the bypass **inside the test harness**, masking real split-brain (ADR-001 "Harder") | High | Med | **High** |
| R-07 | Latent per-slug field outside the enumerated 9 (per-slug `ExtractionContext`/neural enhancer #5170; `client_type_map`) or a future field ships global; census with a `..` rest or a non-exhaustive match lets it through (SR-02) | High | Med | **High** |
| R-08 | INV-C config-parity test **seeds** the per-slug server config field directly rather than driving it through `resolve_slug_config` → `build_project_server` over the wire — believable-but-fake green that never exercises the real derivation (#5285) | High | Med | **High** |
| R-09 | P2 cross-project knowledge-**read** leak (privacy): observe-path briefing/search/compact read the global `ServiceLayer` today; a regression re-flattens `ObserveContext.services` or adds a new flat global handle, and a co-hosted tenant's persisted knowledge surfaces in another project's briefing — persistable via distillation (SR-07) | High | Med (present today) | **High** |
| R-10 | INV-T2 mechanism gap: `take_transcripts_for_feature` folds on `SessionState.feature == fc` ∪ a held-buffer scan (`infra/session.rs:473-497`); per-slug registries fix the cross-slug case, but if the held-buffer scan or distillation input still commingles under the identical `{phase}-{NNN}` name, INV-T2 fails silently | High | Low | **High** |
| R-11 | Observe hot-path latency regression (SR-01/NFR-1): a `*_for` method that locks the slug map, rebuilds `ServiceLayer`, or touches I/O instead of an O(1) `Arc`-clone lookup — regresses the busiest surface (`/v1/{slug}/observe` + per-slug tick `main.rs:1237`) | Med | Low | **Medium** |
| R-12 | #800 (infra-001) multi-slug HTTP fixture not reused / owner unconfirmed → INV-C proof duplicated or a fixture forked here that #800 must later reconcile; C6's single path to proven diverges (SR-08) | Med | Med | **Medium** |
| R-13 | #925 falsely read as subsumed by per-slug registries → reviewer closes #925 → its cross-**feature** metrics-plane leak ships unguarded behind a green vnc-046 suite (SR-09, ADR-005) | Med | Med | **Medium** |
| R-14 | Defensive `Err` from a `*_for` after `resolve_store` already succeeded is mapped to `404` (client error) or panics, instead of `500` (boot-wiring contradiction) — masks a wiring bug as a normal not-found (ADR-001/003 error boundary) | Med | Low | **Medium** |
| R-15 | Distillation persistence blast radius: an INV-T2 / INV-K2 leak is not transient — a cross-slug fold or cross-project knowledge read feeds **persisted** distilled knowledge, so a single leak permanently contaminates the victim project's store | High | Low | **High** |
| R-16 | AC-07 HTTPS==UDS parity asserted by re-seeding rather than driving the same input through both transports, or comparing a wall-clock/`computed_at` field → flake or fake parity (#5285) | Med | Low | **Medium** |

## Risk-to-Scenario Mapping

### R-01: One-directional isolation probe false-GREENs the reverse mis-route
**Severity**: High · **Likelihood**: Med · **Impact**: The suite ships green over a live reverse mis-route (B→A); the exact failure two reviews missed in #5348. Undetectable in production until cross-project contamination surfaces.

**Test Scenarios**:
1. For **every** INV-T/K/C invariant, run BOTH directions as distinct assertions: (A writes via `/v1/{A}/…` → assert present-in-A AND absent-in-B) **and** (B writes via `/v1/{B}/…` → assert present-in-B AND absent-in-A). Neither direction may be inferred from the other.
2. Negative control that would fail a one-directional-only suite: deliberately mis-wire B's route to resolve A's registry in a throwaway harness variant; assert the bidirectional suite RED, confirming the reverse direction is actually exercised (meta-test / mutation check).
3. Assert the isolation half keys on **synchronous observable state** (fold result / returned entries), not the absence of an async effect (#5427 caveat 2).

**Coverage Requirement**: Every AC-01…AC-05 invariant has two named test cases (A-driver, B-driver), each asserting fidelity-in-own AND absence-in-other. No invariant relies on a single writer. A `debug_assert!` for the un-probed direction is **not** coverage (NFR-2 — compiled out of release).

### R-02: Assembled-wiring bypass — hand-passed handles hide the split-brain
**Severity**: High · **Likelihood**: Med · **Impact**: The one bug class this feature exists to catch (write instance ≠ read instance) is invisible if the test injects a shared handle instead of driving production resolution.

**Test Scenarios**:
1. Behavioral suite drives the **assembled production path only**: POST `transcript_delta` via `route_observe` (real `ObserveContext` → `resolver.registry_for(&key)` → `dispatch_request`) → read via that slug's `McpAdapter.cycle_review`. No test may construct a `SessionRegistry` and pass it directly to `dispatch_request`.
2. Run at **N≥2** registered slugs (pattern #5172/#4974) — N=1 cannot distinguish a real per-slug funnel from a global-handle bypass.
3. Review-gate assertion: the behavioral crate contains **no** direct `dispatch_request(registry=…)` hand-pass and **no** `Arc::ptr_eq`/field-overwrite calls (AC-06).

**Coverage Requirement**: All AC-01…AC-07 scenarios route through `route_observe` and `McpAdapter` on ≥2 slugs registered on one cloud instance via the #800 fixture. The suite's own coverage-enumeration comment/table (AC-06) must state this.

### R-03: Field-census guard false-passes on argument threading
**Severity**: High · **Likelihood**: Med · **Impact**: `assert_per_slug_isolation` + the exhaustive destructure prove a field is *classified and set*, but #5427 shows a source-level guard is blind to whether the resolved per-slug handle is actually *used* on the write path. A field set on the server yet read from a global in `dispatch_request` ships green.

**Test Scenarios**:
1. Boot-assertion unit: build a slug server with `session_registry` deliberately left as the constructor default; assert `assert_per_slug_isolation` returns `Err(ServerError)` and aborts boot (not a `debug_assert`).
2. Wiring-pin unit per per-slug handle: `Arc::ptr_eq(resolver.registry_for(&slug), slug_server.session_registry)` and same for `pending_for` — proves the resolver hands back the **instance** the server holds, closing the "set-but-not-threaded" gap for handle-typed fields.
3. Behavioral back-stop (the real enforcement per #5427): INV-T1/T3 drive the write through `route_observe` and read through `McpAdapter`, so a field set-but-read-from-global fails behaviorally even when the census passes.
4. Census compile-guard test: add a throwaway field to `UnimatrixServer`; assert the exhaustive-destructure census **fails to compile** (no `..` rest).

**Coverage Requirement**: For every handle-typed per-slug field (`session_registry`, `pending_entries_analysis`), BOTH a boot-time `Arc::ptr_eq` pin AND a behavioral path assertion exist. The census must be exhaustive (no `..`) and covered by a compile-fail test.

### R-04: store_config + inference_config white-box-only coverage gap
**Severity**: High · **Likelihood**: Med · **Impact**: These two P3 fields have no clean public observation surface (OQ-3). They are the coverage hole SR-05 warns of — the suite can be all-green while their isolation is unproven.

**Test Scenarios**:
1. Wiring-pin unit for `store_config`: build slugs A and B with **different** byte-limits; assert `slug_A.store_config` equals A's resolved config and is **not** the `UnimatrixServer::new` default, bidirectionally (A's ≠ B's). Prefer an instance/value pin derived from `resolve_slug_config`, not a hardcoded literal.
2. Wiring-pin unit for `inference_config`: same shape — A and B declare different inference/blend config; assert each server field equals its own resolved config, neither the default.
3. Boot-assertion coverage: `assert_per_slug_isolation` asserts these config-snapshot fields are the slug's resolved values where a sentinel is checkable (ADR-003 P3 clause).
4. Coverage-enumeration entry: the behavioral suite's coverage list explicitly names `store_config` and `inference_config` as **white-box-only, documented AC-06 exceptions** — never silently omitted.

**Coverage Requirement**: Both fields have a bidirectional wiring-pin unit AND an explicit entry in the suite's coverage-enumeration table. Absence of that enumeration entry is a gate failure (AC-06). These are complements to — never substitutes for — the behavioral suite (OQ-4).

### R-05: transcript_hold / session_registry pairing regression → unbounded memory
**Severity**: High · **Likelihood**: Low · **Impact**: Registry-alone splits the purge gate; held buffers never purge; silent OOM on long-lived cloud instances (SR-03, design-reviewer F1).

**Test Scenarios**:
1. Boot-assertion unit: `input.server.session_registry.has_transcript_hold()` is true for every built slug; build one with an unpaired hold → assert boot `Err`.
2. Ordering test: assert the registry+hold pair is constructed **inside** `build_project_server`, before the `main.rs:1229/1237` tick-context loop clones `input.server.session_registry` — a per-slug `PerSlugTickContext` must observe the wired instance, not the default.
3. Behavioral purge test: post deltas to A to fill a held buffer, trigger the purge/sweep gate (retention-driven), assert held buffers actually purge for A (buffer count returns to bound) — proves the paired hold's gate acts on the same instance the drain routes through.

**Coverage Requirement**: Boot assertion pins the wired hold; a behavioral test proves purge actually fires per slug; an ordering assertion covers the tick-loop-clone hazard (FR-3).

### R-06: ObserveContext reshape — test-double bypass re-admits the split-brain in the harness
**Severity**: High · **Likelihood**: Med · **Impact**: The 3 no-default resolver methods force every `StoreResolver` test double to implement `registry_for`/`pending_for`/`services_for`. A double that returns a fresh or shared-global registry silently makes the harness pass while production would split — the bypass moves into the test infra.

**Test Scenarios**:
1. Audit every `StoreResolver` impl (production + the ~4 doubles at `tests.rs:1982/2004/2472/2651`): each `*_for` must resolve from the **same** `slug → ProjectEntry` map its `resolve_store` reads, returning the entry's stored handle — not a freshly-minted or global one.
2. Wiring-pin unit against the **production** resolver (not a double): `Arc::ptr_eq(resolver.registry_for(&slug), slug_server.session_registry)`.
3. Behavioral suite uses the assembled production resolver via the #800 fixture, not a double (R-02) — the primary defense against a lenient double.

**Coverage Requirement**: No test-double `*_for` may return a handle disconnected from that double's `resolve_store` map. The N≥2 behavioral suite runs against production wiring so a lenient double cannot green the isolation gate.

### R-07: Latent per-slug field outside the 9 ships global
**Severity**: High · **Likelihood**: Med · **Impact**: The known set grew 2→9; #5170 (per-slug `ExtractionContext`/neural enhancer) is already a field outside the ADR-003 handle bundle. A new global field recreates the split-brain.

**Test Scenarios**:
1. Compile-fail census test (R-03 §4): exhaustive `UnimatrixServer` destructure, no `..`; adding any field breaks compilation until classified PER-SLUG / CORRECTLY-GLOBAL / CORRECTLY-PER-INSTANCE.
2. Classification review: every `UnimatrixServer::new` test-default is enumerated and classified in the census; PER-SLUG classification routes the field into the boot assertion (guard 1).
3. Spot-check `ExtractionContext`/#5170 and `client_type_map` are explicitly classified (per-slug vs per-instance) so they are not silently left global.

**Coverage Requirement**: Census is exhaustive with a compile-fail test; every field carries a classification; per-slug classification is pinned by the boot assertion.

### R-08 / R-16: Config-parity and UDS-parity must derive over the wire, not seed the join
**Severity**: High / Med · **Likelihood**: Med / Low · **Impact**: #5285 — seeding the server field or the attribution join produces a believable-but-fake green that never exercises the real derivation chain.

**Test Scenarios**:
1. INV-C: drive config through `resolve_slug_config` → `build_project_server` (register slugs A and B with genuinely different declared config via the #800 fixture), then observe `signal_class_counts` / observation categories / purge behavior through the public surface. Do **not** assign `server.<config field>` directly in the test.
2. AC-07 parity: drive the **same** input through HTTPS (`route_observe` → `McpAdapter`) and local UDS; compare `cycle_review` fold / `MetricVector` field-for-field; normalize/exclude wall-clock fields (`computed_at`) to avoid flake (#5285).

**Coverage Requirement**: No INV-C or parity assertion seeds a server config field or a SQL/struct attribution column; all derive from the registered slug config over the transport.

### R-09 / R-15: P2 knowledge-read leak + distillation persistence blast radius
**Severity**: High · **Likelihood**: Med / Low · **Impact**: A per-slug agent's observe-path briefing/search/compact reads another project's persisted knowledge (privacy leak, present on cloud today); worse, a fold/read leak feeds **persisted** distilled knowledge — contamination is permanent, not transient.

**Test Scenarios**:
1. INV-K2 bidirectional: write distinct knowledge to A and B; assert A's observe-path briefing/search/compact returns **none** of B's entries and vice versa (see invariant map below).
2. Persistence assertion: after an INV-T2 collision (identical `{phase}-{NNN}` cycle in A and B), assert B's **distillation input / persisted store** never contains A's transcript-derived entries — not just the transient fold result.
3. Regression guard: review that no new `ObserveContext` field is added as a flat global handle and the resolver is never bypassed by a side-map (FR-12, #4974).

**Coverage Requirement**: INV-K2 asserted bidirectionally through the public interface; a persistence-level assertion confirms the leak cannot enter the durable store via distillation.

### R-10: INV-T2 fold-mechanism gap under identical cycle names
**Severity**: High · **Likelihood**: Low · **Impact**: `take_transcripts_for_feature` folds on `SessionState.feature == fc` ∪ a held-buffer scan; per-slug registries fix cross-slug commingling, but the collision case is the sharp test — an identical `{phase}-{NNN}` name is the realistic scenario (F2 assumption, confirmed plausible).

**Test Scenarios**:
1. INV-T2 (see map): A and B both run `nxs-001`; write to A; assert A folds A's transcript AND B's `cycle_review` for `nxs-001` folds/counts/distills **nothing** of A's; swap roles.
2. Assert both the candidate **count** and the **distillation input** exclude the other slug — not only the returned transcript bytes.

**Coverage Requirement**: INV-T2 tested with an identical cycle name across slugs, bidirectionally, asserting count + distillation-input exclusion.

### R-11: Observe hot-path latency regression
**Severity**: Med · **Likelihood**: Low · **Impact**: The resolve methods sit on the busiest surface (every observe call + every per-slug tick).

**Test Scenarios**:
1. Assert `registry_for`/`pending_for`/`services_for` are each one `HashMap` lookup + `Arc::clone` — no lock held across resolution, no I/O, no `ServiceLayer` reconstruction (`ServiceLayer::clone` is a handful of `Arc::clone`s). Code/diff review against NFR-1.
2. Assert the `ProjectKey` is parsed once per request (Step 0) and reused across all four `*_for`/`resolve_store` calls.

**Coverage Requirement**: The architect's stated cost class (same as `resolve_store`) is confirmed by review; no I/O or lock-acquisition on the resolve path.

### R-14: Defensive Err mapping (404 vs 500)
**Severity**: Med · **Likelihood**: Low · **Impact**: A `*_for` `Err` after `resolve_store` already resolved is a boot-wiring contradiction (foreclosed by ADR-003), not a client 404.

**Test Scenarios**:
1. Unit: with a store resolvable but a `*_for` forced to `Err`, assert `route_observe` returns `500`, never `404`, and never panics.
2. Confirm `UnknownProject` for a truly unregistered slug still returns `404` upstream of the write (NFR-3, unchanged surface).

**Coverage Requirement**: The 500-not-404 mapping for post-store `*_for` errors is unit-tested; the unknown-slug 404 path is regression-covered.

## Invariant → Test-Scenario Map (the durable guardrail)

Every row is **bidirectional** (A-driver and B-driver cases), **N≥2**, **assembled production wiring** (`route_observe` → `McpAdapter`), **no `Arc::ptr_eq`/field-overwrite in the behavioral crate** (AC-06).

| Invariant | AC | Fidelity half (own-read) | Isolation half (cross-read absent) | Coverage requirement |
|-----------|----|--------------------------|-----------------------------------|----------------------|
| **INV-T1** | AC-01 | Delta → `/v1/{A}/observe` under cycle X; `cycle_review` via A folds it (non-empty candidates/bytes) | — (fidelity-only; #930 core) | Both A and B prove own-fold; drives real `route_observe`→`McpAdapter` (R-02) |
| **INV-T2** | AC-02 | A folds A's transcript under identical `{phase}-{NNN}` name | B's `cycle_review` for that name NEVER folds/counts/**distills** A's; and A↛B | Identical cycle name; assert count + distillation-input exclusion, both directions (R-10, R-15) |
| **INV-T3** | AC-03 | Pending-entries at A's `cycle_review` present | Never observable via B; B↛A | Drive pending state per slug through its own surface, both directions |
| **INV-K1** | AC-04 | Knowledge written as A retrievable by A's observe-path briefing/search/compact | — | Own-read fidelity both slugs |
| **INV-K2** | AC-04 | — | A's observe-path reads NEVER return B's entries; B↛A; plus persistence-level check | Bidirectional; assert durable store not contaminated via distillation (R-09/R-15) |
| **INV-C1** | AC-05 | A's declared config governs A's observable behavior (`signal_class_counts`, observation categories, purge) | — | Derive config via `resolve_slug_config`→`build_project_server`, not seeded (R-08) |
| **INV-C2** | AC-05 | — | B's config never governs A's; A and B declare **different** config | Bidirectional; public surfaces for `signal_class_names`/`observation_registry`/`retention_config` |
| **INV-C (store_config, inference_config)** | AC-05/AC-08 | white-box wiring-pin: field == slug's resolved value, ≠ default | white-box: A's ≠ B's | **Documented AC-06 exception** in the coverage list; bidirectional wiring-pin unit (R-04) |

## Integration Risks

- **Resolver funnel ↔ ProjectEntry construction (ADR-001/002):** `ProjectEntry::from_server` must `Arc::clone` the registry/pending/services **before** `server` moves into `McpAdapter::new`; a clone-after-move or a re-mint breaks convergence-by-construction. Pinned by the boot `Arc::ptr_eq` (R-03) and the behavioral suite (R-02).
- **`dispatch_request` signature threading:** the observe write path must pass the resolved `&registry`/`&pending`/`&services`, and the two vestigial `_vector_store`/`_adapt_service` params (already `_`-unused) must be removed with the `ObserveContext` fields (AC-09) — a dangling reference blocks compile; verify no live reader remains.
- **Tick-context loop (`main.rs:1229/1237`):** clones `input.server.session_registry` into `PerSlugTickContext`; the pair must be wired before this clone or per-slug ticks read the default (FR-3, R-05).
- **Test-double parity (R-06):** the no-default trait extension is a deliberate compile-forcing integration point; a lenient double is an integration risk that lives in the harness.
- **#800 fixture (R-12):** the INV-C proof and N≥2 setup share the multi-slug HTTP fixture; divergent fixtures are an integration/coordination risk on C6's path to proven.

## Edge Cases

- Identical `{phase}-{NNN}` feature-cycle name across A and B (INV-T2 sharp case — the realistic collision, not exotic).
- Slug declares **empty** `transcript_signal_class_names` → `signal_class_counts_json` legitimately `"{}"`; distinguish this from the #930 bug symptom (default-fallback empty) so INV-C1 doesn't false-pass/fail on it.
- Slug with no declared config → must fall back to defaults **deliberately** (fidelity), while a slug that declared config must NOT fall back (INV-C1). Test both "declared" and "not declared" slugs.
- Unregistered/unknown slug → `404 UnknownProject` upstream of any write (unchanged, NFR-3).
- Zero-delta / empty transcript → own-fold empty is correct; ensure the isolation assertion keys on synchronous state, not "eventually empty".
- Purge/retention boundary: held buffer at the per-slug `[retention]` cap → purge must fire (R-05).
- N boundary: N=1 must be treated as insufficient (blindness); suite enforces N≥2.

## Security Risks

Untrusted input surface: the **slug in the URL path** (`parse_project_key`) and the **observe payload** (`transcript_delta`, briefing/search/compact requests) on `/v1/{slug}/observe`.

- **Cross-project knowledge-read (P2, SR-07/R-09):** highest-severity security risk — a per-slug agent reading another co-hosted project's **persisted** knowledge store via the global `ServiceLayer`. Blast radius: another tenant's knowledge in your briefings, persistable via distillation (R-15). Closed by FR-6 + INV-K2; regression vector is any new flat global `ObserveContext` handle or a resolver side-map bypass.
- **Cross-project transcript fold (F2, INV-T2):** a co-hosted slug folding/distilling another slug's transcript under a shared `{phase}-{NNN}` vocabulary; feeds persisted distillation (R-15).
- **Malformed/hostile slug:** `parse_project_key` must reject to `InvalidSlug`/404 before any registry write; no path-traversal or injection via the slug into store/analytics-dir resolution (inherited from vnc-034 per-slug DB; confirm not weakened).
- **Blast radius if the funnel is compromised/bypassed:** a single mis-resolving `*_for` method leaks across every co-hosted project on the instance — the boot assertion (AC-08) is the structural containment; it must abort boot, not warn.
- **No new external surface:** NFR-3 — no wire/protocol/client change; the slug already rides the URL and is parsed pre-write.

## Failure Modes (expected behavior under failure)

- **Unwired per-slug field at boot:** `assert_per_slug_isolation` returns `Err(ServerError)` and **aborts boot loud** for the offending slug — never a silent read-zero at review time (AC-08). Not a `debug_assert` (compiled out of release, NFR-2).
- **New unclassified field added to `UnimatrixServer`:** **compile error** at the exhaustive census destructure (no `..`) — cannot ship unclassified (R-07).
- **Post-store `*_for` resolution error:** `500` (boot-wiring contradiction), never `404`, never panic (R-14).
- **Purge gate split:** prevented by the paired-hold boot check; if the hold is unpaired, boot aborts rather than leaking memory silently (R-05).
- **Isolation regression post-merge:** a future rewire that breaks cloud isolation **trips the bidirectional behavioral suite** through the public interface — the durable, mechanism-independent guardrail (Goal 3).
- **#925 metrics-plane leak:** unaffected by this feature (different plane); must NOT be read as subsumed — stays open on its own track (ADR-005, R-13).

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (hot-path reshape) | R-11 | ADR-001 O(1) `Arc`-clone `*_for` on the existing funnel, no side-map; NFR-1 cost class stated; covered by resolve-path review + parse-once assertion |
| SR-02 (inventory incompleteness) | R-07, R-03 | ADR-003 exhaustive compile-time census (no `..`) + boot assertion over the whole default-never-overwritten class; compile-fail test |
| SR-03 (hold/registry pairing → OOM) | R-05 | ADR-002 registry+hold constructed as a pair before the tick loop; boot `has_transcript_hold()` check + behavioral purge test |
| SR-04 (P3 cut → half-isolation) | R-04, R-08 | ADR-002 takes P3 **in-scope**; if cut, boot assertion still fires on unwired config fields (loud, not silent) + PR risk note |
| SR-05 (false confidence / unobservable invariant) | R-04 | ADR-004 coverage-enumeration list; `store_config`/`inference_config` documented AC-06 white-box exceptions, never silently omitted |
| SR-06 (one-directional false-GREEN) | R-01, R-02 | ADR-004 bidirectional N≥2 assembled-wiring suite; no `debug_assert` as sole guard (NFR-2); mutation/negative-control meta-check |
| SR-07 (P2 knowledge-read privacy leak) | R-09, R-15 | ADR-001/002 `services_for` resolves the slug's `ServiceLayer`; INV-K2 bidirectional + persistence-level assertion |
| SR-08 (#800 fixture overlap) | R-12 | ADR-004 **reuse** (extend, not fork) the #800 multi-slug HTTP fixture; confirm owner before building INV-C fixtures |
| SR-09 (#925 subsume-or-keep) | R-13 | ADR-005 verdict: **NOT subsumed** — orthogonal plane (metrics vs transcript-candidate); #925 stays open; PR must state the distinction |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-03) | ~11 — bidirectional both-direction cases per invariant (8+), assembled-wiring routing, boot-assertion + wiring-pin + compile-fail census |
| High | 8 (R-04, R-05, R-06, R-07, R-08, R-09, R-10, R-15) | ~14 — 2 white-box config pins, purge behavioral, ordering, test-double audit, census classification, derive-over-wire config, INV-K2 bidi + persistence, INV-T2 collision |
| Medium | 5 (R-11, R-12, R-13, R-14, R-16) | ~6 — hot-path review, #800 fixture reuse, #925-not-subsumed PR note, 500-not-404 unit, UDS==HTTPS parity |
| **Total** | **16** | **~31 named scenarios** (all AC-01…AC-05 behavioral cases bidirectional at N≥2; AC-06 coverage-enumeration; AC-07 parity; AC-08 boot+census+wiring-pin; AC-09 cleanup; AC-10 ADR) |

## Knowledge Stewardship
- **Queried:** `/uni-knowledge-search` (context_search) for bidirectional-isolation lessons and construction-parity/boot-assertion risk patterns; `context_get` on #5427 and #5285. Surfaced — #5348 (bidirectional isolation test; one-directional probe false-GREENs a reverse mis-route), #5347 (bidirectional N×M tri-state gate shape), #5427 (source-assertion/string-count tests are blind to argument threading — pair with a behavioral per-site matrix; directly informs R-03 field-census false-pass), #5285 (cloud-path parity must DERIVE topic_signal over the wire, not seed the join — informs R-08/R-16 and AC-07/INV-C), #5172/#4974 (N=2 model-free isolation; N=1 blindness), #5629 (governing construction-parity + funnel-completeness + `Arc::ptr_eq` boot-guard pattern), #5170 (per-slug ExtractionContext outside the handle bundle — evidence for R-07). Also read SCOPE, SCOPE-RISK (SR-01…09), ARCHITECTURE, ADR-001…005, SPECIFICATION.
- **Stored:** nothing novel — the two cross-feature risk patterns that generalize beyond vnc-046 (source-assertion tests blind to argument threading; cloud parity must derive over the wire) are already captured as #5427 and #5285, and the bidirectional-isolation lesson as #5348. No 2+-feature risk pattern emerged that is not already stored. Feature-specific risks live in this document; #930 defect specifics belong on the GH issue (bugs are GH issues, not lessons).
