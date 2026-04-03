# crt-042: Test Plan — Eval Profile (`ppr-expander-enabled.toml`)

## Component Scope

Component 4 of 4. The eval profile is a TOML file committed to
`product/research/ass-037/harness/profiles/ppr-expander-enabled.toml`. It enables the
expander for the A/B eval gate measurement.

**File under test**: `product/research/ass-037/harness/profiles/ppr-expander-enabled.toml`
**Secondary**: `run_eval.py` accepts the profile without modification.

This component has fewer unit tests than the others — its verification is primarily file
existence and eval harness execution. The eval gate (AC-23) is an outcome measurement, not
a binary pass/fail assertion in the test suite.

---

## AC-22: File Exists and Eval Harness Accepts It

**Risk covered**: R-07 (partial), R-06 (procedural)

**Verification step 1**: File existence check.

```bash
test -f product/research/ass-037/harness/profiles/ppr-expander-enabled.toml \
  && echo "AC-22 PASS: file exists" \
  || echo "AC-22 FAIL: file missing"
```

**Expected file content** (from SPECIFICATION.md FR-10):

```toml
[profile]
name = "ppr-expander-enabled"
description = "PPR expander enabled (crt-042). HNSW k=20 seeds -> graph_expand depth=2 max=200 -> expanded pool -> PPR -> fused scoring."
distribution_change = true

[inference]
ppr_expander_enabled = true
expansion_depth = 2
max_expansion_candidates = 200
```

**Verification step 2**: TOML is valid and fields are present.

```bash
# If run_eval.py supports --dry-run or --validate:
python run_eval.py --profile ppr-expander-enabled.toml --dry-run

# If not, parse the TOML directly:
python3 -c "
import tomllib
with open('product/research/ass-037/harness/profiles/ppr-expander-enabled.toml', 'rb') as f:
    cfg = tomllib.load(f)
assert cfg['inference']['ppr_expander_enabled'] == True, 'ppr_expander_enabled must be true'
assert cfg['inference']['expansion_depth'] == 2, 'expansion_depth must be 2'
assert cfg['inference']['max_expansion_candidates'] == 200, 'max_expansion_candidates must be 200'
print('AC-22 PASS: profile parsed and fields validated')
"
```

**Verification step 3**: Eval harness execution.

```bash
cd product/research/ass-037/harness
python run_eval.py --profile ppr-expander-enabled.toml
echo "Exit code: $?"
```

Assert exit code is 0. If `run_eval.py` does not support `--profile` flag, the test fails
at AC-22 and the delivery agent must confirm `run_eval.py` API before the profile is finalized.

---

## AC-23: Eval Gate — MRR and P@5 Measurement

**Risk covered**: R-07 (High), R-06 (High)

This is an outcome measurement, not a unit test assertion. It cannot be placed in `cargo test`.

**Procedure** (Stage 3c execution):

1. Take a DB snapshot AFTER the S1/S2 back-fill migration is committed and verified:
   ```bash
   unimatrix snapshot --output eval-snapshot-crt042.db
   ```

2. Run baseline (expander disabled):
   ```bash
   cd product/research/ass-037/harness
   python run_eval.py --profile conf-boost-c.toml --db eval-snapshot-crt042.db \
     2>&1 | tee eval-baseline.log
   ```
   Extract MRR and P@5 from `eval-baseline.log`. Record as `MRR_baseline` and `P5_baseline`.

3. Run with expander enabled:
   ```bash
   python run_eval.py --profile ppr-expander-enabled.toml --db eval-snapshot-crt042.db \
     2>&1 | tee eval-expander.log
   ```
   Extract MRR and P@5 from `eval-expander.log`. Record as `MRR_expander` and `P5_expander`.

4. **Gate assertions**:
   - `MRR_expander >= 0.2856`: no regression from baseline. **Mandatory gate.**
   - `P5_expander > 0.1115`: any improvement above baseline is the success signal.
   - `P95_delta = P95_expander - P95_baseline <= 50ms`: latency addition gate.
     Measured from `elapsed_ms` in Phase 0 `debug!` traces in `eval-expander.log`.

5. Record both measurements in the RISK-COVERAGE-REPORT.md:

