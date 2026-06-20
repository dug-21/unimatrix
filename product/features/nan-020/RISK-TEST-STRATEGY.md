# Risk-Based Test Strategy: nan-020 — Product Documentation Currency (Doc-Test Enforcement)

Derived from `SCOPE.md`, `SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-09), `architecture/ARCHITECTURE.md`, ADR-001..ADR-004, and `specification/SPECIFICATION.md`.

**Historical grounding (Unimatrix):**
- **#5180** — self-skipping smoke wired into a gate must hard-fail on skip, keyed by distinct exit code; never green. The general rule the doc-test extension inherits.
- **#5183 (ADR-003, nan-019)** — Verify-By-Name gate contract: exit-code discrimination (0/1/3) **AND** an anchored positive run-marker. `run_smoke_gate` is the live instantiation nan-020 must not regress.
- **#5189** — pre-merge-provable test plan for a release shell-gate that only runs post-tag: split coverage PRE-MERGE-PROVABLE vs POST-TAG-CONFIRMABLE; drive the gate logic against a stub; truth-table {0,1,3,early-0} × {marker present/absent}; un-retryable assertions proven non-flaky locally (≥5 runs) AND discriminating (negative control must fail). **This shapes the entire coverage approach below** — the new bundle gates run only post-tag, so their *own correctness* must be proven pre-merge against stubs.
- **#4977** — a green run is vacuous if the harness silently skips on a path/resource mismatch; assert NON-SKIP evidence (timing delta, absence of skip log), not just exit code. Directly applies to R-07 hermeticity (now a REQUIRED gating obligation) — proven by a non-vacuous negative control.
- **vnc-041 AC-06 (negative control) + AC-02 (single-process round-trip deferred):** AC-06 proved a sentinel non-vacuous by a negative control that flips it red; nan-020's R-07 stale-credstore obligation is modeled on this shape. AC-02 **deferred** an in-process round-trip because **Rust-2024 forbids in-process `HOME`/env mutation** as unsound — the same hazard nan-020's R-07 hermeticity must respect at the process/shell boundary (#4903 cross-process self-spawn is the safe analogue).
- **#4473** — warn+continue posture masks missing failure-path tests; every new failure path needs an explicit negative test.
- **#5208** — IMAGE= prebuilt smoke must `docker pull` first; cross-runner cache miss false-fails (exit 4 arm). Invariant the extension must preserve.

The doc-test (Gates 5–7) executes **only post-tag** in `release.yml`. Therefore the highest-leverage coverage is *pre-merge-provable* tests of the gate logic itself driven by stubs — not waiting for a real container round-trip. Real round-trip = POST-TAG-CONFIRMABLE, accepted as PENDING pre-merge (NOT a gap), per #5189.

---

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | New bundle-attach failure (Gates 5–7) folds into existing `fail()`/exit-1 but a code path lets it **early-`exit 0`** or silently continue, so the gate greens though the documented path is broken (ADR-001 risk). | High | Med | **Critical** |
| R-02 | A new failure mode hard-fails but is **mis-attributed** — its message collides with or is indistinguishable from another step, so an operator/dev cannot tell "doc drift" from "route changed" from "binary absent" (SR-02/SR-09; ADR-001 message-distinctness is the only attributability mechanism). | High | Med | **High** |
| R-03 | The extension **regresses the nan-019 Gates 1–4** (per-slug observe smoke, the project's primary release guard): exit-code truth table 0/1/3/4 broken, `set -e` swallows `$?`, or the anchored terminal marker moves/duplicates (SR-04; #5183/#5189). | High | Med | **Critical** |
| R-04 | `node` absent on the host (or wrong/old node vs. the shipped client) makes Gate 6 `init --bundle` skip/false-fail; host JS/node version drift vs. the repo-checkout client masks a real operator break (ADR-002 host/container split; SR-03/NFR-4). | High | Med | **High** |
| R-05 | The **bundle blob handoff across the container→host boundary** corrupts/truncates/empties (stdout capture mixes stderr token-redacted echo, trailing newline, shell quoting), so `init --bundle` fails for handoff reasons, not doc-drift reasons (ADR-002 data flow Gate 5→6). | High | Med | **High** |
| R-06 | `client-bundle` subcommand is **renamed/moved/absent in the shipped image** (a future CLI change nan-020 doesn't own); the doc-test breaks on a surface it guards but doesn't control. Failure must name the command (SR-01/SR-02; A1 confirmed today at `main.rs:293`). | Med | Med | **Medium** |
| R-07 | **Non-hermetic CI / stale-credstore false-green:** Gate 6 writes `~/.unimatrix/<hash>/remote.json` + a `--project-dir` tree on the host; a **prior run's leftover credential/store false-greens** Gate 7 (a stale cred lets observe succeed without the fresh attach actually working). The #4977 vacuous-pass class on the host side. **REQUIRED, gating coverage obligation** (human-elevated from OQ to required) — its sentinel must be proven non-vacuous by a negative control, modeled on vnc-041 AC-06. If this slips the feature's whole purpose is defeated (a broken documented attach greens). | High | High | **Critical** |
| R-08 | The **executable-claim vs narrative-prose boundary rots**: a doc line that SHOULD be tested is classified as prose and drifts unguarded — the exact #768 class this feature exists to kill (ADR-003). Or over-broad classification adds a gate per command (gold-plating, C-3). | High | Med | **High** |
| R-09 | The doc rewrite/doc-test **locks onto a flag form the shipped CLI rejects**: docs/test use `--bundle <blob> --slug` but `init.js:353` RETIRES `--slug` on the bundle path (OQ-A). A `--slug`-bearing example errors at runtime; the doc-test would false-fail or the docs would re-create a broken example. | High | Med | **High** |
| R-10 | **README multi-occurrence miss** (OQ-B): the rewrite converges line 123 (`init --remote unimatrix-bundle:<blob>`) but misses line 587/130 (`init --remote <bundle>`) or other occurrences, so AC-02's "corrected example" is non-exhaustive and a broken form survives. | Med | High | **High** |
| R-11 | **Gate-logic correctness only confirmable post-tag** is treated as PENDING with no pre-merge proof, so a `set -e`/pipe/`pipefail` swallow (the #4873 class) silently turns the new gates' exit 1 into 0 and ships undetected (#5189 PRE-MERGE-PROVABLE core). | High | Med | **High** |
| R-12 | **AC-01 grep zero-occurrence passes while other claims stay stale** — the literal-string assertion (`501`/`W2-7`/curl-observe) is satisfied but un-enumerated drift remains in `docs/client-setup.md` (A4). False sense of currency. | Med | Med | **Medium** |
| R-13 | **uni-docs remit-widen scope creep into Feature 2**: the `.claude/` edit adds a drift-checker / gate / Phase-4 trigger (forbidden; C-5/SR-05), or "blast radius" is left under-defined so the agent either audits all of `docs/` (cost) or misses touched surfaces (drift persists) (SR-07; ADR-004). | Med | Med | **Medium** |
| R-14 | **N5 extension is process/doc-only** with no automated coverage — the "deployable→usable-as-documented" framing and the uni-docs remit text are human-owned; nothing tests that they were written correctly (ADR-004; AC-07/AC-08). | Low | Med | **Low (human-owned)** |
| R-15 | A **second image build / divergent boot config** is silently introduced by the extension despite D-2's reuse-in-place lock (the SCOPE/A2/OQ-D caveat), inflating release wall-clock and diverging from the operator topology (NFR-7). | Low | Low | **Low** |
| R-16 | **Legacy `--remote` mode documented-but-NOT-doc-tested:** the canonical `--bundle <blob>` chain is doc-tested (Gates 5–7); the legacy `--remote <url> --token <tok>` mode (AC-02, SCOPE "two attach modes") is **documented but exercised by NO scenario**. "usable-as-documented" (N5) could be mis-read as covering BOTH modes when only `--bundle` is guarded; `--remote` could silently drift like #768 did. | Med | Low | **Accepted residual** |

---

## Risk-to-Scenario Mapping

### R-01: New bundle-attach failure greens the gate (silent false-pass) — CRITICAL
**Severity**: High · **Likelihood**: Med · **Impact**: The feature's whole reason-for-being is defeated; a broken documented attach path ships with a green release gate, hard-stopping the next operator exactly as #768 did.

**Test Scenarios**:
1. **Stub-driven truth table (pre-merge, per #5189):** for EACH new failure mode in ADR-001's mapping table (emit rc≠0; emit empty/non-`unimatrix-bundle:` blob; `init --bundle` rc≠0; observe ≠204; per-slug store did not grow), drive the gate logic with a stub that forces that condition and assert the script reaches `fail()` → **exit 1**, prints NO terminal run-marker, and `run_smoke_gate` returns non-zero.
2. **Negative control / discrimination:** confirm the happy path (all stubs success) is the ONLY combination that yields exit 0 + marker. Any single forced failure must flip green→red (a tolerance band that hides one failure hides the defect — #5189).
3. **No early-`exit 0`:** inject a forced early `exit 0` before Gates 5–7 complete; assert the anchored marker grep (`grep -qx '\[783-smoke\] ALL GATES PASSED.*'`) fails the gate (AC-06).

**Coverage Requirement**: Every row of ADR-001's new-failure-mode→exit-1 table has a stub test proving it hard-fails AND suppresses the marker. Zero new failure modes reach exit 0. Proven pre-merge against stubs (does not require a real container).

### R-02: Mis-attributed failure (distinctness is the only attributability mechanism) — HIGH
**Severity**: High · **Likelihood**: Med · **Impact**: A red gate gives no actionable signal; a dev wastes a cycle chasing the wrong cause, or worse, edits docs when the route changed (SR-09) or the CLI was renamed (SR-02).

**Test Scenarios**:
1. **Message-prefix assertion:** for each new failure mode, assert the emitted `FAIL:` message contains its UNIQUE prefix from ADR-001's table (e.g. `client-bundle emit failed (rc=...)`, `client-bundle produced no/invalid bundle blob`, `init --bundle failed`, `documented bundle attach observe returned HTTP C (expected 204)`, `bundle-path observe did not land in per-slug store`).
2. **Doc-drift vs route-change discrimination:** simulate a non-204 observe and assert the message distinguishes it from an emit failure (SR-09) — two different forced conditions must produce two different, named messages.
3. **CLI-rename attribution:** simulate `client-bundle` rc≠0 (renamed/absent) and assert the message names the `client-bundle` command (SR-02), not a generic attach failure.

**Coverage Requirement**: Each ADR-001 message prefix is asserted by a test that forces exactly that condition; no two new failure modes share a message. Attributability is verified, not assumed.

### R-03: Regression of nan-019 Gates 1–4 / the load-bearing gate contract — CRITICAL
**Severity**: High · **Likelihood**: Med · **Impact**: Extending the script in place puts the project's primary release guard (per-slug observe smoke, #783) at risk; a regressed exit-code table or moved marker breaks every release, not just docs.

**Test Scenarios**:
1. **Truth-table invariance (pre-merge, #5189):** re-run the existing nan-019 stub truth-table {0,1,3,4} × {marker present/absent} against the extended script; only (0, marker) is green; 3→hard-fail, 4→hard-fail (IMAGE= acquisition arm, #5208), 1→fail all still hold.
2. **`set -e` / exit-code survival:** drive the extended script through `run_smoke_gate`'s `set +e; OUT=$(...); RC=$?` capture and assert RC SURVIVES by execution (exit 1 reads as 1, exit 3 as 3) — the #4873 swallow class, catchable only by running (#5189 step 2).
3. **Append-only / precondition ordering:** assert Gates 5–7 execute ONLY after Gate 4 passes; force a Gate-4 failure and assert the script fails at Gate 4 exactly as today, before any new code runs (ARCHITECTURE regression-assertion section).
4. **Single terminal marker:** assert `[783-smoke] ALL GATES PASSED` appears exactly once, is the last line, and the new gates print BEFORE it (not a second marker).
5. **`run_smoke_gate` unchanged:** diff-assert `release-gate-lib.sh::run_smoke_gate` is byte-unchanged (ADR-001: wrapper not modified).

**Coverage Requirement**: The full nan-019 exit-code truth table passes post-extension; marker is single/terminal; `run_smoke_gate` unmodified; Gates 1–4 are an unbroken precondition. Proven pre-merge.

### R-04: Host node absence / JS version drift breaks or masks Gate 6 (ADR-002) — HIGH
**Severity**: High · **Likelihood**: Med · **Impact**: Either a real operator break is masked (skip→false-green) or the gate false-fails on a provisioning issue. The host is the operator surrogate; its node must be present and compatible.

**Test Scenarios**:
1. **node-absent → hard-fail (NOT exit 3):** force `command -v node` to miss and assert the script `fail()`s (exit 1) with `node not available — the documented init --bundle path cannot be exercised` (ARCHITECTURE: node-absence is a mis-provisioned lane, same class as Docker-absent). Per #5180, a missing prerequisite must hard-fail, never green.
2. **node provisioning + preflight placement:** assert node is **explicitly pinned via `setup-node` in `release.yml`** (the host is the operator surrogate; node presence/version must not be left to runner defaults — a pinned provisioning step is the first line of defense against R-04). AND assert the script's own node preflight runs next to the Docker preflight (before any gate can no-op), so a provisioning miss (setup-node absent/failed) is still caught at script level and hard-fails early/attributably — defense in depth, not reliance on the YAML step alone.
3. **Client provenance:** assert Gate 6 invokes the **repo-checkout** client (`node packages/unimatrix/bin/unimatrix.js`), not a globally/npm-installed one, so the tested client is the shipped bytes (NFR-4); a version-drift check (the checkout's `package.json` version) is the operator surrogate's truth.

**Coverage Requirement**: node-absence is a hard-fail with a distinct message; the test exercises the repo-checkout client; no skip path on the host side greens the gate (#5180/#4977).

### R-05: Bundle blob handoff corruption across the container→host boundary (ADR-002) — HIGH
**Severity**: High · **Likelihood**: Med · **Impact**: `init --bundle` fails for handoff reasons (stderr/stdout mixing, trailing newline, quoting) and is mis-read as doc-drift — or worse, a malformed blob is silently accepted.

**Test Scenarios**:
1. **stdout/stderr separation:** assert Gate 5 captures only `client-bundle` **stdout** (the `unimatrix-bundle:` blob); the token-redacted URL/fingerprint **stderr** echo (`main.rs`) is NOT folded into the captured blob.
2. **Blob shape validation before handoff:** assert Gate 5 greps the captured blob for the `unimatrix-bundle:` prefix and `fail()`s with `client-bundle produced no/invalid bundle blob` on empty/wrong-prefix output (ADR-001 row).
3. **Quoting/whitespace robustness:** drive a blob containing shell-significant characters and a trailing newline through the capture→`init --bundle "$BUNDLE"` handoff; assert it survives intact (the blob is passed quoted; no word-splitting).
4. **Empty-capture guard:** force an empty stdout and assert hard-fail, not an empty-string passed to `init` that errors generically.

**Coverage Requirement**: The blob is validated for prefix and non-emptiness at the boundary; stderr is excluded; the handoff is quoting-safe. A corrupt handoff hard-fails with a Gate-5 (emit) message, not a Gate-6 (attach) message — preserving attributability (ties to R-02).

### R-06: `client-bundle` CLI rename/absence in the shipped image (coupling nan-020 doesn't own) — MEDIUM
**Severity**: Med · **Likelihood**: Med · **Impact**: A future server CLI change silently breaks the doc-test on a surface it guards; correct signal IF attributable, false-fail IF not.

**Test Scenarios**:
1. **Pinned-invocation assertion:** assert the doc-test invokes the exact verified form `unimatrix --project-dir /data client-bundle <slug>` (ARCHITECTURE Integration Surface; `main.rs:293,437`, A1 CONFIRMED).
2. **Rename-detection signal:** force a non-zero rc from the emit invocation and assert the failure names `client-bundle` (R-02 scenario 3) — a rename produces correct, attributable red, not a confusing attach error.
3. **Documented-command parity:** since `client-bundle` is itself a documented executable claim (README "Serving projects"), assert the form used by the doc-test matches the form documented (SR-02) — the test and the doc cannot drift apart.

**Coverage Requirement**: The exact invocation is pinned and asserted; a rename surfaces as a named `client-bundle` failure. This is correct signal, not a defect to mask.

### R-07: Non-hermetic CI / stale-credstore false-green — a prior run's leftover credential false-greens Gate 7 — CRITICAL
**Severity**: High · **Likelihood**: High · **Impact**: The #4977 vacuous-pass class on the host: a stale `~/.unimatrix/<hash>/remote.json` or `--project-dir` tree lets observe succeed without the *fresh* attach actually working, so a broken `init --bundle` greens — **defeating the feature's entire reason-for-being** (a green release gate over a broken documented path, exactly the #768 wound). **Human-elevated: this is a REQUIRED, gating coverage obligation, not a flagged Open Question.** PENDING here IS a gap.

**Test Scenarios** (scenario 3 is the load-bearing REQUIRED negative control — without it the hermeticity sentinel is vacuous):
1. **HOME / credstore isolation:** assert Gate 6 runs with an isolated `HOME` (or `--project-dir` under a fresh throwaway dir) so `~/.unimatrix/<hash>/` cannot inherit a prior run's credential. The credstore must be HOME-isolated or cleaned.
2. **Throwaway project-dir:** assert `--project-dir` points at a fresh per-run temp tree, cleaned on entry (not just exit, so a crashed prior run cannot poison the next).
3. **REQUIRED NEGATIVE CONTROL (discrimination, #4977; modeled on vnc-041 AC-06):** **pre-seed / poison** the credstore with a stale credential (a valid-looking `~/.unimatrix/<hash>/remote.json` from a prior good run) AND point the doc-test at a **deliberately broken attach** (e.g. force `init --bundle` to no-op or target a wrong/dead endpoint); assert **Gate 7 STILL FAILS**. This proves the gate measures the *fresh* attach (the new write landing in the per-slug store this run), NOT residual `~/.unimatrix/<hash>/` state. vnc-041's AC-06 is the proven shape: a negative control that flips the sentinel red is the ONLY thing that proves the green is non-vacuous. **A test that passes with a pre-seeded cred + a broken attach is a vacuous sentinel and fails this risk's coverage requirement.**
4. **Non-skip / fresh-write evidence (#4977):** assert Gate 6/7 emit positive evidence the attach actually ran *this run* — the pinned `Ping` succeeded and the per-slug store grew by the **new** write (delta, not absolute count) — not merely "exit 0".

**Coverage Risk — process/shell-boundary hermeticity (vnc-041 AC-02 hazard):** the hermeticity (isolated `HOME`/credstore) MUST be achieved at the **process / shell boundary** — i.e. by launching the attach under a fresh `HOME=$(mktemp -d)` environment for that invocation, NOT by mutating `HOME` inside a single live process. vnc-041's AC-02 **deferred its single-process round-trip precisely because Rust-2024 forbids in-process env (`set_var`) mutation** as unsound (the cross-process self-spawn pattern at #4903 is the safe analogue). Flag the same hazard here: a doc-test that tries to set/reset `HOME` within one process to isolate the credstore is unsound on the Rust side and will either fail to compile/run or silently not isolate (re-opening the false-green). Hermetic isolation is a **shell-level / per-invocation env** concern, validated at the process boundary. If the round-trip cannot be done hermetically in a single process, accept the split exactly as vnc-041 AC-02 did and document it — do NOT fake isolation in-process.

**Coverage Requirement (REQUIRED / gating):** HOME/credstore/project-dir are per-run hermetic at the **process/shell boundary**; the REQUIRED negative control (stale pre-seeded cred + broken attach) **FAILS Gate 7**, proving the sentinel non-vacuous (vnc-041 AC-06 shape). Repeated CI runs cannot false-green from residue. This coverage is PRE-MERGE-PROVABLE against a stub broken-attach (drive the broken-attach + pre-seeded-cred condition and assert exit 1) — classifying it PENDING is a gap (#5189).

### R-08: Executable-claim classification contract rots (the #768 class) — HIGH
**Severity**: High · **Likelihood**: Med · **Impact**: A line that should be doc-tested is filed as prose and drifts unguarded → another #768; or over-broad classification adds a gate per command (gold-plating, C-3).

**Test Scenarios**:
1. **Worked-example conformance:** assert the rewritten `docs/client-setup.md` classifies each line of ARCHITECTURE's worked-example table correctly — the three executable claims (`client-bundle` emit, `init --bundle`, hook-client observe) ARE on the doc-test's canonical chain; the prose rows (fingerprint rationale, TLS/port notes, token-rotation runbook) carry only the verified-on stamp.
2. **Canonical-chain coverage:** assert the doc-test's tested set is exactly the one canonical claim chain (AC-03), not a gate-per-command (over-broad guard, C-3 / ADR-003).
3. **Under-broad escape hatch:** assert any executable command present in the attach docs is reducible to the canonical chain; if a NEW non-reducible command appears, it is a signal the chain is incomplete → raised to design, NOT left untested by default (ADR-003 boundary discipline).
4. **Classification is operational, not prose-judgment:** verify the three-part ADR-003 test (runnable command / behavioral correctness / on-or-reducible-to canonical chain) is applied — guarding against the "looks correct line-by-line but fails only when run" #768 failure that human review cannot catch.

**Coverage Requirement**: Every executable claim in the rewritten docs is on the doc-tested canonical chain; every prose line is correctly excluded; no command on the attach path is unguarded; no per-command gate added. The classification contract itself is verified against the worked example.

### R-09: Doc/test locks onto `--slug` the shipped CLI rejects on the bundle path (OQ-A) — HIGH
**Severity**: High · **Likelihood**: Med · **Impact**: A `--bundle <blob> --slug` example errors at runtime (`init.js:353` retires `--slug`); the rewrite would ship a broken example (the very #768 class) or the doc-test would false-fail.

**Test Scenarios**:
1. **Docs follow code:** assert both README and `docs/client-setup.md` document `init --bundle <blob>` with **NO** `--slug` (FR-5/OQ-A); grep-assert zero occurrences of `--slug` paired with `--bundle` (AC-02 verification method).
2. **Doc-test follows code:** assert Gate 6 invokes `init --bundle "$BUNDLE"` without `--slug`.
3. **SCOPE-vs-shipped reconciliation:** confirm the SCOPE/AC-02 parenthetical "(+ `--slug`)" is explicitly dropped for the bundle mode (the architecture/spec resolved this; the test asserts the as-shipped truth, not the SCOPE phrasing).
4. **`--slug` still valid elsewhere:** assert `--slug` remains documented only as a server-side `project register` / `client-bundle` argument, not an `init --bundle` argument (so the retirement is scoped, not a blanket removal).

**Coverage Requirement**: No `--slug` on any `init --bundle` form in docs or the doc-test; `--slug` retained for its valid server-side uses. The as-shipped surface is authoritative.

### R-10: README multi-occurrence miss (OQ-B) — HIGH
**Severity**: Med · **Likelihood**: High · **Impact**: A broken `init --remote <bundle>` form survives at line 587/130 even after line 123 is fixed; AC-02's "corrected example" is non-exhaustive.

**Test Scenarios**:
1. **Exhaustive enumeration:** grep-assert ZERO occurrences of `init --remote unimatrix-bundle:` AND zero `init --remote <bundle>`-style bundle-fed-to-`--remote` forms anywhere in README (not just line 123) — OQ-B requires every occurrence converge on canonical `init --bundle <blob>`.
2. **Regex coverage of both phrasings:** assert the AC-02 grep covers both known phrasings (line 123 `init --remote unimatrix-bundle:<blob>` and line 587/130 `init --remote <bundle>`); a single-line check is insufficient.
3. **Positive presence:** grep-assert the canonical `init --bundle <blob>` IS present in both README and client-setup (AC-02 positive method).

**Coverage Requirement**: AC-02's grep is multi-occurrence and regex-based, not line-pinned; zero broken bundle-via-`--remote` forms remain; canonical form present in both files.

### R-11: Gate-logic correctness un-provable pre-merge → swallow class ships (#5189) — HIGH
**Severity**: High · **Likelihood**: Med · **Impact**: Because Gates 5–7 run only post-tag, a `set -e`/pipe/`pipefail` swallow (#4873) could silently turn exit 1 into 0 and reach release undetected.

**Test Scenarios**:
1. **Sourceable-spine reuse (#5189 step 1):** assert the capture/case/grep bytes are exercised by the SAME sourceable helper in both the YAML step and the pre-merge test — never re-typed (the divergence verify-by-name exists to kill).
2. **Exit-code survival by execution (#5189 step 2):** drive the new gate paths with a stub exiting chosen codes and assert RC survives capture by RUNNING (not reading YAML).
3. **Pre-merge core is REQUIRED, not PENDING:** classify the new-gate truth table, blob-validation, hermeticity negative control, and the un-retryable store-grew assertion as PRE-MERGE-PROVABLE; PENDING on these IS a gap (#5189). Real hosted-runner round-trip is POST-TAG-CONFIRMABLE (accepted PENDING, phrased "configured + verified locally; GH execution confirmed post-tag").
4. **Un-retryable non-flaky + discriminating:** the store-grew assertion (no `|| retry`) must be proven non-flaky locally (≥5 runs) AND a negative control (broken attach) must actually fail it (#5189).

**Coverage Requirement**: Gate logic proven pre-merge via stub truth table sharing bytes with YAML; exit-code survival proven by execution; un-retryable assertions proven non-flaky + discriminating; post-tag items correctly labeled PENDING (not asserted as run).

### R-12: AC-01 grep passes while other claims stay stale (A4) — MEDIUM
**Severity**: Med · **Likelihood**: Med · **Impact**: The literal-string assertion greens while un-enumerated drift remains; false currency.

**Test Scenarios**:
1. **AC-01 literal assertions:** `grep -c -E '501|W2-7' docs/client-setup.md` → 0; no fenced block matching `curl .*/observe`; positive grep finds `init --bundle` and `/v1/{slug}/observe` (FR-2 verification).
2. **Obsolete-model sweep:** assert the rewritten file does not instruct "no local binary required / curl-based shell hooks" as the telemetry mechanism (FR-3) — a check beyond the three literal strings.
3. **Enumeration confirmation:** verify the rewrite addressed the enumerated #768 defect set (6× 501/W2-7 callouts, three curl hook blocks, broken `--remote` example) and that #767-owned (`README:62` ONNX) and self-healed items are correctly excluded — so "zero literal occurrences" is not the only evidence of currency.

**Coverage Requirement**: AC-01 grep passes AND the obsolete-model/curl-hook prose is gone AND the enumerated defect set is confirmed addressed; currency is not inferred from literal strings alone.

### R-13: uni-docs remit-widen scope creep / under-defined blast radius (SR-05/SR-07) — MEDIUM
**Severity**: Med · **Likelihood**: Med · **Impact**: Feature 2 leaks in (forbidden, C-5), or "blast radius" is too vague so the agent over-audits or misses surfaces.

**Test Scenarios**:
1. **Authorship-text-only fence:** inspect `.claude/agents/uni/uni-docs.md` diff — scope line widened README→all of `docs/`; NO drift-checker, NO CI gate, NO Phase-4 trigger redesign added (FR-19/AC-07).
2. **Blast-radius definition present:** assert the definition states authorship is blast-radius-scoped (surfaces a change touches) AND states the full-tree-audit non-goal explicitly (FR-18/AC-07/SR-07).
3. **Relaxation bounded:** assert the "no source code reading" relaxation is narrow (read the CLI surface a touched doc documents), not a general code-audit license (ARCHITECTURE ADR-004); prompt-injection defense + "document only what is shipped" rules retained (FR-20).

**Coverage Requirement**: Diff is authorship-text only; blast radius + full-tree-audit non-goal both stated; relaxation bounded; Feature-2 machinery absent. Inspection-based (no automated gate).

### R-14: N5 extension + remit text are human-owned with no automated coverage — LOW (human-owned)
**Severity**: Low · **Likelihood**: Med · **Impact**: The "deployable→usable-as-documented" framing or the remit text could be written wrong with nothing to catch it. **This is a process risk; flag as human-owned — no automated coverage exists or should be built (gold-plating, C-3/ADR-004).**

**Test Scenarios**:
1. **Inspection (AC-08):** the artifact referencing N5 reads "deployable-as-released → usable-as-documented", names the doc-test as the docs-layer guard, N5 status unchanged, no new NFR/capability id minted.
2. **Inspection (AC-07):** the uni-docs remit text is internally consistent (scope, constraints, self-check all admit `docs/`).

**Coverage Requirement**: Human/reviewer inspection against AC-07/AC-08. Explicitly NOT machine-checked. Flagged for the human gate, not the doc-test.

### R-15: Silent second image build / divergent boot config despite D-2 lock — LOW
**Severity**: Low · **Likelihood**: Low · **Impact**: Release wall-clock inflates; topology diverges from the operator's.

**Test Scenarios**:
1. **Reuse-in-place assertion:** assert Gates 5–7 reuse the SAME booted container, volume, slug, port, token, and cert as Gates 1–4 — no second `docker run`/build (ARCHITECTURE D-2 caveat; NFR-7).
2. **No-new-script assertion (NFR-1):** file-count delta under `product/test/infra-001/scripts/` is 0 new scripts; the round-trip lives inside `docker-http-posture-smoke.sh` (AC-04). A sibling is permitted ONLY if the divergence caveat (FR-10/OQ-D) is triggered AND documented.

**Coverage Requirement**: Extension reuses the single boot; zero new scripts unless the divergence caveat is documented.

### R-16: Legacy `--remote` mode documented-but-NOT-doc-tested — ACCEPTED RESIDUAL (consciously accepted gap)
**Severity**: Med · **Likelihood**: Low · **Impact**: AC-02 documents BOTH `--bundle <blob>` (canonical) and the legacy `--remote <url> --token <tok>` (F3 direct attach), but only the `--bundle` chain is exercised by the doc-test (Gates 5–7, AC-03). The `--remote` path is documented and could drift unguarded — the exact #768 class — while "usable-as-documented" (N5) is satisfied only for the `--bundle` half.

**Human decision (this revision): CONSCIOUSLY ACCEPTED GAP — NOT a coverage gap to close.** `--bundle` is the canonical attach path (vnc-038 dumb-client); `--remote` is legacy/unused (F3). Building a second doc-test round-trip for `--remote` is cost without operator value and cuts against the minimal-mechanism / no-gold-plating posture (C-3).

**Mitigation (the only one — recorded so the acceptance is auditable):** the docs (README + `docs/client-setup.md`) must mark the `--remote <url> --token <tok>` form **"legacy"** explicitly, so a reader (and any future N5 reviewer) understands `--remote` is not the canonical, doc-tested path. This is the boundary that prevents "usable-as-documented" from being mis-read as covering BOTH modes.

**Test Scenarios**: NONE for the `--remote` round-trip (accepted). The mitigation IS testable by inspection:
1. **"legacy" label present (inspection):** assert the `--remote <url> --token <tok>` documentation in README and `docs/client-setup.md` carries an explicit "legacy" marker, distinguishing it from the canonical `--bundle` path (AC-02 is satisfied for documentation correctness; this adds the legacy-distinction guard).
2. **Canonical-chain scope statement:** the doc-test / N5 framing names the `--bundle` chain as THE doc-tested path — `--remote` is documented-only by design, not a silent omission.

**Coverage Requirement (acceptance, not closure):** `--remote` round-trip is **accepted as untested**; the sole required mitigation is the docs marking `--remote` "legacy." No `--remote` doc-test scenario is owed. Recorded here so "usable-as-documented" is never misread as covering both modes.

---

## Integration Risks

The doc-test is almost entirely an integration test across boundaries — this is where the bugs live:

- **Container→host blob handoff (R-05):** the single most fragile new seam. stdout (blob) vs stderr (token-redacted echo) separation, quoting, trailing-newline, empty-capture. Covered by R-05 + R-02 attribution.
- **Rust→JS runtime split (R-04, ADR-002):** two runtimes in two places; the image has no node, the host is the operator surrogate. Coverage must confirm BOTH runtimes are present where the design says, and that node-absence hard-fails.
- **Gates 1–4 → Gates 5–7 ordering (R-03):** the existing per-slug-observe assertion is the literal precondition of the new gates; ordering must be enforced and tested.
- **Doc ↔ shipped-CLI coupling (R-06, R-09):** the doc-test couples docs to `client-bundle` and `init --bundle` surfaces nan-020 doesn't own; coverage ensures breakage is attributable and the as-shipped flag form (no `--slug`) is used.
- **Script ↔ `run_smoke_gate` wrapper (R-03, R-11):** the wrapper is UNCHANGED; new failures fold into exit 1 (ADR-001). Coverage proves the wrapper still discriminates 0/1/3/4 and the bytes are shared (not re-typed) between YAML and test.

## Edge Cases

- Empty / whitespace-only / wrong-prefix bundle blob (R-05).
- `client-bundle` writes the blob but also a trailing diagnostic to stdout (must still validate prefix) (R-05).
- node present but wrong major version vs. the repo-checkout client (R-04).
- Stale `~/.unimatrix/<hash>/` from a crashed prior run (clean on entry, not just exit) (R-07).
- A `--project-dir` collision between concurrent CI lanes (hermetic per-run dir) (R-07).
- Observe returns 200/500/501 instead of 204 — message must distinguish doc-drift from route change (R-02/SR-09).
- Per-slug store grows by 0 (attach silently no-ops) vs. grows but in the wrong slug's db (R-07 negative control).
- README has a THIRD undiscovered bundle phrasing beyond lines 123 and 587/130 (R-10 — use a regex, not line pins).
- Early `exit 0` injected anywhere before the terminal marker (R-01 scenario 3 / AC-06).

## Security Risks

nan-020 adds **no application code** and **no new untrusted input surface** — it documents and tests shipped behavior (Spec "NOT in Scope": no CLI/route change). The relevant security surface is the test harness itself:

- **Bundle blob as untrusted-ish data in the harness (low):** the blob is captured from the container's stdout and passed to `init --bundle` on the host. It is server-generated, not attacker-supplied, but the harness MUST NOT `eval`/word-split it — pass it quoted (R-05 scenario 3). Blast radius: a malformed blob should fail attach diagnosably, never execute.
- **Credential residue on the CI host (R-07):** Gate 6 writes a real out-of-tree credential (`~/.unimatrix/<hash>/remote.json`). On shared/self-hosted runners this is a small credential-leak + cross-run-contamination surface. Mitigation = HOME isolation + per-run cleanup (R-07); this also closes the false-green (the security and correctness mitigations coincide).
- **Token redaction preserved:** Gate 5 must capture only stdout; the stderr token-redacted URL/fingerprint echo (`main.rs`) must NOT be logged into CI output unredacted (R-05 scenario 1). Blast radius: a logged bearer token in CI logs.
- **uni-docs remit relaxation (R-13):** the "may read CLI source" relaxation must stay bounded to the touched surface; an over-broad code-read license + retained prompt-injection defense is the relevant agent-surface check (FR-20).

No path-traversal / injection / deserialization risk is introduced (no new parser, no new route, no new file-path-from-input handling).

## Failure Modes (expected behavior when a risk materializes)

| Trigger | Expected behavior |
|---------|-------------------|
| Docker absent | `exit 3` → `run_smoke_gate` HARD-fails the job (never green) — #5180/AC-05 |
| node absent on host (setup-node pin missing/failed) | `fail()` exit 1, `node not available — ...` message; hard-fail (NOT exit 3 — it's a mis-provisioned lane). node is pinned via `setup-node` in `release.yml`; the script preflight is the backstop if provisioning slips (R-04 sc.2) |
| `client-bundle` rc≠0 (renamed/absent) | `fail()` exit 1, message NAMES `client-bundle` (attributable) |
| Empty/invalid blob | `fail()` exit 1 at Gate 5 (emit), distinct from attach failure |
| `init --bundle` rc≠0 | `fail()` exit 1, `init --bundle failed (rc=...)` |
| observe ≠ 204 | `fail()` exit 1, names the HTTP code, distinguishes doc-drift from route change |
| per-slug store did not grow | `fail()` exit 1, `bundle-path observe did not land in per-slug store` |
| stale credential residue | hermetic isolation prevents false-green; negative control with broken attach STILL fails Gate 7 |
| early `exit 0` before marker | anchored marker grep fails the gate (AC-06) |
| Gate 4 (nan-019) regresses | script fails at Gate 4 before any new code runs; original behavior unchanged |

**Invariant across all:** no failure mode reaches exit 0; every failure prints a distinct, attributable message; the terminal run-marker prints ONLY on full success.

## Scope Risk Traceability

| Scope Risk | Architecture Risk(s) | Resolution / Where Covered |
|-----------|---------------------|----------------------------|
| **SR-01** — `client-bundle` Rust subcommand absent/renamed/wrong-path in shipped image | R-06 (also R-02) | A1 CONFIRMED at design (`main.rs:293,437`, ENTRYPOINT in image). Covered: pinned-invocation assertion + named-failure on rename (R-06 sc.1–2; R-02 sc.3). |
| **SR-02** — doc-test coupled to a Rust CLI surface nan-020 doesn't own; rename breaks it silently | R-06, R-02 | Covered: emit failure NAMES `client-bundle` (R-02 sc.3); documented-command parity (R-06 sc.3) — break is correct, attributable signal. |
| **SR-03** — two-runtime path; both runtimes must be present/compatible in the test environment | R-04 | A3 CORRECTED at design (image ships Rust binary, NOT JS; JS runs on host per ADR-002). Covered: node-absent hard-fail + repo-checkout client provenance (R-04 sc.1–3); NFR-4. |
| **SR-04** — extend-in-place may regress nan-019 exit-code contract / primary release guard | R-03 (also R-11) | Covered: truth-table invariance, `set -e` survival, append-only ordering, single marker, wrapper-unchanged diff (R-03 sc.1–5). |
| **SR-05** — `.claude/` remit edit a scope-creep magnet toward Feature 2 | R-13 | Covered: authorship-text-only fence; no drift-checker/gate/trigger (R-13 sc.1, FR-19/AC-07). |
| **SR-06** — executable-claim vs prose distinction load-bearing; over/under-broad both fail | R-08 | ADR-003 made it an operational contract (3-part test + worked example). Covered: worked-example conformance, canonical-chain-only, under-broad escape hatch (R-08 sc.1–4). |
| **SR-07** — blast radius under-specified → over-audit or missed surfaces | R-13 | ADR-004 defines blast radius (surfaces a change touches) + full-tree-audit non-goal. Covered: definition-present + non-goal-present assertions (R-13 sc.2). |
| **SR-08** — a new skip path that early-`exit 0`s re-creates the false-green blind spot | R-01, **R-07 (Critical)** (also R-04, R-11) | ADR-001 maps every new failure to `fail()` exit 1 with distinct messages. Covered: stub truth table + no-early-exit-0 + node/credstore hard-fails (R-01 sc.1–3, R-04 sc.1–2, R-07 sc.1–4). **R-07's stale-credstore false-green is now a REQUIRED gating obligation with a non-vacuous negative control (vnc-041 AC-06 shape).** |
| **SR-09** — underlying route/flag shift mis-attributed as doc drift | R-02 (also R-09) | Covered: observe-code message distinguishes doc-drift from route change (R-02 sc.2); as-shipped flag form locks the `--slug` ambiguity (R-09). |
| _(no scope origin)_ — legacy `--remote` mode documented-but-not-doc-tested | R-16 | **NOT a scope risk; introduced this revision as a consciously ACCEPTED residual** (human decision). Bundle is canonical + doc-tested; `--remote` is legacy/unused. Mitigation = docs mark `--remote` "legacy" (R-16 sc.1). No `--remote` round-trip coverage owed. |

All nine scope risks are covered by at least one architecture risk and test scenario. None accepted/dropped. R-16 is a newly-introduced accepted residual with no scope-risk origin (documented above).

## Coverage Summary

| Priority | Risk Count | Risk IDs | Required Scenarios |
|----------|-----------|----------|--------------------|
| **Critical** | 3 | R-01, R-03, **R-07** | 13 (stub truth tables: new-failure-mode coverage + nan-019 regression invariance + REQUIRED stale-credstore negative control) |
| **High** | 7 | R-02, R-04, R-05, R-08, R-09, R-10, R-11 | 24 |
| **Medium** | 3 | R-06, R-12, R-13 | 9 |
| **Low** | 2 | R-14 (human-owned), R-15 | 4 |
| **Accepted residual** | 1 | R-16 (legacy `--remote` not doc-tested) | 2 (mitigation inspection: "legacy" label) — no round-trip coverage owed |
| **Total** | **16** | — | **52** |

**Coverage posture (per #5189):** the Critical + High gate-logic risks (R-01, R-03, **R-07**, R-11) are **PRE-MERGE-PROVABLE** via stub-driven truth tables sharing bytes with the YAML wrapper — PENDING on these IS a gap. **R-07's stale-credstore negative control (pre-seeded cred + broken attach → Gate 7 STILL fails, vnc-041 AC-06 shape) is now a REQUIRED gating obligation, provable pre-merge against a stub broken-attach.** The real hosted-runner round-trip (R-04/R-05/R-07 in a live container) is **POST-TAG-CONFIRMABLE** — accepted PENDING pre-merge, phrased "configured + verified locally; GH execution confirmed post-tag," never asserted as executed fact before it runs (#4796). **Process/shell-boundary hermeticity hazard (R-07, vnc-041 AC-02):** isolation must be at the process/env boundary, NOT in-process `HOME` mutation (Rust-2024 forbids it as unsound, #4903 self-spawn is the safe analogue) — flagged as a coverage risk. R-14 is human-owned (no automated coverage by design, C-3). R-16 is a consciously accepted residual (legacy `--remote` not doc-tested; mitigation = docs mark it "legacy").

## Knowledge Stewardship
- Queried: `context_search` for false-green/exit-code/skip lessons → #5180 (self-skip→hard-fail rule), #5183 (ADR-003 verify-by-name contract), #5189 (pre-merge-provable shell-gate plan), #4977 (vacuous-pass / assert-non-skip), #4473 (warn+continue masks failure-path tests), #5208 (IMAGE= pull/exit-4 arm). All directly applied — they shaped the pre-merge-provable vs post-tag-confirmable coverage split, the stub truth-table requirement, and the R-07 hermeticity negative-control.
- Revision pass (this edit) — queried vnc-041 for the AC-06 negative-control / AC-02 single-process-round-trip-deferred shape → ADRs #5235–#5239 (per-slug/global seed seam) plus #4903 (cross-process determinism via test-binary self-spawn, the safe analogue to Rust-2024's forbidden in-process env mutation). Applied to R-07's REQUIRED negative control and the process/shell-boundary hermeticity hazard.
- Stored: nothing novel — the recurring "self-skipping CI gate must hard-fail, prove pre-merge against stubs" pattern is already captured at #5180/#5183/#5189 (nan-019); the "negative control proves a hermeticity/skip sentinel non-vacuous" and "Rust-2024 forbids in-process env mutation → isolate at process boundary" patterns are already captured at #4977 and #4903 respectively. nan-020 is a faithful reuse, not a new cross-feature pattern. Re-storing would duplicate. If a doc-test-specific pattern (executable-claim classification as a testable contract) recurs in Feature 2, store it then.
