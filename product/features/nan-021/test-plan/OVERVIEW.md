# Test Plan Overview — nan-021

> **HTTPS-Bridge Integration Fixture.** PURE test-infrastructure feature, zero production diff. The
> deliverable IS the test fixture — a CUMULATIVE extension to `infra-001` (`product/test/infra-001/`).
> This overview maps risks R-01..R-14 to test scenarios, pins the integration-harness plan (which suites
> apply + the new tests this feature ADDS), and specifies the **⚠ first-live-run field-by-field
> validation procedure** that the delivery gate requires.

---

## 1. Test Strategy

This feature has an unusual testing shape: it is itself an integration fixture. "Unit tests" here are the
**stub-driven gate-spine tests** (shell `SMOKE_*_CMD` seams) and the **comparator pure-function tests**
(Python). The fixture's real assertion (live HTTPS-vs-UDS parity) runs in the **release-gate Docker lane**
via `workflow_dispatch`/tag (D-3) — NOT per-PR. So the testing pyramid is:

| Tier | What it covers | Where it runs | Gate |
|------|----------------|---------------|------|
| **Pure-function unit** (Python) | The `MetricVector` comparator + durability-barrier predicate logic — exclusion-set classification, mutation teeth, correlation-token rejection | `pytest` local (no Docker) | Stage 3c hard gate |
| **Gate-spine stub-drive** (shell) | Exit-code discriminator, anchored run-marker grep, acquisition `pull \|\| inspect \|\| exit-N`, orchestration control flow — via `SMOKE_*_CMD` seams | `bash` local (no Docker) | Stage 3c hard gate (R-12 first-green tax mitigation) |
| **infra-001 smoke** | Regression baseline — nan-021 must not break the existing harness | `pytest -m smoke` | MANDATORY minimum gate |
| **Live dual-transport parity** | The fixture's headline assertion (AC-04) | release-gate Docker lane (`workflow_dispatch`/tag) | first-green budgeted across N tag rounds (R-12) |
| **⚠ First-live-run field-by-field validation** | One-time human-confirmed examination of all 18 non-excluded `UniversalMetrics` fields + `phases` (NFR-8 disposition authority) | first live tag run, manual + recorded | DELIVERY GATE — see §4 |

**Why the comparator + barrier are unit-tested OFF Docker:** R-12 (release-only gate never exercised
pre-tag) is High-likelihood. nan-019's lesson (#5258 stub-drive, #5192 gate spine) is the precedent: the
gate-spine bytes and the comparator arithmetic MUST be exercised before the first live tag round, or
failures surface in slow sequence across N rounds. The comparator is the ONLY substantial net-new module
(C4); it is pure-Python over parsed dicts and is therefore the most testable surface — test it hardest.

---

## 2. Risk-to-Test Mapping (R-01..R-14)

