# Gate 3c Report: infra-004

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-06-27
> Result: PASS

## Scope note

Test/CI-only feature (shell + YAML), no `crates/` change. The mandatory integration
gate is `pytest -m smoke` (no cargo test exists for this feature — not a gap, by
design). Validated by re-executing the shipped bytes, not by reading the report.

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof | PASS | All 15 risks mapped; every non-negotiable test name grep-verified present in actual suites; both Critical (R-01, R-05) proven by executing shipped bytes |
| 2. Test coverage completeness | PASS | 86/86 shell + 24/24 smoke re-run green; edge cases (early-exit-0, substring marker, exit-3, unexpected exit) covered; needs-graph cross-component coverage present |
| 3. Specification compliance | PASS | 12 ACs PASS pre-merge; AC-04/AC-11 PENDING-operational (legit carve-out); AC-14 deferred human gate; all FRs/NFRs verified where measurable |
| 4. Architecture compliance | PASS | C-WB/C-TS/C-LN/C-FLIP implemented per ARCHITECTURE; capture-shape §7 honored; §5 blast-radius covered; needs-flip in place; zero crates/ drift |
| 5. Knowledge stewardship | PASS | RISK-COVERAGE-REPORT has `## Knowledge Stewardship` with `Queried:` + `Stored:` (reason given) |

## Integration Test Validation (mandatory)

