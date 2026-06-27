# Gate 3b Report: infra-004

> Gate: 3b (Code Review)
> Date: 2026-06-27
> Result: PASS (1 WARN)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | C-WB / C-TS / C-LN / C-FLIP code matches validated Stage-3a pseudocode line-for-line; R-01 + R-05 invariants honored. |
| 2. Architecture compliance | PASS | Placement, additive lib fn, §5 fail-closed table, append-only `needs:` flip all match ARCHITECTURE + ADR-001..004. |
| 3. Interface implementation | PASS | Capture shape, return-not-exit, anchored `-qxE` grep, canonical INFRA marker (em-dash exact), `resolve_image` sole resolver, no `${GITHUB_REF_NAME#v}`, no docker build. |
| 4. Test case alignment | PASS | All four component test plans realized; logic/static suites pass at reported counts. |
| 5. Code quality | PASS | shellcheck-clean (warning level) x5; YAML parses; no stubs/TODOs; no source file >500 lines. |
| 6. Security | PASS | No hardcoded secrets (`${{ secrets.GITHUB_TOKEN }}`); charset-guarded RUN/marker; echoed-container-output injection mitigated by full-line `-x` anchor; fail-closed apt provisioning. |
| 7. Knowledge stewardship | WARN | Design-phase stewardship present + exemplary; no implementation-wave agent reports committed to verify impl-stage `Queried:`/`Stored:` — coordinator to confirm. |

## Test & Tooling Evidence (run in foreground)

| Suite | Result |
|-------|--------|
| `release-gate-isolation-logic-test.sh` (C-WB) | **39 passed, 0 failed** (rc 0) |
| `release-gate-tristate-logic-test.sh` (C-TS) | **19 passed, 0 failed** (rc 0) |
| `release-gate-logic-test.sh` (sibling `run_smoke_gate`, R-07) | **15 passed, 0 failed** (rc 0) |
| `release-gate-isolation-lane-static-test.sh` (C-LN/C-FLIP) | **13 passed, 0 failed** (rc 0) |
| shellcheck -S warning (3 prod scripts + 3 test scripts) | CLEAN x5 (lib + smoke + 3 tests; lane is YAML) |
| `python -c yaml.safe_load(release.yml)` | parses OK |

> NOTE (per spawn prompt): this is a test/CI-only shell+YAML feature, no `crates/` change. "Compiles" = shellcheck-clean + YAML parses + logic tests pass. Missing cargo is expected, not a gap.

## Detailed Findings

### 1. Pseudocode fidelity — PASS
- **C-WB** (`multi-tenant-isolation-smoke.sh`): `warmup_barrier()` is a near-verbatim realization of the pseudocode — RUN nonce + charset guard, throwaway `infra003-warmup-${RUN}` marker, bidirectional non-substring assertion vs the four cell markers (R-02), one durable `write_then_barrier observe "$SLUG_A" "$SLUG_DIR_A" "$warmup_marker"` on the widened `WARMUP_DEADLINE_SECS` bound (restored after), then a `case "$WTB"` that **consumes** PRESENT to proceed and maps anything else to `infra_fail` (exit 2). Inserted exactly between `assert_routes_live` (line 486) and `run_isolation_matrix` (line 487).
- **C-TS** (`release-gate-lib.sh`): `run_smoke_gate_tristate` matches the pseudocode truth table cell-for-cell.

