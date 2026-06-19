# Test Plan — `create-container-manifest` (release.yml job, `needs:` rewire)

> This component carries the **`needs:`-graph assertion (R-06)** and the **manifest-gating /
> dispatch green-skip (R-08)** — the two topology invariants that keep the gate honest and
> ADR-004 intact. All graph assertions are **pre-merge static** (parse `release.yml`); the
> skip *behavior* is confirmed post-tag/post-dispatch.

## Change under test

`create-container-manifest.needs` changes from `[build-container-x64, build-container-arm64]`
(release.yml 398) to `[smoke-amd64, smoke-arm64]`, **plus** a new
`if: github.event_name != 'workflow_dispatch'` on the job. The builds stay in the graph
transitively (each smoke `needs:` its own-arch build), so re-listing them is redundant.

---

## T4 — `needs:`-graph assertion (R-06) — PRE-MERGE HARD GATE

Parse `release.yml` and assert the closed-set edge invariant. **Zero cross-branch edge into
binary/npm; single manifest block point.**

| Test fn | Assertion | Risk |
|---------|-----------|------|
| `test_manifest_needs_both_smokes` | `create-container-manifest.needs` includes **both** `smoke-amd64` AND `smoke-arm64` | R-08 |
| `test_no_smoke_in_binary_npm_needs` | No `smoke-*` job appears in any `build-linux-*` / `package-npm` / `create-release` `needs` | R-06 |
| `test_no_binary_npm_in_smoke_needs` | No `build-linux-*` / `package-npm` / `create-release` appears in any `smoke-*` `needs` | R-06 |
| `test_create_release_needs_package_npm_only` | `create-release` still `needs: package-npm` only (unchanged) | R-06 |
| `test_smoke_needs_own_arch_build_only` | `smoke-amd64 needs: [build-container-x64]`, `smoke-arm64 needs: [build-container-arm64]` — single own-arch edge, no cross-arch | R-06/R-10 |
| `test_single_manifest_block_point` | The manifest is the ONLY job gated on the smokes; no other job depends on a `smoke-*` | R-06/R-08 |

**Coverage requirement (R-06):** the `needs:`-graph assertion proves **zero cross-branch
edges** and a **single manifest block point**. The closed-set invariant — *smoke jobs depend
ONLY on container-branch jobs; the manifest is the single block point* — is documented here so
a later naive `needs:` edit (coupling smoke to binary/npm, letting an arm64/Docker flake block
an unrelated binary/npm release) is a **flagged** change, not a silent one (memory:
file-scope agents must flag adjacent breakage).

### Mutation check (graph reasoning, local; behavior post-tag)
- `test_smoke_failure_leaves_binary_npm_reachable` (reasoned over the parsed graph): forcing a
  `smoke-*` job to fail leaves `package-npm` and `create-release` reachable/unaffected — only
  the manifest is blocked (ADR-004 / R-06).

---

## T4 — Manifest gating + dispatch green-skip (R-08) — PRE-MERGE config; behavior post-tag

| Test fn | Assertion | Provability |
|---------|-----------|-------------|
| `test_manifest_no_continue_on_error` | Neither smoke job nor the manifest carries `continue-on-error` / an `if:` that lets the manifest proceed on a red smoke | pre-merge static (R-08, backstops R-02) |
| `test_manifest_dispatch_gate_present` | `create-container-manifest` carries `if: github.event_name != 'workflow_dispatch'` | pre-merge static (R-08/R-11, AC-08) |
| `test_manifest_dispatch_gate_keeps_push_needs` | The `if:` does NOT remove the `needs: [smoke-amd64, smoke-arm64]` for the push path | pre-merge static (AC-08) |

### Post-tag / post-dispatch behavioral confirmations (AC-07/AC-08)
- A deliberately-red smoke leaves the manifest step **skipped** (not run) on a real `v*` push.
- A `workflow_dispatch` dry-run leaves the manifest job **green-skipped** (skipped, not
  failed) — the run's pass/fail reduces to the two `smoke-*` job statuses, and no false-red
  manifest-assembly is attempted against per-arch tags the dispatch build never pushed
  (`:<branch>-<arch>` 404). This is **not** a false-red (AC-08 / FR-10 / NFR-11).

**Coverage requirement (R-08):** the manifest `needs` both smokes; a red smoke demonstrably
skips the manifest (config-verified locally; behavior confirmed post-tag). On
`workflow_dispatch` the manifest is gated off and skips cleanly rather than going falsely red.
