# Gate 3b Report: nan-020

> Gate: 3b (Code Review)
> Date: 2026-06-20
> Branch validated: feature/nan-020 @ HEAD (de8939a0)
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | Gates 5–7 + hermetic sandbox match `docker-http-posture-smoke.md` / `hermeticity-sandbox.md`; SMOKE_*_CMD seam is the OVERVIEW stub-drivable design realized |
| Architecture compliance | PASS | ADR-001 (extend-in-place, no new exit codes), ADR-002 (host/container split + setup-node), ADR-003 (canonical chain only), ADR-004 (uni-docs remit), ADR-005 (process-boundary hermeticity + negative control) all honored |
| Interface implementation | PASS | Exit table 0/1/3/4 unchanged; messages match OVERVIEW table B exactly; blob prefix `unimatrix-bundle:`; isolated credstore `$SANDBOX/home/.unimatrix/<hash>/`; single terminal marker |
| Test-case alignment | PASS | Logic test (19) + static test (12) + nan-019 regression (14) cover R-01/R-02/R-03/R-04/R-05/R-07/R-15 scenarios; negative control + positive twin + non-flaky store-grew all present |
| Code quality / anti-stub | PASS | No TODO/FIXME/unimplemented/placeholder stubs; all scripts < 500 lines; tests run green |
| Security | PASS | stdout-only blob capture (no token leak); blob quoted (no word-split/eval); HOME isolation closes credential residue; no new untrusted input surface |
| Knowledge stewardship | PASS | All 5 agent reports carry `## Knowledge Stewardship` with Queried + Stored/nothing-novel; implementer (agent-3) stored novel pattern #5258 |

**Load-bearing checks (this feature):**

| Load-bearing item | Status | Evidence |
|-------------------|--------|----------|
| `release-gate-lib.sh` byte-unchanged | PASS | `git diff HEAD~3..HEAD` empty for the file; static test `test_run_smoke_gate_byte_unchanged` (sha256 matches HEAD baseline) green |
| Gates 5–7 appended after Gate 4, before single marker; 1–4 not reordered | PASS | `bundle_attach_gates` call at line 356, between Gate 4 (line 352) and the terminal marker (line 358); static `test_append_only_ordering` green |
| New failures fold into existing `fail()`/exit-1; NO exit 5/6/7 | PASS | `grep -nE 'exit [567]'` returns none; all six new failure modes call `fail()` (exit 1) with distinct tails matching table B |
| Env-injectable seam present | PASS | SMOKE_EMIT_CMD / SMOKE_INIT_CMD / SMOKE_HOOK_CMD / SMOKE_STORE_SIZE_CMD all wired (lines 73–123) |
| Process-boundary HOME + throwaway --project-dir; clean-on-entry + trap; NO in-process HOME mutation | PASS | HOME set on the spawned child (lines 87/89–91, 100, 107); `mktemp -d` + `rm -rf` + `mkdir` clean-on-entry (lines 169–171); cleanup trap extended (line 41); static `test_no_inprocess_home_mutation` green |
| REQUIRED hermeticity negative control asserted pre-merge (NOT PENDING) + positive twin + non-flaky store-grew | PASS | `test_hermeticity_negative_control_still_red`, `test_hermeticity_discrimination_unisolated_would_green`, `test_hermeticity_positive_twin_run`, `test_store_grew_non_flaky (5/5)` all green in logic test |
| `release.yml`: pinned setup-node@v4 (node 24) on BOTH smoke jobs, after checkout, before run_smoke_gate | PASS | smoke-amd64 (lines 412–416) and smoke-arm64 (lines 438–441); static `test_setup_node_*` (present/pinned-24/ordering) green |
| Gate-logic tests + nan-019 regression run and pass (Docker/node/network-free) | PASS | bundle-logic 19/0, bundle-static 12/0, nan-019 regression 14/0 — all RC=0 (ran directly) |
| Docs: client-setup.md zero 501/W2-7/curl-observe; both modes; --remote legacy; init --bundle no --slug | PASS | `grep -cE '501|W2-7'`=0; curl-observe=0; legacy marked (lines 15/39/41); no `--slug` paired with `--bundle` |
| README four occurrences converged; legacy --remote marked; README:62 ONNX untouched | PASS | Lines 123/130/585/587 converged to `init --bundle <blob>`; legacy form marked (110/113); README:62 not in diff |
| uni-docs.md remit widened to all of docs/, blast-radius-scoped, full-tree non-goal, prompt-injection + ship-only retained, no Feature-2 drift-checker/gate | PASS | Diff confirms all six properties; explicit "No drift-checker, CI gate, or Phase-4 trigger — Feature 2" fence added |

