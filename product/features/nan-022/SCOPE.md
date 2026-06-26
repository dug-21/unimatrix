# Cross-Transport Parity Suite — C0's Proof Artifact

## Problem Statement

C0 (#5304, `personal-cloud`) is the goal's **marquee promise**: "for a remote slug,
retrieval AND behavioral signals AND analytics/learning all function at parity with a
local-UDS deployment of the same workload — **measured, not asserted**." Its
constituents C10 (remote retrieval) and C11 (remote observe) are individually proven,
but the **composed, measured intelligence-pipeline parity across transports** — C0's own
`done_when` — has never been tested. It has been flagged "owed" at multiple gates with no
GH issue tracking it until #837.

#836 (nan-021, just merged) built the HTTPS-bridge integration fixture and proved exactly
**one** of the parity dimensions: analytics/learning — a single live-vs-live
`context_cycle_review` `MetricVector` comparison plus derived `topic_signal` attribution.
Everything else over HTTPS — retrieval ranking, the observe→attribution signal as an
observable in its own right, behavioral `Informs` edges, proactive injection/briefing,
PreCompact restoration, and per-slug isolation — remains **asserted, not proven**.

This feature is the parity *suite* built on the #836 fixture: it runs the SAME workload
over stdio/UDS and over the HTTPS bridge and asserts measured parity across the **full C0
surface**. Passing it is what lets an authorized session flip C0 (#5304) → `proven`.

Affected: every remote `personal-cloud` deployment — until this passes, "cloud == local"
is a claim, not a measurement.

## Goals

1. Extend the nan-021 HTTPS-bridge fixture (in `product/test/infra-001/`) into a
   **cross-transport parity matrix** that drives one canonical workload over BOTH
   transports in one execution and asserts measured parity across all six C0 dimensions.
2. Generalize the nan-021 single-output comparator into a **multi-output parity engine**:
   for each dimension, capture a deterministic output from each leg and compare
   field-for-field MODULO a per-dimension, closed, justified wall-clock/nondeterminism
   exclusion set (the proven nan-021 D-5 discipline, applied per dimension).
3. Prove each of the six dimensions over both transports:
   - **Retrieval** — `context_*` (search / lookup / get) results and ranking identical.
   - **Behavioral signals** — observe → `topic_signal` attribution identical.
   - **Analytics / learning** — `context_cycle_review` `MetricVector`, behavioral
     `Informs` edges, and phase signal identical.
   - **Proactive delivery** — injection / `context_briefing` output identical.
   - **PreCompact restoration** — restored context payload identical.
   - **Per-slug isolation** — no cross-project bleed under either transport.
4. Land it as a **CI-runnable parity matrix** in the existing release-gate Docker lane
   (the nan-019/nan-021 pattern), false-green-proof, skip-when-Docker-absent HARD-fails.
5. If the matrix surfaces a real parity defect, **file it as a new GH bug** and leave the
   gate RED — the fixture doing its job. Do NOT absorb the fix.

## Non-Goals

- **No production-code changes.** Test-only; extend `infra-001` cumulatively (NEVER a
  parallel scaffold, NEVER a fork). The diff is test infra only.
- **Not a fix-it feature.** Any defect the matrix finds is filed, not fixed, here.
- **Not a re-prove of nan-021's analytics slice.** That comparator/workload/barrier is
  CONSUMED verbatim and extended; it is not re-authored.
- **Not a soak / load / performance test.** Parity of observable outcomes, not throughput.
- **Not a Claude-Code-driven integration.** The workload is driven by the harness/bridge,
  not a live CC host (the nan-021 constraint carries forward).
- **Does not broaden C0's surface beyond the six named dimensions**, nor invent new server
  behavior. It measures the shipped behavior.
- **Does not itself amend any exclusion set silently** — see Constraints (disposition
  authority carries over from nan-021 NFR-8).
- Not wired into the JS-only `ci.yml` `pull_request` matrix (release-gate lane only,
  mirroring nan-021 D-3).

## Background Research

### What nan-021 actually built (the reusable substrate)

The fixture lives in `product/test/infra-001/`. The reusable machinery:

- **`harness/parity_workload.py`** — the single declarative `ParityWorkload` manifest
  (ordered tool calls + ONE load-bearing Bash carrying the feature-ID derivation token +
  ONE stable CC session identity that doubles as the run-correlation token), the symmetric
  `durability_barrier`, the `assert_no_seed_reachable` static guard, and the
  `load_https_vector` stale-token guard.
- **`harness/metric_comparator.py`** — the operational definition of parity for the
  analytics dimension: field-for-field equality over the 21 `UniversalMetrics` fields +
  `phases` + `domain_metrics`, MODULO a **closed, enumerated, individually-justified
  3-field wall-clock exclusion set** (`EXCLUDED`), with `AT_RISK_FIELDS` flagged and a
  `field_by_field_record` evidence emitter. This is the **template** the five new
  dimensions each replicate (their own closed exclusion sets).
- **`harness/parity_legs.py`** — `drive_uds_leg` (in-process UDS + hook IPC),
  `run_https_leg` (shell-out to the Docker smoke), `assert_derived_attribution`.
- **`suites/test_https_uds_parity.py`** — pytest-as-orchestrator: drives both legs in ONE
  invocation, ingests the HTTPS vector via the token guard, runs the comparator, emits the
  first-live-run evidence record.
- **HTTPS leg (shell + JS)** — `scripts/cloud-cycle-https-leg.sh` (the
  `UNIMATRIX_HTTPS_SMOKE` target wrapping `run_smoke_gate` for the false-green
  discriminator), `scripts/docker-http-posture-smoke.sh` (`cloud_cycle_gates`),
  `scripts/bridge-cycle-driver.js` (drives `context_*` JSON-RPC THROUGH the shipped
  `mcp-bridge.js` over pinned HTTPS — D-2 bridge-in-path), `scripts/bridge-witness.js`
  (NODE_OPTIONS preload wire-witness).
- **The canonical RecordEvent sequence** (knowledge #5298): the 11-frame hook wire
  sequence both legs MUST emit byte-identically. `context_cycle_review`'s primary
  (col-024) path reads HOOK-written rows, NOT MCP `context_cycle` calls. This is the
  hard-won contract any new observe-driven dimension must conform to.

### The gap this feature closes

C0's `done_when` names three pillars; #837 expands to six dimensions. nan-021 proved
exactly **one** (analytics/learning, via the single `MetricVector` comparison + the
behavioral-attribution side-assertion). The other five are net-new parity surfaces.

### Per-dimension landscape (output → code path → determinism note)

| Dimension | Observable output | Code path | Determinism note |
|---|---|---|---|
| Retrieval | `context_search`/`context_lookup`/`context_get` result ids + ranking | `mcp/tools.rs` search/lookup/get; bridge already speaks `tools/call` | Ranking has nondeterminism traps — HashMap iteration order (#2610), `sort_unstable` tie-breaks (`tools.rs:1598,10758`). Same store + same query SHOULD be deterministic, but ties and embedding cold-start need a tolerance/seed strategy. **Highest-risk dimension.** |
| Behavioral signals | `topic_signal` per observation | `observations` table; nan-021 `assert_derived_attribution` reads it | Already proven string-exact in nan-021 for the UDS leg; needs symmetric HTTPS-leg read + cross-leg compare. |
| Analytics/learning | `MetricVector` + behavioral `Informs` edges + phase | `context_cycle_review`; `context_edge`/graph for Informs | `MetricVector` PROVEN (nan-021). `Informs` edges + phase signal are NET-NEW parity surfaces. |
| Proactive delivery | injection set + `context_briefing` ranked index | `context_briefing` (`tools.rs:1423`), `record_injection` (`session.rs:619`) | Briefing is embedding/cluster-ranked — same nondeterminism class as retrieval. Injection history is session-state. |
| PreCompact restoration | restored compact context payload | `wire.rs:171` (`CompactContext`/PreCompact), HookRequest #670 | Goes over the **hook /observe route**, not the MCP bridge — different HTTPS surface than `context_*`. |
| Per-slug isolation | no cross-project read/write bleed | per-slug store routing (#783, vnc-034); already gated in posture smoke Gates 1–4 | Posture smoke ALREADY proves a per-slug write lands in `/data/.unimatrix/<slug>/` and not the hash dir. Needs the *parity* framing: isolation holds identically under both transports. |

### Transport-surface asymmetry discovered

The HTTPS leg has **two** distinct wire surfaces, and dimensions split across them:
- **MCP bridge** (`mcp-bridge.js`, JSON-RPC `tools/call` over pinned HTTPS): retrieval,
  briefing, cycle_review, edges — anything that is a `context_*` MCP tool.
- **Hook `/observe` route** (pinned HTTPS POST, per-slug funnel): observe/attribution,
  PreCompact, cycle_start/stop record events.

The UDS leg mirrors this: MCP UDS socket vs hook IPC socket. The nan-021 leg drivers
already use both (`UnimatrixUdsClient` + `UnimatrixHookClient`). Each new dimension must be
routed to the correct surface on BOTH legs — getting this wrong silently records nothing
(the #5298 gotcha: legacy/rework frames record nothing).

## Proposed Approach

Generalize nan-021's single-output gate into a **dimension-keyed parity matrix**, reusing
every piece of the substrate:

1. **One workload, more outputs.** Keep the single `ParityWorkload` manifest and the
   single stable session identity (the SR-05 / #832 root-cause defense). Extend the leg
   drivers so a single drive of the workload over each transport yields a *bundle* of
   per-dimension captured outputs (retrieval results, attribution, MetricVector, briefing,
   restored-context, isolation probe), not just the one MetricVector.
2. **A comparator per dimension, on the nan-021 template.** Each dimension gets a small
   comparator module mirroring `metric_comparator.py`: explicit field classification, a
   closed + justified exclusion set, an evidence record, and a loud `ParityMismatch`. The
   matrix runs them all and reports a per-dimension PASS/FAIL table.
3. **Pytest-as-orchestrator, extended.** The existing orchestrator drives both legs in one
   invocation under one run-token; extend it to ingest the dimension bundle from the HTTPS
   leg (token-guarded, never-empty) and run every dimension comparator in the same
   execution. A missing dimension ERRORS — never a vacuous pass (nan-021 R-03 discipline).
4. **Extend the HTTPS leg, don't fork it.** `bridge-cycle-driver.js` already speaks
   `tools/call`; add the retrieval/briefing calls. PreCompact/observe ride the existing
   pinned `/observe` route. The smoke writes a dimension-keyed bundle to
   `$HTTPS_VECTOR_OUT` instead of a single vector. Reuse `cloud-cycle-https-leg.sh` /
   `run_smoke_gate` false-green discriminator as-is.
5. **Conform every observe-driven dimension to the #5298 RecordEvent sequence** —
   byte-identical frames on both legs; never the rework/legacy variants.
6. **Per-dimension first-live-run validation gate** (nan-021 ADR-003 discipline): the
   first live dual-transport run of each new dimension is examined field-by-field; any
   exclusion-set entry is an UNPROVEN ASSUMPTION until confirmed; disposition is a
   PRODUCT/HUMAN call (defect → GH bug; transport-inherent → product-signed amendment).

Rationale: the nan-021 architecture (ADR-001 single-workload/one-identity,
pytest-as-orchestrator, closed-justified-exclusion-set comparator) is exactly the right
shape for N dimensions — it already solved the SR-05 drift hazard and the false-green
hazard once. Generalizing it is far lower-risk than a new scaffold and is mandated by the
"extend infra-001, never fork" constraint.

## Acceptance Criteria

- **AC-01**: One canonical workload (the nan-021 manifest, possibly augmented with
  retrieval/briefing-triggering steps) drives BOTH transports in ONE pytest invocation,
  under ONE stable session identity and ONE run-correlation token. Zero seeded attribution
  anywhere in the path (nan-021 AC-03 static guard extended to cover all new modules).
- **AC-02**: **Retrieval parity** — for an identical `context_search`/`context_lookup`/
  `context_get` query set against an identically-seeded store, both legs return the same
  result ids in the same ranked order, MODULO a closed, justified nondeterminism exclusion
  set (tie-break / score tolerance defined and minimized).
- **AC-03**: **Behavioral-signal parity** — every driven observation attributes
  `topic_signal == feature` (derived, never seeded) identically on both legs; cross-leg
  compared, not just per-leg asserted.
- **AC-04**: **Analytics/learning parity** — `MetricVector` parity holds (consuming the
  nan-021 comparator verbatim) AND behavioral `Informs` edge set + phase signal are equal
  field-for-field MODULO a closed, justified exclusion set.
- **AC-05**: **Proactive-delivery parity** — `context_briefing` ranked index (entry ids +
  order) and the resulting injection set are identical across legs MODULO a closed,
  justified nondeterminism exclusion set.
- **AC-06**: **PreCompact-restoration parity** — the restored compact-context payload is
  identical across legs MODULO a closed, justified wall-clock/ordering exclusion set.
- **AC-07**: **Per-slug isolation parity** — under BOTH transports, a write to slug A is
  not visible to slug B and lands only in slug A's store; no cross-project bleed under
  either transport. (Builds on the posture smoke's existing per-slug Gates.)
- **AC-08**: The suite runs as a **CI-runnable parity matrix** in the release-gate Docker
  lane (`workflow_dispatch`/tag, not per-PR), is false-green-proof (skip-when-Docker-absent
  HARD-fails by distinct exit code; anchored run-marker asserted), and emits a per-dimension
  PASS/FAIL evidence table keyed by the run-correlation token.
- **AC-09**: Every per-dimension exclusion set is **closed, enumerated, and individually
  justified** in code (the nan-021 `EXCLUSION_JUSTIFICATIONS` pattern); no set is silently
  widened — exclusion-set amendments require product sign-off (NFR carried from nan-021).
- **AC-10**: A real parity defect surfaced by any dimension is **filed as a new GH bug**
  and the gate stays RED; the fix is NOT absorbed into this feature.
- **AC-11**: **Zero production-code change** — the diff is test infra only, extending
  `infra-001` cumulatively with no parallel scaffolding and no fork.
- **AC-12**: Passing the full matrix is **C0's proof artifact** — it provides the
  behavioral evidence an authorized session uses to flip C0 (#5304) → `proven`. (This
  feature does NOT itself flip C0.)

## Constraints

- **No production code.** Test-only, cumulative on `infra-001`; no fork, no parallel
  scaffold (nan-021 AC-06/AC-07 carry forward).
- **Bridge-in-path (D-2).** Drive `context_*` THROUGH the shipped `mcp-bridge.js` over
  pinned HTTPS; never POST `mcp_url` directly. Observe/PreCompact ride the pinned
  `/observe` route. Reuse mcp-bridge.js / cert-pin.js / credstore.js / bundle.js / init.js
  as-is; net-new transport/cert/spawn code is a fork smell to FLAG.
- **#5298 RecordEvent contract.** Every observe-driven dimension emits the byte-identical
  11-frame hook sequence on both legs; `context_cycle_review`'s primary path reads
  hook-written rows, not MCP `context_cycle`; never the rework/legacy frame variants
  (they record nothing).
- **Closed-exclusion-set discipline + disposition authority (nan-021 NFR-8 / ADR-003).**
  Per dimension, exclusions are closed/justified; any non-excluded divergence is a
  PRODUCT/HUMAN call (GH bug OR product-signed amendment via `context_correct`), never a
  silent widen by implementer/tester.
- **Single-source the full CONTRACT, not just shared data (#5302).** One manifest, one
  identity, one barrier helper, one comparator template — duplicated framing drifts
  silently (the SR-05 hazard nan-021 closed at the architecture level).
- **Intentional #830 coupling (nan-021 ADR-002).** The fixture relies on the shipped
  single-flight `keep_alive` self-heal; a flake here SIGNALS a #830 regression; do not
  re-implement reconnection.
- **CI lane.** Release-gate Docker lane via `workflow_dispatch`/tag, false-green-proof,
  skip-when-Docker-absent HARD-fails. NOT the JS-only `ci.yml` `pull_request` matrix.
- **Environment.** Docker available (engine 29.5.2, Compose v2.40.3, linux/arm64) for the
  containerized HTTPS/TLS fixture; the off-Docker seam/unit layer (nan-019/nan-021
  precedent) keeps comparator TEETH unit-tested before any tag round.
- **Depends on #836 (nan-021)** — consumes its fixture verbatim.

## Open Questions

1. **Workload shape vs. retrieval/briefing.** nan-021's manifest is a 3-call cycle that
   produces a `MetricVector`. Retrieval and briefing parity need a *pre-seeded store* and a
   *query set* with non-trivial ranking. Do we (a) augment the single workload with a
   deterministic store-seed + query phase, or (b) split into a small set of dimension
   sub-workloads sharing one identity? (Leaning (a) to preserve "one workload" — but
   ranking needs enough entries to be a real ranking, not a degenerate single hit.)
2. **Retrieval ranking determinism — tolerance vs. exact.** Ranking has HashMap-order
   (#2610) and `sort_unstable` tie-break traps, plus embedding cold-start. Is the parity
   assertion exact result-id ordering (requiring a deterministic seed + tie-break), or
   ordered-set-with-tolerance-on-ties? This is the highest-risk dimension and shapes
   whether retrieval parity is even achievable without a production determinism fix (which
   would be a filed bug, not absorbed). **Needs the human's call on acceptable tolerance.**
3. **PreCompact over HTTPS.** PreCompact restoration rides the hook `/observe` route
   (`wire.rs` `CompactContext`, #670), not the MCP bridge. The restored payload may NOT be
   fully capturable from both legs symmetrically: the test-only harness cannot drive a live
   Claude-Code host. The honest disposition is that D5 may be **"measured-where-drivable +
   documented host-side gap"** rather than a full symmetric measurement — a legitimate
   human-signed documented-exception, but it MUST be stated plainly for the flip session and
   NEVER rounded up to "fully measured." Resolved at first live drive.
4. **`Informs` edges + phase as a parity surface.** Are behavioral `Informs` edges
   deterministic for an identical workload, or do they depend on tick/background timing
   that would force a barrier-or-tolerance? (Affects whether AC-04's edge clause is exact.)
5. **Per-dimension granularity in CI.** One matrix gate emitting a per-dimension table, or
   independently-taggable dimension gates? (Leaning one matrix, one tag, per-dimension rows
   — but a single flaky dimension then reddens the whole gate.)
6. **C0 flip bar (six vs three) — RESOLVED (human-confirmed 2026-06-25).** The corrected C0
   (#5304) `done_when` settles it: "Parity is the bar; it is simple and total… the dimension
   list is the present expression of the parity bar and grows with the pipeline; it does not
   narrow the bar," with the disposition that any unreachable dimension is a human-signed
   documented exception, never silently excluded. So the design default
   `blocks_c0_proof=True` for all six is CORRECT and ALIGNED. Not a coin-flip: all six block,
   with the documented-exception escape valve for a legitimately unreachable dimension (e.g.
   the D5 PreCompact host-side gap).

## Tracking

GH Issue #837. (Tracking link updated after Session 1.)

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + `context_search` + `context_get` —
  surfaced C0 (#5304), C11 (#5153), nan-021 ADR-001 (#5286), the canonical RecordEvent
  parity sequence (#5298), the wire-witness pattern (#5296), the MetricVector
  by-identical-sequence pattern, the single-source-the-full-contract lesson (#5302), and
  the HashMap-ordering determinism trap (#2610).
- Stored: nothing yet at scope time — the reusable patterns (multi-output parity-matrix
  generalization of a single closed-exclusion-set comparator; two-HTTPS-surface routing of
  parity dimensions) are candidates to store on completion if they prove out, not before.
