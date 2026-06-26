# Specification — nan-022: Cross-Transport Parity Suite (C0 Proof Artifact)

GH Issue #837. Test-only. Extends nan-021's `infra-001` fixture cumulatively (never a
fork, never a parallel scaffold). Source: `product/features/nan-022/SCOPE.md` +
`SCOPE-RISK-ASSESSMENT.md`.

---

## 1. Objective

Generalize nan-021's single-output HTTPS-vs-UDS parity gate into a **dimension-keyed
parity matrix** that drives ONE canonical workload over BOTH transports (the HTTPS
bridge and stdio/UDS) in ONE pytest invocation and asserts *measured* parity across all
six dimensions of C0's (#5304) marquee promise: retrieval, behavioral signals,
analytics/learning, proactive delivery, PreCompact restoration, and per-slug isolation.
nan-021 proved exactly one dimension (analytics/learning); this feature proves the
remaining five and composes all six into a single false-green-proof release-gate matrix.
Passing the full matrix is the behavioral evidence an authorized session uses to flip C0
to `proven`; this feature does not itself flip C0, fix any defect it surfaces, or change
any production code.

---

## 2. Domain Models & Ubiquitous Language

These terms are load-bearing downstream (architect, pseudocode, tester) and must be used
exactly as defined.

| Term | Definition |
|---|---|
| **Transport** | A wire path the workload is driven over. Two exist: **HTTPS bridge** (the shipped `mcp-bridge.js` over pinned HTTPS for `context_*` MCP tools, plus the pinned `/observe` HTTPS route for hook frames) and **stdio/UDS** (in-process `UnimatrixUdsClient` MCP socket + `UnimatrixHookClient` hook IPC socket). |
| **Leg** | One execution of the canonical workload over one transport. The **UDS leg** runs in-process (`drive_uds_leg`); the **HTTPS leg** is a shell-out to the Docker smoke (`run_https_leg`). Each leg emits a captured-output **bundle**. |
| **Dimension** | One of the six measured parity surfaces (retrieval, behavioral signals, analytics/learning, proactive delivery, PreCompact restoration, per-slug isolation). Each dimension defines: a deterministic captured output, the code path it exercises, a comparator, and a closed exclusion set. |
| **Parity** | Field-for-field equality of a dimension's captured output across the two legs, MODULO that dimension's closed, justified exclusion set. Parity is **measured** (compared cross-leg), never **asserted** (checked per-leg only). |
| **Exclusion set** | A per-dimension, **closed**, enumerated, individually-justified `frozenset` of field paths excluded from the cross-leg comparison because they are provably wall-clock / nondeterminism artifacts and not transport-divergence signals. The nan-021 `EXCLUDED` + `EXCLUSION_JUSTIFICATIONS` pattern, replicated per dimension. No set is silently widened. |
| **Run-correlation token** | The ONE stable CC session identity threaded through the manifest declaration and every observe on both legs (the #832 / SR-05 drift defense). It also stamps the HTTPS-leg output bundle so a stale prior-tag artifact cannot be ingested (`load_https_vector` guard generalized to the bundle). |
| **False-green** | A run that reports PASS without having actually carried cross-transport traffic and compared it: a missing/empty capture, a stale-token ingest, a Docker-absent skip read as pass, or a dimension routed to the wrong wire surface so it records nothing. Every false-green source is a HARD ERROR, never a pass. |
| **PARITY-PASS** | A dimension's two-leg outputs are equal modulo its exclusion set. |
| **PARITY-FAIL** | A non-excluded field diverges *across legs* (cross-transport divergence) — a real C0 parity defect. The gate goes RED; the defect is filed as a GH bug, not fixed here. |
| **INTRA-TRANSPORT-NONDETERMINISM** | A dimension's output is unstable *within a single leg* (re-running the same capture on the same transport diverges). This is a pre-existing determinism bug (e.g. SR-01/#4990 HNSW top-k flip, #2610 HashMap order, `sort_unstable` ties), NOT a cloud-parity failure. Detected by double-capture-and-diff per leg; routed OUT of the red parity verdict and filed as a SEPARATE GH bug. |
| **INFRA-ERROR** | The run could not produce a comparable result for transport/environment reasons distinct from any dimension verdict: a half-open-socket hang (the #839 class, now CLOSED via commit 5b6badad / PR #842; kept as defense-in-depth), a connect/idle deadline expiry, a Docker-absent skip, a missing/stale capture. A distinct exit class — never PARITY-PASS and never PARITY-FAIL. |

---

## 3. Functional Requirements

Each FR is testable. FR→AC traceability is given in §6.

### Workload & identity

- **FR-1** The suite drives ONE canonical `ParityWorkload` manifest over BOTH transports
  in ONE pytest invocation, under ONE stable session identity that is also the single
  run-correlation token. The nan-021 single-manifest / single-identity contract is
  consumed, not re-authored.
- **FR-2** The canonical manifest is augmented (per OQ-1 leaning (a)) with a deterministic
  **store-seed phase** and a **retrieval/briefing query set** sufficient for retrieval and
  proactive-delivery ranking to be *non-degenerate* (see NFR-7). The single-identity /
  single-token / single-barrier invariants are preserved while augmenting.
- **FR-3** Every observe-driven dimension emits the byte-identical 11-frame #5298
  RecordEvent hook sequence on both legs (`SessionRegister` → `cycle_start` → phase-setting
  `TaskCreate` PreToolUse → per-call Pre+Post → `cycle_stop` → `SessionClose`), never the
  rework/legacy frame variants. `context_cycle_review`'s primary col-024 path reads
  hook-written rows, not MCP `context_cycle` calls.
- **FR-4** No `topic_signal` (or any other dimension output) is ever seeded. Workload INPUT
  (tool calls, Bash token content, seed corpus) is declared; all compared OUTPUTS are
  DERIVED over the wire on both legs. The `assert_no_seed_reachable` static guard is
  extended to cover every net-new module and the seed-corpus loader.

### Per-leg capture (the bundle)

- **FR-5** A single drive of the workload over each transport yields a **dimension-keyed
  output bundle** (retrieval results, attribution, MetricVector + Informs edges + phase,
  briefing/injection, restored-context, isolation probe) — not a single MetricVector. Each
  leg writes/returns the full bundle.
- **FR-6** Each dimension's capture is routed to the correct wire surface on BOTH legs:
  `context_*` tools (retrieval, briefing, cycle_review, edges) over the MCP bridge / MCP
  UDS; observe/attribution, PreCompact, cycle start/stop over the `/observe` hook route /
  hook IPC. A capture routed to the wrong surface records nothing and MUST surface as
  INFRA-ERROR (missing capture), never as an empty PARITY-PASS.
- **FR-7** The HTTPS leg is driven THROUGH the shipped `mcp-bridge.js` over pinned HTTPS
  (D-2 bridge-in-path); it never POSTs `mcp_url` directly. It reuses `mcp-bridge.js`,
  `cert-pin.js`, `credstore.js`, `bundle.js`, `init.js`, `bridge-cycle-driver.js`,
  `bridge-witness.js`, and `cloud-cycle-https-leg.sh` / `run_smoke_gate` as-is. Net-new
  transport/cert/spawn/framing code is a fork smell to FLAG.

### Comparison & outcomes

- **FR-8** Each dimension has a comparator module on the nan-021 `metric_comparator.py`
  template: explicit field classification, a closed + individually-justified exclusion set,
  a field-by-field evidence record emitter keyed by the run-correlation token, and a loud
  `ParityMismatch` on any non-excluded cross-leg divergence.
- **FR-9** The orchestrator runs every dimension comparator in the same execution and emits
  a per-dimension PASS/FAIL evidence table keyed by the run-correlation token.
- **FR-10** Each dimension's verdict is exactly one of four outcome classes: **PARITY-PASS**,
  **PARITY-FAIL**, **INTRA-TRANSPORT-NONDETERMINISM**, **INFRA-ERROR** (§2). The matrix
  separates these classes structurally; a PARITY-FAIL reddens the gate, an
  INTRA-TRANSPORT-NONDETERMINISM or INFRA-ERROR does NOT contribute to the parity RED verdict.
- **FR-11** Intra-transport instability is detected by **double-capture-and-diff**: each
  leg's dimension output is captured twice and diffed against itself (modulo the same
  exclusion set). Self-divergence classifies the dimension as INTRA-TRANSPORT-NONDETERMINISM
  for that leg and is excluded from the cross-leg parity verdict (SR-04 / OQ-2).
- **FR-12** A missing, empty, or stale-token capture for any dimension ERRORS
  (INFRA-ERROR) — never a vacuous PARITY-PASS. The nan-021 stale-token guard
  (`load_https_vector`) is generalized to validate the full bundle's run-correlation token.
- **FR-13** The HTTPS leg must run a **transport-health preflight** with a bounded
  connect/idle deadline. A half-open-socket hang (the #839 class, now CLOSED — retained as
  defense-in-depth, not a gating dependency) or any deadline expiry surfaces as INFRA-ERROR
  with a distinct exit class — never a dimension FAIL and never a hang (SR-02).

### Disposition & defect handling

- **FR-14** Any non-excluded cross-leg divergence (PARITY-FAIL) is filed as a new GH bug
  and the gate stays RED; the fix is NOT absorbed into this feature.
- **FR-15** Any detected intra-transport nondeterminism is filed as a SEPARATE GH bug
  (distinct from a parity defect) and is NOT counted in the red parity gate.
- **FR-16** No exclusion set is silently widened. A non-excluded divergence is a
  PRODUCT/HUMAN disposition: either a defect (GH bug, gate RED) or a transport-inherent
  field warranting a product-signed exclusion-set amendment recorded via `context_correct`.
  The implementer/tester never decides.

### CI integration

- **FR-17** The suite runs as a CI-runnable parity matrix in the release-gate Docker lane
  (`workflow_dispatch`/tag), NOT the JS-only `ci.yml` `pull_request` matrix.
- **FR-18** The gate is false-green-proof: skip-when-Docker-absent HARD-fails by a distinct
  exit code; an anchored run-marker tied to the run-correlation token is asserted present
  (proving the wire actually carried traffic this run); a missing dimension ERRORS.
- **FR-19** The comparator TEETH, exclusion-set completeness, and the
  double-capture/outcome-classification logic are unit-testable OFF Docker (the
  nan-019/nan-021 seam precedent), so they are exercised before any release-gate tag round.

---

## 4. Non-Functional Requirements

- **NFR-1 Zero production-code change.** The entire diff is test infra under
  `product/test/infra-001/`. Any `crates/**`, shipped-`lib/`, or production-script change is
  an automatic SCOPE-FAIL. (AC-11)
- **NFR-2 Cumulative, no fork.** Extend existing modules (`parity_workload.py`,
  `metric_comparator.py`, `parity_legs.py`, `test_https_uds_parity.py`, the smoke scripts).
  No parallel scaffold, no second harness, no duplicated manifest/identity/barrier. (AC-11)
- **NFR-3 Single-sourced contract (SR-05 / #5302).** ONE manifest, ONE identity, ONE
  durability barrier helper, ONE comparator template, ONE forbidden-seed closed set across
  all six dimensions. Convention ("conform to nan-021") is not a guard: the design must
  single-source the comparator framing + forbidden-seed set across all six dimensions OR
  carry a cross-dimension equivalence/drift guard.
- **NFR-4 Single determinism/tolerance policy across ranked dimensions (SR-01/SR-03).** The
  embedding/ranking nondeterminism tolerance is single-sourced and shared by the retrieval
  AND proactive-delivery comparators (they share the same failure mode). Two divergent tie
  policies are forbidden.
- **NFR-5 Bounded, non-hanging.** Every leg has a bounded connect/idle deadline; no
  unbounded wait. The HTTPS smoke shell-out keeps the existing outer ceiling
  (`HTTPS_SMOKE_TIMEOUT_S`). (Supports FR-13.)
- **NFR-6 Determinism floor for exact-compared dimensions.** Counts, edge sets, attribution
  strings, and isolation probes are compared EXACTLY (no float tolerance); only the
  ranked-output dimensions (retrieval, briefing) admit the bounded tie-tolerance of NFR-4.
- **NFR-7 Non-degenerate ranking corpus (SR-06).** The seed corpus + query set must produce
  a ranking of sufficient depth that a *stable ranked prefix* of length ≥ N (architect to
  fix N) exists and is the parity signal — never a single-hit degenerate ranking that gives
  a vacuous pass. (AC-02)
- **NFR-8 Exclusion-set discipline / disposition authority (carried from nan-021 NFR-8).**
  Each exclusion set is closed, enumerated, and individually justified in code; amendments
  require product sign-off via `context_correct`. (AC-09)
- **NFR-9 Environment.** Docker (engine 29.5.2, Compose v2.40.3, linux/arm64) for the
  containerized HTTPS/TLS fixture; the off-Docker seam/unit layer keeps the comparator teeth
  tested without Docker.

---

## 5. The Six Dimensions — Captured Output, Comparison, Tolerance, Outcome Classes

Every dimension follows the same shape: each leg captures a deterministic output → the
output is double-captured-and-diffed per leg (INTRA-TRANSPORT-NONDETERMINISM detection) →
the two legs are compared field-for-field modulo a closed exclusion set
(PARITY-PASS/PARITY-FAIL) → a missing/stale capture or transport hang is INFRA-ERROR.

### D1 — Retrieval (AC-02) — HIGHEST RISK

- **Captured output (per leg):** the result `id` list (in ranked order) plus per-result
  scores from an identical `context_search` / `context_lookup` / `context_get` query set
  against the identically-seeded store, over the MCP bridge (HTTPS) / MCP UDS.
- **Comparison:** ordered-set comparison of the **stable ranked prefix** (NFR-7). The exact
  parity signal is *prefix equality of result ids in ranked order*, with a bounded tie/score
  tolerance shared with D4 (NFR-4).
- **Exclusion set / tolerance:** closed, justified. Tie-break instability and HNSW
  approximate-top-k membership flips (SR-01 / #4990 / GH#746) are NOT cross-transport
  divergence — they are intra-transport nondeterminism. Exact-order assertions over the
  unstable tail WILL flake, so the acceptance tolerance is defined over the stable prefix
  only; ties beyond the prefix are excluded.
- **Tolerance-vs-measurement disposition (load-bearing).** Retrieval is THE dimension where
  "measured parity" is most tempted to soften into "tolerant parity." The tie-class tolerance
  MUST be scrutinized at first live run so it CANNOT swallow a real cross-transport ranking
  divergence: a too-loose tolerance that greens a genuine cross-leg prefix difference is a
  false-GREEN, not an acceptable simplification. If exact ordering is unachievable without a
  production determinism fix (#4990 / GH#746 HNSW, no seed API in `hnsw_rs` 0.3.4), that is a
  **FILED BUG + a documented C0 (#5304) exception** — NEVER a quiet widening of the tolerance.
  Widening is a product/human-signed disposition only (NFR-8), never an implementer/tester call.
- **Outcome classes:**
  - Same store, same query, prefix ids differ ACROSS legs → **PARITY-FAIL** (real C0 defect → RED, GH bug).
  - Prefix ids unstable WITHIN one leg on double-capture (HNSW/HashMap/tie flip) →
    **INTRA-TRANSPORT-NONDETERMINISM** (pre-existing bug → separate GH bug, NOT the red gate;
    SR-01 deferred to GH#746).
  - Empty result set / capture absent / bridge hang → **INFRA-ERROR**.

### D2 — Behavioral signals (AC-03)

- **Captured output (per leg):** the set of `topic_signal` values derived for the driven
  observations, read from the per-slug `observations` table (extending the nan-021
  `assert_derived_attribution`, which currently checks the UDS leg only).
- **Comparison:** string-exact cross-leg equality; `unattributed`/NULL is a HARD fail.
  Proven string-exact for UDS in nan-021; this adds the symmetric HTTPS-leg read + cross-leg
  compare.
- **Exclusion set:** empty (attribution is transport-invariant; no wall-clock field).
- **Outcome classes:** `topic_signal` set differs across legs → **PARITY-FAIL**; missing
  capture / wrong wire surface (records nothing) → **INFRA-ERROR**. Intra-transport
  nondeterminism is not expected (string-exact derivation) but is still double-checked.

### D3 — Analytics / learning (AC-04)

- **Captured output (per leg):** the `MetricVector` (consumed verbatim from nan-021) PLUS
  the behavioral `Informs` edge set PLUS the phase signal.
- **Comparison:** `MetricVector` via the nan-021 comparator verbatim (closed 3-field
  wall-clock exclusion set). `Informs` edges compared as a SET field-for-field; phase signal
  compared exactly. The Informs/phase clauses are NET-NEW parity surfaces.
- **Exclusion set:** nan-021's closed 3 wall-clock fields for the MetricVector; a separate
  closed/justified set for any Informs-edge wall-clock/ordering field (e.g. edge creation
  timestamp) — edges compared as an unordered SET, IDs exact. If `Informs` edges depend on
  tick/background timing (OQ-4), the architect must define a barrier or a bounded tolerance;
  spec requires the edge clause be exact once the barrier guarantees the edges have landed.
- **Outcome classes:** non-excluded MetricVector field, Informs-edge id, or phase differs
  across legs → **PARITY-FAIL**; edge timing not yet settled (barrier not satisfied) →
  **INFRA-ERROR** (never compared early).

### D4 — Proactive delivery (AC-05)

- **Captured output (per leg):** the `context_briefing` ranked index (entry ids + order)
  and the resulting injection set, over the MCP bridge / MCP UDS.
- **Comparison:** ordered-set comparison of the briefing ranked prefix + injection-set
  equality. Shares the SINGLE ranking-tolerance policy with D1 (NFR-4) — same entropy class
  (embedding/cluster ranking) plus session-state injection history.
- **Exclusion set / tolerance:** closed, justified; the shared D1/D4 tie/score tolerance
  over the stable prefix. Session-state injection-history fields that are wall-clock/ordering
  artifacts are enumerated and excluded.
- **Outcome classes:** prefix/injection-set differs ACROSS legs → **PARITY-FAIL**; ranked
  prefix unstable WITHIN a leg → **INTRA-TRANSPORT-NONDETERMINISM** (separate bug, not red);
  missing capture → **INFRA-ERROR**.

### D5 — PreCompact restoration (AC-06)

- **Captured output (per leg):** the restored compact-context payload from the PreCompact /
  `CompactContext` path (`wire.rs:171`, HookRequest #670), captured over the `/observe` hook
  route (HTTPS) / hook IPC (UDS) — NOT the MCP bridge.
- **"Restored context identical" defined concretely:** the restored `CompactContext` payload
  — the set of restored entry ids and their restored content/order fields — is byte-equal
  across legs, modulo a closed wall-clock/ordering exclusion set (restoration timestamp,
  any non-content envelope field). Equality is over the *content the server restores*, not
  the host-side presentation.
- **Verification method + OQ-3 limitation handling:** the harness drives the PreCompact
  `/observe` frame on both legs and captures the server-emitted restored payload. **OQ-3
  disposition (human-approved):** PreCompact stays IN scope. The architect must determine at
  design time whether `CompactContext` is symmetrically capturable from both legs purely
  test-only. If a host-side (Claude-Code) component cannot be driven test-only, that is a
  **documented delivery-time measurability call-out**, NOT a scope drop: the spec requires
  the verification method to capture and compare the server-restorable portion symmetrically,
  and to declare any host-side gap explicitly (the un-capturable portion is named, its
  absence is recorded as an INFRA limitation, and the dimension does not silently pass on
  the un-driven portion). **Stated plainly for the flip session:** D5 may legitimately be
  **"measured-where-drivable + documented host-side gap"** rather than a full symmetric
  measurement — the harness cannot drive a live CC host. This is a valid human-signed
  documented-exception (per the C0 #5304 `done_when` escape valve), but it MUST be reported
  honestly to the flip session and NEVER rounded up to "fully measured."
- **Outcome classes:** server-restored payload differs across legs (non-excluded field) →
  **PARITY-FAIL**; host-side component not test-only-drivable → documented INFRA limitation
  (call-out, not a vacuous pass); missing/empty capture → **INFRA-ERROR**.

### D6 — Per-slug isolation (AC-07)

- **Captured output (per leg):** an isolation probe — a write to slug A and a read attempt
  from slug B, plus the on-disk landing location, under each transport. Builds on the posture
  smoke's existing per-slug Gates 1–4 (write lands in `/data/.unimatrix/<slug>/`, not the
  hash dir).
- **Comparison:** the isolation property — slug-A write not visible to slug B AND landing
  only in slug A's store — holds IDENTICALLY under both transports (parity framing of the
  existing per-slug gate).
- **Exclusion set:** empty (boolean isolation property; no wall-clock field).
- **Outcome classes:** isolation holds under one transport but not the other → **PARITY-FAIL**
  (cross-transport isolation divergence); probe not executed / capture absent →
  **INFRA-ERROR**.

---

## 6. Acceptance Criteria (from SCOPE.md AC-01..AC-12)

Every AC-ID from SCOPE.md is mapped to a testable requirement and a verification method.

| AC | Requirement | Verification method |
|---|---|---|
| **AC-01** | One canonical workload drives BOTH transports in ONE pytest invocation, under one stable session identity + one run-correlation token; zero seeded attribution in the path (static guard extended to all new modules). | FR-1..FR-4. Orchestrator-structure tests assert single manifest object, `run_token == workload.session_id`, single barrier; `assert_no_seed_reachable` over all net-new modules + seed loader. |
| **AC-02** | Retrieval parity: identical query set + identically-seeded store → same result ids in same ranked order, MODULO closed tie/score tolerance (minimized). | D1. Stable-prefix ordered-set compare cross-leg; double-capture-diff per leg classifies HNSW/tie flip as INTRA-TRANSPORT-NONDETERMINISM (SR-01). |
| **AC-03** | Behavioral-signal parity: every driven observation `topic_signal == feature` (derived, never seeded) identically on both legs; cross-leg compared. | D2. Symmetric HTTPS+UDS `topic_signal` read; string-exact cross-leg compare; `unattributed`/NULL HARD fail. |
| **AC-04** | Analytics/learning parity: `MetricVector` (nan-021 comparator verbatim) AND `Informs` edge set + phase signal equal field-for-field modulo closed set. | D3. nan-021 MetricVector comparator + net-new Informs-set + phase comparison; barrier before edge compare (OQ-4). |
| **AC-05** | Proactive-delivery parity: `context_briefing` ranked index + injection set identical across legs modulo closed tolerance. | D4. Stable-prefix briefing compare + injection-set compare; shared D1/D4 tolerance (NFR-4). |
| **AC-06** | PreCompact-restoration parity: restored compact-context payload identical across legs modulo closed wall-clock/ordering set. | D5. Symmetric `/observe` PreCompact capture; byte-equal over restored entry ids/content modulo exclusion set; host-side gap declared as documented call-out (OQ-3), never a vacuous pass. |
| **AC-07** | Per-slug isolation parity: under BOTH transports, slug-A write not visible to slug B and lands only in slug A's store. | D6. Isolation probe per transport; cross-transport equality of the isolation property. |
| **AC-08** | CI-runnable parity matrix in the release-gate Docker lane (`workflow_dispatch`/tag), false-green-proof (Docker-absent HARD-fails by distinct exit code; anchored run-marker asserted), per-dimension PASS/FAIL table keyed by run token. | FR-9, FR-17, FR-18. Matrix emits the table; distinct skip exit code asserted; run-marker tied to run token asserted present. |
| **AC-09** | Every per-dimension exclusion set is closed, enumerated, individually justified in code; none silently widened; amendments need product sign-off. | FR-8, FR-16, NFR-8. A meta-test asserts each comparator carries an `EXCLUSION_JUSTIFICATIONS`-style map with one entry per excluded field. |
| **AC-10** | A real parity defect surfaced by any dimension is filed as a new GH bug; gate stays RED; fix not absorbed. | FR-14. PARITY-FAIL → RED; disposition is "file, don't fix". |
| **AC-11** | Zero production-code change; diff is test infra only, cumulative on `infra-001`, no fork. | NFR-1, NFR-2. `git diff` confined to `product/test/infra-001/`; existing-client-not-fork structural tests extended. |
| **AC-12** | Passing the full matrix is C0's proof artifact (evidence to flip C0 #5304 → proven). This feature does NOT itself flip C0. | The matrix's per-dimension PASS table keyed by run token IS the behavioral evidence; flipping C0 is an explicit out-of-scope action (§8). |

---

## 7. Constraints & Dependencies

### Constraints

- **C-1 No production code.** Test-only, cumulative on `infra-001`; no fork, no parallel
  scaffold (nan-021 AC-06/AC-07 carry forward). (NFR-1/NFR-2)
- **C-2 Bridge-in-path (D-2).** Drive `context_*` THROUGH the shipped `mcp-bridge.js` over
  pinned HTTPS; never POST `mcp_url` directly. Observe/PreCompact ride the pinned `/observe`
  route. Reuse `mcp-bridge.js` / `cert-pin.js` / `credstore.js` / `bundle.js` / `init.js`
  as-is; net-new transport/cert/spawn code is a fork smell to FLAG. (FR-7)
- **C-3 #5298 RecordEvent contract.** Every observe-driven dimension emits the byte-identical
  11-frame hook sequence on both legs; `context_cycle_review`'s primary path reads
  hook-written rows, not MCP `context_cycle`; never the rework/legacy frame variants (they
  record nothing — the SR-08 vacuous-pass trap). (FR-3, FR-6)
- **C-4 Closed-exclusion-set discipline + disposition authority (nan-021 NFR-8 / ADR-003).**
  Per dimension, exclusions are closed/justified; any non-excluded divergence is a
  PRODUCT/HUMAN call (GH bug OR product-signed amendment via `context_correct`), never a
  silent widen. (FR-16, NFR-8)
- **C-5 Single-source the full CONTRACT, not just shared data (#5302 / SR-05).** One manifest,
  one identity, one barrier helper, one comparator template, one forbidden-seed set. (NFR-3)
- **C-6 Intentional #830 coupling (nan-021 ADR-002).** The fixture relies on the shipped
  single-flight `keep_alive` self-heal; do NOT re-implement reconnection. That self-heal covers
  only SIGNALLED (404) eviction; the silent half-open-socket eviction (#839 / #5303) it did NOT
  cover is now CLOSED (landed via commit 5b6badad / PR #842, 2026-06-25), so the C0 precondition
  is met and delivery is UNBLOCKED. As **defense-in-depth** (not a gating dependency), an
  HTTPS-leg hang must still surface as INFRA-ERROR via the bounded preflight/deadline (FR-13),
  never as a parity result.
- **C-7 CI lane.** Release-gate Docker lane via `workflow_dispatch`/tag, false-green-proof,
  skip-when-Docker-absent HARD-fails. NOT the JS-only `ci.yml` `pull_request` matrix. (FR-17)
- **C-8 Outcome-class separation (SR-04 / OQ-2).** Cross-transport divergence (PARITY-FAIL,
  RED) and intra-transport nondeterminism (separate filed bug, NOT this gate) MUST be
  structurally separated; conflating them poisons the red gate with non-cloud flakes.
  Retrieval intra-transport ranking determinism is a PREREQUISITE, surfaced as a separate
  outcome class, NOT a parity-gate RED. (FR-10, FR-11)
- **C-9 #5298 / SR-08 missing-capture-errors.** A dimension routed to the wrong wire surface
  records nothing; that MUST ERROR (INFRA-ERROR), never empty-pass (nan-021 R-03 carried to
  all six dimensions). (FR-12)

### Dependencies

- **Depends on #836 (nan-021).** Consumes its fixture (`infra-001`) verbatim: the manifest,
  identity, barrier, MetricVector comparator, leg drivers, smoke scripts, and bridge driver.
- **#5298** — the canonical RecordEvent contract (11-frame hook sequence) every
  observe-driven dimension must conform to byte-identically.
- **#839 / #5303** — the silent half-open-socket eviction bug, now CLOSED (commit 5b6badad /
  PR #842, 2026-06-25); no longer a gating dependency. The suite still classifies any half-open
  hang as INFRA-ERROR as defense-in-depth, never read as a verdict (SR-02 / C-6).
- **GH#746 / #4990 (SR-01)** — HNSW approximate-top-k flip from per-process OS entropy
  (`hnsw_rs` 0.3.4, no seed API), deferred; shapes the D1/D4 ranked-prefix tolerance and the
  INTRA-TRANSPORT-NONDETERMINISM outcome class.
- **Existing substrate components** consumed/extended:
  `harness/parity_workload.py`, `harness/metric_comparator.py`, `harness/parity_legs.py`,
  `harness/uds_client.py`, `harness/hook_client.py`, `suites/test_https_uds_parity.py`,
  `scripts/cloud-cycle-https-leg.sh`, `scripts/docker-http-posture-smoke.sh`,
  `scripts/bridge-cycle-driver.js`, `scripts/bridge-witness.js`.
- **Environment:** Docker (engine 29.5.2, Compose v2.40.3, linux/arm64); Python 3.11 +
  pytest; stdlib-only harness (zero new runtime deps).

---

## 8. NOT In Scope

- **No production-code changes.** Any defect found is filed, not fixed.
- **Not a fix-it feature** for any parity, determinism (SR-01/#4990/GH#746), or transport
  bug surfaced — those are filed as GH bugs.
- **Not a re-prove of nan-021's analytics slice** — its comparator/workload/barrier is
  consumed verbatim, not re-authored.
- **Not a soak / load / performance test** — parity of observable outcomes, not throughput.
- **Not a Claude-Code-driven integration** — the workload is driven by the harness/bridge,
  not a live CC host (nan-021 constraint carried forward; relevant to the D5 OQ-3 host-side
  call-out).
- **Does not broaden C0's surface** beyond the six named dimensions, nor invent new server
  behavior.
- **Does not itself amend any exclusion set silently** — amendments are product-signed.
- **Does not itself flip C0 (#5304) → `proven`** — it produces the proof artifact; an
  authorized session performs the flip (AC-12).
- **Not wired into the JS-only `ci.yml` `pull_request` matrix** — release-gate lane only.

---

## 9. Open Questions (for architect / human)

- **OQ-A (architect, SR-01/SR-03/NFR-4/NFR-7): ranked-prefix depth N + the single tolerance
  policy.** What is the stable ranked-prefix length N for retrieval/briefing parity, and what
  is the exact bounded tie/score tolerance? Spec mandates ONE policy shared by D1+D4 over a
  non-degenerate corpus; the concrete N and corpus size are an architecture/test-design call.
- **OQ-B (architect, OQ-1/SR-06): workload augmentation shape.** How is the seed corpus +
  query phase woven into the SINGLE manifest while preserving one identity / one token / one
  barrier? (Spec leans (a): augment the single workload, per SCOPE OQ-1.)
- **OQ-C (architect, OQ-3/SR-07): PreCompact symmetric capturability.** Is the
  `CompactContext` restored payload fully capturable test-only from both legs, or is there a
  host-side (CC) component? Determine at design time; if not test-only-drivable, document the
  delivery-time measurability call-out (D5) — do NOT drop PreCompact from scope.
- **OQ-D (architect, OQ-4): `Informs` edges + phase determinism.** Are `Informs` edges
  deterministic for the identical workload, or do they need a barrier/tolerance for
  tick/background timing? Affects whether AC-04's edge clause compares exactly post-barrier.
- **OQ-E (human, OQ-6/AC-12): C0 flip bar — RESOLVED (human-confirmed 2026-06-25).** All SIX
  dimensions block the flip. The corrected C0 (#5304) `done_when` makes parity the total bar:
  "Parity is the bar; it is simple and total… the dimension list is the present expression of
  the parity bar and grows with the pipeline; it does not narrow the bar," with any unreachable
  dimension a human-signed documented exception, never silently excluded. Design default
  `blocks_c0_proof=True` for all six is correct and aligned — not a pending coin-flip. The
  documented-exception escape valve covers a legitimately unreachable dimension (e.g. the D5
  PreCompact host-side gap).

---

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + `context_get` — surfaced the canonical
  RecordEvent 11-frame parity sequence (#5298, the SR-08 vacuous-pass / wrong-frame trap),
  the MetricVector by-identical-sequence comparator pattern, nan-021 ADR-001
  single-workload/one-identity (#5286), the false-green discriminator (#5290/#5296), the
  single-source-the-full-contract lesson (#5302/SR-05), and the C0 marquee-promise capability
  (#5304). No storage — spec decisions are feature-specific (read-only tier).
