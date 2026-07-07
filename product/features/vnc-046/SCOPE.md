# vnc-046 — Per-Slug State Isolation for the Cloud (HTTPS) Observe Path

## Problem Statement

On a multi-project cloud (HTTPS) instance, each project slug is served by its own
per-slug `UnimatrixServer`, but the observe/write path and several read paths silently
fall back to **daemon-global** state instead of that slug's state. This is a per-slug
**split-brain**: the object that writes and the object that reads are different instances.

The defect surfaced as bug #930 — `transcript_delta` frames are 204-ACKed on the wire
but never appear in the `cycle_review` transcript fold. Root cause: HTTPS `/observe`
applies deltas into the daemon-global `SessionRegistry`, while the per-slug server that
serves `cycle_review` reads its own constructor-default, never-written, empty registry
(`server.rs:416`; the per-slug build path at `http_provision.rs:261-273` never performs
the overwrite that the daemon/stdio paths at `main.rs:976/994/1695` do). Over UDS/stdio
the fold works because those paths share the one global registry; over HTTPS it is empty.

The bug audit (three reviewers on #930) found this is not one field. `UnimatrixServer::new`
mints **test-only defaults** for a whole set of fields; the daemon/stdio boot paths
overwrite them; the per-slug HTTP path overwrites **none**. Symmetrically, `ObserveContext`
resolves only the *store* per request (vnc-038) and holds every other handle as the global
one. The audit inventoried **9 distinct NEEDS-PER-SLUG-FIX state items** across three fix
patterns, plus two vestigial dormant split-brains to delete.

Two of these are worse than data-invisibility — they are **cross-project leaks** on a
co-hosted server:
- Same-named feature cycles across slugs (`{phase}-{NNN}` is a shared vocabulary) fold each
  other's transcripts through the shared registry — and the fold feeds distillation into
  **persisted** knowledge.
- Observe-path briefing/search/compact **reads** go through the global `ServiceLayer`, so a
  per-slug agent reads the **wrong project's** knowledge store (vnc-038 fixed only the store
  *write* funnel; the observe-path read funnel through `services` stayed global).

The personal-cloud goal (#5519) names, as an **OSS-in-scope** invariant (not an
enterprise deferral): "one cloud serves N projects, each fully isolated … no cross-project
sharing in OSS," with "cross-project contamination structurally impossible" via the
`resolve_store(request)` funnel. On the transcript/knowledge/config paths that invariant is
**currently not met** on cloud. This session designs the fix to restore it — once, at the
seam — rather than patch instances and ship inconsistent half-isolation.

Why now: cloud fidelity (C0 north-star — "full intelligence-pipeline fidelity over HTTPS ==
local") is broken for every multi-project HTTPS deployment. #930 blocks it; the audit shows
a two-line patch would ship a working transcript fold beside briefings that still read the
wrong project's knowledge.

## Goals

1. Restore full observe-path fidelity over HTTPS for every slug: data written via a slug's
   `/v1/{slug}/observe` surface is what that slug's MCP read paths (`cycle_review`,
   `context_briefing`, per-slug tick) observe — HTTPS behavior equals local UDS behavior.
2. Make cross-project contamination on the transcript, knowledge, and config paths
   **structurally impossible** on a co-hosted multi-project cloud server, honoring the
   `resolve_store`-style single-funnel isolation seam.
3. Deliver **solution-independent behavioral isolation tests** (first-class deliverable —
   see the dedicated section below) that assert the durable isolation invariants through the
   public HTTPS interface, so any future technology or wiring change that breaks cloud
   isolation TRIPS these tests.
4. Convert the entire "constructor-default field never overwritten on the per-slug path" bug
   class from silent-read-zero into loud-at-boot (per-slug boot assertion), so a future
   per-slug field cannot silently regress isolation.

## Non-Goals

