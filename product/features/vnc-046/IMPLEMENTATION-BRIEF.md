# vnc-046 — Implementation Brief

Per-slug state isolation for the cloud (HTTPS) observe path. Coordination artifact for
Session 2 delivery — routes to the source docs; it does not restate their technical
decisions. Read the linked docs for the authoritative detail.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-046/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-046/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/vnc-046/specification/SPECIFICATION.md |
| Architecture | product/features/vnc-046/architecture/ARCHITECTURE.md |
| ADR-001 (resolution funnel) | product/features/vnc-046/architecture/ADR-001-per-slug-resolution-funnel.md |
| ADR-002 (construction parity) | product/features/vnc-046/architecture/ADR-002-per-slug-construction-parity.md |
| ADR-003 (boot assertion / class guard) | product/features/vnc-046/architecture/ADR-003-boot-assertion-class-guard.md |
| ADR-004 (behavioral isolation seam) | product/features/vnc-046/architecture/ADR-004-bidirectional-behavioral-isolation-seam.md |
| ADR-005 (#925 reconciliation) | product/features/vnc-046/architecture/ADR-005-issue-925-reconciliation.md |
| Risk-Test Strategy | product/features/vnc-046/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-046/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/vnc-046/ACCEPTANCE-MAP.md |

## Lineage

- **Supersedes bug #930** — the cloud/HTTPS `transcript_delta` fold defect. #930 is entirely
  within this feature's **P1**; it is the entry point and cannot regress (INV-T1 / AC-01). Do
  **not** close #930 without a human decision — note it will be resolved by this feature's P1.
- **#925 kept OPEN — disjoint defense-in-depth (ADR-005).** #925 (cycle-review foreign-session
  metrics sweep) is on a *different plane* (metrics-plane SQL, cross-feature within one slug) than
  this feature's INV-T2 (transcript-candidate plane, cross-slug). NOT subsumed. Human owns any
  #925 close/keep call; the PR must state the plane × granularity distinction so no reviewer
  closes it as "subsumed."
- **Depends on #800 (infra-001 multi-slug HTTP fixture) — OPEN.** The behavioral suite (AC-06)
  and the INV-C config-parity proof **extend** the #800 fixture rather than fork one (SR-08,
  ADR-004). Fixture owner unconfirmed — see Alignment Status WARN.

## Goal

Restore full observe-path fidelity over HTTPS for every project slug (HTTPS behavior equals
local UDS) and make cross-project contamination on the transcript, knowledge-read, and config
paths **structurally impossible** on a co-hosted multi-project cloud server — by completing the
vnc-038 per-request resolution funnel once at the seam, not by patching instances. Ship a
solution-independent bidirectional behavioral isolation suite as the first-class durable
guardrail, and convert the whole "constructor-default field never overwritten on the per-slug
path" bug class from silent-read-zero into loud-at-boot.

## Wave / Pattern Structure

The #930 architect audit split 9 NEEDS-PER-SLUG-FIX state items into three fix patterns, plus a
vestigial deletion and a guardrail seam. Deliver all in one pass at the seam (refusing the
inconsistent half-isolation of fixing some and leaving others global). Governing pattern: #5629
(construction parity + funnel completeness + `Arc::ptr_eq` boot guard).

| Pattern | Scope | State items | Closes | FRs |
|---------|-------|-------------|--------|-----|
| **P1** | Construct per-slug + converge write/read (mutable shared state) | `session_registry`, `transcript_hold` (paired), `pending_entries_analysis` | #930 core; F2 cross-slug transcript fold | FR-1…FR-5 |
| **P2** | Resolve-per-request `ObserveContext.services` | `services` | P2 cross-project knowledge-**read** leak (SR-07, privacy/security floor) | FR-6, FR-7 |
| **P3** | Static per-slug config-snapshot overwrite in `build_project_server` | `observation_registry`, `inference_config`, `store_config`, `retention_config`, `transcript_signal_class_names` | Config-parity (INV-C1/C2); `signal_class_counts_json == "{}"` symptom | FR-8…FR-10 |
| **Vestigial** | Delete dormant split-brains | `ObserveContext.vector_store`, `ObserveContext.adapt_service` | AC-09 | FR-11 |
| **Guardrail seam** | One-funnel resolution methods + loud-at-boot class guard | `registry_for`/`pending_for`/`services_for`; `assert_per_slug_isolation` + field census | AC-08; SR-02 class closure | FR-12, FR-13 |

