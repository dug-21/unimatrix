# Test Plan — `dogfood-switchover.sh` prune retire (GATE C)

Component: `scripts/dogfood-switchover.sh`. Retire `PRUNE_FRAGMENT` / `pruneStaleUniHooks` / `commandReferencesTarget` / `shellTokens` / `emitAndWrite`; `promote`/`rollback` collapse to `mergeSettings(..., {dryRun})` owning their own write (matching `initRemote`, nan-016 ADR-003 #4926). Keep `--dry-run`, exit codes (0/2/5/7), completeness checks (R-05), env-only param passing, mode handling.

**Binding risk: R-04 (Critical) — parity must be proven on REAL legacy input BEFORE deletion (#4938).** The proof is executed in the `dogfood-effect.test.js` harness (real script → real settings); this plan owns the static-absence assertion, the ordering gate, and the P1–P8 parity-row mapping. The harness mechanics live in dogfood-effect-harness.md.

---

## AC-09a — static fragment absence (grep)

`test_switchover_has_no_bespoke_prune` (add to `dogfood-effect.test.js`, reading `SWITCH_SH` source) OR a delivery-time grep assertion. Reads `scripts/dogfood-switchover.sh` and asserts it contains **none** of:

- `PRUNE_FRAGMENT`
- `pruneStaleUniHooks`
- `commandReferencesTarget`
- `shellTokens`
- `emitAndWrite`

And asserts **both arms own the write through mergeSettings**: each of `run_promote` / `run_rollback` calls `mergeSettings(..., { dryRun })` (forwarding the script's `--dry-run`) and writes `result.content` itself (or relies on a non-dryRun `mergeSettings` write — implementer's collapse choice; the assertion checks no separate bespoke prune step remains and `mergeSettings` is the merge authority).

Equivalent CLI form for the coverage report:
```bash
grep -E 'PRUNE_FRAGMENT|pruneStaleUniHooks|commandReferencesTarget|shellTokens|emitAndWrite' \
  scripts/dogfood-switchover.sh && echo FAIL || echo PASS
```

## AC-09 (retained behavior) — script contract unchanged

The retire must NOT regress the script's own surface. Assert (harness already exercises most via real invocation):

- `--dry-run` forwards `dryRun:true` and writes nothing (T1/T1d style under dry-run; settings byte-unchanged).
- Exit codes preserved: bad/absent mode → 2; incomplete client → 5; node fragment throw (malformed settings) → 7. (Completeness checks at lines 115–126 stay.)
- Env-only parameter passing retained (no interpolation into a JS string literal).
- Mode handling (`promote` | `rollback`) retained.

`test_switchover_dry_run_writes_nothing` and `test_switchover_exit_codes` may be added to the harness if not already implied by T1/T2; otherwise verified by a delivery-time manual run on a scratch settings file.

## GATE C — P1–P8 parity proof on REAL legacy input (binding)

The parity proof runs in `dogfood-effect.test.js` (see dogfood-effect-harness.md and OVERVIEW §GATE C). Each P-row maps to a passing harness assertion on **real legacy-shaped input — never a pre-narrowed seed**:

| # | Real legacy input (in `buildSeedSettings`) | Required outcome | Arm | Asserting test |
|---|---|---|---|---|
| P1 | genuine `"*"` Rust `PreToolUse` uni hook | pruned; fresh under `PRETOOLUSE_CYCLE_MATCHER` | promote | T1 `assertCleanPromoteState` |
| P2 | stale node-client uni hook | pruned | rollback | T2 (b) node-client count 0 |
| P3 | `.../index.js.bak` uni hook | pruned (unconditional) | both | T1+T2: no `.bak` survivor |
| P4 | old-client-dir uni hook (`dogfood-client-OLD/...`) | pruned | promote | T1: no old-dir survivor |
| P5 | genuine `LD_LIBRARY_PATH=<dir> <rust> hook <e>` | **kept** by identity | rollback | T2 (a): exact Rust form |
| P6 | quoted spaced-path target (#4931) | **kept**; quoting irrelevant | both | unit `test_cross_group_quoted_spaced_path_target_kept` (see merge-settings plan) |
| P7 | foreign hook alongside stale uni hooks | preserved byte-for-byte | both | T1+T2 `foreignPresent` |
| P8 | group emptied solely of uni hooks | dropped; event key kept | both | T1+T2: no empty group, event key present |

**Ordering gate:** P1–P8 GREEN on real legacy input → only then the fragment-deletion commit. **Deletion must not precede the parity proof.** Coverage report records the parity-proof commit SHA and the deletion commit SHA and confirms parity-proof ≤ deletion.

> P6 is intentionally proven at unit level (a spaced install path is not realizable in the `os.tmpdir()` harness install dir; see OVERVIEW Open Question 2). The unit identity assertion is the load-bearing #4931 proof; the harness proves P1–P5/P7/P8 on real install.

## Coverage requirement

Every row P1–P8 has a passing assertion on real legacy-shaped input for the correct arm; the bespoke prune is statically gone; the parity proof precedes deletion. Do not retire on tests that exercise only pre-narrowed seeds.
