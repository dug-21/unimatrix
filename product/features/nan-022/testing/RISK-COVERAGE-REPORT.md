# Risk Coverage Report: nan-022

Cross-Transport Parity Suite — C0 (#5304) proof artifact (#837). Stage 3c execution.
**FIRST LIVE RUN** of the six-dimension × two-transport parity matrix.

Environment: Docker engine 29.5.2 (linux/arm64) PRESENT; host `sqlite3` 3.40.1 provisioned
(was absent — installed to match a correctly-provisioned release lane); Python 3.11.2 + pytest
9.0.3; node v24.16.0; binary `target/release/unimatrix` v0.8.7.

---

## Executive Verdict

| Layer | Result |
|-------|--------|
| Off-Docker teeth (pytest 218 / node 26 / shell 32) | **GREEN** |
| Integration smoke gate (`-m smoke`, 24 tests) | **GREEN** (mandatory gate met) |
| nan-021 MetricVector parity (`test_https_uds_parity`) | **GREEN** (live cross-leg) |
| Live six-dimension matrix (`test_https_uds_parity_matrix`) | **RED — verdict=ERROR (exit 7)** |

The matrix gate is correctly **non-green**. Across 4 live runs the **stable** outcomes are:
**behavioral PASS, analytics PASS** (genuine cross-transport parity proven); **precompact
INFRA** (D5 documented host-side gap); **isolation PARITY_FAIL** (D6 harness measurability
asymmetry). **retrieval/proactive** are intermittently PARITY_FAIL (1/4 runs) from the
HNSW top-k boundary flip (#4990/GH#746). Two GH bugs filed (#844, #845); no exclusion set was
silently widened; disposition authority for all unproven entries remains PRODUCT/HUMAN.

This feature produces the proof ARTIFACT; it does NOT flip C0 (AC-12). On this evidence C0 is
NOT yet flippable: D1/D4 carry an HNSW determinism dependency and D5/D6 carry measurability
gaps — each a human-signed documented exception or a fixed-in-follow-up item, never silently
excluded.

---

## Per-Dimension Live Evidence Table (4 runs)

| Dim | Run A | R1 | R2 | R3 | Stable outcome | Disposition |
|-----|-------|----|----|----|----------------|-------------|
| retrieval (D1) | PASS | PASS | **FAIL** | PASS | flaky | HNSW top-k flip → GH#844 (filed bug + C0 exception; tolerance NOT widened) |
| behavioral (D2) | PASS | PASS | PASS | PASS | **PASS** | genuine cross-transport parity (`topic_signal='nan-021'` both legs) |
| analytics (D3) | PASS | PASS | PASS | PASS | **PASS** | MetricVector + Informs edges + phase parity (consumed nan-021 comparator) |
| proactive (D4) | PASS | PASS | **FAIL** | PASS | flaky | tracks D1 (briefing ranks same corpus) → GH#844 |
| precompact (D5) | INFRA | INFRA | INFRA | INFRA | **INFRA (documented)** | `measurable=false`, host-side gap NAMED (OQ-2 resolved); never vacuous-passed |
| isolation (D6) | FAIL | FAIL | FAIL | FAIL | **FAIL (harness)** | UDS single-slug fixture cannot probe cross-slug → GH#845 (NOT a real leak) |

### D1/D4 flip evidence (GH#844)
Failing-run retrieval query[0] (`"…cross-transport parity"`, k=5): HTTPS `[3,1,5,4]` vs UDS
`[3,1,2,5,4]` — entry **2** drops out of the HTTPS top-k below the similarity cutoff. Both legs
are INTRA-stable per run (double-capture capture_1 == capture_2 on each leg), so the classifier
correctly declines INTRA-NONDET and calls PARITY_FAIL — yet the same config passes on other runs.
Root cause is per-process HNSW entropy (`hnsw_rs` 0.3.4, no seed API; #4990/GH#746). Per R-01 /
C-4 this is a FILED BUG + documented C0 exception, **never** a quiet tolerance widening.

### D5 measurability (OQ-2 resolved — ADR-006)
`CompactContext` restoration has a Claude-Code host-side component the harness cannot drive
test-only. The `/observe` PreCompact frame IS captured live (HTTPS gate logged "PreCompact frame
landed (204)"); restoration is `measurable=false` with a NAMED `host_side_gap`. Recorded as a
documented measurability limitation in the evidence table, **never rounded up to fully-measured
and never a silent drop**. D5 stays a human-signed documented exception for the flip session.

### D6 measurability asymmetry (GH#845)
HTTPS `slug_a_writes_visible_to_b=false` (real two-slug container, correct isolation); UDS
`=true` — a measurement artifact: the single-slug `daemon_server` fixture has no slug B, and the
UDS probe's `feature="…slug-b"` hint does not route to a separate store (verified: slug-B-hinted
search returns the slug-A marker). Analogous to the D5 gap. NOT a real cross-tenant leak (HTTPS
leg proves isolation holds). Documented exception until the UDS fixture is made two-slug.

### OQ resolutions from this run
- **OQ-2 (D5 host-side gap):** CONFIRMED — measured-where-drivable + named host-side gap. Stated plainly for the flip session.
- **OQ-3 (seed-corpus depth):** RESOLVED — the original near-identical corpus deduped to ONE entry (server `similarity:1.00|duplicate:true`); fixed to 8 distinct subjects so ≥3 survive (non-degenerate, depth-5 rankings now land).
- **OQ-4 (Informs/phase determinism):** RESOLVED — analytics (edges + phase) is a stable exact-post-barrier PASS across all 4 runs; no tolerance needed.

---

## Coverage Summary (risk → tests → result)

| Risk | Description | Test(s) | Result | Coverage |
|------|-------------|---------|--------|----------|
| R-01 | Ranking nondeterminism / loose tolerance | `test_ranking_tolerance` (off-Docker matrix); live D1/D4 double-capture | PASS off-Docker; live FLIP → GH#844, tolerance proven NOT to swallow it | Full |
| R-02 | Half-open hang read as verdict | `test_transport_health`; live UDS preflight | PASS (preflight healthy after fix); INFRA-class proven distinct | Full |
| R-03 | Wrong-surface records nothing → vacuous | `test_parity_legs` registry-vs-driver; live #5298 11-frame both legs | PASS — 11-frame byte-identity emitted both legs; no rework/legacy frame | Full |
| R-04 | WAL-flush capture timing | `test_parity_legs` barrier order; **live D2 WAL bug found + fixed** | PASS after fix (WAL sidecar copied); pre-barrier read = INFRA proven | Full |
| R-05 | Exclusion-set / capture-shape drift | `assert_comparator_contract` off-Docker | PASS | Full |
| R-06 | Thin corpus → degenerate ranking | corpus-depth teeth; **live degenerate corpus found + fixed** | PASS after fix (8 distinct subjects, depth-5 rankings) | Full |
| R-07 | Intra double-capture mis-tuned | classifier-order teeth; live two-stable-legs → PARITY_FAIL | PASS — cross-leg divergence on two intra-stable legs correctly NOT reclassified INTRA | Full |
| R-08 | PreCompact host-side gap as pass | `measurable=False` call-out teeth; live D5 | PASS — documented, never vacuous (see D5) | Full |
| R-09 | Cross-language bundle mismatch | `load_https_bundle` teeth; live JS/shell→Python emit | PASS — six-key bundle round-trips live | Full |
| R-10 | Release-only matrix never-green | off-Docker teeth + **pre-tag local Docker exercise** | PASS — full matrix driven on local Docker pre-tag; live layers surfaced here, not on a tag | Full |
| R-11 | Informs edges / phase timing | live analytics barrier-gated edge+phase | PASS — exact post-barrier, stable across runs (OQ-4) | Full |
| R-12 | Stale-token bundle | `load_https_bundle` token guard teeth; live run-marker | PASS | Full |
| R-13 | Manifest augmentation breaks ONE-identity | single-`ParityWorkload`/token teeth; live same-manifest replay | PASS | Full |
| R-14 | ABC adapter alters MetricVector logic | adapter golden teeth | PASS | Full |
| R-15 | Forbidden-seed audit misses module | `assert_no_seed_reachable` over net-new modules + seed loader | PASS | Full |
| R-16 | Fork smell — net-new transport/cert | `git diff` confined to infra-001 | PASS — all 7 changed files within infra-001; no net-new transport/cert/spawn code | Full |

No risk lacks coverage. R-01, R-04, R-06 were the live-realized risks — the fixture caught each
exactly as designed (R-01/R-06 surfaced; R-04 was a real WAL-flush INFRA bug now fixed).

---

## Test Results

### Off-Docker unit (Tier A — teeth)
- pytest parity teeth: **218 passed**, 4 deselected (the live `-m parity`/`integration` set)
- node `--test` (bridge driver + capture): **26 passed**, 0 failed
- shell stub-drive logic tests: `release-gate-bundle-assembly-logic-test` **11/0**;
  `release-gate-cloud-cycle-logic-test` **21/0**

### Integration smoke (MANDATORY gate)
- `pytest -m smoke`: **24 passed**, 606 deselected (3m27s). Substrate healthy. Gate met.

### Live cross-leg (Tier C — `-m parity`, local Docker)
- `test_https_uds_parity` (nan-021 MetricVector): **PASS** (after `load_https_vector` bundle-aware fix)
- `test_https_uds_parity_matrix` (six-dimension): **FAIL — verdict=ERROR exit 7** (correct: D5 INFRA + D6 FAIL + intermittent D1/D4)
- `test_c3_orchestrator_seam_with_fixture_https_vector`: PASS
- HTTPS leg smoke: ALL GATES PASSED (gates 1–8 + D2/D5/D6 captures), six-key bundle emitted

---

## Stage-3c fixes applied (this feature's code, on the live path — per failure-triage)

All discovered at first live run; all within `product/test/infra-001/` (AC-11 holds). Each is a
defect in nan-022 Stage-3b code surfaced because the off-Docker teeth used synthetic dicts, never
the real server/transport. Fixed in-place (not xfp'd — they are this feature's bugs), re-run green.

1. **Comparator drift-guard false-fail (import order)** — `bind_comparators` REBINDS the global
   `DIMENSIONS` (not in-place), so `test_https_uds_parity.py`'s early `from … import DIMENSIONS`
   captured the stale string-bound tuple → drift guard saw `comparator='RetrievalComparator'`
   (str). Fixed: import `parity_comparator` before `parity_dimensions` (mirrors the matrix file).
2. **UDS preflight false half-open INFRA** — K5 `uds_socket_leg` sent a bare `\n` and waited for
   an unsolicited reply; the request-driven MCP/hook sockets never reply to a non-protocol nudge,
   so a HEALTHY daemon false-classified as a #839 half-open hang. Fixed: liveness = the shipped
   `UnimatrixUdsClient` initialize handshake (real reply; true half-open still trips), hook =
   bounded connect. C-2 honored (reuses shipped clients, no net-new path). This was the R-02
   false-INFRA boundary realized.
3. **D2 behavioral WAL-flush INFRA (R-04)** — `cloud-bundle-lib.sh` `vol cat`-copied only
   `unimatrix.db` (checkpointed, 0 rows); the 7 attributed rows lived in the uncheckpointed
   `-wal` (997 KB). False "empty capture is INFRA". Fixed: copy `-wal`/`-shm` sidecars so sqlite3
   sees the durable post-barrier view. Proven: db-only=0 rows vs db+wal=`topic_signal='nan-021'`.
4. **Degenerate seed corpus (R-06 / OQ-3)** — the 5 near-identical seed entries deduped to ONE
   (`similarity:1.00|duplicate:true`), so retrieval returned 1 hit < floor 3 → INFRA. Fixed:
   8 semantically-distinct subjects; ≥3 survive dedup; depth-5 rankings now land.
5. **Briefing parser assumed JSON (D4)** — `context_briefing` does NOT honour `format=json`; it
   always emits the `# id topic cat conf` text table. Both the Python `_parse_briefing_result`
   and the JS `parseBriefingResult` `json.loads`'d it → empty briefing → INFRA. Fixed: text-table
   fallback in BOTH parsers (byte-identical, R-09 contract), unit-tested both sides.
6. **nan-021 test out-file shape** — nan-022's C5' widened the HTTPS out-file from
   `{run_token, metric_vector}` to `{run_token, dimension_bundle}`; `load_https_vector` couldn't
   read it. Fixed: bundle-aware fallback (`dimension_bundle.analytics.metric_vector`), keeping the
   nan-021 test UNCHANGED in spirit (AC-11/AC-04).

No integration test was deleted, commented out, or `xfail`'d to force green. No exclusion set was
widened. The two non-fixed findings (HNSW flip, D6 asymmetry) are filed as GH bugs, not absorbed.

## GH Issues filed (no production fix in this PR — AC-10)
- **#844** — D1/D4 cross-transport parity RED from HNSW top-k boundary flip (root cause GH#746/#4990).
- **#845** — D6 isolation parity false-RED: UDS single-slug fixture cannot symmetrically probe cross-slug visibility.

## Gate-3b advisory WARNs (non-blocking — disposition)
- Cosmetic rollup docstring drift: noted, non-blocking, no behavior impact.
- Two orchestration/CLI-shim helpers absent from the no-seed audit list: neither emits compared
  output, so neither can seed a compared value — confirmed safe at the live run; no action.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | One `ParityWorkload`, `run_token==session_id`, one barrier; both legs replay the same manifest live; `assert_no_seed_reachable` over all net-new modules + seed loader (teeth + live) |
| AC-02 | PARTIAL | D1 stable-prefix policy proven off-Docker; live PASS 3/4 runs; the 1/4 flip is the HNSW boundary (GH#844, R-01 disposition — not a tolerance failure) |
| AC-03 | PASS | Live `topic_signal='nan-021'` string-exact both legs (D2); derived not seeded; PASS all runs |
| AC-04 | PASS | MetricVector via consumed comparator + Informs-edge set + exact phase, barrier-gated; stable PASS all runs (D3) |
| AC-05 | PARTIAL | D4 `BriefingComparator` imports the SAME `ranking_parity`; live PASS 3/4; the 1/4 flip tracks D1/GH#844 |
| AC-06 | PASS (documented gap) | D5 `/observe` PreCompact captured live; `measurable=false` + named `host_side_gap`; documented measurability call-out, never vacuous (OQ-2) |
| AC-07 | OPEN (harness gap) | D6 isolation boolean compared EXACTLY; live HTTPS proves isolation; UDS leg cannot symmetrically measure on a single-slug fixture → GH#845 documented exception |
| AC-08 | PASS | `rollup` exit-code truth table off-Docker; live evidence table keyed by run token; skip-when-Docker-absent HARD-fails by distinct code; anchored run-marker present (`[783-smoke] ALL GATES PASSED`) |
| AC-09 | PASS | `assert_comparator_contract` + unjustified-entry NEG off-Docker; every EXCLUDED key justified; one `FORBIDDEN_SEED_SITES` |
| AC-10 | PASS | Real divergences (D1/D4 flip, D6 asymmetry) filed as GH#844/#845, gate stays RED, fix NOT absorbed; no fix code in the production diff |
| AC-11 | PASS | `git diff` confined to `product/test/infra-001/` (7 files); no `crates/**` / shipped-`lib/` / production-script change; bridge-in-path reuse — no net-new transport/cert/spawn code |
| AC-12 | PASS | The per-dimension table keyed by run token IS the proof artifact; no C0-flip action in the diff; `blocks_c0_proof=True` for all six; C0 NOT flipped by this feature |

---

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + the surfaced ADRs (#5305 dimension matrix, #5313
  outcome model, #5321 bundle emit, #5322 rollup precedence, #5258 off-Docker seam, #5267
  never-green-on-tag) — applied the off-Docker-teeth-then-live-Docker discipline and the
  four-valued outcome model directly.
- Stored: a reusable Stage-3c pattern was discovered (off-Docker parity teeth pass on synthetic
  dicts while the FIRST live run surfaces a cluster of substrate-shape defects — WAL-not-copied,
  server-dedup-collapses-near-identical-seed, briefing-returns-text-not-json, request-driven-
  socket-has-no-liveness-reply, bind-rebinds-not-mutates). See the stewardship note below; this
  becomes a storable cross-feature pattern on a 2nd parity-matrix reuse.
