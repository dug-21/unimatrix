# Test Plan Overview — 1036

Node-only (`packages/unimatrix`). Runner: `node:test` via `npm test` (`run-hook-client.js` sets `package`, `wiring-e2e`, ...). Gates: `check-test-count.js` (per-file counts never decrease — migrated tests are edited, not deleted), `check-zero-deps.js`, `trust-selfcontained.test.js`.

## Strategy

| Layer | Purpose | Discharges AC? |
|---|---|---|
| Unit (writer, resolver, plan, git-exclude, ownership) | Branch coverage, injected `fs`/`platform`/`fault` | No — support only |
| Assembled (`init()`/`wire()`/`status.collect()` with injected `env`,`home`,`platform`,`fault`) | Real plan + real writer, no mocks of the seam | Yes |
| Spawned CLI (`bin/unimatrix.js init|status|init --dry-run`, `scrubbedEnv`) | Process boundary, exit codes, stderr, real git | Yes |
| Real-binary (`claude mcp get`, `opencode debug config --standalone`) | Harness actually reads the file (AC-1c, AC-2b) | Yes; skip with reason naming binary + #1053 |

Principle: every AC test drives the assembled path (ACCEPTANCE-MAP header). A test that hand-builds a `Step`/leg or injects the writer it should exercise does not count toward an AC.

Env hygiene (R-09): every test uses `scrubbedEnv(extra)`; `assertTmpHome` rejects overrides outside tmp. One sentinel run: all four vars set to a non-tmp sentinel dir, full `package` set, sentinel tree hash unchanged.

## Risk-to-Test Mapping

| Risk | Pri | Primary plan file(s) | Key tests |
|---|---|---|---|
| R-01 modes | Med | safe-write, install-store, credstore, init | `mode-only drift repaired reports updated` (named per scenario 1-6), umask 000/002/022, mode-keep control |
| R-02 fixpoint (incl. non-canonical owned `settings.local.json` → `unchanged`, FR-16) | High | wire-plan, merge-settings, claude-json, codex-install, opencode-install, git-exclude | every mutate twice → `null`; indent/CRLF/no-newline fixtures; double init zero renames |
| R-03 containment | High | safe-write, init, wire | S-cases + spawned init outside/inside/prefix-sibling/parent-dir/swap/case/dangling; exclude exempt |
| R-04 path divergence | High | config-dirs, status | `collect(ctx).configDirs` == init `resolved:`; grep FR-01 |
| R-05 same-file | High | config-dirs, wire-plan | XDG/OPENCODE/trailing slash/symlink/`CLAUDE_CONFIG_DIR=<home>`; win32 drive case |
| R-06 migration | High | trust-writer-core, install-store, credstore | FR-33 carried tests green first; symlinked `trust.json` exact text |
| R-07 dry-run | High | wire-plan, init | tree hash + modes; parity harness all fixtures |
| R-08 idempotency | High | install-store, init | double init `--bundle`, overrides, git; mtime/bytes/mode |
| R-09 env leak | High | test-infrastructure | sentinel run; `assertTmpHome` |
| R-10 §6a names | Low | atomic-write-removal, status | grep S-5; exact JSON fields/value sets |
| R-11 gating | Med | wire-plan, wire | verify-failed W1 → R4 not run; W2 skipped-foreign → no R1; W5 malformed → twin kept |
| R-12 swallow | Med | codex-install, opencode-install | rename fault W3/W4/W5 → exit 1 naming file |
| R-13 CAS | Med | safe-write, wire | conflict once/always via `fault` |
| R-14 git | Med | git-exclude, init | one real-git fixture per FR-42 row; `GIT_DIR` leak; `a*b[1]!` |
| R-15 fidelity | Med | claude-json, merge-settings, opencode-install | `__proto__`, >2^53 ints, key order, indent |
| R-16 parse posture | Med | opencode-install, claude-json, merge-settings | user opencode exit 0; project opencode exit 1 (D-029) |
| R-17 edge values | Low | config-dirs | var × {"", "  ", ~/x, cfg, NUL, abs/} |
| R-18 status local-scope | Med | status | five `claudeMcp.status`; D-028 default profile listed not `wired` |
| R-19 foreign-at-key | Med | wire, claude-json, opencode-install | each RT row + foreign at key |
| R-20 plugin predicate | Low | hook-ownership | FR-15 positives/negatives |
| R-21 pkg swap | Low | install-store | fault between staging and rename |
| R-22 temp | Low | safe-write | `wx` spy, name regex, collision cap 5 |
| R-23 trust hot path | Low | trust-writer-core, credstore | selfcontained, byte-identical copy, require cache; Stage 3c compares hook-path latency to nan-024 ADR-005 budget |
| R-24 win32 | Med | safe-write, config-dirs | injected platform/fs: retry ×10, dir fsync skip, case-insensitive compare |
| R-25 real-binary | Med | init (real-binary section) | AC-1c, AC-2b with skip reason |

