# Risk-Based Test Strategy: nan-022

Cross-Transport Parity Suite — C0's proof artifact (#837). TEST-ONLY; extends the nan-021
`infra-001` fixture into a six-dimension × two-transport parity matrix. This strategy
identifies what could make the suite **fail at its one job** — being a trustworthy C0 proof
artifact. The two cardinal failures are **false-RED** (the suite reds the gate or errors on
something that is not a cloud parity defect, blocking the C0 flip on noise) and **false-GREEN**
(the suite passes without actually measuring cross-transport parity — a vacuous pass that
flips C0 → `proven` on no evidence). Every risk below is scored by which cardinal failure it
drives.

Historical grounding (Unimatrix): #5267 (release-only gate chains never-green-on-tag, budget
N rounds + pre-tag real-server exercise), #5177 (multi-wave features under-test the earlier
wave's parity ACs → vacuous pass), #5302 (single-source the full CONTRACT or it drifts —
nan-021 hit it twice), #5285 (derive `topic_signal` over the wire, never seed the join),
#4822 (hook-client parity-corpus build-request trap).

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Retrieval/briefing exact-order assertion flakes on HNSW top-k membership flip (#4990/GH#746) → false-RED; OR stable-prefix tolerance set too loose → vacuous pass | High | High | **Critical** |
| R-02 | A silent half-open-socket hang on the HTTPS leg (the #839 class — now CLOSED via commit 5b6badad / PR #842; INFRA-ERROR handling retained as defense-in-depth) read as a dimension verdict (FAIL) instead of INFRA-ERROR → false-RED across all six dimensions | High | Low | **High** |
| R-03 | A dimension routed to the wrong wire surface (MCP-bridge vs `/observe`) records NOTHING and passes vacuously — the #5298 legacy/rework-frame trap → false-GREEN | High | Med | **Critical** |
| R-04 | WAL-flush capture timing: per-dimension DB-reading captures (D2 observations, D6 landing) read a pre-checkpoint snapshot if not gated by the durability barrier → latent INFRA-ERROR misread as PARITY-FAIL/empty → false-RED or false-GREEN | High | Med | **Critical** |
| R-05 | Six hand-authored exclusion sets / capture shapes drift from the single contract; drift guard incomplete → silent widening greens a real divergence → false-GREEN | High | Med | **High** |
| R-06 | Seed corpus / query set too thin → degenerate single-hit ranking → stable prefix is trivially equal → vacuous retrieval/briefing pass (#5177) → false-GREEN | High | Med | **High** |
| R-07 | Double-capture-and-diff intra-transport detector mis-tuned: too strict reds healthy ranking (false-RED), too loose lets a real cross-transport divergence be reclassified as INTRA-NONDET and silently dropped (false-GREEN) | High | Med | **High** |
| R-08 | PreCompact host-side (CC) component not test-only-drivable; `measurable=False` call-out silently treated as PASS, OR the un-driven portion vacuously passes → false-GREEN on D5 | Med | High | **High** |
| R-09 | The dimension bundle is a cross-language contract (Python ingest vs JS/shell emit); a key/shape mismatch yields a missing/None capture read as empty-pass instead of INFRA-ERROR → false-GREEN | High | Med | **High** |
| R-10 | New release-only matrix gate never green on a real tag: failures surface in sequence across N tag rounds (#5267); off-Docker seam tests give false confidence | Med | High | **High** |
| R-11 | `Informs` edges + phase signal depend on tick/background timing; compared before they land → spurious PARITY-FAIL (false-RED) or compared as absent → vacuous (false-GREEN) | Med | Med | Medium |
| R-12 | Run-correlation token guard regression: a stale prior-tag HTTPS bundle ingested as this run's result → false-GREEN/false-RED on phantom data | High | Low | Medium |
| R-13 | Augmenting the manifest (seed+query phase) breaks the ONE-identity/ONE-token/ONE-barrier invariant → cross-leg framing drift (the SR-05/#832 hazard re-opened) | Med | Med | Medium |
| R-14 | Analytics `MetricVectorComparator` ABC adapter alters the consumed nan-021 `compare_metric_vectors` logic (AC-04 re-prove forbidden) → silent change to a proven comparator | Med | Low | Low |
| R-15 | Forbidden-seed audit misses a net-new module / the seed-corpus loader → a compared OUTPUT is seeded not derived (the #5285 trap) → false-GREEN | High | Low | Medium |
| R-16 | Scope-creep / fork smell: net-new transport/cert/spawn code added to the HTTPS leg instead of reusing shipped `mcp-bridge.js` → AC-11 SCOPE-FAIL + a parallel parity path that drifts | Med | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: Retrieval/briefing ranking nondeterminism — false-RED or vacuous tolerance
**Severity**: High **Likelihood**: High
**Impact**: HNSW approximate top-k membership flips from per-process OS entropy (`hnsw_rs`
0.3.4, no seed API — verified bugfix-742/#4990, deferred GH#746), plus HashMap order (#2610)
and `sort_unstable` ties. A naive exact-id-order assertion flakes the highest-value dimension
(false-RED, blocking the C0 flip on noise). Over-correcting — a stable-prefix tolerance set so
loose that the prefix is trivially short — greens a real cross-transport divergence (vacuous).

**Disposition (load-bearing).** Retrieval is THE dimension where "measured parity" is most
tempted to soften into "tolerant parity." The tie-class tolerance MUST be scrutinized at first
live run so it CANNOT swallow a real cross-transport ranking divergence. If exact ordering is
unachievable without a production determinism fix (#4990 / GH#746 HNSW, no seed API), that is a
FILED BUG + a documented C0 (#5304) exception — NEVER a quiet widening of the tolerance.
Widening is a product/human-signed disposition only (NFR-8), never an implementer/tester call.

**Test Scenarios**:
1. Off-Docker unit: feed `ranking_parity` two id-lists identical in a deep prefix but churned
   in the tail → verdict `matched=True`, `stable_prefix_len ≥ N`, tail churn recorded not failed.
2. Off-Docker unit: two lists that diverge WITHIN the stable prefix → `matched=False` (must
   surface as a real PARITY-FAIL candidate, not tolerated).
3. Off-Docker unit: equal-score tie-class permuted between legs → tie-class membership equal,
   position ignored → `matched=True`; a tie-class with a *missing member* → `matched=False`.
4. Off-Docker unit: scores absent → policy degrades to membership-only on the prefix; assert
   this fallback is the documented justified path, not a silent loosening.
5. Boundary: prefix length exactly N vs N-1 — assert the policy does not pass on a prefix
   shorter than the NFR-7 floor (couples to R-06).

**Coverage Requirement**: The single `ranking_parity` policy is exercised off-Docker across
deep-prefix-match, in-prefix-divergence, tie-class permutation, tie-class-member-loss, and
scores-absent fallback BEFORE any tag round. Tolerance contents are an enumerated justified
`EXCLUDED` entry (AC-09). The prefix floor N is asserted non-trivial. At first live run the
tolerance is scrutinized against a real cross-transport divergence to prove it cannot swallow
one; an unreachable exact-order requirement is a filed bug + documented C0 exception, never a
silent widening.

### R-02: half-open hang read as a parity verdict — false-RED on all six
**Severity**: High **Likelihood**: Low
**Impact**: The #830 self-heal the fixture couples to covers only SIGNALLED (404) eviction; it
did NOT cover the #839 silent half-open-socket hang, but #839 is now CLOSED (commit 5b6badad /
PR #842, 2026-06-25), so the C0 precondition is met and delivery is unblocked. The INFRA-ERROR
handling below is retained as **defense-in-depth**, not a gating dependency: if any residual
half-open hang reads as a dimension FAIL — or worse, hangs the whole suite unbounded — the gate
false-REDs every dimension and never produces a verdict.

**Test Scenarios**:
1. Off-Docker unit: `preflight_leg` against an unreachable / never-responding socket → raises
   `InfraError` within the bounded connect deadline; never blocks unbounded.
2. Off-Docker unit: a socket that accepts then never replies (half-open simulation) → idle
   deadline expires → `InfraError`, classified INFRA-ERROR, distinct exit code, not PARITY-FAIL.
3. Roll-up unit: a dimension result of INFRA-ERROR → gate exits with the distinct ERROR code,
   is NOT counted toward the parity RED verdict, and surfaces the transport-health detail in output.
4. Boundary/tuning: a slow-but-healthy leg completing just under the idle deadline → PASS, not
   misread as INFRA (guards against an over-tight deadline that manufactures false INFRA).

**Coverage Requirement**: Every leg has a bounded connect + idle deadline; a hang is provably
classified INFRA-ERROR off-Docker; the roll-up never converts INFRA-ERROR into a parity RED;
the deadline is tuned with explicit head-room over a healthy-leg latency.

### R-03: Wrong-surface routing records nothing → vacuous pass (#5298 trap)
**Severity**: High **Likelihood**: Med
**Impact**: The HTTPS leg has two surfaces (MCP-bridge `tools/call` vs pinned `/observe`).
Routing an observe-driven dimension to the wrong surface, or emitting a rework/legacy frame
variant (`post_tool_use_rework_candidate`, `{"type":"PostToolUse"}`), records NOTHING — a
silent vacuous pass, not a loud fail. This is the single most dangerous false-GREEN path.

**Test Scenarios**:
1. Off-Docker unit: every `Dimension.wire_surface` is one of the two constants and matches the
   capture the leg driver actually performs (registry-vs-driver consistency).
2. Live (Docker): assert the byte-identical 11-frame #5298 RecordEvent sequence is emitted on
   BOTH legs for each observe-driven dimension (behavioral, precompact, analytics-cycle,
   isolation-write) — via the wire-witness; assert NO rework/legacy frame appears.
3. Negative: force a dimension's capture to the wrong surface (fault injection) → capture is
   empty → MUST raise INFRA-ERROR via the never-empty guard, NEVER PARITY-PASS.
4. Assert `_assert_hook_ok` / hook-Error-frame assertion fires when the `/observe` route
   returns an error frame.

**Coverage Requirement**: Registry routing matches driver behavior (off-Docker guard); #5298
11-frame byte-identity proven on both legs live; an empty capture for ANY dimension is a hard
INFRA-ERROR, proven by fault injection.

### R-04: WAL-flush capture timing — pre-checkpoint snapshot (latent INFRA for D2/D6)
**Severity**: High **Likelihood**: Med
**Impact**: D2 (behavioral `topic_signal` from `observations`) and D6 (isolation on-disk
landing) read the per-slug store directly. If a DB-reading capture runs before the durability
barrier confirms the WAL has flushed/checkpointed, it reads a pre-checkpoint snapshot —
rows/landing not yet visible. This presents as an empty or partial capture, which can be
misread as PARITY-FAIL (false-RED) or, if the other leg is equally early, as a matching empty
set (false-GREEN). The spec flagged this explicitly: per-dimension DB-reading captures MUST
respect the durability-barrier WAL-flush discipline.

**Test Scenarios**:
1. Live (Docker): every DB-reading capture (D2 observations read, D6 landing read,
   analytics-cycle-events read) is gated behind `durability_barrier`/`observe_count` BEFORE the
   capture is taken — assert the barrier is satisfied (expected observe count reached, dir
   byte-size incl `-wal` settled) prior to the read.
2. Negative: read BEFORE the barrier → assert capture is empty/partial → MUST classify
   INFRA-ERROR (barrier-not-satisfied), never PARITY-FAIL and never an empty-equals-empty pass.
3. Symmetry: the barrier discipline is applied IDENTICALLY on both legs (the same shared
   `durability_barrier` helper) so one leg cannot be checkpoint-gated while the other is not.
4. D3 edge/phase reads (R-11) share this barrier; assert no DB-reading capture bypasses it.

**Coverage Requirement**: No DB-reading dimension capture (D2, D3 cycle/edges, D6) is taken
before the symmetric durability barrier is satisfied on that leg; a pre-barrier read is an
INFRA-ERROR, proven by negative test; barrier is the SAME helper on both legs.

### R-05: Six exclusion sets / capture shapes drift from the single contract → false-GREEN
**Severity**: High **Likelihood**: Med
**Impact**: #5302 (twice in nan-021): convention is not a guard. Six near-duplicate comparators
each re-declaring `EXCLUDED`, justifications, and the forbidden-seed list drift silently; a
widened set greens a real divergence.

**Test Scenarios**:
1. Off-Docker `assert_comparator_contract(DIMENSIONS)`: every `Dimension.comparator` is a
   `DimensionComparator` subclass; each declares a NON-EMPTY `EXCLUDED` whose every key appears
   in `EXCLUSION_JUSTIFICATIONS` (no unjustified exclusion — AC-09).
2. Off-Docker: `FORBIDDEN_SEED_SITES` is referenced from the ONE definition; assert no module
   carries a private copy (grep/import-graph assertion).
3. Off-Docker: registry `capture_key`s are unique and exactly match the on-disk bundle schema
   keys (no orphan key, no unhandled dimension).
4. Negative: add an exclusion entry with no justification → the guard FAILS off-Docker (before
   any tag round, per #5258 seam discipline).

**Coverage Requirement**: The cross-dimension drift guard runs off-Docker and fails loud on an
unjustified exclusion, a duplicated forbidden-seed list, or a capture_key/schema mismatch.

### R-06: Thin corpus → degenerate ranking → vacuous parity pass (#5177)
**Severity**: High **Likelihood**: Med
**Impact**: #5177 — multi-wave/extended features under-test the ranking ACs. A single-hit or
near-trivial ranking makes the stable prefix trivially equal across legs: AC-02/AC-05 "pass"
while measuring nothing.

**Test Scenarios**:
1. Off-Docker: assert the seed corpus size and query set yield a ranking of depth such that the
   NFR-7 stable-prefix floor N is achievable and N > 1 (non-degenerate).
2. Live (Docker): assert each retrieval/briefing query returns ≥ N results on both legs before
   the ranking comparator runs; a result set shorter than N → INFRA-ERROR (degenerate-corpus
   guard), not a vacuous pass.
3. Assert the corpus is seeded via the real `context_store` path identically on both legs (not
   SQL/struct-injected — couples to R-15).

**Coverage Requirement**: The corpus produces a non-degenerate ranking (depth ≥ N > 1) on both
legs; a too-short result set errors rather than vacuously passing.

### R-07: Intra-transport double-capture detector mis-tuned → drops real divergence or reds healthy
**Severity**: High **Likelihood**: Med
**Impact**: The double-capture-and-diff classifier decides whether a flip is INTRA-NONDET
(dropped from the red gate) or proceeds to cross-leg compare. Too strict → healthy ranking
churn reds (false-RED). Too loose → a leg that is actually intra-stable but cross-transport
divergent gets reclassified as INTRA-NONDET and the real C0 defect is silently dropped
(false-GREEN — the most insidious failure of the outcome model).

**Test Scenarios**:
1. Off-Docker: a leg whose two captures differ only in the tolerated tail → intra-STABLE →
   proceeds to cross-leg compare.
2. Off-Docker: a leg whose two captures differ WITHIN the stable prefix → INTRA-NONDET for that
   leg → routed out of the red gate, filed separately (GH#746), NOT reddening.
3. Off-Docker (critical): BOTH legs intra-stable but cross-leg prefix divergent → MUST be
   PARITY-FAIL, NEVER reclassified as INTRA-NONDET. Assert the classifier order is INFRA →
   INTRA → cross-compare, and a cross-leg divergence on two stable legs cannot escape to INTRA.
4. Off-Docker: the intra-diff uses the SAME K3 tolerance as the cross-leg compare (no second
   tolerance) — assert single-sourced.

**Coverage Requirement**: Classifier order (INFRA→INTRA→PARITY) proven; a cross-leg divergence
on two intra-stable legs can never be reclassified as INTRA-NONDET; intra and cross use one
tolerance.

### R-08: PreCompact host-side gap silently treated as pass → false-GREEN on D5
**Severity**: Med **Likelihood**: High
**Impact**: PreCompact restoration may have a host-side (Claude-Code) component the test-only
harness cannot drive (the nan-021 no-live-CC constraint). If `measurable=False` is treated as
PASS, or the un-driven portion vacuously passes, D5 contributes a false-GREEN to the C0 proof.

**Test Scenarios**:
1. Off-Docker: a `precompact` capture with `measurable=False` and a non-null `host_side_gap` →
   the roll-up records a DOCUMENTED MEASURABILITY LIMITATION in the evidence table, NOT
   PARITY-PASS and NOT silently green.
2. Off-Docker: `measurable=True` with two restored payloads → field-for-field compare modulo
   the closed wall-clock/ordering set → PARITY-PASS/FAIL normally.
3. Live (Docker): determine at first drive whether `CompactContext` `/observe` frames are
   symmetrically capturable from both legs; if only partially, assert `host_side_gap` names the
   exact un-driven portion and the dimension does NOT pass on it.
4. Assert a `measurable=False` D5 interacts with `blocks_c0_proof` correctly (OQ-A human call):
   default it still blocks unless a human dispositions otherwise.

**Coverage Requirement**: `measurable=False` is a visible call-out, never a pass; the un-driven
portion is named and never vacuously passes; the measurability determination is made at first
live drive and recorded in the evidence table.

### R-09: Cross-language bundle contract mismatch → missing capture read as empty-pass
**Severity**: High **Likelihood**: Med
**Impact**: The dimension bundle is a real cross-language contract — JS/shell (`cloud_cycle_gates`,
`bridge-cycle-driver.js`) emits `{run_token, dimension_bundle:{...}}`; Python ingests it. A key
typo, a missing dimension key, or a `null` where a dict is expected can yield a `None`/absent
capture that — if not guarded — reads as empty-equals-empty PARITY-PASS.

**Test Scenarios**:
1. Off-Docker: `load_https_bundle` rejects a bundle missing ANY required `capture_key` →
   `InfraError` (never-empty guard), never a partial pass.
2. Off-Docker: a bundle with a `null` capture for a non-PreCompact dimension → INFRA-ERROR
   (only D5 is permitted a null `restored_payload`, and only with `measurable=False`).
3. Contract test: a fixture bundle matching the documented on-disk schema round-trips through
   the Python ingest and every dimension's comparator without a KeyError/shape error.
4. Live (Docker): the JS/shell-emitted bundle for a real run satisfies the same schema the
   off-Docker contract test asserts.

**Coverage Requirement**: Every required capture_key present and non-empty or INFRA-ERROR;
the on-disk bundle schema is contract-tested both off-Docker (Python side) and live (JS emit);
only D5 may carry a justified null.

### R-10: New release-only matrix gate never green on a tag (#5267)
**Severity**: Med **Likelihood**: High
**Impact**: #5267 — release-only gate chains reached for the first time fail in sequence across
N tag rounds; each layer masks the next. Off-Docker seam tests prove arithmetic, not the live
release topology. Treating round-2 failures as regressions wastes rounds.

**Test Scenarios**:
1. Off-Docker seam: the comparator TEETH, outcome classification, roll-up, and exit-code truth
   table are unit-tested without Docker (the #5258 / nan-019 precedent) BEFORE any tag.
2. Pre-tag real-server exercise: drive the full matrix against the local Docker HTTPS fixture
   (not the release tag) so the live layers surface before the release round.
3. Assert the skip-when-Docker-absent path HARD-fails by the distinct exit code (false-green-
   proof) and an anchored run-marker tied to the run-correlation token is present.

**Coverage Requirement**: Off-Docker teeth proven pre-tag; the full live matrix is exercised
on the local Docker fixture before any release tag; budget multiple tag rounds and treat
sequentially-revealed failures as new layers, not regressions.

### R-11: `Informs` edges + phase timing → premature or absent compare
**Severity**: Med **Likelihood**: Med
**Impact**: `Informs` edges/phase signal are net-new analytics parity surfaces (the MetricVector
slice is proven). If they depend on tick/background timing and are compared before landing,
they spuriously diverge (false-RED) or are compared as absent (vacuous).

**Test Scenarios**:
1. Live (Docker): edges/phase are compared only AFTER a barrier guarantees they have landed
   (couples to R-04 WAL discipline); pre-barrier compare → INFRA-ERROR, never PARITY-FAIL.
2. Edges compared as an unordered SET, IDs exact; phase compared exactly.
3. Off-Docker: any wall-clock/ordering edge field (e.g. creation timestamp) is a justified
   `EXCLUDED` entry; the edge-ID set itself is NOT excluded.

**Coverage Requirement**: Edge/phase compare is barrier-gated and exact post-barrier; only
wall-clock edge fields excluded with justification; pre-barrier compare errors.

### R-12: Stale-token bundle ingested → verdict on phantom data
**Severity**: High **Likelihood**: Low
**Impact**: A prior tag-round's HTTPS bundle left on disk, ingested as this run's result,
yields a verdict on data the wire never carried this run.

**Test Scenarios**:
1. Off-Docker: `load_https_bundle` with a bundle whose `run_token` != expected → `InfraError`
   (the nan-021 stale-token guard generalized).
2. Live: the anchored run-marker tied to the run-correlation token is asserted present this run.

**Coverage Requirement**: Token-guarded ingest rejects any non-matching run_token; run-marker
proves this-run traffic.

### R-13: Manifest augmentation breaks ONE-identity/ONE-token/ONE-barrier invariant
**Severity**: Med **Likelihood**: Med
**Impact**: Adding the seed+query phase could fork the identity/token/barrier, re-opening the
SR-05/#832 cross-leg framing drift hazard nan-021 closed.

**Test Scenarios**:
1. Off-Docker: assert a single `ParityWorkload` object, `run_token == workload.session_id`, one
   barrier helper, after augmentation.
2. Off-Docker: both legs replay the SAME augmented manifest byte-identically (manifest
   round-trip `to_json`/`from_json` stable).

**Coverage Requirement**: One manifest/identity/token/barrier preserved post-augmentation,
asserted structurally.

### R-14: ABC adapter alters the consumed nan-021 MetricVector logic (AC-04)
**Severity**: Med **Likelihood**: Low
**Impact**: Wrapping `compare_metric_vectors` in `MetricVectorComparator` must not change its
logic — AC-04 forbids re-proving/re-authoring the proven analytics comparator.

**Test Scenarios**:
1. Off-Docker: the adapter delegates to the unchanged `compare_metric_vectors`/`EXCLUDED`;
   a golden nan-021 MetricVector pair produces the identical diff list through the adapter.

**Coverage Requirement**: Adapter is behavior-identical to the consumed nan-021 comparator on
golden inputs.

### R-15: Forbidden-seed audit misses a net-new module → seeded output (#5285)
**Severity**: High **Likelihood**: Low
**Impact**: #5285 — cloud-path parity must DERIVE `topic_signal` over the wire, never seed the
attribution join. If the seed-corpus loader or a net-new module seeds a compared OUTPUT, the
parity is measuring an injected value → false-GREEN.

**Test Scenarios**:
1. Off-Docker: `assert_no_seed_reachable` covers EVERY net-new module AND the seed-corpus
   loader; the forbidden-seed sites (`_seed_observation_sql_lifecycle`,
   `_seed_attributed_observations_832`, `make_stamped_event(...,topic_signal)`) stay unreachable.
2. Off-Docker: assert the seed corpus writes CONTENT only via `context_store`; no compared
   output (`topic_signal`, MetricVector fields, edge IDs) is in the seed path.

**Coverage Requirement**: No-seed static guard extended to all net-new modules + seed loader;
seed writes content, never compared outputs.

### R-16: Fork smell — net-new transport/cert/spawn code (AC-11)
**Severity**: Med **Likelihood**: Low
**Impact**: Adding new transport/cert/spawn code instead of reusing shipped `mcp-bridge.js`
violates AC-11 (SCOPE-FAIL) and creates a parallel parity path that drifts from production.

**Test Scenarios**:
1. `git diff` confined to `product/test/infra-001/`; no `crates/**`, shipped `lib/`, or
   production-script change (SCOPE-FAIL guard).
2. Review-flag any net-new transport/cert/spawn/framing code in the HTTPS leg.

**Coverage Requirement**: Diff confined to test infra; bridge-in-path reuse verified; no
re-implemented transport code.

---

## Integration Risks

The suite is dense with cross-boundary seams; bugs concentrate here:

- **Cross-language bundle seam (R-09)** — JS/shell emit vs Python ingest of the dimension
  bundle. The on-disk JSON schema is the contract; a key/shape skew silently produces a missing
  capture. Highest-value integration test: a schema round-trip exercised on BOTH sides.
- **Two-HTTPS-surface fan-out (R-03)** — analytics and isolation each touch BOTH surfaces and
  need explicit fan-out in the leg drivers; a missed fan-out captures half a dimension.
- **Durability-barrier / DB-read ordering (R-04)** — the seam between "observe landed" and "DB
  read for capture" is where pre-checkpoint snapshots leak in; both legs must gate identically.
- **Bridge-in-path (D-2)** — new retrieval/briefing `tools/call`s ride the EXISTING
  `bridge-cycle-driver.js`; the integration assertion is that the bridge actually carried the
  new calls (SSE + Mcp-Session-Id replay), not a direct `mcp_url` POST.
- **#830 self-heal coupling** — a flake here SIGNALS a #830 regression and must not be
  re-implemented; the #839 half-open hang it did not cover is now CLOSED (commit 5b6badad / PR
  #842), but any residual half-open hang is still classified INFRA as defense-in-depth (R-02).
- **Classifier ordering (R-07)** — INFRA→INTRA→PARITY is itself an integration of three
  detectors; a wrong order lets a real defect escape into the dropped INTRA bucket.

## Edge Cases

- Empty result set / zero-length ranking on one or both legs (degenerate → R-06 INFRA, not pass).
- Result set shorter than the stable-prefix floor N (R-06).
- Tie-class straddling the prefix boundary — the last in-prefix position is a tie (R-01).
- Scores absent from the server response → membership-only fallback (R-01).
- One leg intra-stable, the other intra-unstable → dimension is INTRA-NONDET (not a half-compare).
- D5 `measurable=False` on one leg only (asymmetric measurability) — must be a call-out, not pass.
- Stale `$HTTPS_VECTOR_OUT` from a prior round present on disk (R-12).
- Slow-but-healthy leg near the idle deadline (R-02 boundary — not INFRA).
- A dimension key present in the registry but absent from the emitted bundle (R-09).
- `null` capture: permitted ONLY for D5 with `measurable=False`; INFRA-ERROR for any other.

## Security Risks

The suite is test-only and accepts no untrusted external input in production; its attack
surface is the test-harness boundary, assessed for completeness:

- **Untrusted input the suite accepts**: the HTTPS-leg bundle file (`$HTTPS_VECTOR_OUT`) read
  by the Python ingest, and TLS material in the Docker fixture. The bundle is harness-produced,
  not adversarial; the token guard (R-12) already rejects mismatched/stale bundles. Blast radius
  of a malformed bundle is a test failure, not production compromise — but it MUST be an
  INFRA-ERROR (R-09), never a vacuous pass that fakes a C0 proof.
- **Deserialization**: the bundle is JSON parsed by stdlib; assert it tolerates a malformed/
  truncated bundle by erroring (INFRA), not by partial-parsing into an empty-pass.
- **Cert pinning**: the suite REUSES the shipped `cert-pin.js`; introducing new cert/transport
  code (R-16) would broaden the production attack surface and is a fork smell to FLAG — the
  pinned-HTTPS posture must remain the shipped one, exercised in-path (D-2).
- **Per-slug isolation (D6)** is itself a security property — the parity framing proves cross-
  project read/write bleed does not differ by transport. A false-GREEN here would mask a real
  cross-tenant leak; D6 must compare the isolation boolean EXACTLY (no tolerance, NFR-6) and a
  missing probe is INFRA-ERROR (R-03/R-09), never an assumed-isolated pass.
- **Blast radius if compromised**: a corrupted suite produces a false C0 proof artifact — the
  damage is a wrongly-flipped C0 (#5304) → `proven`, i.e. an unmeasured "cloud == local" claim
  shipped to every remote `personal-cloud` deployment. This is why every false-GREEN path
  (R-03, R-05, R-06, R-08, R-09, R-15) is treated as severity-High regardless of likelihood.

## Failure Modes

How the suite must behave when each thing goes wrong:

- **Transport hangs** (the #839 half-open class, now CLOSED; handled as defense-in-depth) →
  bounded deadline trips → `InfraError` → INFRA-ERROR class → distinct ERROR exit code, surface
  the transport-health detail, NEVER a parity RED or a hang (R-02).
- **Intra-transport ranking flip (#4990/GH#746)** → double-capture detects self-divergence →
  INTRA-TRANSPORT-NONDETERMINISM → recorded + filed against GH#746 → does NOT redden the gate (R-07).
- **Real cross-transport divergence** → two intra-stable legs diverge → PARITY-FAIL → gate RED,
  emit first-live-run field-by-field evidence record → file a NEW GH bug, fix NOT absorbed (AC-10).
- **Missing/empty/wrong-surface capture** → never-empty guard → INFRA-ERROR, never a vacuous
  pass (R-03, R-09).
- **Pre-checkpoint DB read** → barrier-not-satisfied → INFRA-ERROR, never PARITY-FAIL or
  empty-equals-empty pass (R-04).
- **PreCompact host-side undrivable** → `measurable=False` + named `host_side_gap` → documented
  measurability call-out in the evidence table, never a silent pass (R-08).
- **Docker absent** → skip path HARD-fails by distinct exit code (false-green-proof, AC-08).
- **Stale prior-round bundle** → token guard → INFRA-ERROR (R-12).
- **Exclusion-set widening attempt** → only via product sign-off + `context_correct`; an
  implementer/tester never silently widens; an unjustified entry fails the off-Docker drift
  guard (R-05, AC-09).

Disposition authority (carried verbatim from nan-021 NFR-8): any non-excluded cross-leg diff is
a PRODUCT/HUMAN call — GH bug (gate RED) OR product-signed exclusion amendment. The
implementer/tester never decides.

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (HNSW top-k flip partly unfixable; exact-order assertion flakes) | R-01, R-07 | ADR-004 stable-prefix + tie-class tolerance; ADR-002 INTRA-TRANSPORT-NONDETERMINISM class routes the GH#746 flip out of the red gate. Tested off-Docker (R-01 scenarios) + classifier-order proof (R-07). |
| SR-02 (#830 self-heal did not cover the #839 silent half-open hang — #839 now CLOSED, commit 5b6badad / PR #842; INFRA handling retained as defense-in-depth) | R-02 | ADR-002 K5 transport-health preflight + bounded connect/idle deadline → INFRA-ERROR with distinct exit code. Covered by R-02 scenarios incl. half-open simulation + slow-healthy boundary. |
| SR-03 (briefing shares retrieval's entropy class; one source reds both) | R-01 | ADR-004 single `ranking_parity` policy single-sourced across D1 + D4; R-01 scenario 4 + R-07 scenario 4 assert one tolerance, no second policy. |
| SR-04 (gate muddles cross-transport vs intra-transport failure classes) | R-02, R-07 | ADR-002 four-valued outcome model; double-capture-and-diff. R-07 proves the classifier order and that a real cross-leg divergence cannot escape into the INTRA bucket. |
| SR-05 (six exclusion sets drift — the #5302 hazard) | R-05, R-13 | ADR-003 `DimensionComparator` base class + ONE `FORBIDDEN_SEED_SITES` + `assert_comparator_contract` drift guard (off-Docker). R-05 + R-15 cover the guard; R-13 covers manifest single-source. |
| SR-06 ("one workload" vs non-degenerate ranking; thin → vacuous pass) | R-06 | ADR-007 augmented single workload w/ deterministic seed-corpus + query phase; NFR-7 stable-prefix floor N. R-06 asserts depth ≥ N > 1 and errors on a too-short set. |
| SR-07 (PreCompact may have a host-side component the harness can't drive) | R-08 | ADR-006 PreCompact stays in scope on `/observe`; capture carries `measurable`/`host_side_gap`; a gap is a documented call-out, never a drop or vacuous pass. R-08 scenarios. |
| SR-08 (two HTTPS surfaces; wrong routing records nothing — vacuous pass) | R-03, R-09 | ADR-005 registry `wire_surface` explicit routing + #5298 11-frame conformance + never-empty guard → missing capture is INFRA-ERROR. R-03 (routing/frames) + R-09 (bundle contract) scenarios. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 4 (R-01, R-02, R-03, R-04) | ~18 scenarios — mostly off-Docker teeth + live #5298/barrier proofs |
| High | 6 (R-05, R-06, R-07, R-08, R-09, R-10) | ~22 scenarios — off-Docker guards, contract tests, pre-tag real-server exercise |
| Medium | 4 (R-11, R-12, R-13, R-15) | ~9 scenarios — barrier-gated compare, token guard, single-source, no-seed audit |
| Low | 2 (R-14, R-16) | ~3 scenarios — adapter golden test, diff-confinement guard |

The bulk of the proving load is OFF-Docker (comparator teeth, classifier order, tolerance
policy, drift guard, bundle contract, no-seed audit) — exercisable before any release tag per
the #5258 seam discipline and #5267's pre-tag-gate lesson. The live Docker matrix proves the
#5298 frame byte-identity, the WAL-flush barrier ordering, the bridge-in-path carriage, and
the cross-language bundle emit — the layers #5267 warns surface only on a real run.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` for parity/false-green/HNSW/transport-hang lessons
  and cross-transport comparator drift patterns -- surfaced #5267 (never-green-on-tag, budget N
  rounds + pre-tag real-server gate), #5177 (multi-wave under-tests earlier parity ACs →
  vacuous pass), #5302 (single-source the full CONTRACT or it drifts), #5285 (derive
  topic_signal over the wire, never seed), #4822 (hook-client parity-corpus build-request trap).
- Stored: nothing novel to store -- the cross-cutting risk pattern here (generalizing a single
  closed-exclusion-set comparator into an N-dimension matrix; two-HTTPS-surface routing; a
  four-valued outcome model separating cross-transport divergence from intra-transport
  nondeterminism and transport-infra hangs) is captured in nan-022 ADRs #5306-#5312 and is
  one-feature-deep; it becomes a storable cross-feature pattern only if a 2nd feature reuses the
  matrix shape (per the pattern-stewardship 2+-feature bar).
