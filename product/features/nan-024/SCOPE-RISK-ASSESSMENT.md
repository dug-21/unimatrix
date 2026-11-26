# Scope Risk Assessment: nan-024

## User-Facing Entry Points & Behavioral Outcomes

| Entry point | Path-independent outcome they must observe |
|---|---|
| `npx unimatrix init` (local, clean repo) | Claude + Codex hooks fire; each event yields a stored observation with correct provider; no Rust hook process spawned |
| Cloud attach (init remote) | Hook/MCP entries byte-identical to local; events stored over HTTPS |
| Re-run `init` / `wire` over a legacy install | Exactly one Unimatrix entry per event across all written scopes; foreign entries intact |
| Harness fires hook from written config (Claude, Codex) | Observation stored; on unresolvable client/binary, non-zero exit with visible error (never silent) |
| Legacy `unimatrix hook <EVENT>` already in user config | Event handled by Node client and stored |
| `npm cache clean` / reinstall / upgrade, then a hook fires | Still works, or fails loud |
| `UNIMATRIX_BINARY=<path>` set | Hook and MCP both use that binary |
| User-level override (redirect file in `~/.unimatrix/`, or `UNIMATRIX_BINARY`) with released launcher (D-013) | Hook and MCP run the overridden binary and Node client; removing override reverts; no repo/git change. Invalid target fails loud |
| `wire --harness opencode` | Retrieval MCP works; no plugin provisioning written |
| `init` with `.opencode/` present | No `.opencode/plugins/`, no `.opencode/package.json`, `plugin[]` unchanged |
| Headless `npx -y @dug-21/unimatrix mcp-bridge` | Unchanged: MCP reachable |

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---|---|---|---|---|
| SR-01 | No wiring form satisfies R1 (no PATH) + R3 (survive cache clean) + R5 (portable) + no-npx-on-hot-path together; npx is portable but slow, absolute path is fast but non-portable | High | High | Score all five candidates honestly; escalate early per D-003 rather than weaken R3's fail-loud clause |
| SR-02 | Latency budget (25 ms p95) unachievable with Node cold start + any resolution step (npx, shim chain) | High | Med | Measure Node cold start on CI hardware before choosing form; budget is per-event, not per-warm-process |
| SR-03 | Runtime binary/credential resolution in client adds failure modes (missing daemon, wrong binary version) that previously surfaced at init | Med | Med | Define resolution order and error output; distinct exit codes |
| SR-04 | Retiring Rust hook without #682 soak (D-001) — Node/Rust behavioural divergence reaches all local users at once | High | Med | Make parity-layer2 (AC-08) a gate, not advisory; keep hook.rs oracle tests running |

| SR-18 | D-013 override seam: redirect file and `UNIMATRIX_BINARY` precedence undefined; override read on hot path adds latency (SR-02) and is a trust boundary — a writable `~/.unimatrix/` redirect executes arbitrary code per hook | High | Med | Spec precedence (env > redirect > bundled), one resolution routine shared by hook+MCP, validate target exists/executable, fail loud on bad target |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---|---|---|---|---|
| SR-05 | Codex user/project merge semantics unverified (OQ-2); AC-13 cannot be specified without it | High | Med | Verify against Codex source/docs before spec finalizes AC-13 for Codex |
| SR-06 | AC-01 "byte-identical" local vs cloud may conflict with any mode-specific field (transport, credential hints) | Med | Med | Spec must state that all mode differences resolve at runtime; nothing mode-specific in written config |
| SR-07 | Dogfood out of scope (D-009) but this repo's tracked configs still carry legacy forms; tests or CI may read them | Low | Med | Tests must use fixture repos, never this repo's configs |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---|---|---|---|---|
| SR-08 | Ownership regexes (`merge-settings.js:20-27`) miss npx/new forms → duplicate entries, Claude double-fires | High | High | Single ownership predicate covering every legacy + new form; table-driven tests per form (AC-05) |
| SR-09 | Over-broad ownership regex deletes foreign entries (R4 violation) | High | Med | Negative tests: foreign commands containing "unimatrix" substrings preserved |
| SR-10 | Removing string arm / C-10 breaks nan-023 tests; temptation to delete rather than update (AC-12) | Med | High | Gate on test count not decreasing in touched suites |
| SR-11 | Published OpenCode plugin still emits Rust `hook`; launcher route change alters its behaviour silently | Med | Med | Confirm OpenCode plugin path lands on Node via launcher, or document it explicitly |
| SR-12 | Launcher `hook` route breaks Rust pass-through (`LD_LIBRARY_PATH`) for other subcommands | Med | Low | Regression test non-hook subcommands still exec Rust |

## Path-Divergence Risks

| Risk ID | Entry point | Divergence | Recommendation |
|---|---|---|---|
| SR-13 | Harness-fired hook | CI tests call Node client directly (seam) while real harness command form (npx/shim/PATH) is untested | AC-07 must execute the exact written command string |
| SR-14 | Cloud attach | Local path tested end-to-end, cloud only unit-tested; HTTPS credential resolution silently fails | E2E for both UDS and HTTPS from written config |
| SR-15 | Post-cache-clean hook | Hook exits 0 with no output when client missing → silent observation loss | Must fail loud (R3); test asserts non-zero + stderr |
| SR-16 | `UNIMATRIX_BINARY` | Honoured in MCP path but ignored in hook path (or vice versa) | Test both paths independently |
| SR-19 | Override (D-013) | Override honoured by binary but Node client still loads bundled copy (or only MCP honours it) — partial redirect, mixed versions | Test asserts both binary and client resolved from override, via the written harness command |
| SR-20 | Override, pass-through commands (AC-14) | Override honoured for `hook`/MCP but pass-through Rust commands (e.g. `serve`, daemon start) still exec the bundled binary, so the client talks to a mismatched daemon; or a stale/deleted redirect silently falls back to the installed package | Test all three command classes per override source; a present-but-invalid redirect fails loud, only an absent one falls back |
| SR-17 | Re-run init | Dedupe works within one scope but not across `settings.json`/`settings.local.json`/user | AC-13 test seeds entries in every scope |

## Assumptions

- Background "Harness scopes": Claude/Codex scope behaviour from ass-109 Q6, not re-verified — wrong ⇒ AC-13 targets wrong files.
- Background "Binary override (P6)": `resolve-binary.js` is reusable at hook runtime within latency budget.
- Constraints "One Node process per hook event": assumes chosen form does not introduce a wrapper process.
- Goal 6 / AC-14: assumes the launcher is the single resolution point; legacy direct-path Rust entries and npx-cache absolute `node` entries bypass it, so the override is ignored for them until init rewrites.
- Proposed Approach 1: assumes all legacy `unimatrix hook` configs invoke `bin/unimatrix.js`, not the Rust binary directly by path — `<path>/unimatrix hook` forms bypass the launcher.

## Design Recommendations

- Resolve wiring form (SR-01, SR-02) with measured numbers first; it gates SR-08 regex design.
- One ownership predicate module, exhaustively table-tested for positive and foreign cases (SR-08, SR-09).
- Legacy direct-path Rust entries must be rewritten by init, since launcher routing cannot catch them (Assumptions #4).
- Every E2E test drives the literal written harness command (SR-13, SR-14).
- Fail-loud contract for client-unresolvable cases is a spec requirement (SR-15).
- Override seam (SR-18, SR-19): single resolver, defined precedence, fail-loud validation; include in latency measurement.