## Detailed Findings

### Pseudocode fidelity
**Status**: PASS
**Evidence**: The implemented `bundle_attach_gates()` (smoke script lines 137–214) realizes the
`docker-http-posture-smoke.md` pseudocode step-for-step: node preflight → Gate 5 emit (stdout-only,
rc captured without a pipe) → blob prefix validation → hermetic sandbox setup → Gate 6 consume
(HOME on child, no `--slug`) → Gate 7 fresh-BEFORE sample, hook fire under the same isolated HOME,
observe-code discrimination, store-delta assertion. The pseudocode's "implementation note" left the
hook-client invocation shape open; the implementer resolved it via the fail-open client + the
store-delta as the load-bearing assertion — exactly as the pseudocode authorized. The
`SMOKE_*_CMD` injection seam is the OVERVIEW/hermeticity-sandbox stub-drivable design made concrete,
not a departure.

### Architecture compliance
**Status**: PASS
**Evidence**: ADR-001 — `run_smoke_gate` byte-unchanged (diff empty + sha256 test); new failures
use existing `fail()` exit 1, no codes 5/6/7. ADR-002 — emit runs in a throwaway container
(`docker run --rm ... client-bundle`), consume runs host-side JS via the repo-checkout client;
setup-node@v4 pinned on both jobs. ADR-005 — isolation is at the process boundary (HOME on the
spawned child only; static `test_no_inprocess_home_mutation` confirms no in-process mutation),
clean-on-entry, proven by a negative control that is asserted pre-merge.

### Interface implementation
**Status**: PASS
**Evidence**: Exit-code truth table 0/1/3/4 preserved (nan-019 regression 14/0). The six new
failure messages match OVERVIEW table B byte-for-byte (verified by `test_msg_*` assertions). Blob
contract: prefix test `unimatrix-bundle:*`, quoted handoff, stderr dropped. Isolated credstore
path resolves under `$SANDBOX/home`. Single terminal marker at line 358 (last `log`).

### Test-case alignment
**Status**: PASS
**Evidence**: Critical risks covered pre-merge: R-01 (truth table, no-early-exit-0 via
marker-suppressed test), R-03 (byte-unchanged wrapper, append-only ordering, single marker, RC
survival via nan-019 regression), R-07 (REQUIRED negative control + discrimination + positive twin
+ non-flaky 5/5 store-grew). R-16 (legacy `--remote` not doc-tested) correctly accepted as residual
— only the "legacy" label is asserted by inspection. The split into logic/static test files keeps
both under the 500-line ceiling without losing coverage.

### Code quality / anti-stub
**Status**: PASS
**Evidence**: Scan for TODO/FIXME/unimplemented/placeholder/PENDING returned only two benign
comment hits — "PENDING IS a gap — it is proven here pre-merge" (asserting the opposite of a stub)
and "loud placeholder warning" (describing a runtime URL warning, not code). All scripts < 500
lines (358/353/202/fixtures). All three test suites run green directly (Docker/node/network-free).

### Security
**Status**: PASS
**Evidence**: nan-020 adds no application code and no new untrusted input surface. Harness-side:
Gate 5 captures stdout only (`2>/dev/null` on the emitter) so the token-redacted stderr echo never
enters the blob or CI log; the blob is passed quoted throughout (no word-split, no eval); credential
residue is closed by HOME isolation + clean-on-entry + trap teardown. No path-traversal, injection,
or deserialization surface introduced.

### Knowledge stewardship
**Status**: PASS
**Evidence**: All five `product/features/nan-020/agents/*-report.md` carry a `## Knowledge
Stewardship` block. Read-only agents (pseudocode, spec) have Queried entries with read-only-tier
reasons. Active agents: architect stored ADRs #5249–#5252; the implementer (agent-3-mechanism)
queried #5192/#5189/#5180/ADR-005 and stored novel pattern #5258 ("Stub-drive appended Docker-smoke
gates + prove HOME hermeticity via negative control") via /uni-store-pattern.

## Rework Required

None.

## Notes on scope of this gate

This feature ships bash scripts, a YAML workflow, two doc files, and one agent definition — no Rust
application code. `cargo build --workspace` / `cargo audit` are therefore not applicable to the
nan-020 diff; the equivalent build-correctness proof is the three executable gate-logic suites,
which run green. The real container round-trip (Gates 5–7 against a live image) is correctly
classified POST-TAG-CONFIRMABLE per the Risk Strategy (#5189) — its pre-merge proof is the
stub-driven truth table + negative control, all present and asserted (not PENDING).