```markdown
## Eval Gate Results
| Metric | Baseline | Expander | Delta | Gate |
|--------|----------|----------|-------|------|
| MRR | {MRR_baseline} | {MRR_expander} | {delta} | >= 0.2856: PASS/FAIL |
| P@5 | {P5_baseline} | {P5_expander} | {delta} | > 0.1115: PASS/FAIL |
| P95 latency | {baseline_ms}ms | {expander_ms}ms | {delta_ms}ms | <= 50ms: PASS/FAIL |
```

**If the eval gate fails (MRR regression)**:
Per SR-05 / R-07, the delivery lead must be named as the investigation owner in the PR
description. Failure path:
1. Confirm S1/S2 Informs edges are bidirectional (AC-00 gate was confirmed).
2. Confirm Phase 0 insertion point is before Phase 1 (AC-02 unit test passes).
3. Confirm BFS actually traverses edges (non-empty graph_expand result for a test query).
4. If all three pass: accept eval failure, leave `ppr_expander_enabled = false` as default,
   document in PR description that the feature ships behind the flag but the eval gate
   did not demonstrate improvement at current graph density.

**Note**: The feature flag exists precisely for this scenario. The expander is not broken
if eval fails — it means the graph density at eval time is insufficient. The flag stays
`false`; a subsequent eval after graph density increases can re-run the gate.

---

## R-06: Back-Fill Race Prevention (Procedural Check)

**Risk covered**: R-06 (High)

This is not a unit test — it is a procedural gate enforced before the eval snapshot is taken.

**Procedure** (mandatory before `unimatrix snapshot`):

1. Confirm the S1/S2 back-fill migration is committed:
   ```bash
   git log --oneline | grep -i "back-fill\|informs.*bidirectional" | head -5
   ```

2. Confirm the migration has run on the live DB:
   ```sql
   SELECT COUNT(*) FROM GRAPH_EDGES WHERE relation_type = 'Informs';
   -- Expected: > 0 if S1/S2 edges exist; count should be approximately 2x the
   -- pre-back-fill count (both directions now present).
   ```

3. Take the snapshot ONLY after confirming the above. Document in PR description:
   "Snapshot taken at commit {sha}, after S1/S2 back-fill migration verified complete."

**If the back-fill migration has not run**: do NOT take the eval snapshot. The eval with
single-direction Informs edges will produce non-reproducible results that cannot be compared
against a future post-back-fill run.

---

## Phase 0 Latency Measurement (From eval-expander.log)

The `debug!` trace emitted by Phase 0 contains `elapsed_ms`. During the eval run with
`RUST_LOG=unimatrix_server::services::search=debug`, extract latency distribution:

```bash
grep 'Phase 0 (graph_expand) complete' eval-expander.log \
  | grep -oP 'elapsed_ms=\K[0-9]+' \
  | sort -n \
  | awk 'BEGIN{n=0} {a[n]=$1; n++} END{print "P50="a[int(n*0.5)] " P95="a[int(n*0.95)] " P99="a[int(n*0.99)]}'
```

Record P50, P95, P99 elapsed_ms in the RISK-COVERAGE-REPORT.md. Compare P95 against the
50ms-over-baseline gate. If the baseline (expander=false) shows no Phase 0 trace, baseline
P95 Phase 0 cost is 0ms; the gate is simply `P95_expander <= 50ms`.

---

## Test Count Summary

| Check | AC | Risk |
|-------|-----|------|
| File existence (`test -f ppr-expander-enabled.toml`) | AC-22 | R-07 |
| TOML fields validation (python parse) | AC-22 | R-07 |
| `run_eval.py --profile ppr-expander-enabled.toml` exits 0 | AC-22 | R-07 |
| Eval MRR >= 0.2856 measurement | AC-23 | R-07 |
| Eval P@5 > 0.1115 measurement | AC-23 | R-07 |
| P95 latency delta <= 50ms extraction | AC-24 (supplemental) | R-04 |
| Back-fill committed before snapshot (procedural) | R-06 | R-06 |

**Total**: 3 shell checks + 4 eval measurements + 1 procedural gate = Component 4 complete.