| Risk | Pri | Test home (component) | Test type | Coverage requirement |
|------|-----|-----------------------|-----------|----------------------|
| **R-01** Incomplete D-5 set → parity flakes | Critical | C4 comparator + first-run gate (§4) | unit (classification) + live (≥20× burst) + manual (first-run) | Every 1 of 21 `UniversalMetrics` fields + `PhaseMetrics.{duration_secs,tool_call_count}` is EXPLICITLY classified `deterministic` or `excluded` in code; the excluded set is named as a literal of exactly 3 fields; zero out-of-set divergence across a ≥20× repeat burst. Negative: inject a 1 s artificial delay into one leg → comparator still PASSES (proves it excludes wall-clock, not luck). |
| **R-02** Over-broad set → vacuous green | Critical | C4 comparator mutation harness | unit (mutation) | Force one **structural** (non-excluded) field to differ (drop one HTTPS observe) → comparator MUST FAIL. Assert exclusion set is MINIMAL — `total_tool_calls`, `session_count`, `knowledge_entries_stored`, hotspot counts, `phases` key set are NEVER excludable. Each excluded field carries an inline wall-clock justification. |
| **R-03** Fragile single-execution seam | Critical | C4 orchestrator + C3 | unit (token reject) + integration (orchestration) | One `pytest` invocation owns both legs (pytest-as-orchestrator). HTTPS vector file written fresh under `$SANDBOX` (not a fixed path), absent at test start. HTTPS vector carries a run-correlation token = this run's stable session identity; comparator REJECTS a vector whose token ≠ this run. Smoke shell-out non-zero OR missing HTTPS-vector file → pytest **ERRORS** (not skips, not empty-compares). |
| **R-04** Bridge silently bypassed | Critical | C2 bridge-cycle | integration + negative control | `mcp-bridge.js` process actually spawned; cycle MCP traffic over ITS stdio JSON-RPC; assert ZERO direct cycle-`mcp_url` POSTs. FR-9: `Mcp-Session-Id` captured on `initialize` and replayed byte-stable + `text/event-stream` parsed (observable in bridge stderr/log) — not just a 200. Negative control: JSON-only `Accept` (no `text/event-stream`) MUST FAIL framing (#5129). |
| **R-05** keep_alive idle eviction → 404 | Critical | C2 bridge-cycle | integration (timing) | First tool call follows `initialize`/session-id capture with NO interposed fixed wait (gate 8 event-driven, drive immediately). A mid-cycle eviction is survived by the SHIPPED single-flight self-heal (#830/#5280) — fixture does NOT re-implement reconnection. Heal-exhausting 404 → HARD fail with captured bridge stderr, never a silent dropped observe. |
| **R-06** Fire-and-forget WAL → observes not durable | Critical | C4 durability barrier (shared helper) | unit (predicate) + integration (symmetry) | A bounded deadline-poll (cap ~10 s, sleep 1) gates BOTH `context_cycle_review` calls; predicate = expected observe count (from manifest) present, sampled at per-slug store **DIR** granularity incl. `-wal`, never `unimatrix.db` alone. SAME helper, SAME predicate/deadline on both legs. Timeout → HARD fail "observes not durable" + observed-vs-expected, never an empty compare. Non-empty asserted AFTER the barrier. |
| **R-07** topic_signal → unattributed | High | C2/C3 workload + C4 manifest | integration + assertion | `feature_cycle` pinned to a VALID registry feature so `enrich_topic_signal_with_source` resolves the `declared` branch (not vote/registry-fill/unattributed); slug/feature registered before driving. Assert `topic_signal == feature` EXACTLY for every driven observation — `unattributed` is a HARD fail (near-miss guard). Bash content carries the parseable feature-ID token (FR-3 load-bearing). |
| **R-08** Docker false-green / false-fail | High | C5 gate wiring | shell stub-drive | Acquisition reuses nan-019 `docker pull \|\| docker image inspect \|\| exit-4` verbatim; image-unacquirable (4) is a DISTINCT exit from Docker-absent (3). Docker-absent → exit 3, `run_smoke_gate` treats 3 as HARD failure (skip-is-failure). Anchored whole-line marker via `grep -qxE '\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*'`; exit-0-WITHOUT-marker FAILS. |
| **R-09** Divergent CC session identity (#832) | High | C4 manifest (single driver) | integration + structural | ONE stable session identity threaded through declaration + all observes on EACH leg, SAME value on BOTH legs. Workload is ONE driver fed to both (divergent identity structurally impossible). This fixture IS the #832 guard: cycle-join attribution (`topic_signal == feature`) holds only if the id is stable. |
| **R-10** Accidental fork of infra-001 | Medium | C1/C2/C3/C5 (all extensions) | manual diff review | Every net-new helper names the existing asset it extends (smoke gate-lib / `UnimatrixUdsClient` / `UnimatrixHookClient` / JS bridge machinery). C4 (`parity_workload.py` + comparator) is the ONLY substantial net-new module. No net-new server-spawn / cert-pin / credstore / bundle path. |
| **R-11** `projectHash` recomputed | Medium | C1/C2 standup | structural grep | `projectHash` passed to `node mcp-bridge.js <projectHash>` is READ BACK from `init --bundle` (stdout/log, or by listing the single dir under `$SANDBOX/home/.unimatrix/`) — NOT recomputed. ZERO hashing primitive imported/invoked in the bridge-spawn path. |
| **R-12** First-green tax (release-only gate) | High | C5 gate wiring | shell stub-drive | Pre-merge `SMOKE_*_CMD` stub-drive unit-tests the gate-spine arithmetic (exit-code discrimination, run-marker grep, orchestration flow) BEFORE the live tag run (mirrors nan-019 #5258). First-green budgeted as MULTIPLE tag rounds sequencing failures (cert read → bridge spawn → cycle → review → parity), not assumed one-shot. |
| **R-13** Child stderr swallowed | Medium | C1/C2 standup | structural grep | Every child (`mcp-bridge.js`, `init`, container) writes stderr to a `$SANDBOX` file, tail-dumped on failure only (ADR-005) — never `2>/dev/null` on a token-free child. The `emit_bundle` child STAYS suppressed (bearer in blob) — the one deliberate exception, asserted as intentional. |
| **R-14** Hermeticity leak | Low | C1 standup | structural | `init --bundle` writes the credstore ONLY under `$HOME=$SANDBOX/home` (nan-020 precedent #5258); sandbox home fresh per run. No real `~/.unimatrix` outside the sandbox read or written. |

**Priority coverage budget (from RISK-TEST-STRATEGY §Coverage Summary):** Critical 6 (R-01..R-06) → 18
scenarios; High 3 (R-07/R-08/R-09) → 10; Medium 4 (R-10..R-13) → 8; Low 1 (R-14) → 2. The per-component
plans below allocate every one.

---

## 3. Integration Harness Plan (infra-001)

### 3a. Existing suites to run (Stage 3c)

This feature is itself an integration fixture, but it must NOT regress the existing harness. Per the suite
selection table:

| Run | Suite | Why | Gate level |
|-----|-------|-----|------------|
| **MANDATORY** | `pytest -m smoke` | nan-021 extends `conftest.py` + clients; smoke is the minimum regression gate | hard |
| Recommended | `lifecycle` | nan-021 exercises the cycle→review flow + restart/persistence-adjacent store behavior; `_seed_observation_sql_lifecycle` lives here (AC-03 no-seed audit target) | run, triage failures |
| Recommended | `tools`, `protocol` | the cycle/review tool surface + MCP handshake the bridge speaks | run, triage failures |
| As-applicable | the nan-021 release-gate Docker lane (the new fixture itself) | the headline live parity assertion | `workflow_dispatch`/tag, NOT per-PR |

Failures in existing suites are triaged per USAGE-PROTOCOL.md: feature-caused → fix; pre-existing →
GH Issue + `xfail`; bad assertion → fix. **Never fix unrelated failures in this PR.**

### 3b. New tests this feature ADDS

nan-021 adds substantial net-new integration coverage. Per the planning guidance (new lifecycle flow over a
new transport → addition to lifecycle/new module):

| New test home | Tests to add | Maps to |
|---------------|--------------|---------|
| **`harness/parity_workload.py`** (NEW, C4 — the sole substantial net-new module) | the declarative workload manifest, the single dual-transport driver, the symmetric durability-barrier helper, the `MetricVector` comparator | FR-5/FR-7/FR-10, AC-04 |
| **`harness/` pytest test** (extends `conftest.py`, C3+C4) | `test_parity_https_vs_uds_metricvector` — the pytest-as-orchestrator that drives UDS, shells out to the smoke's C2 gate, ingests `MetricVector(HTTPS)`, runs the comparator. Plus pure-function tests: `test_comparator_mutation_drops_observe_fails`, `test_comparator_excludes_only_wallclock`, `test_comparator_rejects_stale_correlation_token`, `test_barrier_symmetric_predicate`, `test_barrier_timeout_hard_fails` | AC-03/AC-04 |
| **`scripts/docker-http-posture-smoke.sh`** (extend, C2 — NEW `cloud_cycle_gates` gate fn) | the bridge-spawn + stdio JSON-RPC cycle drive + pinned `/observe` + durability barrier + HTTPS-side `context_cycle_review`, emitting `MetricVector(HTTPS)` to a `$SANDBOX` file w/ correlation token | AC-01/AC-02 |
| **shell stub-drive test** (extends nan-019 stub-drive pattern, C5) | gate-spine arithmetic via `SMOKE_*_CMD` seams: exit-code discriminator, anchored run-marker grep, `pull \|\| inspect \|\| exit-4` acquisition | AC-05, R-12 |

**Do NOT add to the unrelated suites** (`volume`, `security`, `confidence`, `contradiction`,
`edge_cases`) — nan-021's behavior is already covered by the new C4 module + the C2 gate; those suites are
regression-only here.

### 3c. New tests NOT to write (boundary)

- Retrieval (`context_search`/`get`) over the bridge — that is vnc-039 AC-03's job, explicitly out of scope.
- Re-authored bridge/cert-pin/credstore/bundle behavior — that is a FORK smell (AC-07); exercise the
  shipped JS as-is.
- Soak/load — SL1 (#703), #818/#819 are enabled substrates, not deliverables.

---

## 4. ⚠ First-Live-Run Field-by-Field Validation Procedure (DELIVERY GATE — NFR-8 / ADR-003 #5293)

**This is the single place this fixture touches the DEFINITION of C0 parity. It is NOT the implementer's
call, and it is NOT skippable.** The 3-field D-5 exclusion set (`MetricVector.computed_at`,
`UniversalMetrics.total_duration_secs`, `PhaseMetrics.duration_secs`) is a **load-bearing UNPROVEN
ASSUMPTION** until the first live dual-transport run is examined field-by-field.

### Procedure (performed ONCE, on the first live tag run, recorded)

1. **Capture both raw vectors.** On the first live release-gate run, the comparator emits BOTH parsed
   `MetricVector` dicts (HTTPS and UDS) to a `$SANDBOX` artifact (not just pass/fail) — keyed by the run
   correlation token (R-03). This is the field-by-field evidence record.
2. **Examine ALL 18 non-excluded `UniversalMetrics` fields** (the 20 minus the 2 already-excluded
   duration/wall-clock — i.e. every field except `total_duration_secs`; `computed_at` is on `MetricVector`
   not `UniversalMetrics`). For each field record: HTTPS value, UDS value, equal? The 18:
   `total_tool_calls`, `session_count`, `search_miss_rate`, `edit_bloat_total_kb`, `edit_bloat_ratio`,
   `permission_friction_events`, `bash_for_search_count`, `cold_restart_events`, `coordinator_respawn_count`,
   `parallel_call_rate`, `context_load_before_first_write_kb`, `total_context_loaded_kb`,
   `post_completion_work_pct`, `follow_up_issues_created`, `knowledge_entries_stored`,
   `sleep_workaround_count`, `agent_hotspot_count`, `friction_hotspot_count`, `session_hotspot_count`,
   `scope_hotspot_count`.
3. **Examine the `phases` BTreeMap** — key set equal? per-phase `tool_call_count` equal? (`duration_secs`
   per phase is excluded.)
4. **Examine `domain_metrics`** — key set + values equal.
5. **Examine the at-risk session-lifecycle fields FIRST** (the prime transport-inherent suspects):
   `cold_restart_events`, `coordinator_respawn_count`, `context_load_before_first_write_kb`,
   `total_context_loaded_kb`, `permission_friction_events`. A first-run divergence on ANY of these is the
   expected divergence locus and MUST surface loudly with the field name + both values.
6. **Confirm the run matched ONCE** before the gate is relied on as a parity proof.

### Disposition authority (when a non-wall-clock field diverges — on the first run OR any later run)

The divergence is surfaced by the comparator **loudly with the field name + both values** and escalated to
a **HUMAN / PRODUCT call**, dispositioned as exactly ONE of:

- **(a) Real parity defect** → **file a GitHub bug.** The fixture did its job (a good catch — this is WHY
  C0 is measured, not asserted). The gate stays **RED** until addressed.
- **(b) Transport-inherent field** → add to the exclusion set **ONLY with explicit product sign-off + a
  recorded rationale appended to ADR-003 (#5293) via `context_correct`** — naming the field, the
  transport-inherent reason, and the approver.

**NEVER silently widen the exclusion set to turn a red green.** That IS the R-01/R-02 failure mode
(reactive widening hides real divergence). The implementer/tester is NOT the decider — product/human is.

### How the tester surfaces this (not silently absorbed)

- The comparator's failure message names the divergent field + both values + which leg (R-02 teeth).
- The RISK-COVERAGE-REPORT.md (Stage 3c) records the first-run field-by-field table (18 fields + phases +
  domain_metrics, HTTPS vs UDS, equal?) as evidence, and explicitly states the disposition of any
  divergence (defect → GH#; or amendment → ADR-003 correction id + approver).
- A divergence on a session-lifecycle field is reported as a **product decision point**, never an
  exclusion-set edit made by the tester.

---

## 5. Cross-Component Test Dependencies

- **C4 is the spine.** The workload manifest (C4) is the single source of truth both C2 (HTTPS, shell) and
  C3 (UDS, Python) replay. The comparator + barrier (C4) consume both legs' vectors. Test C4 first and
  hardest (pure-function, off-Docker).
- **C2 depends on C1** (standup) for a live HTTPS endpoint + bridge credstore; C2 emits `MetricVector(HTTPS)`
  to the `$SANDBOX` file C4 ingests.
- **C3 depends on conftest fixtures** (UDS daemon) and the C4 manifest/driver.
- **C5 wraps everything** — the gate-spine stub-drive (C5) is independent of the live legs and is the R-12
  pre-merge safety net; it must pass before any tag round.
- **The orchestration seam (R-03)** binds C2↔C3↔C4: one pytest invocation owns both, joined by a
  `$SANDBOX` file + correlation token.

---

## 6. Acceptance Criteria Coverage (AC-01..AC-07)

| AC | Covered by | Component plan |
|----|------------|----------------|
| AC-01 cloud path stood up cumulatively | C1 standup test + file-check (NOT `serve --stdio`) | c1-https-standup.md |
| AC-02 full cycle through bridge over pinned HTTPS (bridge carried it) | C2 bridge-cycle + FR-9 SSE/session-id replay + JSON-only negative control | c2-bridge-cycle.md |
| AC-03 derived `topic_signal == feature`, no seed | C2/C3 workload + C4 manifest + grep audit | c2-bridge-cycle.md, c3-uds-baseline.md |
| AC-04 live-vs-live parity + **first-run gate** | C4 comparator + barrier + §4 procedure | c4-workload-comparator.md |
| AC-05 false-green-proof release-gate Docker lane | C5 gate wiring stub-drive | c5-gate-wiring.md |
| AC-06 zero production diff | C5 (`git diff` scope check) | c5-gate-wiring.md |
| AC-07 extends infra-001, no fork | all components (diff review) + R-11 `projectHash` read-back | c5-gate-wiring.md (+ each) |

---

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search + context_get(#5293, #5291) — strong hits:
  #5293 (ADR-003 comparison contract + first-live-run gate + disposition authority), #5291 (ADR-006
  symmetric durability barrier), #5290 (ADR-005 Docker false-green discriminator), #5286 (ADR-001 hybrid
  single-driver), #5258/#5192 (nan-019 stub-drive gate spine reused pre-tag), #5265 (fire-and-forget WAL
  durability barrier), #5280/#830 (idle-eviction self-heal), #5129 (rmcp SSE), #5208 (inspect-no-pull
  cache-miss false-fail).
- Stored: nothing novel at Stage 3a — test scenarios are feature-specific; the generalizable patterns
  (release-only first-green tax, symmetric durability barrier, complete-not-over-broad exclusion set,
  false-green skip-guard) already exist as #5265/#5266/#5267/#5208/#5280/#5192. The candidate new pattern
  ("live-vs-live parity gates need a symmetric durability barrier + closed exclusion set + a first-run
  field-by-field human gate") is still single-feature (nan-021 only); per the 2-feature threshold it is not
  yet stored. Stage 3c may revisit if the first-live-run procedure generalizes.
