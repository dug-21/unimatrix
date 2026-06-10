# vnc-031 Test Plan — OVERVIEW

Cross-matcher-group stale uni-hook prune in `mergeSettings` (Step 3c), dogfood-script prune retire, and effect-harness attribution repoint. **Install-surface only — pure JS, `node --test`. No Rust, no infra-001.**

## Test Strategy

This feature has an unusual test-design property, called out by the Risk Strategy: ADR-001's object-identity keep-rule makes the highest-severity *correctness* risk (SR-01: zeroing a managed event) **unrepresentable by construction**. So the happy-path AC tests are necessary but **not sufficient**. Test design attacks the two residual risks the construction does not close:

1. **R-01 (Critical) — identity silently degrading to a string/command compare.** A future "simplification" to `entry.command === fresh.command`, or an in-loop string bug, still passes happy-path tests but prunes the survivor (or keeps a stale near-twin) whenever command *shape* varies. The discriminating tests deliberately vary the command shape (collapsible whitespace / arg order / `LD_LIBRARY_PATH` prefix / quoted spaced path) between a stale near-twin and the fresh keep-target so **only an object-identity implementation stays green**. A `command ===` reimplementation must turn at least one test red.
2. **R-04 (Critical) — script-retire parity not proven on real legacy input.** A green `merge-settings.test.js` does **not** authorize deleting `PRUNE_FRAGMENT`. The P1–P8 parity proof (GATE C below) must run on a settings file carrying a *genuine* `"*"` Rust `PreToolUse` uni hook plus `.bak`/old-client-dir uni hooks — never a pre-narrowed seed (#4938).

Three layers, all in `node:test`:

| Layer | Runner | Surface | Files |
|-------|--------|---------|-------|
| Unit (in-memory `mergeSettings`) | `node --test` | `lib/merge-settings.js` Step 3c | `packages/unimatrix/test/merge-settings.test.js` (extend) |
| Effect harness (real scripts → real settings → re-fire) | `node --test` | `scripts/dogfood-switchover.sh` + installed `mergeSettings` | `packages/unimatrix/test/dogfood-effect.test.js` (repoint) |
| Static / ordering gate | grep + manual commit ordering | `scripts/dogfood-switchover.sh` source | GATE C procedure below |

**Test discipline (NFR-06, cumulative):** extend the existing fixtures/helpers in `merge-settings.test.js` (`tempSettingsPath`, `writeSettings`, `writeOptIn`, `seedWith*`, `expectedLocalCommand`, `BINARY`, `DEFAULT_EVENTS`) and the existing helpers in `dogfood-effect.test.js` (`makeScratchRoot`, `buildSeedSettings`, `assertCleanPromoteState`, `noPrunePromoteContent`, `promote`, `rollback`, `uniHooks`, `reFire`). **No isolated scaffolding, no new test files.** New `seedWith*` helpers (e.g. `seedWithCrossGroupStale`) are added to the existing file, derived from the same inputs (#4263 — fixture fields diverge when not derived from one source).

## Risk → Test Mapping

| Risk | Priority | Component test plan | Primary scenarios |
|------|----------|--------------------|-------------------|
| R-01 identity degrades to string-compare | **Critical** | merge-settings-step3c | shape-varying near-twin keep; exact-fresh-command survivor assert (not `includes`) |
| R-02 zero-uni-hook fail-loud | High | merge-settings-step3c | `assert(count !== 0, ...)`; adversarial no-managed-entry-on-input seed |
| R-03 wrong-scope prune in/out managed group | High | merge-settings-step3c | in-group dup + cross-group stale compose to one survivor; multi-stale-group |
| R-04 script-retire parity (real legacy input) | **Critical** | dogfood-switchover-retire | P1–P8 on real legacy input both arms; static fragment-absence; deletion ordering gate |
| R-05 both call arms identical | High | merge-settings-step3c + dogfood-switchover-retire | string-arm + object-arm clean migration on same seed |
| R-06 idempotency incl. stale-`"*"`-run-1 | High | merge-settings-step3c | `deepStrictEqual` 2-run on stale seed; 3-run no-growth |
| R-07 foreign + near-miss + non-command survive | High | merge-settings-step3c | foreign `"*"` survives; near-miss uni-looking survives; `type:"url"` survives |
| R-08 emptied-group drop / event-key retain | High | merge-settings-step3c | `"*"` group dropped, event key kept; foreign-retaining group NOT dropped |
| R-09 partition seam 3c vs 3b | Med | merge-settings-step3c | opt-out tests unchanged; combined SubagentStop+`"*"` seed, disjoint actions |
| R-10 vnc-027 adjacency | High | merge-settings-step3c | cycle frame is keep-target; SubagentStop opt-in matrix unchanged |
| R-11 action-string contract | Med | merge-settings-step3c | cross-matcher phrase substring-disjoint; `[dry-run]` prefix |
| R-12 consumer init-test sensitivity | Med | dogfood-effect-harness + OVERVIEW | full suite green; fixture touches justified not rubber-stamped |
| R-13 harness negative control kept | Med | dogfood-effect-harness | clean-state assertion non-vacuous; `assert.throws` on unpruned post-state |
| R-14 delivery-base dependency | Med | GATE A (manual) | `git merge-base` check #706 + #4811 before prune lands |
| R-15 signature stability | Low | merge-settings-step3c | both arm shapes call current signature; no new param |

Every SR-01..SR-07 is covered (traceability in RISK-TEST-STRATEGY.md §Scope Risk Traceability). R-14 is the SCOPE Assumption-1 dependency, covered by GATE A.

## Integration / Harness Plan

**infra-001 applicability: N/A.** infra-001 exercises the compiled `unimatrix-server` Rust binary over MCP JSON-RPC. vnc-031 touches **zero** Rust / server / MCP surface (NFR-04, Non-Goal 5) — it is a pure in-memory JS transform of `.claude/settings.json` plus the existing single write. No infra-001 suite (`protocol`, `tools`, `lifecycle`, `volume`, `security`, `confidence`, `contradiction`, `edge_cases`) maps to this change, and the smoke gate validates server behavior unrelated to the install surface. **The smoke gate does not apply; state this explicitly in the RISK-COVERAGE-REPORT.** The feature's integration surface is entirely within the `packages/unimatrix` JS package and is validated by that package's own `node:test` runner plus the two script-level harnesses.

The actual integration boundaries (Risk Strategy §Integration Risks) and how each is tested:

| Integration boundary | Risk | How tested |
|---|---|---|
| Step 3 ↔ Step 3c (live object reference, not re-read content) | R-01, R-03 | merge-settings: no-managed-entry-on-input seed (capture must follow create); shape-varying near-twin |
| Step 3c ↔ Step 3b ordering (3→3c→3b) | R-09 | merge-settings: combined SubagentStop(opt-out)+`"*"`(cross-group) seed in one merge |
| `mergeSettings` ↔ `dogfood-switchover.sh` (write-ownership handoff) | R-04, R-13 | dogfood-effect harness: promote/rollback via simplified script; negative control repoint |
| `mergeSettings` ↔ `init`/`initRemote` (install-surface shape shift) | R-12 | Full `packages/unimatrix` `node --test`; any fixture change justified |

### Package-level integration commands (Stage 3c)

```bash
# 1. Unit + harness — the whole package suite (AC-10)
node --test packages/unimatrix/test/

# 2. Targeted unit suite (merge-settings Step 3c)
node --test packages/unimatrix/test/merge-settings.test.js

# 3. Effect harness (real script → real settings → re-fire); self-skips loudly if
#    npm/tar cannot stage the temp install (suiteSkipReason) — a SKIP is not a PASS.
node --test packages/unimatrix/test/dogfood-effect.test.js
```

> The dogfood-effect harness builds and installs the client into an OS-tmp dir, runs the *real* `dogfood-switchover.sh promote|rollback` against scratch settings, then re-fires the installed entrypoint. It is the closest analog to an integration suite this feature has. If it SKIPs (`suiteSkipReason`, npm/tar unavailable), that is recorded as a gap in the coverage report — never counted as green.

## GATE C — P1–P8 Parity Proof Procedure (binding, blocks the `PRUNE_FRAGMENT` deletion commit)

GATE C is the binding obligation of ADR-003 / SR-04 / R-04 / #4938: **prove the source prune (Step 3c) subsumes every case the script's whole-shell-token matcher handled, on REAL legacy-shaped input, BEFORE the commit that deletes `PRUNE_FRAGMENT`.** A green `merge-settings.test.js` does not satisfy this gate — the proof must run on a settings file carrying a genuine `"*"` Rust `PreToolUse` uni hook plus `.bak`/old-client-dir uni hooks, not a pre-narrowed cycle-matcher seed.

### How it is demonstrated

The proof is executed inside the **`dogfood-effect.test.js` harness** (real script → real settings), not as a fresh standalone script. The harness already installs a real client, builds a real legacy-shaped seed (`buildSeedSettings`), runs the real `promote`/`rollback`, and asserts clean state via `assertCleanPromoteState`. Stage 3c extends the seed and the existing T1/T2 tests to carry **all** P-shapes simultaneously, so the parity proof IS the harness run on real input.

1. **Extend `buildSeedSettings`** (the existing real-legacy seed) to additionally carry, on `PreToolUse`: a genuine `"*"` Rust uni hook (already present, P1), a `.../index.js.bak` uni hook (P3), and an old-client-dir uni hook `.../dogfood-client-OLD/lib/hook-client/index.js PreToolUse` (P4), plus the existing foreign `Bash` hook (P7), plus a managed event whose only stale entry sits alone in a `"*"` group (P8). Keep this derived from one source object (#4263).
2. **Run `promote`** (object arm) on that seed → assert via `assertCleanPromoteState` that P1/P3/P4 are gone, the fresh entrypoint command is the single survivor under `PRETOOLUSE_CYCLE_MATCHER` (P1 keep), foreign preserved byte-for-byte (P7), emptied `"*"` group dropped + event key retained (P8).
3. **Run `rollback`** (string arm) on a seed carrying a stale node-client uni hook (P2), `.bak` (P3), foreign (P7), and the genuine `LD_LIBRARY_PATH=<binDir> <rustBinary> hook <event>` command (P5) and a quoted spaced-path keep-target (P6) → assert every surviving uni hook is the exact legacy Rust form (P5 keep by identity, no dirname heuristic), node-client count == 0, quoting irrelevant (P6 kept), foreign preserved (P7).
4. **Map each P-row to a passing assertion** in the harness output. The parity table below must be GREEN for the correct arm.

| # | Legacy-shaped input | Required Step 3c outcome | Arm | Harness assertion |
|---|---|---|---|---|
| P1 | Stale `"*"` `PreToolUse` Rust uni hook | Pruned; fresh entry under `PRETOOLUSE_CYCLE_MATCHER` kept | promote | `assertCleanPromoteState` (a)+(b): zero stale, matcher === constant |
| P2 | Stale node-client uni hook (prior install) | Pruned | rollback | T2 (b): node-client count == 0 |
| P3 | `.../index.js.bak` uni hook | Pruned (uni-owned, not kept object) | both | promote+rollback: no `.bak` survivor |
| P4 | Old-client-dir uni hook (`dogfood-client-OLD/...`) | Pruned | promote | promote: no old-dir survivor |
| P5 | Rollback `LD_LIBRARY_PATH=<dir>` genuine just-written command | **Kept** (keep-target by identity) | rollback | T2 (a): every uni hook === expected Rust form |
| P6 | Quoted spaced-path target (#4931 bug) | **Kept** by object identity; quoting irrelevant | both | survivor command byte-equals the quoted form |
| P7 | Foreign hook alongside stale uni hooks | Preserved byte-for-byte | both | `foreignPresent(...)` |
| P8 | Matcher group emptied solely of uni hooks | Group dropped, event key retained | both | no empty `"*"` group; event key present |

### Ordering gate (binding)

P1–P8 demonstrated GREEN on real legacy input for the correct arm → **only then** the commit that deletes `PRUNE_FRAGMENT` / `pruneStaleUniHooks` / `commandReferencesTarget` / `shellTokens` / `emitAndWrite`. **Deletion must NOT precede the parity proof.** Verified by commit ordering: the parity-proof harness change lands and is GREEN in (or before) the same delivery sequence as, but never after, the fragment-deletion commit. The RISK-COVERAGE-REPORT records the two commit SHAs and confirms parity-proof ≤ deletion.

## GATE A and GATE B (delivery-time, recorded in the coverage report)

- **GATE A (R-14, pre-prune):** before the Step 3c prune lands, confirm crt-052 #706 (transcript-fed cycle-review distillation) and vnc-027 #4811 (matcher-narrowing) are reachable on the **delivery base branch** (`git merge-base` / `git log` check), not merely "on main as of writing." Manual; recorded in the coverage report. Blocks the prune commit (AC-01..AC-08). Without it, broad-`"*"` telemetry is pruned with no live replacement source.
- **GATE B (R-13 / OQ-5, AC-09 harness):** the `dogfood-effect.test.js` clean-state attribution is repointed from the (retired) script prune to `mergeSettings`-alone, and the **negative control is preserved, not deleted**. nan-016 already shipped the clean-switch rework, so this is an **attribution repoint** (where `noPrunePromoteContent` reconstructs the no-prune state), NOT a fresh harness rewrite or a nan-016 RUNBOOK assertion flip (the RUNBOOK carries no surviving-`"*"` assertion — confirmed). Do not let scope balloon into a nan-016 test rewrite (OQ-C, R-13). See dogfood-effect-harness.md.

## Acceptance Criteria → Test Plan

| AC | Verification | Plan file |
|----|-------------|-----------|
| AC-01 | `test_legacy_star_pretooluse_migrates_clean` | merge-settings-step3c |
| AC-02 | extended `test_each_event_has_exactly_one_unimatrix_entry` + `assert(count !== 0)` | merge-settings-step3c |
| AC-03 | foreign-`"*"` + near-miss + non-command preservation | merge-settings-step3c |
| AC-04 | `test_cross_group_drops_emptied_group_keeps_event` | merge-settings-step3c |
| AC-05 | `test_cross_group_migration_idempotent` | merge-settings-step3c |
| AC-06 | `test_cross_group_migration_string_arm` + `_object_arm` | merge-settings-step3c |
| AC-07 | existing SubagentStop opt-out tests unmodified; phrase disjoint | merge-settings-step3c |
| AC-08 | `test_cross_group_emits_action_and_dry_run_prefix` | merge-settings-step3c |
| AC-09 | (a) grep fragment-absent; (b) harness clean-state + negative control; (c) GATE C parity | dogfood-switchover-retire + dogfood-effect-harness |
| AC-10 | `node --test packages/unimatrix/test/` green; fixture touches justified | this OVERVIEW (suite gate) |

## Open Questions

1. **Object-arm `seedWithCrossGroupStale` reuse.** The string-arm seed uses `BINARY`/`expectedLocalCommand`; the object arm uses `buildHookClientCommand(clientPath, event)`. The seed helper must accept a `commandForEvent`-style producer so both arms derive the fresh command from one source (#4263). Resolve in 3b: the helper signature is `seedWithCrossGroupStale(fp, { staleCommand, foreign })` and the *fresh* command is asserted per-arm via the arm's own producer — confirm with the implementer.
2. **P6 quoted-spaced-path keep-target in the harness.** The genuine #4931 case needs an installed client dir whose path contains whitespace. The harness installs into `os.tmpdir()/dogfood-client-test-<rand>` (no spaces). Plan: assert P6 as a *unit* test in `merge-settings.test.js` (keep-target with a `node "/a b/.../index.js" <event>` command kept by identity) rather than forcing a spaced install dir in the harness — the unit identity assertion is the load-bearing proof; the harness covers P1–P5/P7/P8 on real install. Confirm this split is acceptable for GATE C (identity proof is implementation-level; harness proves real-input subsumption).