## Cross-Component Dependencies
- C2→C3: core tests (trust-writer-core) run before safe-write API tests; FR-33 migrated tests must pass before C16 deletion.
- C1→C4/C15: resolver output shape feeds plan and status; parity test lives in status.
- C4 consumes C5,C7–C10 mutates; gating tests in wire-plan use real step runners with `fault`.
- D-026/D-007 share root-realpath logic: one test asserts both agree for a symlinked root.

## Integration Harness Plan

infra-001 (`product/test/infra-001`) drives the Rust `unimatrix-server` over MCP JSON-RPC. This feature changes no Rust and no server tool, so **no infra-001 suite applies functionally**. Stage 3c still runs the mandatory smoke gate (`pytest suites/ -m smoke --timeout=60`) as a no-regression check; no other suite, and no new infra-001 tests. The Rust workspace link smoke (infra-002) is not required (no Rust change) — record as N/A.

The feature's integration layer is the Node package's own spawned-CLI suites:

| Suite | New scenarios |
|---|---|
| `test/init-integration.test.js` | AC-1c, AC-2b real binaries; AC-5 double init; AC-8 real git; AC-9 parity harness; D-025 mode fixtures via real shim; D-026 spawned outside-root |
| `test/wire.test.js` | AC-1a/b, AC-3, AC-4 RT table, AC-6 scanner, AC-7 fault seam, R-11 gating |
| `test/status.test.js` (new; new-module coverage permitted) | S-1, R-18, SR-09 parity |
| `wiring-e2e*.test.js`, `c14-*.test.js` | existing; must stay green (manifest shape changes: store leg actions, new skip actions) |

Real-binary checks (devcontainer has both binaries; CI skips):
- **AC-1c**: temp HOME, `CLAUDE_CONFIG_DIR=<tmp>/cfg`, cwd=project, spawned `init`, then `claude mcp get unimatrix` → exit 0, output contains `unimatrix`; seeded default `~/.claude.json` entry still present; `status --json` `alternateProfiles` lists it. Skip: `claude not on PATH; real-binary check tracked in #1053`.
- **AC-2b**: two cases (`OPENCODE_CONFIG_DIR=<tmp>/cfg`; only `XDG_CONFIG_HOME=<tmp>/xdg`); helper `runOpencodeDebugConfig()` asserts argv contains `--standalone`, bounded timeout (e.g. 20 s), `HOME=<tmp>`; parse JSON array; document with `path === resolved opencode.json` holds `mcp.servers.unimatrix` or `mcp.unimatrix` deep-equal to written entry; no document at `<tmp>/.config/opencode/opencode.json` holds an owned entry; parse failure fails with raw output. Precedence case with both vars. Skip text: `opencode not on PATH; real-binary check tracked in #1053`.
- Codex: file-level only (D-012, #1053).

## Stage 3c Commands
```
cd packages/unimatrix && npm test
node test/check-test-count.js && node test/check-zero-deps.js
cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60
```
