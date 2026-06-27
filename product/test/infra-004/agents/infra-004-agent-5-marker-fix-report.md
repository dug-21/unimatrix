# Agent Report: infra-004-agent-5-marker-fix (#859 marker PII-shape fix)

Implements the design-reviewed + product-reviewed fix for GH #859 (folded into
infra-004; prerequisite for AC-11 cold-model GREEN). Test-only; NO production
crates/ change, NO scanner change. B1/B2/B3 + N2 + N4 all folded in.

## Files modified / created
- `product/test/infra-001/scripts/isolation-probe-lib.sh` (modified) — added the
  construction-safe nonce helpers `_b36` / `_default_nonce` (B1/B3) and the PII-shape
  regression canary `assert_marker_pii_safe` (B2/N4). Housed here so the smoke script
  stays <=500 lines.
- `product/test/infra-001/scripts/multi-tenant-isolation-smoke.sh` (modified) —
  `derive_markers` and `warmup_barrier` RUN defaults both route through `_default_nonce`
  (single seam, cannot diverge); `assert_marker_pii_safe` invoked from
  `assert_markers_distinct` (four cell markers) and `warmup_barrier` (warmup marker),
  AFTER the R-12 charset guard.
- `product/test/infra-001/scripts/release-gate-isolation-logic-test.sh` (modified) —
  sources the new fixture and runs the (c) cases (kept <=500 lines).
- `product/test/infra-001/scripts/fixtures/isolation-nonce-logic-cases.sh` (created) —
  the (c) deterministic off-Docker cases: adversarial epoch/pid battery via the real
  default path (RUN UNSET, PID_OVERRIDE/EPOCH_OVERRIDE), default-path self-check passes
  (N3 false-positive guard), canary trips on a shaped regression, shared golden set.
- `crates/unimatrix-server/src/infra/scanning.rs` (modified) — N2 test-only anchor
  `test_scan_isolation_gate_golden_markers_pass` feeds the SHARED golden markers through
  `ContentScanner::global().scan()` and asserts `Ok`. Drift-proof coupling to the real
  scanner. No production code touched (`#[cfg(test)]` only).

## Nonce format
`RUN = <base36(pid)>x<base36(epoch)>` — pid and epoch base36-encoded SEPARATELY
(lowercase `0-9a-z`) and joined with the LETTER `x` (not a hyphen, not a digit).
Example: pid=18530, epoch=1782573915 -> `RUN=eaqxthaqu3` -> marker
`infra003-mcp-a-eaqxthaqu3`. Each component is <=6 chars (a 10-digit epoch is <=7),
so the 10-digit phone grouping cannot form within a component and the letter separator
blocks any run across the boundary; the `\d{3}-\d{2}-\d{4}` SSN shape needs hyphens
that never appear inside RUN. Shapes are structurally unreachable, not merely rare.

PRESERVED: R-12 charset `[a-z0-9-]`; R-18/R-02 pairwise non-substring guards; the
`infra003-{obs,mcp,warmup}-{a,b}-` prefixes the sqlite `query_for` predicates match;
all read-as-barrier predicates. The new canary runs AFTER the charset guard; the
`[2-9]` phone anchor means the leading `003` in `infra003` cannot be read as a phone
start (N3 false-positive guarded and tested).

## Tests (foreground)
- `release-gate-isolation-logic-test.sh`: **43 passed, 0 failed** (existing 39 + 4 new (c) cases)
- `release-gate-tristate-logic-test.sh`: **19 passed, 0 failed**
- `release-gate-logic-test.sh`: **15 passed, 0 failed**
- `release-gate-isolation-lane-static-test.sh`: **13 passed, 0 failed**
  -> existing 86 preserved + 4 new = 90 total
- Rust anchor `cargo test -p unimatrix-server --lib test_scan_isolation_gate_golden_markers_pass`:
  **1 passed, 0 failed**
- `shellcheck -S warning` on all 4 shell files (incl. new fixture): **clean (exit 0)**

## Flags (not fixed — adjacent / environment)
- Full `cargo build --workspace` / the `unimatrix` bin TEST binary link OOMs in this
  sandbox (`ld terminated with signal 9 [Killed]`) — an environment memory limit, NOT a
  code defect. The lib and lib-tests compile and pass; production code is unchanged
  (test-only addition). Run the Rust anchor via `--lib` (as above) to avoid the heavy
  bin link, or run on a higher-memory runner.
- Pre-existing rustfmt drift in `crates/unimatrix-server/src/uds/listener.rs` (surfaced
  by `cargo fmt -p unimatrix-server -- --check`) — unrelated to this change; flagging,
  not fixing (out of scope). `scanning.rs` is fmt-clean.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced #5355 (marker PII-safety lesson:
  MCP context_store scans, observe does not; derive letter-dominant + add a self-check),
  #5354 (R-18 collision logic-test pattern), #85 (ADR-002 content-scanning). Applied:
  confirmed the observe/MCP scan asymmetry and the charset-reachability reduction (only
  PhoneNumber + SSN reachable under `[a-z0-9-]`); extended to construction-safety and the
  bash-ERE/real-scanner coupling.
- Stored: nothing novel -- per project rule "bugs are GH issues, not lessons," the
  concrete gate defect stays on #859; #5355 already captures the generalizable trap
  completely. The bash-ERE-reduction + Rust-scanner-anchor coupling is a candidate
  proven pattern for the post-merge retro, not now (advisory until evidenced GREEN).
