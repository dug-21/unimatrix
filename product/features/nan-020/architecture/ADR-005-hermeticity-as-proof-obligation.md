## ADR-005: Host-Side Bundle-Consume Hermeticity Is a Proof Obligation (HOME-isolated credstore + throwaway --project-dir), Provable by a Negative Control, Achieved at the Process Boundary

### Context

Gate 6's `init --bundle` (the JS host leg, ADR-002) writes a REAL out-of-tree credential to
`~/.unimatrix/<projectHash>/remote.json` and a `--project-dir` working tree on the smoke host
(`init.js:362–518`; credstore is HOME-keyed per vnc-039 #5125). Gate 7 then fires a hook event
and asserts the per-slug store grew. The hazard (R-07, was OQ-C; the #4977 vacuous-pass class):
**a PRIOR run's leftover credential in `~/.unimatrix/<hash>/` can satisfy Gate 7's
MCP/round-trip leg WITHOUT a fresh attach.** The hook client resolves config from the
HOME-keyed store (#5123/#5125); if a stale `remote.json` survives, observe can POST and the
store can grow even when *this run's* `init --bundle` did nothing useful. Gate 7 then greens.

This is not a hygiene nit — it is self-defeating. nan-020 exists to close the
"green-gate-but-broken-documented-path" blind spot (#768). A doc-test that false-greens off
residual state REPRODUCES THE EXACT BLIND SPOT IT EXISTS TO CLOSE. Hermeticity here is
therefore a PROOF OBLIGATION, not a best-effort cleanup — the gate must measure the *fresh*
attach, never residue. OQ-C had deferred this to "the tester / test plan, not an architecture
blocker"; the human review correctly elevated it: WHETHER the gate measures fresh state is an
architectural property of the consume step, so it is decided here.

Two prior decisions shape the mechanism:

- **#5246 (vnc-041 lesson, AC-06 shape):** a gate/sentinel that can pass by residual reasoning
  alone is vacuous; prove it with an EMPIRICAL sentinel AND a NEGATIVE CONTROL — assert it
  fires when it should (delta>0) AND does-NOT pass when it should not (a deliberately broken
  condition must turn the gate red). vnc-041 used this to prove its seed sentinel wasn't
  vacuous; we apply the same shape to prove the hermeticity assertion isn't vacuous.
- **vnc-041 AC-02 hazard (Rust-2024 HOME mutation):** vnc-041 had to DEFER an in-process
  single-process round-trip because Rust-2024 makes `std::env::set_var("HOME", …)` `unsafe`/
  forbidden, so HOME could not be re-pointed inside the running process. The SAME constraint
  binds here. The mechanism MUST therefore isolate HOME at the **process/shell boundary** —
  by setting `HOME=` (and `--project-dir`) in the environment of the **spawned child** (`node
  … init --bundle`), never by mutating the harness's own process HOME in place.

### Decision

**The host-side consume step (Gates 6–7) runs in a per-run, HOME-isolated, throwaway sandbox
established at the SHELL/PROCESS boundary, and its hermeticity is itself proven by a negative
control. No prior `~/.unimatrix/<hash>/` state can satisfy the gate.**

1. **Per-run throwaway sandbox, set on the child, not the parent.** Before Gate 6 the script
   creates a fresh temp root, e.g. `SANDBOX="$(mktemp -d)"`, and invokes the JS leg with an
   ISOLATED HOME and project-dir scoped to that root, exported only into the spawned child:

   ```sh
   HOME="$SANDBOX/home" \
     node packages/unimatrix/bin/unimatrix.js init --bundle "$BUNDLE" \
       --project-dir "$SANDBOX/proj"
   ```

   With `HOME=$SANDBOX/home`, the HOME-keyed credstore resolves to
   `$SANDBOX/home/.unimatrix/<hash>/remote.json` — a path that cannot exist before this run.
   The same isolated `HOME`/`--project-dir` are used for the Gate 7 hook fire so the hook
   client reads THIS run's credstore, not the runner's real `~/.unimatrix/`. This sidesteps
   the Rust-2024 HOME-mutation constraint by construction: HOME is set in the child's
   environment by the shell, the harness never calls `set_var` and never re-points its own
   HOME in process (vnc-041 AC-02 hazard avoided — it is a process-boundary isolation, the one
   form that IS achievable).

2. **Clean-on-ENTRY, not just on exit.** The sandbox is created fresh and removed on entry if a
   collision exists (and on `trap` exit). Cleaning only on exit is insufficient: a crashed
   prior run leaves poison that the next run would inherit. (Because HOME is redirected into a
   `mktemp -d` root, normal runs are already collision-free; the entry guard + trap make a
   crashed-run residue impossible too.)

3. **The hermeticity is itself proven by a NEGATIVE CONTROL (the #5246 / vnc-041 AC-06 shape).
   This is the load-bearing half — isolation you cannot prove is vacuous.** A pre-merge test
   (the gate-logic test that drives the script against stubs, R-11) MUST:
   - **Poison + break:** pre-seed a STALE valid-looking credential into the location a
     non-isolated run would read (the runner's real `~/.unimatrix/<hash>/remote.json`), AND
     point Gate 6 at a deliberately BROKEN attach (e.g. `init --bundle` made to fail / produce
     no fresh credential).
   - **Assert STILL-RED:** Gate 7 MUST still FAIL. If isolation works, the stale cred is
     unreachable (wrong HOME) so the broken attach yields no working credstore and observe
     does not succeed → the gate is red. A version of the harness WITHOUT the HOME isolation
     would pass this scenario (the stale cred satisfies observe) — that PASS is the vacuous
     false-green the negative control is designed to catch. The control therefore proves the
     gate measures the fresh attach, not residue.
   - **Positive twin:** the happy path (real fresh attach into the isolated sandbox) is the
     ONLY combination that greens Gate 7 (delta>0 from THIS run's write), mirroring #5246's
     "fires when it should AND not when it shouldn't" pair.

4. **Non-skip / positive-evidence (#4977).** Gate 6/7 emit positive evidence the attach
   actually ran THIS run — the pinned `Ping` succeeded and the per-slug store grew by the NEW
   write — not merely "exit 0". A store delta of 0, or a delta attributable to a pre-existing
   credential, is a fail.

### Consequences

- Easier: the gate now MEASURES the documented fresh attach, immune to cross-run residue; the
  feature cannot reproduce the #768 blind spot it exists to close; the negative control gives a
  durable, pre-merge-provable guarantee the assertion is non-vacuous (R-07, R-11).
- Easier: the isolation also closes the credential-residue security surface on shared/
  self-hosted runners (a real out-of-tree bearer no longer accumulates under the runner's real
  HOME) — the correctness and security mitigations coincide.
- Harder: the script must manage a temp sandbox lifecycle (create/clean-on-entry/trap-cleanup)
  and thread `HOME`/`--project-dir` into the child; the pre-merge test must stand up the
  poison-and-break negative control. Accepted: this is the proof obligation, not gold-plating.
- Constraint on implementers: HOME isolation MUST be at the process/shell boundary (env on the
  spawned `node` child); do NOT attempt in-process HOME mutation (Rust-2024 forbids it —
  vnc-041 AC-02). The negative control is REQUIRED pre-merge, not PENDING — an un-proven
  hermeticity assertion is itself the vacuous-pass it claims to prevent (#5189/#4977/#5246).
- Cross-ref: ADR-002 (host/container split establishes the consume step this isolates),
  ADR-001 (a hermeticity-detected miss hard-fails via `fail()` exit 1 with the store-no-grow /
  attach-broken message, no new exit code).
