## Security Review (uni-security-reviewer): risk LOW, no blocking findings

Scope: test infrastructure only (`product/test/infra-001/**` plus feature docs). No production code, no new dependencies, no CI/workflow edits.

### Findings
| # | Sev | Location | Finding | Blocking |
|---|-----|----------|---------|----------|
| F1 | Low | `harness/conftest.py:388` | If `require_embed_ready` fails (timeout or daemon exit), the fixture kills the daemon and calls `pytest.fail` before the `try/finally`. The isolated data dir `store_dir` is already resolved at that point, so `shutil.rmtree(store_dir)` never runs. That dir is `$HOME/.unimatrix/<hash16>` under the inherited HOME, so a failed readiness wait leaves a stale per-test dir in the runner's real `~/.unimatrix`. Fix: rmtree `store_dir` in that failure path, or move the wait inside the `try`. | No |
| F2 | Low (hardening) | `cloud-cycle-lib.sh:161` | `read_back_project_hash` does not check that the derived name is the documented 16-hex hash before it becomes an argv to `node mcp-bridge.js`/drivers and a path component. No injection is possible today: every expansion is quoted, the value is passed as argv and never `eval`'d, and the sandbox is hermetic. An optional `case "$PROJECT_HASH" in *[!0-9a-f]*|'') fail ...` check would fail loud on layout drift. | No |
| F3 | Info | `cloud-cycle-lib.sh:155-161` | Quoting and word-splitting are sound. `find` output is captured once and counted with `grep -c .` (`|| true` covers the zero-match exit 1). `n>1` fails before `dirname`/`basename`, so multi-line input never reaches them. A filename containing a newline would inflate the count and fail closed. `2>/dev/null` on an unreadable root yields the "absent" fail (fail-closed). `-type f` excludes a symlinked `remote.json`, which is stricter than the old `[ -f ]`. | No |
| F4 | Info | `docker-http-posture-smoke.sh:264-270` | The Gate-6 0600 mode check on `remote.json` (the credstore trust boundary) is kept on the selected `cred_file`. | No |
| F5 | Info | `harness/daemon_readiness.py:27-29` | Appending `unimatrix_server::infra::embed_handle=info` is a valid target (lib crate `unimatrix_server`, `pub mod infra`). Its only side effect is that an inherited more-verbose per-target level for that one module drops to info. The marker string does not match the `failed to load` / `load task panicked` lines (`embed_handle.rs:148,150,159,167`). No probe call, so the parity legs stay symmetric. | No |
| F6 | Info | `claim-floor-lib.sh:31-36` | The conditional source of `cloud-cycle-lib.sh` re-assigns `SCRIPTS_DIR`/`REPO_ROOT_DEFAULT`/`BRIDGE_JS` to identical values (same dir), and it also sources `cloud-bundle-lib.sh` and sets `PARITY_PHASE`. Only definitions and assignments, no top-level execution. The `declare -F` guard prevents a double source on the smoke path. | No |

Secrets: none. The fixture tokens (`"t"`, `sha256:x`) are dummies.

### Blast radius
The worst case is a false gate verdict on the release lane. A wrong hash selection would make the bridge attach fail, which surfaces as exit 1 (fail-closed). A readiness false-positive is not possible: the wait matches only success lines. A readiness false-negative turns into a fixture ERROR with the stderr tail. Nothing can corrupt data or disclose information. These gates newly run Gates 7–10 on the post-nan-024 layout, so a release-only failure on the tag lane is the main regression risk. The `workflow_dispatch` dry-run in the design review (N3) covers it.

### Verification
`release-gate-cloud-cycle-logic-test.sh` passed 26/0 and `release-gate-claim-floor-logic-test.sh` passed 29/0 (run locally). I did not run `test_daemon_readiness.py` because pytest is not installed in this env, and shellcheck is not installed either.