**Load-bearing pairing constraint (F1 / SR-03):** `transcript_hold` MUST move as a constructed
pair with `session_registry`, wired **before** the `main.rs:1229/1237` tick-context loop clones
it. Registry-alone splits the purge gate → held buffers never purge → unbounded memory growth.
Never wire one without the other.

## Component Map

Components from the architecture. **Stage 3a complete** — all 7 component pseudocode + test-plan
files below are present and verified 1:1 against this map (paths confirmed by the Delivery Leader).

| Component | Source file(s) | Pseudocode | Test Plan |
|-----------|----------------|-----------|-----------|
| resolution-funnel (`StoreResolver` trait) | `http/router/seam.rs` | pseudocode/resolution-funnel.md | test-plan/resolution-funnel.md |
| project-resolver (`MultiProjectRouter` / `ProjectEntry`) | `http/router/project_resolver.rs` | pseudocode/project-resolver.md | test-plan/project-resolver.md |
| observe-context (`ObserveContext` reshape) | `http/router.rs` | pseudocode/observe-context.md | test-plan/observe-context.md |
| observe-handler (`route_observe`) | `http/router/handlers.rs` | pseudocode/observe-handler.md | test-plan/observe-handler.md |
| project-provisioner (`build_project_server`) | `http_provision.rs` | pseudocode/project-provisioner.md | test-plan/project-provisioner.md |
| boot-assertion (`assert_per_slug_isolation` + field census) | `main.rs` | pseudocode/boot-assertion.md | test-plan/boot-assertion.md |
| isolation-suite (behavioral INV-T/K/C) | `tests/` (extend `project_routing_integration.rs`, reuse #800 fixture) | pseudocode/isolation-suite.md | test-plan/isolation-suite.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

### #800 multi-slug HTTP fixture — IN-SCOPE (integration-harness deliverable, human directive)

Folded into scope for this delivery (was a dependency). The infra-001 Python fixture
(`multi_slug_http_server` conftest boots HTTP with ≥2 registered slugs + per-slug `config.toml`;
per-slug MCP client extending `harness/client.py`; new `suites/test_project_isolation.py`) is
**built and run by the tester in Stage 3c** per `test-plan/OVERVIEW.md` — it is NOT a Stage 3b
rust/js dev-wave component. Extends infra-001, does not fork (SR-08).

### Stage 3a Open Questions (routed to Gate 3a → Stage 3b)

Surfaced by the pseudocode/test-plan agents; resolve at Gate 3a or in the owning Stage 3b component:
1. **Boot-assertion vs `from_servers` move (needs sign-off).** ADR-003's `assert_per_slug_isolation(input: &ProjectServerInput, ...)` collides with `from_servers` consuming inputs before the resolver exists. Recommended refinement: capture a per-slug `IsolationProbe` (Arc clones) in the existing pre-move loop, assert after the router is built. → boot-assertion component.
2. **Per-slug signature scanner.** Daemon builds its registry `.with_signature_scanner(...)` (`main.rs:852`); ADR-002 param list omits it. Confirm whether FR-9 `signal_class_counts` needs a per-slug scanner or derives from `transcript_signal_class_names` alone. → project-provisioner.
3. **`categories` classification mismatch.** NFR-5 says global; code threads per-slug `slug_categories` (`main.rs:1183`). The census must classify to match the code, not the brief. → boot-assertion census.
4. **Vestigial-field blast radius.** Deleting `vector_store`/`adapt_service` forces dropping the two `_`-params from `dispatch_request` across ~100 call sites (mostly `uds/listener.rs` tests) in ONE pass — cannot be half-done. → observe-context / observe-handler wave must own this atomically.
5. **#800 fixture cert/bearer reuse** and **`inference_config` boot sentinel checkability** — confirm in Stage 3b (test-plan OQ-1/OQ-2).

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| How per-slug observe state is resolved | New `registry_for`/`pending_for`/`services_for` methods **on `Arc<dyn StoreResolver>`** beside `resolve_store`/`adapter_for`, resolving from the same `slug → ProjectEntry` map. **No parallel side-map** (vnc-034 #4974 guard). No trait default impl. | OQ (mechanism), SR-01 | ADR-001 |
| Where per-slug state is constructed | Full construction parity in `build_project_server`: build the per-slug registry+hold pair, pending, and set the 5 config-snapshot fields; `ProjectEntry::from_server` `Arc::clone`s the handles off `server` before it moves into `McpAdapter`. | OQ-1 (P3 in-scope), SR-03/04/07 | ADR-002 |
| P3 config-family scope boundary | **IN-SCOPE now** (same `build_project_server` pass, 3 new params-at-end). Not deferred. If speed forces a cut, P1+P2 is the floor and the boot assertion still fires loud on the unwired config fields. | OQ-1 (resolved) | ADR-002 |
| P2 `ObserveContext.services` | **CONFIRMED in-scope** — cross-project knowledge-read leak with a privacy/security dimension; security-class floor. | OQ-2 (resolved) | ADR-001, ADR-002 |
| Regression / class guard | Real runtime boot assertion (`assert_per_slug_isolation`, returns `Result<(), ServerError>`, aborts boot) + compile-time **exhaustive field census** (no `..`) closing the whole default-never-overwritten class — NOT a `debug_assert`, NOT only the 9 known fields. | Goal 4, SR-02/06, OQ-4 | ADR-003 |
| Primary acceptance gate | **Bidirectional N≥2 behavioral suite** through the public `/v1/{slug}/…` interface, assembled production wiring; white-box guards are required **complements**, never substitutes. Two config fields lacking a public surface get a documented AC-06 white-box exception. | Goal 3, SR-05/06/08, OQ-3/4 | ADR-004 |
| #925 relationship | **NOT subsumed** — orthogonal plane (metrics-plane cross-feature vs transcript-candidate cross-slug). #925 stays open on its own track; human owns close/keep. | SR-09 | ADR-005 |

## Files to Create / Modify

| File | Change |
|------|--------|
| `http/router/seam.rs` | Extend `StoreResolver` trait with `registry_for`/`pending_for`/`services_for` (no default impl). |
| `http/router/project_resolver.rs` | `ProjectEntry` gains `session_registry`/`pending_entries_analysis`/`services` fields; `from_server` `Arc::clone`s them off `server` **before** the move into `McpAdapter`; new methods resolve O(1). |
| `http/router.rs` | Reshape `ObserveContext` to `{ resolver, embed_service, server_version }`; delete 3 global handles + 2 vestigial fields. |
| `http/router/handlers.rs` | `route_observe` resolves registry/pending/services from the already-parsed `key`, passes to `dispatch_request`; 500-not-404 mapping for post-store `*_for` errors. |
| `http_provision.rs` | `build_project_server`: construct per-slug registry+hold pair + pending; set the 5 config-snapshot fields; append 3 params (`store_config`, `retention_config`, `signal_class_names`). |
| `main.rs` | Add `assert_per_slug_isolation` (generalizing `assert_wave_b_precondition`), called once per built slug at boot; add exhaustive `UnimatrixServer` field-census (no `..`); update the `build_project_server` call site + tick-loop ordering. |
| `uds/listener.rs` | `dispatch_request` call sites: pass resolved `&registry`/`&pending`/`&services`; remove the two vestigial `_`-params if their last live reference is gone. |
| Test doubles (`http/router/tests.rs` ~1982/2004/2472/2651) | Each `StoreResolver` double implements the 3 new methods resolving from its **own** `resolve_store` map — never a fresh/global handle (R-06). |
| `tests/` (extend `project_routing_integration.rs`) | Behavioral isolation suite (INV-T/K/C), N≥2, bidirectional, assembled wiring, reusing the #800 fixture. |

## Data Structures

Full signatures in ARCHITECTURE.md "Integration Surface". Key shapes:

- `ProjectEntry` (extended): `+ session_registry: Arc<SessionRegistry>`,
  `+ pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>`, `+ services: ServiceLayer` —
  `Arc::clone`d from `server` in `from_server` before it moves into `McpAdapter::new`.
- `ObserveContext` (reshaped): `{ resolver: Arc<dyn StoreResolver>, embed_service: Arc<EmbedServiceHandle>, server_version: String }` —
  DROP `session_registry`, `pending_entries_analysis`, `services`, `vector_store`, `adapt_service`.
- `ProjectKey` / `RouteError` — unchanged (`enum ProjectKey { Slug(ProjectSlug) }`;
  `enum RouteError { UnknownProject, InvalidSlug(String) }`).

## Function Signatures (new / changed)

```rust
// ADR-001 — new on the StoreResolver trait (no default impl)
fn registry_for(&self, key: &ProjectKey) -> Result<Arc<SessionRegistry>, RouteError>;
fn pending_for(&self, key: &ProjectKey)  -> Result<Arc<Mutex<PendingEntriesAnalysis>>, RouteError>;
fn services_for(&self, key: &ProjectKey) -> Result<ServiceLayer, RouteError>;

// ADR-002 — build_project_server gains 3 params-at-end (crt-056 idiom)
//   append: store_config: &Arc<StoreConfig>,
//           retention_config: &Arc<RetentionConfig>,
//           signal_class_names: &Arc<Vec<String>>

// ADR-003 — per-slug boot assertion, returns Result so boot aborts loud
fn assert_per_slug_isolation(
    input: &ProjectServerInput,
    resolver: &dyn StoreResolver,
    config: &UnimatrixConfig,
) -> Result<(), ServerError>;
```

**Error boundary:** an `Err` from a `*_for` method *after* `resolve_store` already succeeded is a
boot-wiring contradiction (foreclosed by ADR-003's boot assertion) → map to `500`, never `404`,
never panic (R-14). `UnknownProject` for a truly unregistered slug still 404s upstream, unchanged.

**Hot-path cost (SR-01 / NFR-1):** each `*_for` is one `HashMap` lookup + `Arc::clone`
(`ServiceLayer::clone` = a handful of `Arc::clone`s) — no I/O, no lock, no DB; same cost class as
the existing `resolve_store(&key)`. `ProjectKey` parsed once (Step 0) and reused.

## Acceptance Gate — Bidirectional Invariant Suite

The durable guardrail (ADR-004). Every behavioral invariant AC (AC-01…AC-05) is **bidirectional
at N≥2** (lesson #5348 / pattern #5172): register slugs A and B on one cloud instance, drive each
slug's write through its **own** `/v1/{slug}/…` route, and assert per data class BOTH
(i) present-in-own (fidelity) AND (ii) absent-in-every-other (isolation), in **both** directions.
A one-directional probe false-GREENs the symmetric reverse mis-route (SR-06). Assembled
production wiring only (POST delta via `route_observe` → read via that slug's `McpAdapter`) — no
hand-passed handles, no `Arc::ptr_eq`/field-overwrite in the behavioral crate (AC-06).

- **Transcript (P1):** INV-T1 fidelity (AC-01, #930), INV-T2 cross-slug isolation under identical
  `{phase}-{NNN}` name — assert count + **distillation-input** exclusion (AC-02), INV-T3
  pending-entries isolation (AC-03).
- **Knowledge (P2):** INV-K1/K2 (AC-04) — own-read fidelity + cross-read isolation, plus a
  **persistence-level** check that distillation cannot durably contaminate the victim store.
- **Config (P3):** INV-C1/C2 (AC-05) — derive config over the wire via `resolve_slug_config` →
  `build_project_server`, never seeded. Public surfaces: `signal_class_names → signal_class_counts`
  (`cycle_review`), `observation_registry → status`, `retention_config → purge behavior`.
- **Documented AC-06 white-box exception:** `store_config` (byte-limit) and `inference_config`
  (briefing blend) lack a clean public surface → covered by the ADR-003 boot assertion + a
  bidirectional wiring-pin unit, **enumerated in the suite's coverage list**, never silently
  omitted (SR-05).

White-box complements (AC-08): boot assertion (`Arc::ptr_eq` registry/pending convergence +
`has_transcript_hold()` pairing + P3 non-default sentinels) and the compile-time field census
(exhaustive destructure, no `..` → a new field is a compile error until classified).

## Constraints

- **No wire/protocol/client change** (NFR-3, NG-5) — slug rides the URL, parsed to `ProjectKey::Slug`
  in `route_observe` pre-write. Server-side only.
- **One-funnel discipline** (FR-12) — resolution methods on `Arc<dyn StoreResolver>`; no parallel
  side-map (vnc-034 #4974 guard). No trait default impl (a default re-admits the bypass).
- **`transcript_hold` pairs with `session_registry`** (FR-2/3, SR-03) — wire together, before the
  `main.rs:1229/1237` tick loop.
- **Correctly-global handles stay global** (NFR-5) — `embed_service` (one ONNX model), `categories`
  (operator allowlist). `client_type_map` stays correctly-per-instance.
- **Release-hard, not `debug_assert`-only** (NFR-2, SR-06) — the un-probed isolation direction must
  have release-observable coverage via the behavioral suite.
- **Behavioral tests N≥2, assembled production wiring, bidirectional**, in the external `tests/`
  crate; extend existing fixtures — `project_routing_integration.rs`, #800 fixture, pattern #5172.
  Never hand-pass a registry into `dispatch_request` (R-02, structurally hides instance-split).
- **#930 must be resolved (P1)** — transcript fold is the entry point; cannot regress.
- **Workspace rules** (NFR-7) — no `unsafe`, no non-test `.unwrap()`, ≤500 lines/file, clippy clean,
  #878 `--jobs 1` link discipline for the large server test binaries.

## Dependencies

- **#800 (infra-001 multi-slug HTTP fixture) — OPEN.** Vehicle for the N≥2 suite + INV-C proof;
  extend, do not fork (SR-08). Confirm owner before building INV-C fixtures. Also capability C6's
  single path to proven.
- **#925 — OPEN, kept open (ADR-005).** Orthogonal metrics-plane defect; not subsumed.
- **vnc-034 (ADR-007)** — slug identity + per-slug DB/vector isolation; the seam this realizes.
- **vnc-038 (ADR-003 #5082)** — per-request store funnel; this completes it (registry/pending/services/config).
- **vnc-040 (#5209/#5217)** — `resolve_slug_config`, the P3 config source.
- **crt-056** — per-slug config-driven `ServiceLayer`; confirmed config-driven against code (ADR-002).
  P2 only needs the resolver to hand it back per-request.
- **crt-054 / ADR-010** — `assert_wave_b_precondition`, extended by ADR-003.
- **Governing patterns:** #5629 (construction parity + funnel completeness + boot guard), #5172/#4974
  (N=2 model-free isolation / N=1 blindness), #5348/#5347 (bidirectional test shape), #5427
  (source-assertion tests blind to argument threading), #5285 (cloud parity must derive over the wire).
- **Crates:** unimatrix-server (primary), unimatrix-store/core as consumed. No new external services.

## NOT in Scope

- **NG-1** Per-user / per-OAuth-subject isolation (`http-{subject_hash}-…`) — enterprise per-USER
  boundary. This feature isolates by **slug**, not user.
- **NG-2** Multi-TENANT isolation — OSS is single-tenant, N-projects; enterprise seam-only.
- **NG-3** vnc-027 UDS↔HTTP transport session-id split (#4828) — separate pre-existing family.
- **NG-4** Local UDS/stdio paths — already correct; MUST remain untouched (NFR-4).
- **NG-5** Wire / protocol / client changes.
- **NG-6** Prescribing the mechanism beyond the ratified ADRs.
- **NG-7** Cross-project knowledge sharing / owner-store fan-out — enterprise out-of-scope (#5519).
- **Correctly-global** (`embed_service`, `categories`) and **correctly-per-instance**
  (`client_type_map`) fields are NOT per-slug-ified.
- **#925 metrics-plane fix** — different plane; stays on its own track (ADR-005).

## Alignment Status

Vision guardian returned **PASS** (5 PASS, 1 WARN, 0 VARIANCE, 0 FAIL). Directly realizes goal
#5519's OSS per-project isolation invariant + C0 fidelity via the prescribed one-funnel mechanism;
honors Architectural Principles 3/6/7; enterprise boundaries correctly deferred as seam-only.

**WARN (carry as a delivery dependency, does not block design):** the primary behavioral gate
(AC-06 suite + INV-C proof) depends on **OPEN #800** (multi-slug HTTP fixture) with an
**unconfirmed owner**. If #800 slips or its fixture shape diverges, the primary gate has no vehicle.
**Action:** confirm #800 status/owner before Session 2 delivery. This is the same coordination point
as SR-08 / ADR-004 fixture-reuse / capability C6's path to proven.

## Knowledge Stewardship

- **Queried:** read-only compilation of the vnc-046 design artifacts (SCOPE, SCOPE-RISK,
  SPECIFICATION, ARCHITECTURE, ADR-001…005, RISK-TEST-STRATEGY, ALIGNMENT-REPORT). No Unimatrix
  query needed — this synthesizer compiles existing artifacts; governing knowledge (#5629, #5348,
  #5172, #5427, #5285) is already captured and cited in the source docs.
- **Stored:** nothing — synthesizer is storage-exempt. Architectural decisions live in the ADRs
  (architect owns any Unimatrix ADR write); #930 defect specifics belong on the GH issue (bugs are
  GH issues, not lessons).
- **Declined:** creating any new ADR or lesson — no novel cross-feature knowledge emerged from
  compilation.
