# Scope Risk Assessment: nan-016

> F5 narrow slice — UDS-local dogfood re-release + switchover mechanism (delivered, NOT executed). Surviving AC-01..AC-06. Risks below are product/scope-level only; architecture-level risks follow in RISK-TEST-STRATEGY.md.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `npm pack`+extract vs `npm install --prefix` produces different on-disk trees; the chosen mechanism may not freeze a complete, runnable client (missing `files`-array assets, postinstall side-effects). | High | Med | Architect must pin one mechanism and assert the installed tree contains the full runtime set (`lib/hook-client/**`, `bin/`, `skills/`) and that NO postinstall runs against the host. |
| SR-02 | Fixed dir `~/.unimatrix/dogfood-client/` chosen because npm global prefix is node-version-pinned (#4923). But a stale prior install at that path can silently shadow a new build — idempotent re-run (AC-01) must mean clean replace, not overlay. | Med | High | Spec: AC-01 idempotency must clear/replace the target dir atomically; define behavior when target pre-exists. |
| SR-03 | Build reproducibility (Goal 1 / AC-01) depends on the in-repo `packages/unimatrix` build being deterministic; lockfile drift or transient deps could make "re-run = soak reset" non-reproducible. | Med | Low | Confirm build is dependency-free (C-9 gate) and produces byte-stable output for AC-05's byte-identical claim. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | "Delivered but NOT executed" — the capability may be un-validatable without a live flip, making AC-02/AC-03 vacuous. Verification-by-effect (fixture/dry-run/scratch dir) must be a real proof, not a tautology. | High | Med | Spec AC-02/AC-03 must define a concrete effect-harness (scratch `.claude/settings.json` + scratch project root) that exercises `mergeSettings` and re-fires a hook against the installed path — NOT a string-diff of the script. |
| SR-05 | The switchover repoints via `mergeSettings`, which deliberately narrows the stale PreToolUse `"*"` matcher — a behavioral delta beyond a command swap. The effect-test must capture this delta, or the eventual live flip surprises the operator. | Med | Med | Effect-test asserts the resulting matcher set, not just the command string. Runbook (AC-04) must call out the matcher-narrowing as an intended delta. |
| SR-06 | Soak-clock boundary: nan-016 enables but must not start the F6 (#682) clock. A test/runbook that flips live settings (even transiently and reverts) could be read as "executed" and perturb a future soak. | Med | Low | Hard constraint: no test touches this repo's live `.claude/settings.json`. Architect routes all switchover exercise through scratch paths. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | Project-root-hash shared state (#4923): installed copy and Rust binary share `~/.unimatrix/{hash}/`. Correct for state-sharing, but "isolation" (AC-03) is CODE-freezing only, not state isolation. Conflating the two yields a wrong test. | High | Med | Spec AC-03 explicitly: isolation = installed `lib/` bytes/behavior unchanged after editing in-repo source; NOT state-dir separation. Cite #4923. |
| SR-08 | C-7 fail-open: switchover must not introduce a hook path that can break the host session, and must not depend on the daemon being up. A copy-install with broken/partial node resolution could exit non-zero. | High | Low | Architect ensures the emitted `node <path>/index.js` command fail-opens identically to today's binary hook; effect-test includes a daemon-absent case. |
| SR-09 | AC-05 byte-identical `npx … init` regression: nan-016 adds scripts/runbook but must not perturb `lib/init.js`/`merge-settings.js` paths or the C-04 size gate. | Med | Low | Treat init wiring as frozen (C-8); regression-test the init local path unchanged. |

## Assumptions

- **A1 (Proposed Approach §1, AC-01):** A copy-install can freeze a fully-runnable client. If `packages/unimatrix` runtime assets aren't all in the `files` array / pack output, the frozen copy is incomplete → SR-01.
- **A2 (Background, #4923):** The installed copy run from this repo's cwd derives the same socket hash as the in-repo client. Verified in code and Unimatrix #4923; if a future node/cwd change breaks the walk-up-to-`.git` logic, SR-07 isolation reasoning changes.
- **A3 (Goal 3, AC-02/03):** The capability is fully validatable WITHOUT a live flip. If any AC truly requires the live repo flipped, the "delivered-not-executed" boundary is unachievable → SR-04.
- **A4 (C-7):** A local UDS daemon already runs and hooks fail-open if absent. Switchover does no daemon lifecycle management; if the eventual operator assumes it does, the runbook is misleading → SR-08.

## Design Recommendations

1. **Make verification-by-effect concrete (SR-04, SR-05, SR-06):** Spec must mandate a scratch-fixture harness — a throwaway project root + scratch `settings.json` — that runs the real switchover script through `mergeSettings` and re-fires a hook against the installed path. No test may touch this repo's live settings.
2. **Frame AC-03 as code-freezing, not state isolation (SR-07):** Architect cite #4923; the proof is "edit in-repo source → installed bytes/behavior unchanged," exercised against the external absolute path.
3. **Guarantee idempotent clean-replace + complete frozen tree (SR-01, SR-02):** Pin one install mechanism, assert full runtime asset set, and define pre-existing-target behavior so the F6 reset point is deterministic across container rebuilds.