- **NG-1 — Per-user / per-OAuth-subject isolation.** The `http-{sid}` session-key evolution
  to `http-{subject_hash}-…` is per-USER isolation under OAuth (W2-3), an enterprise
  boundary orthogonal to project identity. Project identity is transport-derived from the
  URL slug and present on every observe call today; this feature isolates by **slug**, not
  by user. Per-subject isolation is deferred to the enterprise track.
- **NG-2 — Multi-TENANT isolation.** OSS is single-tenant, N-projects. The enterprise
  multi-tenant boundary is out of scope (seam only, per goal #5519).
- **NG-3 — The vnc-027 UDS↔HTTP transport session-id split (#4828).** A separate, pre-existing
  transport-split family; unchanged by this work.
- **NG-4 — Local UDS/stdio paths.** They already share the one global registry/services and
  are correct; they must remain untouched (the daemon-global registry stays theirs).
- **NG-5 — Wire / protocol / client changes.** Per-slug routing is viable with **no** wire
  change: the slug rides in the URL (`/v1/{slug}/observe`) and is parsed to `ProjectKey::Slug`
  in `route_observe` *before* any registry write. No client bundle, protocol, or edge-client
  change is in scope.
- **NG-6 — Prescribing the implementation mechanism.** Whether isolation is realized via
  per-slug registries resolved on the funnel, slug-namespaced session keys, or another shape
  is the architect's decision in the design session. This SCOPE fixes *what must be true*
  (the invariants), not *how*.
- **NG-7 — Cross-project knowledge sharing / owner-store fan-out.** An explicit enterprise
  out-of-scope item in goal #5519.

## Background Research

### Confirmed against code (not re-derived — cross-checked the audit)
- `UnimatrixServer::new` (`server.rs:403-446`) mints test-only defaults for the eight fields
  the audit names; each carries a doc-comment "overwritten in main.rs daemon/stdio paths"
  (`server.rs:415-443`). Confirmed verbatim.
- `build_project_server` (`http_provision.rs:261-273`) calls `UnimatrixServer::new` and
  threads config only into the per-slug `ServiceLayer` — it overwrites **none** of the eight
  fields. Confirmed.
- `ObserveContext` (`http/router.rs:81-102`) holds `resolver` (per-request store funnel,
  vnc-038) but carries `session_registry`, `pending_entries_analysis`, and `services` as flat
  **global** handles, plus `vector_store` and `adapt_service` (the two vestigial fields,
  unused in `dispatch_request` per the audit's read of `listener.rs:777/779`). Confirmed.

### Inventory — 9 NEEDS-PER-SLUG-FIX items, 3 fix patterns (from the #930 architect audit)
- **P1 — construct-per-slug + converge write/read** (mutable shared state):
  `session_registry`, `transcript_hold`, `pending_entries_analysis`.
  `transcript_hold` MUST move as a constructed *pair* with `session_registry` (design-reviewer
  F1): registry-alone splits the purge gate → held buffers never purge → unbounded growth.
  **#930-as-filed is entirely within P1.**
- **P2 — resolve-per-request** (`ObserveContext` only): `services`. The per-slug server
  already *has* a correct config-driven `ServiceLayer` (crt-056); the observe context must
  stop reading the global one. This is the **cross-tenant knowledge-read leak** — observe-path
  briefing/search/compact (`listener.rs:1417/1498/1534/1587`).
- **P3 — static per-slug config-snapshot overwrite in `build_project_server`** (mirror
  `main.rs:978-989`): `observation_registry`, `inference_config`, `store_config`,
  `retention_config`, `transcript_signal_class_names`. Per-slug config-parity — today each
  slug silently uses builtin domain packs, default byte-limit/inference/retention, and empty
  signal-class names (the empty `transcript_signal_class_names` also feeds a #930 symptom:
  `signal_class_counts_json == "{}"`).
- **Vestigial (delete):** `ObserveContext.vector_store` and `ObserveContext.adapt_service`
  are dormant split-brains (`_`-prefixed, unused in `dispatch_request`). Delete in the same
  pass or they become the next split-brain when a caller starts using them.

### Prior per-slug work this realizes (dependencies)
- **ADR-007 / vnc-034** slug identity + register/attach; per-slug DB/vector isolation shipped.
- **vnc-038 (ADR-003, #5082)** per-slug observe on the per-request store funnel — resolved
  only the store; left `session_registry`/`pending`/`services` global. This feature completes
  that funnel.
- **vnc-040 (#5217/#5209)** per-slug config (resolve_slug_config) — the config source P3 threads.
- **crt-056** per-slug config-driven `ServiceLayer` — already constructed per slug; P2 just
  needs `ObserveContext` to use it.
- **Pattern #5629** (stored by the audit): construction-parity + funnel-completeness invariant
  + the `Arc::ptr_eq` boot-assertion guard. This is the governing pattern for this feature.
- **Testing pattern #5172**: N=2 model-free cross-slug isolation proof — N=1 cannot
  distinguish a real per-slug funnel from a global-handle bypass (#4974). Behavioral isolation
  tests here must be N≥2.

## Isolation Invariants — FIRST-CLASS DELIVERABLE (human directive)

The design MUST include **solution-independent behavioral isolation tests** that validate
personal-cloud per-slug isolation **regardless of implementation technology**. They assert
observable behavior through the **public HTTPS interface** (`/v1/{A}/…` and `/v1/{B}/…`),
NEVER internal wiring. No `Arc::ptr_eq`, no field-overwrite assertions in this suite — those
are complementary **white-box guards** (valuable, see AC-08) but the behavioral suite MUST
stand alone and be implementation-agnostic, so that any future rewire that breaks cloud
isolation trips it.

Setup for all invariants: two registered slugs A and B on one cloud instance; drive each
only through its own `/v1/{slug}/…` surface. Each invariant has a **fidelity** (own-read)
half and an **isolation** (no-cross-read) half — both must hold.

### Transcript class (P1)
- **INV-T1 (transcript fidelity):** A `transcript_delta` posted to `/v1/{A}/observe` under a
  feature cycle, then `cycle_review` for that cycle via A's MCP surface, MUST fold that
  transcript (non-empty candidates / transcript bytes present). *(This is the #930 invariant.)*
- **INV-T2 (transcript isolation — collision case):** With A and B using the **identical**
  `{phase}-{NNN}` feature-cycle name, `cycle_review` via B's surface MUST NEVER fold, count,
  or distill A's transcript. B sees only B's own held buffers.
- **INV-T3 (pending-entries isolation):** Pending-entries analysis surfaced/drained at
  `cycle_review` for A is never observable via B's surface, and vice versa.

### Knowledge / services class (P2)
- **INV-K1 (knowledge-read fidelity):** Knowledge written to A (via A's surface) is
  retrievable by A's **observe-path** reads (briefing/search/compact through `/v1/{A}/observe`).
- **INV-K2 (knowledge-read isolation):** A's observe-path briefing/search/compact MUST NEVER
  return B's knowledge entries, and B's MUST NEVER return A's. *(Closes the vnc-038 read-side
  gap; the write-side store funnel is already isolated.)*

### Config class (P3)
- **INV-C1 (config fidelity):** A's declared per-slug config governs A's observable behavior —
  A's domain-pack observation categories, A's transcript signal-class names (reflected in
  `cycle_review` `signal_class_counts`), A's store byte-limit, and A's retention/purge policy
  apply to A. A never silently falls back to builtin/global defaults when A declared its own.
- **INV-C2 (config isolation):** B's config never governs A's observable behavior. Where A and
  B declare different config (different domain packs / signal classes / byte-limits /
  retention), each slug's surface reflects only its own.

These are framed as **durable properties**: the acceptance gate is "post/read as A, assert B
can never observe A's transcript, knowledge, or config; assert A's own reads do see A's data,"
for every data class. Enumerate them as invariants in the spec so downstream traces to them.

## Proposed Approach (direction only — architect owns the mechanism)

The bug audit already validated the *direction*: complete the per-slug isolation funnel so
that observe-path state resolves per slug the same way the store already does (vnc-038),
rather than patching instances. Governing pattern is #5629 (construction parity + funnel
completeness). The design session must:
1. Ratify a **per-slug-observe-context ADR** — the resolver resolves per key not just the
   store but the registry (+ paired hold), pending, and services; config-snapshots are set
   on the per-slug server at `build_project_server`. New resolver methods live ON the
   `Arc<dyn StoreResolver>` beside `resolve_store`/`adapter_for` — **never** a parallel
   `slug→X` side-map (re-opens the vnc-034 #4974 ceremonial-funnel guard).
2. Emit the behavioral isolation suite (N≥2, assembled production wiring: POST delta via
   `route_observe` → read via that slug's `McpAdapter`) as the primary acceptance gate, plus
   the white-box boot assertion + wiring-pin unit guards as complements.
3. Decide P3's placement (see Open Questions).

The architect proposes the mechanism; this SCOPE fixes the invariants and boundaries.

## Acceptance Criteria

- **AC-01:** INV-T1 holds — assembled-wiring behavioral test proves transcript fidelity for a
  slug over HTTPS (#930 fixed).
- **AC-02:** INV-T2 holds — behavioral test proves no cross-slug transcript fold under an
  identical feature-cycle name (F2 leak closed).
- **AC-03:** INV-T3 holds — pending-entries analysis is per-slug isolated.
- **AC-04:** INV-K1 and INV-K2 hold — observe-path knowledge reads are per-slug: own-slug
  reads see own knowledge; cross-slug reads never see the other slug's knowledge.
- **AC-05:** INV-C1 and INV-C2 hold for every in-scope config field (subject to the P3
  Open-Question resolution) — each slug's declared config governs only its own observable
  behavior; no silent fallback to builtin/global defaults.
- **AC-06:** The behavioral isolation suite asserts only through the public `/v1/{slug}/…`
  interface — no `Arc::ptr_eq`, no field-overwrite assertions in that suite; it stands alone
  and is implementation-agnostic. Runs at N≥2 (per #5172; N=1 cannot surface a global-handle
  bypass).
- **AC-07:** HTTPS observe-path fidelity equals local UDS behavior for the in-scope state;
  UDS/stdio paths are unchanged.
- **AC-08:** A per-slug boot assertion (extending crt-054/ADR-010 `assert_wave_b_precondition`)
  fails loud at startup for every built slug server whose in-scope per-slug state is not
  wired to the instance its write path uses (white-box complement to the behavioral suite).
- **AC-09:** The two vestigial `ObserveContext` fields (`vector_store`, `adapt_service`) are
  deleted.
- **AC-10:** A ratified ADR records the per-slug-observe-context decision and its relation to
  goal #5519's OSS per-project isolation invariant and the ADR-007 seam.

## Constraints

- **No wire/protocol/client change** — slug is already on the URL and parsed pre-write (NG-5).
- **One-funnel discipline** — per-slug resolution methods live on `Arc<dyn StoreResolver>`
  beside `resolve_store`/`adapter_for`; no parallel side-map (vnc-034 #4974 guard).
- **`transcript_hold` pairs with `session_registry`** — never wire one without the other, and
  wire before the tick-context loop (`main.rs:1229/1237`) or the sibling tick defect stays
  live (design-reviewer F1).
- **Correctly-global handles stay global** — `embed_service` (one loaded ONNX model, crt-056
  C-3) and `categories` (operator allowlist) are NOT per-slug.
- **Behavioral isolation tests run at N≥2** in the external `tests/` integration crate,
  against assembled production wiring; extend existing fixtures (cumulative test infra) — see
  `project_routing_integration.rs` and pattern #5172.
- **Rust workspace rules** — no `unsafe`, no `.unwrap()` in non-test code, max 500 lines/file,
  clippy clean; respect the #878 build-memory guardrails (`--jobs 1` link discipline for the
  large server test binaries).
- **Bugfix #930 must be resolved by this feature** (P1) — the transcript fold is the entry
  point; it cannot regress.

## Open Questions

1. **P3 scope boundary (the key decision).** Does the 5-field config-snapshot family
   (`observation_registry`, `inference_config`, `store_config`, `retention_config`,
   `transcript_signal_class_names`) land **in-scope now**, or as a tracked follow-up on the
   ADR-007 seam? Researcher recommendation: **in-scope** — it is the same
   `build_project_server` pass, mechanically cheap (mirror `main.rs:978-989`, thread 3
   already-available params), and completes config-parity so INV-C1/C2 close. Deferring it
   ships working transcript+knowledge isolation beside config that still reads
   builtin/global defaults — the same inconsistent half-isolation the design session exists to
   avoid. But it is the negotiable boundary; if speed forces a cut, P1+P2 are the floor and P3
   becomes a tracked follow-up with a PR risk note.
2. **P2 is non-negotiable in-scope — confirm.** `ObserveContext.services` is a cross-project
   knowledge-**read** leak with a privacy dimension (a slug reads another project's knowledge
   store). Researcher treats it as security-in-scope alongside P1. Confirm no objection.
3. **Behaviorally-observable config surfaces.** Some config invariants (INV-C1/C2) need a
   stable public surface to assert through — `signal_class_counts` in `cycle_review`,
   observation categories via status, retention/purge behavior. If any in-scope config field
   lacks a clean behavioral observation point, does the human accept a white-box guard for
   that field as a documented exception to AC-06, or is it deferred to P3-follow-up?
4. **White-box guards as required complements.** Confirm the per-slug boot assertion (AC-08)
   and wiring-pin unit tests are *required complements* to the behavioral suite, not
   substitutes. Researcher recommendation: required — the assertion forecloses the whole bug
   class at boot; the behavioral suite proves the observable property.
5. **Follow-up filing.** If P3 (or any config field) is deferred, the per-slug follow-up issue
   on the ADR-007 seam is a **human decision to file** (project norm: don't auto-file outward
   commitments). Confirm the human will own that filing.

## Tracking

https://github.com/dug-21/unimatrix/issues/934

Supersedes bug #930 (becomes P1 within this feature — resolved by P1, do not close without human
decision). Keeps #925 open as disjoint defense-in-depth (ADR-005). Depends on #800 (multi-slug
HTTP fixture) for the behavioral / INV-C proof.

## Knowledge Stewardship
- **Queried:** `mcp__unimatrix__context_briefing` (20 hits, task-scoped to vnc-046 per-slug
  isolation) + targeted `context_get` — pattern #5629 (construction-parity + funnel-completeness
  invariant; the governing pattern), goal #5519 (personal-cloud OSS per-project isolation
  invariant the leaks bend), ADR-007 #5135 (container HTTP-enable / multi-project routing
  enabler), testing pattern #5172 (N=2 model-free cross-slug isolation — behavioral test
  design constraint). Also read the four #930 GH comments (investigator root-cause +
  PER-SLUG-ROUTING-VIABLE addendum, design review, uni-zero product review, architect audit +
  9-item inventory + scope verdict).
- **Stored:** nothing — the governing architecture invariant is already captured as #5629
  (stored by the #930 audit) and the #930 defect specifics belong on the GH issue (bugs are GH
  issues, not lessons). Feature-specific scope lives in this SCOPE.md, not Unimatrix.
- **Declined:** the per-slug-observe-context ADR itself — that ratified decision belongs to the
  architect in the design session, not pre-empted from problem-space research.
