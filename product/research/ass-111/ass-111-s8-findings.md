# FINDINGS: ass-111 S8 — V1 refactor debt

**Spike**: ass-111 S8 (add-on, campaign ass-111, GH #1050)
**Date**: 2026-11-26
**Approach**: evaluation; measurement for the size inventory and effort basis
**Confidence**: empirical for the inventory, the per-step gate cost and every count the effort model uses. The effort model is [D], because it applies one measured historical rate to the measured counts. Everything else is directional. Tags follow S7: [M] measured, [D] derived from measured, [E] estimate.
**Agent**: researcher-ass-111-s8 (uni-spike-researcher)
**Snapshot**: `main` at a59964e7 (2026-11-26)

Unimatrix entries consulted: #3328, #301, #299, #377, #3586, #3977, #2618, #4531, #3778, #4537, #5372, #310 (full text); #240, #4593, #2957, #2400, #3439, #1203, #3605, #3631, #4491, #4780 (briefing/search summaries). No Unimatrix writes.

No code, rule file or CI file was edited. The timing runs only touched mtimes; `git status` shows no tracked changes. Scratch scripts (`loc.py`, `fns.py`, `fnall.py`, `sites.py`, `sqlsrc.py`) and their outputs are in the session scratchpad. They are not shipped.

---

## Measurement basis

**Method.** `loc.py` is a Rust-aware line classifier. It handles nested block comments, raw, byte and C strings, char literals and lifetimes. It classifies each line as code, comment or blank. It then splits each file into **production** and **test** lines:
- Test lines are every item gated by `#[cfg(test)]` or `#[cfg(any(…test…))]`, found by brace matching.
- Whole files count as test when declared through `#[cfg(test)] mod x;` (followed transitively) or when they sit under `tests/`, `benches/` or `examples/`.

**Cross-checks.**
- Raw totals match `wc -l` exactly (288,423).
- Blank and comment counts match `grep` on the files spot-checked (search.rs, attribution.rs, migration.rs).
- Test-region boundaries were checked by hand in tools.rs (6148 → EOF, plus `carry_fault` at 5983–6146) and config.rs (5090 → EOF).
- tokei 15.0.0 agrees on workspace code lines to within 0.4%: 214,698 vs 213,783.
- tokei miscounts single files. It reports 864 extra code lines in `services/search.rs` (+16.6%) and +19% in `observe/attribution.rs`, consistent with its string detection being thrown off by a `'"'` char literal (search.rs:2215).

**The evidence block reproduces exactly** [M]: 460 files and 288,423 lines. 138 files are over 500 lines, 52 over 1,000 and 26 over 2,000. 70.6% of lines (203,499) are in files over 500.

**But two thirds of the lines are tests** [M]:

| | Lines | Share | Code | Comment | Blank |
|---|---|---|---|---|---|
| Production (non-test) | 96,570 | 33.5% | 62,770 | 24,910 | 8,890 |
| Test | 191,853 | 66.5% | 151,013 | — | — |
| — in 171 test-only files | 84,211 | | | | |
| — inline in 234 mixed files | 107,642 | | | | |
| `#[test]` functions | 7,264 | | server 5,006 · store 708 · observe 656 · engine 508 · others 386 | | |

**Over-limit counts under different counting rules** [M]:

| Rule | Files over the limit | Lines in those files |
|---|---|---|
| Raw `wc -l` > 500 (today's rule as written) | 138 | 203,499 |
| Production lines > 500 | **37** | 48,787 (50.5% of production lines) |
| Production lines > 800 / > 1,000 / > 2,000 | 18 / 16 / 5 | |
| Production code-only > 500 / > 400 | 19 / 24 | |
| Non-test files > 500 raw that pass once their tests leave | **48 of 85** | |
| Test-only files > 1,000 raw | 15 | 23,409 |
| Test bodies (file or inline region) > 1,000 / > 1,500 | 40 / 27 | 92,949 / 77,879 |

**Production file distribution** [M]: 289 non-test files. Median 187 lines, p75 343, p90 541, p95 1,228, max 6,145.

**Function length** [M]. There are 2,038 production functions. Measured in code lines (clippy's rule), 73 exceed 100, 36 exceed 150, 22 exceed 200, 15 exceed 300 and 9 exceed 500. The 9 over 500:

| Function | Location | Code lines |
|---|---|---|
| `run_main_migrations` | store/migration.rs | 1,161 |
| `context_cycle_review` | mcp/tools.rs | 1,022 |
| `dispatch_request` | uds/listener.rs | 679 |
| `run_maintenance` | services/status.rs | 534 |
| `merge_configs` | infra/config.rs | 527 |
| `create_tables_if_needed` | store/db.rs | 527 |
| `format_status_report` | mcp/response/status.rs | 526 |
| `tokio_main_daemon` | main.rs | 514 |
| `search` | services/search.rs | 501 |

No file limit can be met for these without decomposing the function.

**Growth** [M] (git snapshots, line counts on the 1st of each month):
- `tools.rs` was about 2,600 lines at vnc-008 (2026-03-04). It was 6,609 on 04-01 and 13,787 on 08-01. It has been flat since (13,808).
- The other top-10 files follow the same curve.
- Rust churn: Aug +468, Sep +2,175, Oct 0, Nov +5,883.

The debt accumulated between April and July and is not growing now. That makes now a cheap moment to install a ratchet.

**Why the rule did not hold** [M]:
- `uni-validator.md:109` says "No source file exceeds 500 lines — flag any file over this limit as FAIL".
- Lessons #2618, #4531, #3586 and #3778 show the gate firing on new or edited small files.
- Yet 138 files stand over the limit. A rule with 138 standing violations cannot be applied literally, or every gate fails. Validators therefore applied it to new files only.
- The failure is a missing ratchet on existing violators, not the number 500.

**Per-step gate cost** [M] (warm cache, 16 cores, jobs=6):
- Incremental `cargo check -p unimatrix-server --tests` takes 12.6 s after touching tools.rs, 13.3 s after a 525-line file, and 14.0 s after a store file. Check cost is crate-granular and independent of file size.
- `cargo test -p unimatrix-server --lib` takes 75 s wall: 4,691 passed, test execution 11.1 s.
- A split step gated on check and the per-crate lib tests therefore costs about 1.5 min. Compile time is not the dominant refactor cost, provided `--workspace` stays at the final gate only (#3328, #2957).

**SQL-site correction to S2** [M]. S2 and the architect's C6 cite 297 server-side and 499 store-side SQL sites. Those counts include inline test modules. Production-only:
- **100 sites in 21 server files**, led by export.rs 17, observation.rs 13, import/inserters.rs 11, import/mod.rs 10, status.rs 9, edge_write.rs 9, tools.rs 6.
- **413 in the store crate**.

198 of the 298 server sites are test code. The C6 lock-in, and the cost of a later A→B reversal, is about a third of what S2 stated.

---

## Findings

### Q: "Map over-limit modules to S1 gaps (G1–G17) and V2 stages; which splits are prerequisites (Stage 0)?"

**Answer**: Six production modules are Stage 0 prerequisites: tools.rs, server.rs, uds/listener.rs, store/migration.rs, store/db.rs and background.rs. In each one either:
- Stage 1 changes land in the first V2 delivery; or
- a cross-cutting gap concentrates its call sites there, so every later stage re-enters the file.

About 14 more over-limit modules should be pre-split as the first step of the delivery that first touches them (the nan-010 ADR-001 pattern, #3586). The rest are either exempt, because no V2 work lands in them, or retire with V1 and should never be refactored.

The V2 tenancy seams themselves are already small, compliant files [M]:

| Seam | Production lines |
|---|---|
| `http/router/seam.rs` (G1/G3) | 395 |
| `project_resolver.rs` (G2) | 297 |
| `token_table.rs` (G5) | 292 |
| `principal.rs` (G7/G10) | 160 |
| `http/auth.rs` | 336 |
| `slug_guard.rs` | 140 |
| `legacy_token_config.rs` (G8) | 39 |

The debt sits in the older V1 core that those seams call into.

**Production call sites per gap** [M] (production code only):

| Gap or item | Sites | Where they are |
|---|---|---|
| G6 `.require_cap(` | 14 | **all 14 in tools.rs**. Definition at server.rs:637 |
| G15 `external_identity` | 18 | tools.rs 15, server.rs 3 |
| G9 `AuditEvent {` construction | 35 in 13 files | tools.rs 13, server.rs 4, store schema.rs 3, store_ops.rs 3, background.rs 2, listener.rs 2 |
| G2/G3 `resolve_store`/`adapter_for` | 14 | background.rs 4, seam.rs 4, status.rs 2, project_resolver.rs 2, others 2 |
| G14 `client_type_map` | 6 | server.rs 4, server_field_census.rs 2 |
| G12 `session_key(` | 4 | session.rs 2, session_transcript.rs 1, transcript_hold.rs 1 |
| G13 `ProjectConfigEntry` | 3 | config.rs |
| `agent_id` occurrences (G6/G9 attribution surface) | 268 | tools.rs 79, server.rs 26, store registry.rs 23 |

**Map** (production / test lines [M]; dispositions are the recommendation):

| Module | Prod / test | Gaps and stage items that land here | Disposition |
|---|---|---|---|
| `mcp/tools.rs` | 6,145 / 7,663 | G9 (Stage 1). G6 (Stage 3). G15 (Stage 5). The SS-R3 qualified-id format in every tool schema (Stage 2). Retraction and `deprecated_visibility` (Stage 4). Publish/fork tools (Stage 6). Also holds `context_cycle_review` (1,022 code lines; no V2 gap) | **Stage 0** |
| `server.rs` | 1,351 / 3,104 | The G6 `require_cap` definition. G9 audit helpers (`insert_with_audit` and others). G14. G15 (`build_context_with_external_identity`) | **Stage 0** |
| `uds/listener.rs` | 3,507 / 6,712 | LI-2 sinks (Stage 1) sit inside `dispatch_request` (`listener.rs:1706-1790, 2457-2531`). `dispatch_request` is **also the HTTP observe path** (`http/router/handlers.rs:27,186`): shared V2 code living in a V1-only directory. G16 retirement cannot delete `uds/` until it is extracted | **Stage 0** (extract hook dispatch out of `uds/`; the UDS transport part retires) |
| `store/migration.rs` | 2,551 / 0 | Every schema change: C1 vector BLOB and G9 columns (Stage 1), Stage 2 tables, C15 migrate-on-load (Stage 4). `run_main_migrations` is one 1,161-line function | **Stage 0** (mechanical ladder split, one file per version step) |
| `store/db.rs` | 1,317 / 392 | `create_tables_if_needed` (527 code lines) changes with C1 and G9 | **Stage 0** (split per table group) |
| `background.rs` | 1,921 / 3,157 | Tick fixes 1–3 (Stage 1). Resolver re-cut, 4 of the 14 G2/G3 sites (Stage 2). Fair-share scheduler superseding crt-056 ADR-005 (Stage 4) | **Stage 0** |
| `infra/config.rs` | 5,074 / 7,812 | G13 and G8 config (Stage 2). The PCD replaces file-plus-restart config (Stage 3, S5). D8 `learning_mode`. Highest fan-in: 33 production files reference it | Pre-split at first touch (Stage 2). Replace `merge_configs` with the PCD; do not decompose it |
| `services/status.rs` | 1,837 / 2,515 | Tick fix 1 (`run_maintenance`, 534 lines; status-only contradiction scan). Compaction in multi-project mode | Pre-split at first touch (Stage 1) |
| `services/nli_detection_tick.rs` | 1,084 / 2,643 | Tick fix 3 (`run_graph_inference_tick`, 414); the NLI arena cap | Pre-split at first touch (Stage 1) |
| `main.rs` | 2,456 / 12 | G2 dynamic resolver replaces router construction in `tokio_main_daemon` (514; Stage 2). G16 deletes `tokio_main_stdio` (444 lines) at α exit | Pre-split at first touch (Stage 2). Retire the stdio half |
| `services/search.rs` | 1,453 / 4,598 | Deterministic and curated `learning_mode` (Stage 4). Retraction visibility (Stage 4). The `FederatedSearch` wrapper (Stage 6, S3). `search()` is 501 code lines | Pre-split at first touch (Stage 4) |
| `infra/session.rs` | 1,243 / 2,423 | G12 (Stage 2) | Pre-split at first touch |
| `store/read.rs` | 1,915 / 1,516 | C6: all new V2 queries land in the store crate | Pre-split at first touch |
| `store/analytics.rs` | 842 / 650 | LI-7 principal attribution of learning rows (`execute_analytics_write`, 411 lines; Stage 3) | Pre-split at first touch |
| `store/write_ext.rs` | 753 / 430 | G9; publish/fork provenance (Stage 6) | Pre-split at first touch |
| `import/mod.rs`, `export.rs` | 591 / 1,722 and 862 / 2,161 | The migration importer (Stage 3). The **export format is frozen at the final V1 tag** (S7 Q3) | Extract tests only. Leave export.rs source untouched; split import when the importer is built |
| `services/observation.rs`, `services/behavioral_signals.rs`, `infra/validation.rs` | 676, 603, 611 | Learning sinks (LI-1/LI-2), DA10 edge validation (Stage 4) | Pre-split at first touch |
| `mcp/response/retrospective.rs`, `mcp/response/status.rs`, `store/cycle_review_index.rs`, `mcp/graph_read_subgraph.rs`, `engine/graph.rs`, `observe/types.rs`, `engine/wire.rs`, `eval/*`, and `context_cycle_review` inside tools.rs | ~8,400 production | No V2 gap | **Exempt** (baselined; may not grow) |
| `uds/hook.rs` | 1,228 / 3,351 | Deprecated Rust hook client, kept only as the parity oracle (#4751; "Rust binary is not a client") | **Retire**, never refactor |
| UDS transport in `listener.rs` (`handle_connection`), `uds/mcp_listener.rs` (313 / 581), `tokio_main_stdio` | — | G16 | **Retire** at α exit |

**Evidence**:
- Call-site counts from `sites.py` over production regions only.
- `dispatch_request` callers found by grep.
- Seam file sizes from `loc.json`.
- LI-2 and LI-3 line references from S4 (lines 115–116 and 248–251).
- `FederatedSearch` placement from S3 (line 102).
- The PCD from S5 Q4.
- Retirement list from S7 Q2.

**Recommendation**: Add a Stage 0 item, "split the six prerequisite modules". Bind the remaining over-limit modules to their first-touching stage as a mandatory pre-split step, recorded in each IMPLEMENTATION-BRIEF as a split-candidate (lesson #4531, mitigation 2).

### Q: "Decomposition plan + sequencing + rough effort for a solo operator; decide refactor-then-evolve vs new codebase (bears on D7, Option A/P6)."

**Answer**: **Refactor-then-evolve.** The decision holds even if the effort estimate is 3× too low.

**Plan**:

Every wave must be behavior-neutral, keep the build compiling at every step, and gate with `cargo check -p <crate> --tests` per step, `cargo test -p <crate> --lib` per module, and `--workspace` once per wave (#3328, #2957, #377).

- **W0 — the ratchet** (Q4). It lands first, so the debt cannot grow while it is paid down.
- **W1 — move tests out of line.**
  - For each of the 142 inline test modules over 100 lines (105,114 lines), replace `mod tests { … }` with `#[cfg(test)] mod tests;` plus a sibling file.
  - The module path stays `parent::tests`, so `use super::*` and access to private items are unchanged. That means zero visibility edits and zero production diff.
  - Then split test files over 1,000 lines by concern (#3778, #4537).
  - This alone clears 48 of the 85 non-test violators.
- **W2 — split the six Stage 0 modules.**
  - Keep a re-export facade in the old module path (#299, #3586), so external call sites change by zero lines. Fan-in (config.rs: 33 production files) then costs nothing.
  - `tools.rs`: split into per-tool-group `impl UnimatrixServer` blocks, each `#[rmcp::tool_router(router = …)]`, combined in `server.rs:432` with `+`. Verified for the pinned rmcp 1.7.0: `ToolRouter` implements `Add`/`AddAssign` (`router/tool.rs:578-590`), and rmcp-macros 1.7.0 accepts `router =` (`tool_router.rs:93-127`). Move `context_cycle_review` into its own file as an exempt baseline entry.
  - `listener.rs`: move `dispatch_request` and its per-event handlers out of `uds/` into a transport-neutral hook-dispatch module, one file per event family. Decompose `dispatch_request` into per-arm handlers. This is the one Stage 0 function decomposition, and it is also where LI-2 lands.
  - `migration.rs`: one file per version step.
  - `db.rs`: DDL per table group.
  - `server.rs` and `background.rs`: split by concern (audit helpers, identity/context, usage; tick loop, extraction, maintenance).
- **W3 — just-in-time.** Each remaining over-limit module is pre-split as the first commit of the delivery that first touches it (Q1 table). Decompose a giant function only when the stage changes it: `run_maintenance`, `run_graph_inference_tick`, `tokio_main_daemon`, `search`, `execute_analytics_write`. `merge_configs` is replaced by the PCD.

**Effort basis.**

Counts [M]:
- 37 production files over 500 lines.
- Stage 0 set: 16,792 production lines and 21,028 test lines.
- Just-in-time set: about 19,100 production lines, plus about 3.7k in borderline files of 500–585 lines.
- Exempt or retire: about 10.4k production lines (about 8.4k exempt, about 2k retire), not refactored.
- 9 functions over 500 lines and 22 over 200.
- 142 inline test modules over 100 lines.
- 40 test bodies over 1,000 lines.

Measured rate, from one sample [M]: vnc-008 (commit 175c9715) touched 37 files (+2,463/−2,278). That included splitting a 1,650-line `response.rs` and extracting `ToolContext`. It ran in one design-plus-delivery cycle, about 11 h wall (research doc 2026-03-03 15:50 UTC → impl commit 2026-03-04 03:03 UTC), with 14 compile cycles and 1,673 tests green at every step (#299, #310).

Gate cost [M]: about 1.5 min per step (see Measurement basis).

| Work | Basis | Sessions [D] |
|---|---|---|
| W0 ratchet script and baseline | One script, mirrors `check-test-count.js` | 1 |
| W1 test extraction | Mechanical, path-preserving; 105k lines | 1–2 |
| W1 test-file splits | 40 bodies, 93k lines, cut by concern at about 15k lines per session | 5–7 |
| W2 Stage 0 moves | 16,792 production lines at ≤2,500 per session (vnc-008 rate) | 7 |
| W2 `dispatch_request` decomposition | 679 code lines; LI-2 lands in it | 1 |
| **Stage 0 subtotal** | | **≈15–18** |
| W3 just-in-time moves | about 19,100 lines at ≤2,500 per session | 8 |
| W3 giant-function decompositions the stages need | 5 functions | 5 |
| **Total burn-down of the V2-relevant debt** | | **≈28–31** (range 15–45 at ±50% on the single rate sample) |

**New-codebase comparison** [D]:
- The current tree took 357,538 added Rust lines across about 115 feature deliveries plus about 221 fix commits (1,180 commits, Feb–Nov 2026). That is about 3.1k lines added per feature delivery.
- A rewrite must author 96.6k production lines and enough test coverage to re-prove the proven capabilities. Re-establishing even half of today's 191.9k test lines makes about 192k lines, which is **≈60+ feature-sized deliveries**.
- A rewrite also still needs the V1 export format and importer (S7 Q3), and it contradicts S7's evidence base: the verdict rests on Option A "extending existing seams".
- The refactor's ≈30 sessions are each smaller than a feature delivery and add no logic.
- The rewrite only breaks even if the refactor rate is about 2× worse than the sample on all W2+W3 work (2 × ≈30 ≈ 60). The Stage 0 subset (≈15–18) wins even at 3×.

**D7 (repo boundary).**
- The refactor evidence supports S7's single-repo default.
- The crate is the measured compile unit: an incremental check costs the same 13 s whichever file changes. unimatrix-server holds 68% of production lines (66,009 of 96,570) and 5,006 of 7,264 tests.
- A later repo split, or faster builds, would need crate boundaries. File splits do not provide them.
- New V2 subsystems should be born as their own crates: the control plane (inside or beside the store crate, per C6), the OAuth resource server and attestation. That keeps a later split cheap without a Stage 0 crate extraction.
- Extracting existing server modules into crates is a separate, optional lever. It is not a prerequisite.

**Evidence**:
- `loc.json`, `fnall.py`, `sites.py`.
- The vnc-008 commit and its SCOPE.md (tools.rs about 2,600 lines, response.rs about 2,550 at the time).
- Git history numstat.
- The rmcp 1.7.0 sources in the cargo registry.
- Timing runs.

**Recommendation**:
1. Choose refactor-then-evolve.
2. Run W0–W2 as Stage 0 work (≈15–18 sessions [D]) and W3 inside the stages.
3. Keep D7's single-repo default, with a new rule: new V2 subsystems are new crates.
4. Feed the Stage 0 subtotal into KC4 as the refactor-debt component of Stage 0 and Stage 1 sizing.

### Q: "Reconcile with 'freeze V1, security fixes only.'"

**Answer**: There is no conflict once "freeze" is scoped the way S7 Q2 defines V1: a set of V1-only surfaces, not a code line. A behavior-neutral refactor of shared code on `main` is V2 Stage 0 work. It ships to V1 users in normal releases, as Stage 1 already does. The hypothesis needs three amendments:

1. **"Security fixes only" applies to V1-only surfaces, never to shared code.** Shared-code refactors are allowed and required. Read literally, the current phrasing would block W1/W2.
2. **Never refactor the retire list**: `uds/hook.rs`, the UDS transport, `tokio_main_stdio`, `uds/mcp_listener.rs`, the legacy token, `tokens.json` and the client bundle. Baseline them as frozen and delete them at α exit. Their baseline entries go with them.
   - A security fix that grows a frozen file is pre-authorized to raise that file's baseline, with the issue linked. This is the only automatic raise.
   - Any other raise is a human decision.
3. **Extracting shared code out of a V1-only module is V2 work, not V1 work.** The example is `dispatch_request` leaving `uds/`. G16 retirement depends on it.

Sequencing:
- Cut S7's final V1 tag (the two security bugfixes plus the frozen export format) **before** W1/W2 land. The human then has a pre-refactor binary to fall back to if a refactor regresses the live product.
- `export.rs` gets test extraction only. Its source stays untouched so the frozen format cannot drift.

**Evidence**: S7 Q2 ("V1 is not a separate code line"; the V1-only surface list); S7 Stage 1 ("ships to V1 users"); the `dispatch_request` callers; the deprecation of `uds/hook.rs` (#4751, auto-memory "rust-binary-is-not-a-client").

**Recommendation**:
- Restate the V1 disposition as: "V1-only surfaces are frozen (security fixes only) and retired at α exit. Shared code is not frozen; behavior-neutral refactor of it is Stage 0."
- Cut the final V1 tag before Stage 0 refactor waves.

### Q: "Limit policy: is 500 the right limit? Propose a better one (e.g. separate thresholds for src vs tests, counting rules), and how to HOLD it — enforcement mechanism (CI check / clippy / script, ratchet so files can't grow, baseline exemptions burn-down). ci.yml changes go via issue."

**Answer**: The number 500 is defensible for **production** files. The counting rule and the missing ratchet are what failed.

- 252 of 289 production files already comply. The median is 187 and p90 is 541. A lower limit adds churn without evidence; a higher one (800 to 1,500, mathlib-style) would legitimize 18–26 monoliths.
- Today's rule counts inline tests, so the number is meaningless: 48 of the 85 non-test violators are over only because of tests.
- The rule has no ratchet, so it cannot be applied to existing violators.

**Proposed policy** (recommendation only; `rust-workspace.md` not edited):

| # | Rule | Limit | Violators today [M] |
|---|---|---|---|
| R1 | Tests are out of line: `#[cfg(test)] mod x;` plus a file named `tests.rs`, `*_tests.rs` or `tests/**`. New inline `#[cfg(test)] mod x { … }` blocks are banned; existing ones are baselined | — | 234 files with inline test code (142 over 100 lines) |
| R2 | Production `.rs` file | **500 raw lines** (`wc -l`, comments and blanks included). Split proactively at 450 (#4531) | 37 once R1 is done |
| R3 | Test file | **1,000 raw lines**. Test cases are independent, but agents still read whole files, and the files accumulate (#4531, #4537) | 15 test-only files today; 40 test bodies after W1 |
| R4 | Production function | **150 code lines** via `clippy::too_many_lines` (`too-many-lines-threshold = 150` in `clippy.toml`; `too_many_lines = "warn"` in `[workspace.lints.clippy]`). Ratchet toward clippy's default of 100 later | 36 (100 lines → 73) |
| R5 | Exemptions | Only through the baseline file. Never through an inline allow without a baseline entry | — |

**Counting rules**:
- Raw `wc -l` per file, deliberately. It is deterministic and needs no toolchain or lexer.
- R1 is what makes raw counts honest.
- Code-only counting was rejected. It needs a lexer: tokei miscounted single files by up to 19%. And comment-stripping pressure is the #4780 failure mode.
- File kind is decided by path and name. The script also checks that every `#[cfg(test)] mod x;` resolves to a test-named file, which makes the naming convention enforceable.
- `#[cfg(any(test, feature = "test-support"))]` counts as test (10 sites).

**How to hold it — the ratchet**:

- **Tool choice** (survey):
  - Clippy has **no file-length lint**. `too_many_lines` is per function, in the pedantic group, default threshold 100. It skips blank and comment-only lines, and has no test exemption (clippy lint configuration docs; `clippy_lints/src/functions/too_many_lines.rs`).
  - rustfmt has no limit.
  - Third-party per-file limits exist: alint `file_max_lines`, and dictator-rust `rust/file-too-long` (default 400 code lines). No native baseline or ratchet support was verified for either.
  - tokei and scc only count lines; scc was not tested.
  - The established ratchet pattern is a committed per-file exception list whose entries may not grow and are burned down. Lean mathlib has done this since PR #9822 (2024-01-19), with a 1,500-line limit.
  - **Recommend a small in-repo script.** Do not add a dependency.
- **Mechanism**: `scripts/check-rust-file-size` (Node or bash, no cargo). It mirrors the existing ratchet `packages/unimatrix/test/check-test-count.js` and its committed `fixtures/test-counts.json`, including `--write-baseline <rev>` and the rule "the baseline changes only in a commit that states why".
  - Baseline file `scripts/rust-size-baseline.json`: `{path: {kind, lines, inline_test_mods}}`, plus a count of `allow(clippy::too_many_lines)` attributes.
  - FAIL when:
    - (a) an unlisted file exceeds its kind's limit;
    - (b) a listed file exceeds its baseline (no growth);
    - (c) a listed file is now within the limit or deleted (stale entry: remove it, so burn-down is recorded);
    - (d) a listed file shrank but the baseline did not (`--tighten` lowers numbers and never raises them);
    - (e) any baseline entry was raised or added relative to the merge base, unless it is listed in an `approved_raises` block that names the GH issue carrying the human decision. This is the CAP-CHANGE rule of ADR-005 vnc-027 (#5372): raising a cap to make a gate pass is never an agent action;
    - (f) the inline-test-module count or the `too_many_lines` allow count grew.
  - The script prints a burn-down line: baselined files, excess lines over the limit, allow count.
- **Where it runs**:
  - In GH CI, as a step in the existing ubuntu job that already runs `scripts/check-inference-sites.sh`. No Rust toolchain is needed. That matters, because GH CI does not build Rust (`.cargo/config.toml`: "GH CI is JS-client-only").
  - In the local pre-PR gate.
  - At Gate 3b, where the validator replaces its subjective "flag any file over 500" with the script's exit code.
  - R4 is enforced by the existing local `cargo clippy --workspace -- -D warnings` gate. The 36 existing functions get `#[allow(clippy::too_many_lines)]` as the in-code baseline, counted by the script.
- **Burn-down targets**:
  - Stage 0 exit: the six Stage 0 modules are off the baseline and W1 is complete.
  - α exit: retire-list entries are deleted with their code.
  - GA: only exempt-class entries remain, and each entry carries a reason.

**Evidence**:
- Violator counts and distribution from `loc.json` / `fnall.py`.
- The tokei cross-check.
- Clippy lint configuration docs (`too-many-lines-threshold`, default 100; no `allow-*-in-tests` option covers it) and `too_many_lines.rs` counting logic (blank and comment-only lines skipped). Clippy `functions/mod.rs` lists the lint as pedantic, since 1.34.0.
- The mathlib thread "Linter for large files?" (1,500-line limit, per-file exception caps that may not grow, burn-down).
- alint / dictator-rust docs.rs sources.
- ci.yml jobs (inference-site bash check; test-count non-decrease guard).
- `uni-validator.md:109`; #5372; #4531.

**Recommendation**:
1. Adopt R1–R5.
2. File a GH issue for the ci.yml step (draft below).
3. Have the rule-file owner restate the `rust-workspace.md` line as: "Max 500 lines per production file and 1,000 per test file (`wc -l`); tests out of line; functions ≤150 code lines (`clippy::too_many_lines`); enforced by `scripts/check-rust-file-size` against a committed baseline: baselined files may not grow, and raises are human decisions."
4. Update `uni-validator.md:109` and `uni-rust-dev.md:89/165` to point at the script.

**Draft GH issue** (for the ci.yml change; not filed by this spike):
> **ci: Rust file-size ratchet (ass-111 S8)**. Add a step to the `Enforce inference site conventions` ubuntu job that runs `scripts/check-rust-file-size` against `scripts/rust-size-baseline.json`. No Rust toolchain is needed. FAIL on: a new over-limit file (production 500, test 1,000 raw lines); growth of a baselined file; a stale or untightened entry; or any baseline raise not listed in `approved_raises` with a human-decision issue. Baseline generated at the merge commit with `--write-baseline`. Acceptance: green on `main` at landing; red on a fixture that adds a line to `mcp/tools.rs`.

---

## Unanswered Questions

1. **Refactor rate beyond one sample.** The ≤2,500 production lines per session rate rests on vnc-008 alone. Per-feature session-hour data in Unimatrix cycle reviews was not mined. Reason: time-boxed. The decision is robust to 2× error, but the KC4 sizing should re-derive the rate from W1/W2's first two sessions.
2. **Private-item coupling for W2 source splits.** Moving items into sibling modules can require `pub(super)` or `pub(crate)` widening where siblings call private helpers. This was not counted per file, because it needs a resolved call graph (rust-analyzer). It does not affect W1, which keeps module paths.
3. **scc and dictator-rust baseline support.** Neither was installed or tested. Not needed, given the in-repo-script recommendation.
4. **Tick-function decomposition risk.** `run_maintenance` and `run_graph_inference_tick` are Stage 1 tick-fix targets. Whether to decompose them before or with the fix is a design-session call; it depends on how their tests isolate phases.

---

## Out-of-Scope Discoveries

- **S2's SQL-site count includes test code.** There are 100 production SQL sites in the server crate, not 297, and 413 in store, not 499. The C6 lock-in, and the cost of reversing the Option A→B storage choice, is about a third of what S2 stated. Carry the correction into the architect's C6 ADR.
- **`dispatch_request` lives under `uds/` but serves HTTP observe.** The directory name misrepresents the V2 surface. Any "delete `uds/`" step at G16 would break `/observe`. Record this in the G16 retirement design.
- **Compile cost is crate-granular** (13 s check whatever the file). unimatrix-server is 68% of production lines. Splitting the crate, for example along mcp / hooks / config / services lines, is the build-time lever that file splits are not. It may deserve its own spike if Stage 2+ cycle counts climb (#3328).
- **Nine functions over 500 lines and 36 over 150.** `run_main_migrations` alone is 1,161 lines in a single function. Every schema change lengthens it, so migration review cost grows linearly. The ladder split fixes this regardless of V2.
- **The hook-client size gate (ADR-005 vnc-027, #5372) is the in-repo precedent** for a human-only cap-raise rule. The Rust ratchet should cite it rather than invent a new rule.

---

## Escalations for human

These are decisions to add to S7's list. This spike does not make them.

- **D20 — Refactor-then-evolve vs new codebase.** Recommend refactor-then-evolve. A rewrite is ≈60+ feature-sized deliveries [D], against ≈30 smaller split sessions for the full V2-relevant burn-down and ≈15–18 for Stage 0 [D]. *Authority class*: H2 (scope). *Gates*: Stage 0.
- **D21 — The limit policy R1–R5 and the ratchet.** It redefines a workspace rule:
  - production files 500 raw lines;
  - test files 1,000;
  - tests out of line;
  - functions 150 code lines;
  - a baseline that may not grow, with raises human-only.

  *Authority class*: H2 (scope / process rule); the ci.yml change goes through an issue. *Gates*: W0, the first Stage 0 commit.
- **D22 — Amend the V1 freeze wording.**
  - "Security fixes only" applies to V1-only surfaces.
  - Shared code is refactorable as Stage 0.
  - Frozen files may grow only for security fixes, with an issue link.
  - The final V1 tag is cut before the refactor waves.

  *Authority class*: H4 (accepting gaps), amending D2's accepted-gap scope. *Gates*: Stage 0.
- **D7 addendum (folds into D7).** Single repo, plus the rule that new V2 subsystems are new crates. *Authority class*: H2. *Gates*: Stage 2.

---

## Proposed amendments to ass-111-findings.md

S7 has not been edited by this spike.

- **KC4 (decision memo).** Append: "S8 sized the refactor-debt component: Stage 0 ≈15–18 split sessions [D] (W0 ratchet, W1 test extraction, W2 six prerequisite modules); ≈13 more inside Stages 1–6 [D]. Re-derive the rate from the first two W1/W2 sessions before re-confirming GO."
- **"Why conditional"** (memo) — add item 6: "The V1 core that V2 extends is concentrated in 37 over-limit production files (48.8k lines, 50.5% of production code). The six Stage 0 modules must be split before Stage 1 code lands (S8)."
- **D7.**
  - Add to the recommendation: "S8 found no refactor reason to split the repo; the crate, not the repo, is the measured build unit. New V2 subsystems (control plane, OAuth RS, attestation) are new crates from day one, keeping a later split cheap."
  - Add to Merges: "S8 Q2".
- **Stage 0 capabilities.** Add:
  - "Size ratchet (S8 R1–R5) landed via issue";
  - "W1 test extraction";
  - "W2 split of tools.rs, server.rs, uds/listener.rs (hook dispatch extracted out of `uds/`), store/migration.rs (ladder), store/db.rs, background.rs".
  - Exit adds: "the six modules are off the size baseline".
- **Stage 1.**
  - Add a note: "Each delivery pre-splits any over-limit module it first touches (status.rs, nli_detection_tick.rs) as its first commit (#3586)."
  - LI-2 is implemented in the extracted hook-dispatch module, not in `uds/listener.rs`.
- **Stages 2–6.** Add the same pre-split rule. Bind config.rs to Stage 2 (G13) and Stage 3 (PCD replaces `merge_configs`), main.rs to Stage 2 (G2), and search.rs to Stage 4/6.
- **V1 disposition (Q2) and D2.**
  - Replace "No new work on V1-only surfaces; security fixes only" with: "V1-only surfaces are frozen (security fixes only; a frozen file's size baseline may rise only for a linked security fix) and retired at α exit. Shared code is not frozen: behavior-neutral refactor of it is Stage 0 work and ships in normal releases."
  - Add to Recommendation step 2: "cut the final V1 tag before Stage 0 refactor waves; export.rs receives test extraction only."
  - Add `uds/hook.rs` (parity oracle) and `uds/mcp_listener.rs` to the retirement list.
- **Decision list.** Add D20 (refactor-then-evolve), D21 (limit policy and ratchet) and D22 (freeze wording), as in "Escalations for human". Fold the crate rule into D7.
- **"Actions that are not human decisions".**
  - Add "File GH issue: CI Rust file-size ratchet (S8 draft)". Owner: GH issue.
  - Add "Correct the SQL-site counts in C6 to 100 server / 413 store production sites". Owner: uni-architect, in the V2 design session.
- **V1 defects table.** Add row 13: "Files over the 500-line rule (37 production, 138 raw) with no ratchet; the gate is applied to new files only" — impact: review and agent-context cost, compile cycles — disposition: Stage 0 (W0–W2), then just-in-time.
- **R8 (unresolved risks).** Change "Unknown" to "Refactor component sized by S8 [D]; feature effort for Stages 1–3 still unsized".

---

## Recommendations Summary

- **Q1**: Six modules are Stage 0 prerequisites: tools.rs, server.rs, uds/listener.rs (extract hook dispatch out of `uds/`), store/migration.rs, store/db.rs and background.rs. About 14 more are pre-split at first touch. About 8.4k lines are exempt, and the retire list (uds/hook.rs, UDS transport, stdio main) is never refactored.
- **Q2**: Refactor-then-evolve, in waves W0 ratchet → W1 tests out of line → W2 six modules with re-export facades → W3 just-in-time. Stage 0 is ≈15–18 sessions and the total ≈30 [D]. A rewrite is ≈60+ deliveries [D]. D7 stays single-repo, with new V2 subsystems as new crates.
- **Q3**: "Security fixes only" binds V1-only surfaces, not shared code. Shared-code refactor is Stage 0. Cut the final V1 tag before the refactor waves.
- **Q4**: Keep 500 for production files, but count with tests out of line (R1). Test files get 1,000. Functions get 150 code lines via `clippy::too_many_lines`. Hold it with an in-repo, toolchain-free ratchet script and committed baseline (no growth, stale entries fail, `--tighten` only lowers, raises are human-only per #5372). It runs in the existing ubuntu CI job (via issue), the pre-PR gate and Gate 3b.