### 2. Architecture compliance — PASS
- ARCHITECTURE §3/§4 exit-code→action mapping implemented exactly: 0+marker→0, 0-no-marker→1, 1→1, 2→0 (warning+marker), 3→1, *→1.
- §5 fail-closed: only script-exit-2 maps to non-blocking; every harness step (`set -euo pipefail`, sqlite3 apt step) fails closed. ADR-001 (#767-derived 180s, model-load delta), ADR-002 (additive fn), ADR-003 (containment + self-contained sqlite3), ADR-004 (dispatch path / no `if:` guard) all honored.

### 3. Interface implementation (R-05 / R-01 invariants) — PASS
- **R-05**: `set +e; out="$(IMAGE="${image}" "$@" 2>&1)"; rc=$?; set -e` — **no pipe** between smoke and `$?`; `return`, never `exit`; `set -e` re-enabled after capture. Verified statically (`test_tristate_no_pipe_static_return_not_exit`) and by execution (rc 1/2/3 survive capture).
- Canonical INFRA marker `[infra004-gate] INFRA — ISOLATION NOT VERIFIED THIS RUN` present verbatim with U+2014 em-dash (byte-checked; `test_tristate_marker_byte_identical` passes).
- GREEN anchor `grep -qxE '\[[a-z0-9-]+-smoke\] ALL GATES PASSED.*'` — full-line `-x`; runtime line is `log()`-prefixed `[infra003-smoke] ALL GATES PASSED — ...` (TAG=infra003-smoke, `log(){ printf '[%s] %s\n' "$TAG" ...}`) → matches. Substring-in-longer-line rejection covered.
- **R-01**: PRESENT is a real durable own-store write round-trip via `write_then_barrier` (the same `SMOKE_*_CMD` seam a real write uses — `test_warmup_uses_write_then_barrier_not_store_size`), consumed to gate proceed-to-matrix, timeout→INFRA (never RED/GREEN).
- C-LN: `resolve_image` is sole resolver; `IMAGE` exported; no `docker build`; **no** `${GITHUB_REF_NAME#v}` (`test_lane_no_ref_strip`); invokes `run_smoke_gate_tristate`, not `run_smoke_gate`.

### 4. Test case alignment — PASS
- C-WB plan: placement/ordering (485<486<487), non-substring, bound documentation, stub-seam truth-table reachability — all exercised (39 tests).
- C-TS plan: full truth table via real sourced lib, capture-shape invariants, INFRA visibility, anchored marker, sibling no-regression, set-e-safe harness (19 tests + sibling 15).
- C-LN/C-FLIP plan: node+sqlite3 provisioning, resolve_image, IMAGE export, no rebuild, no ref-strip, tristate invocation, lane ∈ manifest `needs:` (13 tests).

### 5. Code quality — PASS
- `run_smoke_gate` **byte-unchanged**: first 60 lines of `release-gate-lib.sh` byte-identical old vs new; change is purely appended `run_smoke_gate_tristate`.
- `create-container-manifest.needs:` is **append-only**: `[smoke-amd64, smoke-arm64, embed-amd64, embed-arm64, multi-tenant-isolation-amd64]` — 4 originals intact + the one new lane.
- File sizes: smoke 487, lib 103, isolation-logic-test 483, tristate-test 265, lane-static-test 198 — all <500. `release.yml` is 696 but is a pre-existing CI workflow config (not a feature-authored source file; net +53 lines this feature) — informational, not a defect.
- No TODO/FIXME/placeholder/unimplemented in any added line.

### 6. Security — PASS
- GHCR auth via `${{ secrets.GITHUB_TOKEN }}` — no hardcoded credentials.
- Workflow-command-injection surface (diagnostic full-log echo of container stdout) mitigated: GREEN credit requires the full-line `-x` anchored marker, so a forged marker inside arbitrary output is not credited (documented in RISK §Security).
- Input validation: `RUN` and warmup marker charset-guarded to `[a-z0-9-]`; non-substring invariant enforced at runtime.
- sqlite3 `apt-get` provisioning fails closed (blocks), the safe direction.

### 7. Knowledge stewardship — WARN
- Design-phase agent reports (architect, spec, risk) and 3a artifacts (pseudocode/test-plan OVERVIEW) all carry `## Knowledge Stewardship` blocks with `Queried:`/`Stored:` entries — exemplary.
- No implementation-wave (shell/YAML dev) agent reports are committed under `product/test/infra-004/agents/`, so impl-stage `Queried:` (evidence of `/uni-query-patterns` before coding) and `Stored:`/"nothing novel" entries cannot be verified from the repo. The three impl waves were committed directly by the coordinator. This is plausibly a coordinator-held artifact rather than a true stewardship miss; flagged as WARN, not a blocking FAIL, because every code/test/security/confinement check passes cleanly and design-phase stewardship is present.

## AC-15 confinement
`git diff --stat main...HEAD`: exactly the 3 production files (`release.yml`, `multi-tenant-isolation-smoke.sh`, `release-gate-lib.sh`) + 3 test scripts + feature docs. **Zero `crates/` paths.** PASS.

## Rework Required
None blocking. Recommended (non-blocking): coordinator attach/confirm implementation-wave stewardship evidence (impl agents ran `/uni-query-patterns`; `Stored:` or "nothing novel -- {reason}") to fully satisfy Gate 3b check 7.

---

# Gate 3b — iter2 (#859 fold-in)

> Re-validation of commit `511ba824` (feature/infra-004 HEAD): PII-safe gate markers.
> Date: 2026-06-27
> Result: **PASS** (prior iter1 WARN now resolved)
> Scope: TEST-ONLY fold-in. Re-checked the changed files only (per validation-iteration discipline); the iter1 PASS for C-WB/C-TS/C-LN/C-FLIP stands and is regression-confirmed by unchanged counts.

## What changed
`crates/unimatrix-server/src/infra/scanning.rs` (`#[cfg(test)]` anchor only), `product/test/infra-001/scripts/{isolation-probe-lib.sh, multi-tenant-isolation-smoke.sh, release-gate-isolation-logic-test.sh}`, new `product/test/infra-001/scripts/fixtures/isolation-nonce-logic-cases.sh`, + 2 agent reports. **Zero production crates/ change; zero scanner behavior change.**

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode/approach fidelity (B1/B2/B3 + N2/N3/N4) | PASS | Code matches the #859 design-reviewed + product-reviewed approach exactly. |
| 2. Architecture compliance | PASS | No server change; observe/MCP scan asymmetry left intact (correct, vision-mandated); test-only marker-contract fix. |
| 3. Interface implementation | PASS | `_default_nonce` single seam routed by both `derive_markers` + `warmup_barrier`; PID_OVERRIDE/EPOCH_OVERRIDE injectable; prefixes/predicates preserved. |
| 4. Test case alignment | PASS | (c) cases realize the documented Missing Test; golden set shared bash↔Rust; counts 43/19/15/13 = 90 + Rust anchor 1. |
| 5. Code quality | PASS | shellcheck -S warning CLEAN x4; all files ≤500 (smoke 499, logic 491 — fixture split kept both under); no stubs/TODO/unsafe added. |
| 6. Security | PASS | Canary withholds offending digits (N4); charset-reduced ERE only (no bash-invalid `\d`/`\s`/`\b`); synthetic markers only. |
| 7. Knowledge stewardship | PASS | agent-5 + investigator reports carry `## Knowledge Stewardship` (Queried + Stored/"nothing novel -- reason"); design + product reviews on #859 likewise. **Resolves the iter1 WARN.** |

## Detailed findings

### B1 — construction-safe nonce is genuinely shape-safe (not probabilistic) — PASS
`_default_nonce -> <b36(pid)>x<b36(epoch)>` (isolation-probe-lib.sh). Phone shape requires 10 consecutive digits (`[2-9][0-9]{2}[0-9]{3}[0-9]{4}`, hyphens optional and absent inside RUN); base36 components are ≤7 chars and the **letter** `x` separator plus prefix hyphens (bounded by letters `mcp`/`a`) block any cross-boundary digit run — 10 consecutive digits cannot form. SSN shape (`[0-9]{3}-[0-9]{2}-[0-9]{4}`) needs internal hyphens that never appear inside RUN. Shapes are structurally unreachable. Verified the golden encoding `eaqxthaqu3 = b36(18530)·x·b36(1782573915)` is correct.

### B2 — charset-reduced ERE canary, fail-loud, no digit echo — PASS
`assert_marker_pii_safe` uses ERE `1?[2-9][0-9]{2}-?[0-9]{3}-?[0-9]{4}` / `[0-9]{3}-[0-9]{2}-[0-9]{4}` (no `\d`/`\s`/`\b`/`(?:)`). Invoked AFTER the R-12 charset loop in `assert_markers_distinct` and after the charset guard in `warmup_barrier`. `infra_fail` (exit 2, INFRA). Message reports shape CATEGORY with "digits withheld" (N4). `test_c_canary_trips_on_regression` proves teeth + no-echo.

### B3 — injectable seam routed by both call sites — PASS
`derive_markers` and `warmup_barrier` both use `RUN="${RUN:-$(_default_nonce)}"`; the off-Docker (c) cases drive the REAL default path (RUN unset) via PID_OVERRIDE/EPOCH_OVERRIDE — closing the historical RUN=t1 blind spot.

### N2 — Rust scanner anchor is TEST-ONLY, shared golden set — PASS
`test_scan_isolation_gate_golden_markers_pass` lives in `#[cfg(test)] mod tests`; feeds the shared golden literals through `ContentScanner::global().scan()` asserting `Ok`. No production/scanner change. Drift-proof coupling to scanning.rs.

### N3 — false-positive guard — PASS
`test_c_default_path_self_check_passes` asserts real default-path markers PASS the canary (exit 0, no trip); the `[2-9]` phone anchor means the `003` prefix cannot read as a phone start.

### Preserved invariants — PASS
R-12 `[a-z0-9-]` charset, R-18/R-02 pairwise non-substring guards, `infra003-{obs,mcp,warmup}-{a,b}-` prefixes (sqlite predicates), read-as-barrier predicates all untouched. No regression to C-WB/C-TS/C-LN/C-FLIP — tristate 19, smoke-gate 15, lane-static 13 unchanged. `listener.rs` NOT modified (pre-existing fmt drift untouched); no unrelated fmt churn (scanning.rs diff is purely the additive `#[cfg(test)]` block).

## Test evidence (foreground)

| Suite | Result |
|-------|--------|
| `release-gate-isolation-logic-test.sh` | **43 passed, 0 failed** (rc 0) — 39 prior + 4 new (c) |
| `release-gate-tristate-logic-test.sh` | **19 passed, 0 failed** (rc 0) |
| `release-gate-logic-test.sh` | **15 passed, 0 failed** (rc 0) |
| `release-gate-isolation-lane-static-test.sh` | **13 passed, 0 failed** (rc 0) — total 90 |
| `cargo test -p unimatrix-server --lib test_scan_isolation_gate_golden_markers_pass` | **1 passed, 0 failed** (rc 0) |
| shellcheck -S warning (probe-lib, smoke, isolation-logic-test, nonce-fixture) | CLEAN x4 |

> Full-workspace cargo link OOMs in this sandbox (environment memory limit, ld signal-9) — validated the Rust anchor via `--lib` per spawn instruction; production code unchanged.

## Note (out of gate scope)
Working tree carries an uncommitted edit to `/workspaces/unimatrix/CLAUDE.md` (a Behavioral-Rules wording change) — unrelated to infra-004, not a feature artifact. Flagged, not actioned.

## Iter2 verdict
**PASS.** The #859 fold-in matches the design-reviewed approach; the nonce is structurally shape-safe; the canary, seam, and Rust anchor are correctly implemented; all invariants preserved; 90 shell + 1 Rust test green; stewardship complete (iter1 WARN resolved). No rework required.