- `pytest -m smoke`: **re-run in foreground → 24 passed, 601 deselected, rc=0 in 208s.** Matches reported 24/24.
- Shell logic/static suites: **re-run all four in foreground → 39 + 19 + 15 + 13 = 86/86, 0 failed, each prints its summary line (R-14 completeness witness).** Matches reported 86/86.
- xfail markers: feature added **none**. The xfail markers present under `suites/*.py`
  are all pre-existing infra-001 markers (GH#405, GH#111, GH#406, ONNX-env limits) —
  the feature diff touches no `suites/*.py` file, so none were introduced. Report's
  "none filed, no xfails" claim is accurate for this feature.
- No integration tests deleted/commented: confirmed — `git diff --name-only main...HEAD`
  contains zero `suites/*.py` paths; 24 smoke tests collected and passed.
- RISK-COVERAGE-REPORT includes integration test counts: yes (24 smoke + 86 shell tabulated).

## Detailed Findings

### Check 1 — Risk mitigation proof
**Status**: PASS
**Evidence**: Per the Gate-3c lesson (#2758), every non-negotiable test function name
listed in RISK-COVERAGE-REPORT was grep-verified against the actual suite source —
all present (no report-only ghost names):
- C-TS / R-05: `test_tristate_rc_survives_capture`, `test_tristate_no_pipe_static_return_not_exit`,
  `test_tristate_captures_stderr[_fail]`, `test_tristate_only_exit2_nonblocking`,
  `test_tristate_marker_anchored_substring`, `test_tristate_marker_whole_line_anywhere_is_green`,
  `test_tristate_marker_byte_identical`, `test_tristate_infra_exit2_nonblocking_visible`,
  `test_tristate_infra_exit2_canonical_marker_pinned`, `test_run_smoke_gate_sibling_unchanged_exit4`
  → all in `release-gate-tristate-logic-test.sh`.
- C-WB / R-01: `test_warmup_present_requires_durable_read_roundtrip`, `test_warmup_result_is_consumed`,
  `test_warmup_uses_write_then_barrier_not_store_size`, `test_warmup_present_proceeds_to_matrix`,
  `test_warmup_timeout_is_infra_not_pass`, `test_warmup_marker_non_substring_asserted`,
  `test_warmup_row_inert_to_negatives`, `test_assert_routes_live_precedes_barrier`,
  `test_warmup_bound_default_documented`, `test_post_barrier_{green,red,infra}_still_drives`
  → all in `release-gate-isolation-logic-test.sh`.
- C-LN/C-FLIP: lane static suite functions all present in `release-gate-isolation-lane-static-test.sh`.

**Critical R-01 (ceremonial-warmup)** — refuted as ceremonial in the *shipped* bytes:
`multi-tenant-isolation-smoke.sh:430 warmup_barrier()` reuses `write_then_barrier` (a
real durable own-store write→read-as-barrier round-trip through `SMOKE_*_CMD`, NOT a
liveness-only `store_size` poll), is invoked at line 486 between `assert_routes_live`
and `run_isolation_matrix`, and timeout → `infra_fail` (exit 2, never RED/GREEN).
Load-bearing + consumed-to-gate proven pre-merge; the cold-path zero-flap leg is the
AC-11 carve-out.

**Critical R-05 (swallowed-exit-code)** — `release-gate-lib.sh:83 run_smoke_gate_tristate`
uses the exact capture shape `set +e; out="$(IMAGE="${image}" "$@" 2>&1)"; rc=$?; set -e`
(no pipe between smoke and `$?`), `return`s on every path (never `exit`), and maps
exit 2 → return 0 (visible) while 1/3/other → return 1. Full truth table proven by the
real sourced lib (19/19 executed green).

### Check 2 — Test coverage completeness
**Status**: PASS
**Evidence**: All 15 Phase-2 risks have a row in the coverage matrix. Pre-merge risks
(R-01..R-09 logic legs, R-02, R-06, R-07, R-08, R-14) are PASS via executed suites.
Edge cases from the strategy (early-exit-0 not credited, substring marker rejected by
`-qxE` full-line anchor, exit-3 SKIP hard-fails, unexpected exit blocks) are each a
truth-table cell. Cross-component coverage: `test_lane_in_manifest_needs` (needs-graph)
+ smoke no-regression (24/24). No Phase-2 risk lacks coverage.

### Check 3 — Specification compliance
**Status**: PASS
**Evidence**: AC-01/02/03/05/06/07/08/09/10/12/13/15 PASS pre-merge (verified by code
review + executed suites). AC-15 confirmed: `git diff --name-only main...HEAD` has zero
`crates/` paths; production diff = `multi-tenant-isolation-smoke.sh`, `release-gate-lib.sh`,
`.github/workflows/release.yml` only. AC-04 and AC-11 are PENDING-operational — a genuine
CI-only carve-out (require a real `workflow_dispatch` cold build + GHCR pull + first-boot
HF download; not unit-testable pre-merge). AC-14 / the R-15 chronic-INFRA VARIANCE is the
human gate explicitly deferred post-delivery — not failed here, per spawn instruction.

### Check 4 — Architecture compliance
**Status**: PASS
**Evidence**: All four components match ARCHITECTURE §2. C-FLIP confirmed at
`release.yml:666` — `create-container-manifest.needs:` includes `multi-tenant-isolation-amd64`.
The lane (`release.yml:632-663`) calls `resolve_image` + `run_smoke_gate_tristate`, no
docker build, `IMAGE` exported. The forbidden `${GITHUB_REF_NAME#v}` (R-09/C-4) is absent
from the lane — the one workflow occurrence (`release.yml:248`) is inside the pre-existing
`package-npm:` job, out of scope; `test_lane_no_ref_strip` correctly scopes the assertion
to the lane. §7 capture-shape invariants and §5 blast-radius mapping are honored.

### Check 5 — Knowledge stewardship compliance
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md carries a `## Knowledge Stewardship` block with a
`Queried:` entry (`context_briefing` → #5192/#5350/#5349/#5354/#5335/#840) and a `Stored:`
entry ("nothing novel — patterns already captured as #5192/#5345/#5267/#4974/#5354"),
i.e. a reason is supplied after "nothing novel". No WARN.

## Carve-Out Confirmation (NOT masking a pre-merge gap)

| Item | Verdict | Why genuinely CI-only |
|------|---------|-----------------------|
| AC-04 / AC-11 cold-model dispatch GREEN | LEGITIMATE | Requires a real `workflow_dispatch` cold build + GHCR pull + first-boot HF download on a runner (C-10/SR-05). Pre-merge load-bearing construction of the barrier IS proven; only the empirical cold-path leg is deferred. |
| R-10 `:v<ver>` tag-push resolution | LEGITIMATE | First executes on a real tag post-merge; one tag round budgeted; tag-path INFRA degrades non-blocking (safe). |
| R-11 / R-12 | LEGITIMATE | Branch-point==main + GHCR-write-from-branch are run-time operational facts. |
| AC-14 / R-15 VARIANCE (OQ-3) | DEFERRED (human) | Human-accepted residual; explicitly out of Gate 3c scope. Not failed. |

## Rework Required

None.

---

# Gate 3c — iter2 (#859 fold-in)

> Date: 2026-06-27
> Result: **REWORKABLE FAIL** (narrow — coverage report not regenerated; all execution evidence green)
> Scope: re-validation after folding the #859 marker-PII fix (commit `511ba824`) into infra-004.

## What #859 was and what the fold-in does

The first AC-11 cold-model dispatch INFRA'd (never GREEN) because the gate's
numeric `RUN=$$-$(date +%s)` nonce could form a phone-number-shaped substring
(`530-1782573` inside `...18530-1782573915`); the production `ContentScanner` on
the MCP `context_store` write leg rejected it (-32006), while the observe leg
(no scanning) accepted it. Latent-flaky by epoch digits. The fold-in makes the
PII shapes **structurally unreachable**:
- `_default_nonce = <b36(pid)>x<b36(epoch)>` — pid/epoch base36-encoded separately,
  joined by the letter `x`; each component ≤6 chars so a 10-digit phone run cannot
  form within a component and the letter separator blocks any cross-boundary run.
- Single `_default_nonce` seam routes BOTH `derive_markers` and `warmup_barrier`
  (cannot diverge); injectable `PID_OVERRIDE`/`EPOCH_OVERRIDE` for off-Docker drive.
- `assert_marker_pii_safe` charset-reduced ERE canary (phone+SSN — the only two of
  six scanner patterns reachable under `[a-z0-9-]`), invoked AFTER the R-12 charset
  guard from `assert_markers_distinct` (4 cell markers) + `warmup_barrier`.
- Adversarial off-Docker (c) battery (RUN unset, epoch/pid battery incl. the captured
  failing pair) + a test-only Rust `ContentScanner` anchor over a SHARED golden set.

## Summary (iter2)

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof (incl. marker-PII #859 class) | WARN | Marker-PII class PROVEN green by execution (Rust anchor + (c) battery + canary), but NOT yet documented in RISK-COVERAGE-REPORT — see Rework |
| 2. Test coverage completeness | WARN | 90 shell + 1 Rust anchor + 24 smoke all re-run green; report still tabulates 86 and omits the anchor + (c) cases |
| 3. Specification compliance (AC-15 amend) | PASS | Sole crates/ delta is the `#[cfg(test)]` anchor in `scanning.rs` (inside `mod tests`); no production/scanner change. AC-04/AC-11 remain CI-only; AC-14 deferred human gate |
| 4. Architecture compliance (invariants preserved) | PASS | R-12 charset, R-18/R-02 non-substring, `infra003-{obs,mcp,warmup}-{a,b}-` prefixes, read-as-barrier predicates all preserved; only RUN derivation changed |
| 5. Knowledge stewardship | PASS | agent-5-marker-fix + investigator reports each carry `## Knowledge Stewardship` with `Queried:` + `Stored:`/reason |

## Execution evidence (re-run, foreground, shipped bytes)

- `release-gate-isolation-logic-test.sh`: **43 passed, 0 failed** (39 prior + 4 new (c) cases)
- `release-gate-tristate-logic-test.sh`: **19 passed, 0 failed** (R-05 unregressed)
- `release-gate-logic-test.sh`: **15 passed, 0 failed** (R-07 sibling unregressed)
- `release-gate-isolation-lane-static-test.sh`: **13 passed, 0 failed**
  → **90/90** total (43+19+15+13).
- Rust anchor `cargo test -p unimatrix-server --lib test_scan_isolation_gate_golden_markers_pass`:
  **1 passed, rc=0** — the real `ContentScanner::global().scan()` accepts all 12 golden
  derived markers (incl. the captured-failing pid/epoch re-encoded). Deterministic
  proof the derived markers never trip the scanner.
- `pytest -m smoke`: **24 passed, 601 deselected, rc=0 in 207.69s** — no server change →
  prior 24/24 no-regression confirmed.
- File sizes: smoke.sh 499, isolation-probe-lib.sh 172, isolation-logic-test 491,
  fixture 136 — all ≤500. No `suites/*.py` touched; no new `xfail`.

## Detailed findings (iter2)

### Check 1 — Risk mitigation proof (incl. marker-PII class)
**Status**: WARN
**Evidence**: The #859 root-cause class is now PROVEN green by direct execution:
the Rust anchor exercises the real production scanner against the shared golden set
(including `infra003-mcp-a-eaqxthaqu3`, the re-encoding of the exact pid/epoch that
INFRA'd), and the off-Docker (c) battery drives the REAL default path (RUN unset)
through `PID_OVERRIDE`/`EPOCH_OVERRIDE` over an adversarial epoch/pid battery —
asserting no phone/SSN shape, the in-gate self-check passes (N3 false-positive
guard), and the canary trips with teeth on a hand-built shaped marker without
echoing digits (N4). This converts the probabilistic CI flake into a deterministic
pre-merge guarantee. **The two Critical risks remain covered and unregressed**:
R-01 (warmup barrier) — the 39 isolation-logic cases all still pass; R-05
(swallowed-exit-code) — tristate 19/19 unchanged.
**Issue**: RISK-COVERAGE-REPORT.md does NOT yet document this coverage (see Rework).

### Check 2 — Test coverage completeness
**Status**: WARN
**Evidence**: All 15 Phase-2 risks remain covered; the marker-PII class is a NEW,
additional covered risk traceable to #859 and proven green. The execution is
complete. **However** the coverage report still tabulates the pre-fold-in totals
(86, not 90), lists no Rust anchor under "Unit tests (cargo) — Not applicable", and
has no risk row for the marker-PII / content-scan-collision class.

### Check 3 — Specification compliance (AC-15 amendment)
**Status**: PASS
**Evidence**: `git show 511ba824 -- crates/...scanning.rs` shows the only crates/
delta is `test_scan_isolation_gate_golden_markers_pass`, added inside `mod tests`
(`#[cfg(test)]`). No production code, no scanner pattern change. AC-15 amends cleanly
from "no crates/ change" to "no crates/ PRODUCTION change; one test-only scanner
anchor." AC-04/AC-11 (cold-model GREEN) remain CI-only and PENDING-operational
(the re-run dispatch is to be gathered after this gate, per spawn). AC-14 (N3
proven) remains the human's post-delivery call. Confirmed, not failed.

### Check 4 — Architecture compliance / invariant preservation
**Status**: PASS
**Evidence**: `derive_markers` diff changes only `RUN` derivation; M_OBS_A/B,
M_MCP_A/B literals (`infra003-{obs,mcp}-{a,b}-${RUN}`) and the warmup marker
(`infra003-warmup-${RUN}`) prefixes are byte-unchanged → sqlite `query_for`
LIKE/equality predicates and read-as-barrier predicates intact. R-12 charset guard
(`*[!a-z0-9-]*`) retained and runs BEFORE the canary (base36 `0-9a-z` + `x` stays in
charset). R-18/R-02 pairwise non-substring loop retained. The canary's `[2-9]` phone
anchor means the `003` in the `infra003` prefix cannot be read as a phone start (N3).

### Check 5 — Knowledge stewardship
**Status**: PASS
**Evidence**: `infra-004-agent-5-marker-fix-report.md` and
`infra-004-mcp-infra-investigator-report.md` each carry a `## Knowledge Stewardship`
block with a `Queried:` entry (`context_briefing` → #5355/#5354/#85 and
#5343/#5131 respectively) and a `Stored:` entry (investigator stored lesson #5355;
agent-5 "nothing novel — reason given, bugs-are-GH-issues rule"). Reasons supplied.

## Rework Required (iter2)

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| RISK-COVERAGE-REPORT.md is stale post-#859 fold-in: tabulates 86 (not 90), omits the Rust anchor `test_scan_isolation_gate_golden_markers_pass`, omits the four (c) nonce-safety cases, and has no risk row for the marker-PII / MCP-content-scan-collision class. | uni-tester | Regenerate the report: shell total 86→90 (isolation-logic 39→43); add a "Unit tests (cargo)" entry for the test-only Rust anchor (1 passed) noting the AC-15 amendment; add a marker-PII coverage row traceable to #859 mapping to the (c) battery + canary + Rust anchor; note the class as an ADDITIONAL covered risk (the #859 root cause), deterministic pre-merge guarantee. |

## Verdict rationale

The fix itself is sound and FULLY proven by re-executing the shipped bytes (90 shell
+ 1 Rust anchor + 24 smoke, all green); the two Critical risks are unregressed; all
load-bearing invariants are preserved; AC-15 amends cleanly to a single test-only
anchor. The ONLY gap is that the gate's primary mapping artifact —
RISK-COVERAGE-REPORT.md — was not regenerated to document the new #859 marker-PII
coverage (wrong counts, missing the anchor and the (c) cases, missing the
root-cause risk row). Because traceability of the incident's own risk class through
the coverage report is a Gate-3c standard, this is a **REWORKABLE FAIL** with a
single narrow, tester-owned documentation update. No code rework, no re-test of the
fix is required — only the report regeneration. Per spawn, the AC-11 cold-model
GREEN re-run remains a post-gate operational artifact.
