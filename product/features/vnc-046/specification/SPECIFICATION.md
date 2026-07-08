# vnc-046 — Specification: Per-Slug State Isolation for the Cloud (HTTPS) Observe Path

Source: `product/features/vnc-046/SCOPE.md` · Risk: `SCOPE-RISK-ASSESSMENT.md` (SR-01…SR-09) ·
Diagnosis source of record: GH #930 (investigator + addendum, design review, uni-zero review, architect audit).
Scope open questions resolved by `reviews/uni-zero-scope-review.md`: **P3 IN-SCOPE**, **P2 confirmed in-scope**,
white-box guards are **required complements** (not substitutes), config fields lacking a public surface get a
**documented white-box exception** (never silent omission).

## Objective

On a multi-project cloud (HTTPS) instance, each project slug is served by its own per-slug `UnimatrixServer`, but
the observe/write path and several read paths silently fall back to daemon-global state — a per-slug split-brain
where the object that writes and the object that reads are different instances (bug #930). This feature restores
full observe-path fidelity over HTTPS for every slug (HTTPS behavior equals local UDS) and makes cross-project
contamination on the transcript, knowledge, and config paths structurally impossible on a co-hosted server, by
completing the per-slug isolation funnel once at the seam rather than patching instances. It delivers
solution-independent behavioral isolation tests as a first-class deliverable and converts the whole
"constructor-default field never overwritten on the per-slug path" bug class from silent-read-zero into
loud-at-boot.

## Functional Requirements

Each requirement is testable; verification is named in Acceptance Criteria. Requirements are grouped by the three
fix patterns from the #930 architect audit plus vestigial deletion.

### P1 — Construct per-slug + converge write/read (mutable shared state)

- **FR-1** Each per-slug `UnimatrixServer` MUST read transcript state from the same `SessionRegistry` instance that
  its slug's `/v1/{slug}/observe` write path applies deltas into. A `transcript_delta` posted to a slug's observe
  surface MUST be foldable by that slug's `cycle_review` MCP read. (Closes #930 core data-invisibility.)
- **FR-2** `transcript_hold` MUST be wired as a constructed pair with `session_registry` — never one without the
  other. The registry a slug reads MUST carry the same hold its purge/sweep gate acts on. (Per design-reviewer F1 /
  SR-03: registry-alone splits the purge gate → held buffers never purge → unbounded memory growth.)
- **FR-3** The `session_registry`/`transcript_hold` pair MUST be wired **before** the per-slug tick-context loop
  (`main.rs:1229/1237` clones `input.server.session_registry` into `PerSlugTickContext`), so per-slug tick contexts
  read the wired instance, not the empty constructor default. (Sibling tick defect must not stay live.)
- **FR-4** `pending_entries_analysis` MUST be resolved for the same slug on both the observe write path
  (`ObserveContext`) and the per-slug MCP read path (`cycle_review` drain), converging on one per-slug instance —
  the same treatment as `session_registry`, not left global. (Sibling data-invisibility.)
- **FR-5** `ObserveContext` MUST resolve `session_registry` and `pending_entries_analysis` **per request** from the
  parsed `ProjectKey`, replacing the flat daemon-global handles, so the observe write path targets the same per-slug
  instances the MCP read path uses.

### P2 — Resolve per request: cross-project knowledge-read isolation

- **FR-6** `ObserveContext.services` MUST be resolved per request from the parsed `ProjectKey` so observe-path
  briefing/search/compact **reads** (`listener.rs:1417/1498/1534/1587`) go through that slug's `ServiceLayer`, not
  the daemon-global one. A per-slug agent MUST read only its own project's knowledge store. (Closes the vnc-038
  read-side gap: vnc-038 isolated only the store *write* funnel; SR-07 privacy leak — a slug currently reads another
  project's persisted knowledge.)
- **FR-7** P2 MUST reuse the per-slug server's already-config-driven `ServiceLayer` (crt-056); it is a
  resolve-per-request wiring change on `ObserveContext`, not a reconstruction of the service layer. (If crt-056's
  per-slug `ServiceLayer` is not actually config-driven, that is an architect escalation, not a silent scope
  expansion — SCOPE-RISK Assumption.)

### P3 — Static per-slug config-snapshot overwrite (config parity)

- **FR-8** `build_project_server` MUST overwrite the five per-slug config-snapshot fields on the per-slug
  `UnimatrixServer` from that slug's resolved config (vnc-040 `resolve_slug_config`), mirroring the daemon path
  (`main.rs:978-989`): `observation_registry`, `inference_config`, `store_config`, `retention_config`,
  `transcript_signal_class_names`. A slug MUST NOT silently fall back to builtin domain packs, default byte-limit,
  default inference/retention, or empty signal-class names when it has declared its own config.
- **FR-9** `transcript_signal_class_names` MUST be populated from the slug's config so `cycle_review`'s
  `signal_class_counts_json` reflects the slug's declared signal classes rather than `"{}"` (also a #930 symptom).
- **FR-10** Config-snapshot resolution MUST honor the one-funnel discipline (see FR-14): config-parity fields are set
  on the per-slug server at construction; they MUST NOT introduce a parallel slug→config side-map.

### Vestigial deletion

- **FR-11** The two dormant `ObserveContext` fields MUST be deleted in this pass: `vector_store` (`_vector_store`,
  unused in `dispatch_request`, `listener.rs:777`) and `adapt_service` (`_adapt_service`, unused, `listener.rs:779`).
  Leaving them makes them the next split-brain when a caller starts using them.

### Isolation seam / boot guard

- **FR-12** Per-slug resolution of `session_registry`, `pending_entries_analysis`, and `services` MUST be exposed as
  methods on `Arc<dyn StoreResolver>` beside `resolve_store`/`adapter_for` (e.g. `registry_for`/`pending_for`/
  `services_for`). No parallel `slug→X` side-map. (One-funnel discipline; vnc-034 #4974 ceremonial-funnel guard.)
- **FR-13** A per-slug boot assertion (extending crt-054/ADR-010 `assert_wave_b_precondition`) MUST fail loud at
  startup for every built slug server whose in-scope per-slug state is not wired to the instance its write path uses —
  including `Arc::ptr_eq(server.session_registry, resolver.registry_for(slug))` and that `session_registry` carries a
  wired `transcript_hold`. The assertion MUST guard the whole "constructor-default never overwritten" field class —
  every `UnimatrixServer::new` test-default classified per-slug / correctly-global / correctly-per-instance — not
  only the enumerated 9 items. (SR-02 inventory-incompleteness backstop.)

## Non-Functional Requirements

- **NFR-1 (hot-path performance, SR-01)** Per-request per-slug resolution (`registry_for`/`pending_for`/
  `services_for`) MUST be O(1): an `Arc`-clone map/handle lookup, no I/O, no lock acquisition, no DB touch — the same
  cost class as the `resolve_store(&key)` the observe path already runs each call. The `ProjectKey` is parsed once
  per request and reused. The architect MUST state the hot-path cost class explicitly. No latency regression on
  `/v1/{slug}/observe` or per-slug tick contexts (`main.rs:1237`).
- **NFR-2 (release-hard guards, SR-06)** Isolation guarantees MUST hold in **release** builds. No `debug_assert!`
  may be the sole guard for any isolation direction — `debug_assert!` is compiled out of release and yields zero
  behavioral coverage. The behavioral suite (AC-06) provides release-observable coverage; AC-08 white-box guards may
  be `debug_assert!`/boot-assertion, but they are complements to, never substitutes for, the behavioral suite.
- **NFR-3 (no wire/protocol/client change, NG-5)** No client bundle, wire protocol, or edge-client change is in
  scope. The slug rides in the URL (`/v1/{slug}/observe`) and is parsed to `ProjectKey::Slug` in `route_observe`
  before any registry write; isolation is realized server-side only. `UnknownProject` already 404s upstream — no new
  error surface.
- **NFR-4 (local paths untouched, NG-4)** UDS/stdio local paths already share the one daemon-global registry/services
  and are correct; they MUST remain unchanged. The daemon-global registry stays theirs.
- **NFR-5 (correctly-global handles stay global)** `embed_service` (one loaded ONNX model, crt-056 C-3) and
  `categories` (operator allowlist) are NOT per-slug and MUST remain global. Do not per-slug-ify them.
- **NFR-6 (memory bound)** Per-slug registries introduce one `SessionRegistry`+`TranscriptHold` pair per slug;
  personal-cloud N is small, and the paired hold (FR-2) keeps buffer purge functional so growth stays bounded. No
  unbounded per-slug memory growth.
- **NFR-7 (workspace rules)** No `unsafe`; no `.unwrap()` in non-test code; ≤500 lines/file; clippy clean; respect
  #878 build-memory guardrails (`--jobs 1` link discipline for large server test binaries).

## Acceptance Criteria

AC-IDs are preserved from SCOPE.md so they trace downstream. **Every behavioral invariant AC (AC-01…AC-05) is
BIDIRECTIONAL (SR-06 / lesson #5348):** the suite registers two slugs A and B on one cloud instance, drives each
slug's write through its OWN `/v1/{slug}/…` route, and asserts, for each data class, BOTH:
(i) **present-in-own** — the writer's own read surface sees its data (fidelity half), and
(ii) **absent-in-other** — the other slug's read surface never sees it (isolation half),
**in both directions** (A-writes → assert present-in-A AND absent-in-B; B-writes → assert present-in-B AND
absent-in-A). A one-directional probe false-GREENs the symmetric reverse mis-route (B's route resolving INTO A's
store); both directions are mandatory. Runs at **N≥2** (pattern #5172/#4974: N=1 cannot distinguish a real per-slug
funnel from a global-handle bypass). All behavioral assertions go through the public HTTPS interface only — no
`Arc::ptr_eq`, no field-overwrite assertions in the behavioral suite (AC-06).

| AC | Invariant(s) | Statement | Verification method |
|----|--------------|-----------|---------------------|
| **AC-01** | INV-T1 | Transcript fidelity: a `transcript_delta` posted to `/v1/{A}/observe` under a feature cycle is folded by A's `cycle_review` (non-empty candidates / transcript bytes). Bidirectional: also holds for B via B's surface. | Behavioral integration test, assembled production wiring: POST delta via `route_observe` → `cycle_review` via that slug's `McpAdapter`; assert non-empty fold for the writer. Both A and B. (#930 fixed.) |
| **AC-02** | INV-T2 | Transcript isolation (collision case): with A and B using the **identical** `{phase}-{NNN}` cycle name, `cycle_review` via B NEVER folds/counts/distills A's transcript, and via A NEVER folds B's. Each sees only its own held buffers. | Bidirectional behavioral test at N=2, identical cycle name across slugs: write to A, assert A-folds-A-present AND B-folds-A-absent; write to B, assert B-present AND A-absent. Assert candidate count and distillation input exclude the other slug. |
| **AC-03** | INV-T3 | Pending-entries isolation: pending-entries analysis surfaced/drained at `cycle_review` for A is never observable via B, and vice versa; each sees its own. | Bidirectional behavioral test: drive pending-entries state for A via A's surface; assert present at A's `cycle_review`, absent at B's; repeat with roles swapped. |
| **AC-04** | INV-K1, INV-K2 | Knowledge-read fidelity + isolation: knowledge written to A is retrievable by A's observe-path reads (briefing/search/compact through `/v1/{A}/observe`); A's observe-path reads NEVER return B's entries, and B's never return A's. | Bidirectional behavioral test: write knowledge as A; assert A's observe-path briefing/search/compact returns it AND B's returns none of it; repeat with roles swapped. (Closes vnc-038 read-side gap; SR-07.) |
| **AC-05** | INV-C1, INV-C2 | Config fidelity + isolation for every in-scope P3 field: A's declared per-slug config governs only A's observable behavior; B's config never governs A's; no silent fallback to builtin/global defaults when A declared its own. | Per-field, bidirectional. **Behaviorally observable fields** (asserted through the public interface): `transcript_signal_class_names` → `cycle_review` `signal_class_counts`; `observation_registry` (domain-pack observation categories) → status surface; `retention_config` → purge/held-buffer behavior. **Fields with no clean public surface** (see OQ-3 / AC-08 exception below): `store_config` (byte-limit) and `inference_config` (briefing blend) — covered by the AC-08 boot assertion + a wiring-pin unit guard, recorded as a **documented AC-06 exception**, NOT deferred and NOT silently omitted (SR-05). A and B declare *different* config so each surface reflects only its own. |
| **AC-06** | (suite property) | The behavioral isolation suite asserts only through the public `/v1/{slug}/…` interface — no `Arc::ptr_eq`, no field-overwrite assertions in that suite; it stands alone, is implementation-agnostic, runs at N≥2, and is **bidirectional** per data class. The suite MUST **enumerate its own coverage**: for each invariant, whether it is covered behaviorally or only white-box, so any gap is visible, not implied (SR-05). | Review of the suite: assert no white-box calls in the behavioral crate; assert N≥2 fixture; assert both-direction cases per invariant; assert the coverage-enumeration comment/table exists and lists the two AC-05 white-box-only fields. |
| **AC-07** | (parity) | HTTPS observe-path fidelity equals local UDS behavior for all in-scope state; UDS/stdio paths are unchanged. | Behavioral parity assertion (HTTPS fold == UDS fold for the same input) + diff review confirming no change to UDS/stdio construction paths (NFR-4). |
| **AC-08** | (white-box complement) | A per-slug boot assertion (extending `assert_wave_b_precondition`) fails loud at startup for every built slug server whose in-scope per-slug state is not wired to the instance its write path uses (incl. `Arc::ptr_eq` registry check + wired-hold check), covering the whole constructor-default field **class** (SR-02), not only the 9 items. White-box **complement** to the behavioral suite, not a substitute (OQ-4). | Unit test: build a slug server with a deliberately-unwired field; assert the boot assertion panics/fails. Wiring-pin unit test: `Arc::ptr_eq(observe_ctx.registry_for(slug), slug_server.session_registry)` per built slug. Covers the two AC-05 white-box-only config fields. |
| **AC-09** | (cleanup) | The two vestigial `ObserveContext` fields (`vector_store`, `adapt_service`) are deleted. | Diff review + compile: fields absent from `ObserveContext`; no remaining references. |
| **AC-10** | (decision) | A ratified ADR records the per-slug-observe-context decision, its relation to goal #5519's OSS per-project isolation invariant and the ADR-007 seam, and the #925 subsume-vs-defense-in-depth relationship (see Dependencies). | ADR stored in Unimatrix (architect); referenced from the feature and PR. |

## Domain Models / Ubiquitous Language

- **Slug** — an operator-registered project identifier that rides in the URL path (`/v1/{slug}/observe`,
  `/v1/{slug}/…`). Transport-derived project identity, present on every observe call. Isolation in this feature is
  **by slug**, not by user (per-user/OAuth-subject isolation is NG-1, enterprise-deferred).
- **ProjectKey** — the parsed routing key; `ProjectKey::Slug(slug)` is produced by `parse_project_key` in
  `route_observe` Step 0 **before** any registry write. The single input to every per-request resolution.
- **Per-slug registry (and paired hold)** — the `SessionRegistry` + `TranscriptHold` pair constructed for a specific
  slug and wired onto that slug's `UnimatrixServer`. Holds in-memory, instance-bound transcript/session state. The
  pair moves together (FR-2): the registry a slug reads and the hold its purge gate acts on must be the same slug's.
- **Resolution funnel** — the single set of methods on `Arc<dyn StoreResolver>` (`resolve_store`, `adapter_for`, and
  the new `registry_for`/`pending_for`/`services_for`) that map a `ProjectKey` to that slug's state. One funnel, no
  parallel side-map (vnc-034 #4974 guard). O(1) Arc-clone lookups.
- **Split-brain** — the defect shape: a write path and a read path holding two different instances of the same
  logical state, so writes land in one and reads see the other (empty) one. #930 is a `SessionRegistry` split-brain.
- **Constructor-default (test-only) field** — a field `UnimatrixServer::new` mints as a placeholder that the
  daemon/stdio boot paths overwrite but `build_project_server` did not. The bug class this feature forecloses.
- **Isolation invariant** — a durable, solution-independent property asserted through the public HTTPS interface,
  with a **fidelity** (own-read present) half and an **isolation** (cross-read absent) half, holding **bidirectionally**
  across N≥2 slugs. INV-T1/T2/T3 (transcript), INV-K1/K2 (knowledge), INV-C1/C2 (config).
- **Config-parity** — a slug's declared per-slug config (vnc-040 `resolve_slug_config`) actually governing that
  slug's observable behavior, versus silently reading builtin/global defaults.

## User / Agent Workflows

1. **Cloud transcript fold (INV-T1, #930).** An agent streams `transcript_delta` frames to `/v1/{A}/observe` (204
   ACKs, offset advances) under feature cycle `X`; later calls `cycle_review` for `X` via A's MCP surface and sees
   the folded transcript (non-empty candidates, transcript bytes, distillation input) — identical to local UDS.
2. **Co-hosted collision (INV-T2).** Slugs A and B both run feature cycle `nxs-001`. Each agent's `cycle_review`
   folds only its own slug's held buffers; neither sees the other's transcript, and distillation-fed knowledge stays
   project-scoped.
3. **Observe-path knowledge read (INV-K1/K2).** An agent's briefing/search/compact through `/v1/{A}/observe` returns
   only project A's knowledge; project B's persisted entries never surface in A's briefings.
4. **Per-slug config (INV-C1/C2).** Slug A's declared domain packs, signal-class names, byte-limit, and retention
   policy govern A's observable behavior (e.g. A's `signal_class_counts`); B's config never leaks into A's surface.
5. **Loud-at-boot regression guard (AC-08).** On startup, any future per-slug field left unwired to its write-path
   instance fails the boot assertion immediately, rather than silently reading zero at review time.

## Constraints

- **No wire/protocol/client change** (NFR-3, NG-5) — slug is already on the URL and parsed pre-write.
- **One-funnel discipline** (FR-12) — per-slug resolution methods live on `Arc<dyn StoreResolver>` beside
  `resolve_store`/`adapter_for`; no parallel side-map (vnc-034 #4974 guard).
- **`transcript_hold` pairs with `session_registry`** (FR-2/FR-3, SR-03) — never wire one without the other; wire
  before the tick-context loop (`main.rs:1229/1237`) or the sibling tick defect stays live.
- **Correctly-global handles stay global** (NFR-5) — `embed_service`, `categories`.
- **Release-hard, not `debug_assert`-only** (NFR-2, SR-06) — the un-probed isolation direction must have
  release-observable coverage via the behavioral suite; a `debug_assert!` is compiled out of release.
- **Behavioral tests at N≥2, assembled production wiring, bidirectional** — in the external `tests/` integration
  crate; extend existing fixtures (cumulative test infra) — `project_routing_integration.rs`, pattern #5172. POST
  delta via `route_observe` → read via that slug's `McpAdapter`; never hand-pass a registry into `dispatch_request`
  (that structurally hides instance-split bugs).
- **Bugfix #930 must be resolved (P1)** — the transcript fold is the entry point; it cannot regress.
- **Workspace rules** (NFR-7) — no `unsafe`, no non-test `.unwrap()`, ≤500 lines/file, clippy clean, #878
  `--jobs 1` link discipline.

## Dependencies

- **#800 (infra-001 multi-slug HTTP fixture) — OPEN.** The vehicle for the INV-C1/C2 config-parity behavioral proof
  and the N≥2 multi-slug behavioral suite generally. The behavioral suite MUST **extend the #800 multi-slug HTTP
  fixture** (cumulative test infra) rather than fork one, so config-parity is proven once, not twice (SR-08). The
  architect/tester must confirm the fixture owner before building INV-C fixtures. This is also capability C6's single
  path to proven.
- **#925 (cycle-review foreign-session sweep) — OPEN, same family as INV-T2.** Per-slug registries may **subsume**
  #925's cross-slug fold sweep (structural isolation makes the sweep redundant), or the sweep may be retained as
  **defense-in-depth** atop structural isolation. The architect MUST reconcile this in the ADR (AC-10) and state the
  relationship explicitly; do not ship two overlapping mechanisms without a stated relationship (SR-09). The human
  owns any #925 close/keep decision — do not auto-file or auto-close.
- **vnc-034 (ADR-007)** slug identity + register/attach; per-slug DB/vector isolation — the seam this realizes.
- **vnc-038 (ADR-003 #5082)** per-slug observe on the per-request store funnel — resolved only the store; this
  feature completes the funnel (registry/pending/services/config).
- **vnc-040 (#5217/#5209)** per-slug config (`resolve_slug_config`) — the config source P3 threads.
- **crt-056** per-slug config-driven `ServiceLayer` — already constructed per slug; P2 needs `ObserveContext` to use
  it (FR-7).
- **crt-054 / ADR-010** `assert_wave_b_precondition` — the boot assertion FR-13/AC-08 extends.
- **Governing patterns:** #5629 (construction-parity + funnel-completeness + `Arc::ptr_eq` boot guard), #5172 (N=2
  model-free cross-slug isolation proof), #5348/#5347 (bidirectional isolation-test lesson), #5175 (config-parity
  tests drive the provisioner's public assembly from the external crate).

## NOT in Scope

- **NG-1** Per-user / per-OAuth-subject isolation (`http-{subject_hash}-…`). Enterprise per-USER boundary; this
  feature isolates by **slug**, not user.
- **NG-2** Multi-TENANT isolation. OSS is single-tenant, N-projects; the enterprise multi-tenant boundary is seam-only.
- **NG-3** The vnc-027 UDS↔HTTP transport session-id split (#4828). Separate pre-existing family; unchanged.
- **NG-4** Local UDS/stdio paths. Already correct; must remain untouched (NFR-4).
- **NG-5** Wire / protocol / client changes (NFR-3).
- **NG-6** Prescribing the implementation mechanism. This spec fixes *what must be true* (the invariants); the
  architect owns *how* (per-slug registries on the funnel vs. slug-namespaced keys vs. another shape).
- **NG-7** Cross-project knowledge sharing / owner-store fan-out. Explicit enterprise out-of-scope (goal #5519).
- **Correctly-global fields** (`embed_service`, `categories`) and **correctly-per-instance** fields
  (`client_type_map`, runtime-populated by `initialize`) are NOT per-slug-ified.

## Open Questions

All five SCOPE open questions were resolved by the uni-zero scope review; recorded here for downstream trace:
- **OQ-1 (P3 scope boundary): RESOLVED — IN-SCOPE.** The 5-field config-snapshot family lands now (same
  `build_project_server` pass, mechanically cheap). No P3 follow-up to file.
- **OQ-2 (P2 non-negotiable): RESOLVED — CONFIRMED in-scope.** Cross-project knowledge-read leak with a privacy
  dimension; security-class floor.
- **OQ-3 (behaviorally-observable config surfaces): RESOLVED.** Fields with a clean public surface
  (`signal_class_names`→`signal_class_counts`, observation categories→status, retention→purge behavior) are asserted
  behaviorally; `store_config` (byte-limit) and `inference_config` (briefing blend) — which lack a clean public
  surface — get a **documented white-box exception** (AC-08 boot assertion + wiring-pin), recorded in the suite's
  coverage enumeration, never silently omitted (SR-05).
- **OQ-4 (white-box guards): RESOLVED — required complements, not substitutes.**
- **OQ-5 (follow-up filing): MOOT** given P3 in-scope. If any config field is ultimately cut, the human owns filing
  the ADR-007-seam follow-up (project norm: don't auto-file outward commitments).

Remaining for the architect (not blocking this spec):
- Confirm crt-056's per-slug `ServiceLayer` is actually config-driven (SR Assumption); if not, P2 is a deeper fix.
- Reconcile #925 subsume-vs-defense-in-depth in the ADR (AC-10 / SR-09).
- Confirm the #800 fixture owner before building INV-C fixtures (SR-08).

## Knowledge Stewardship
- **Queried:** `mcp__unimatrix__context_briefing` (task-scoped to vnc-046 per-slug observe-path isolation, 20 hits) —
  surfaced ADR-003 vnc-034 #4950 (resolve_store isolation seam / single funnel), ADR-003 vnc-038 #5082 (per-slug
  observe on the per-request funnel — resolved only the store, the gap this feature completes), vnc-040 #5217
  (per-slug-vs-global config classification), and the personal-cloud goal capability entries #5519/#5533/#5579/#5594
  (OSS per-project isolation invariant, one-seam discipline, per-slug config, full-fidelity rollup). Read all five
  GH #930 comments (investigator root-cause + PER-SLUG-ROUTING-VIABLE addendum, design review F1/F2, uni-zero fix
  review, architect audit 9-item inventory + 3-pattern scope verdict), SCOPE.md, SCOPE-RISK-ASSESSMENT.md (SR-01…09),
  and the uni-zero scope review (resolves OQ-1…5).
- **Stored:** nothing — read-only tier. The governing architecture invariant is already captured as pattern #5629
  and the bidirectional-test lesson as #5348; #930 defect specifics belong on the GH issue (bugs are GH issues, not
  lessons). Feature-specific spec decisions live in this SPECIFICATION.md.
