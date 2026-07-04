# Gate 3b Report: crt-057

> Gate: 3b (Code Review)
> Date: 2026-07-04
> Result: PASS (with WARNs + mandatory Gate-3c carry-forward)
> Validated at: HEAD 49e208ba (branch feature/crt-057)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | 7-param `retrieve_scoped_candidates` + `resolve_transcript_scope_bounds` companion match OVERVIEW/ADR-006/brief; rename applied; all 13 components realized |
| 2. Architecture compliance | PASS | ADR-001..006 honored; 4 purge calls + 3 purge fns deleted (real deletes); exhaustive retention match re-homed (no `_` arm); single reader `snapshot()` reused (#4848); summary arm gone ×4 |
| 3. Interface implementation | PASS | Three orthogonal axes; `r#match` `#[serde(rename="match")]`; window defaults 120_000ms/3 blocks; response-transient search-status outside `RetrospectiveReport`; unchanged types byte-unchanged |
| 4. Test-case alignment | WARN | Impl-phase unit tests comprehensive & correct; AC-10 / AC-19 / summary-rejection-behavioral / fold-×4-naming tests are tester-phase deliverables — **carry-forward to Gate 3c** |
| 5. Code quality | PASS | `cargo build` + `cargo clippy -D warnings` clean; no stub/TODO/`todo!()`/`unimplemented!()`; no `.unwrap()` in new prod; new modules < 500 lines |
| 6. Security | PASS | Regex size-bounded (1 MiB) + validated up front; dependency-free ISO-8601 parser total/no-panic; no secrets/path-traversal/injection; `cargo audit` = 1 pre-existing transitive advisory (no fix available) |
| 7. Knowledge stewardship | PASS | `crt-057-agent-4-server-report.md` has `## Knowledge Stewardship` with `Queried:` + `Stored:` (#5439) |

## Detailed Findings

### 1. Pseudocode fidelity — PASS
**Evidence**: The authorized 7-param widening is reconciled across docs AND code:
- Code `distill_handler.rs:69-77` matches OVERVIEW §"Renamed function signature" and ADR-006 §"As-built signature" exactly (`registry, feature_cycle, observations, cfg, scope, reviewer_session_id, resolved_bounds`).
- Companion `resolve_transcript_scope_bounds(scope, hotspots, cycle_events)` at `tools.rs:5034` matches the OVERVIEW/ADR-006 companion helper — anchor-first, phase via `compute_phase_stats` (sec→ms, self-bounding), `None ⇒ absent section (FR-7)`, never an error. Real and wired end-to-end (not dormant): 4 call sites `tools.rs:2386/2596/3329/3381`.
- All 13 pseudocode components have corresponding implementation; the rename kept the `distill-before-purge.md` path per the OVERVIEW's 1:1 map note.

### 2. Architecture compliance — PASS
**Evidence**: All CORE deletions/moves confirmed in committed code (not `#[allow]`-masked):
- **4 purge calls DELETED**: `test_exhaustiveness_fifth_return_fails` asserts `self.purge_cycle_transcripts(&feature_cycle)` count == 0 in the handler body; `git grep` shows no remaining call, only rationale comments.
- **3 purge fns DELETED (real)**: `purge_cycle_transcripts` (server.rs), `clear_transcripts_for_feature` (session.rs), `purge_held_for_feature` (transcript_hold.rs) have no definition anywhere — only deleted-with-rationale comments. `#[allow(dead_code)]` in server.rs (203/220/243/250) is on unrelated pre-existing struct fields, not the deleted fns.
- **Exhaustive retention match re-homed**: `server.rs:300 reclaim_permitted_by_retention` — both variants explicit, NO `_` arm; compile-gate test `test_retention_match_no_wildcard` (server.rs:3544); consumed at the sole surviving reclamation driver `services/status.rs:1624`.
- **Summary arm dropped ×4**: render dispatch is `markdown | json | _ => ERROR_INVALID_PARAMS` at all four loci (`tools.rs:2582/3507/4397/4478`) with exact message `Unknown format '…'. Valid values: "markdown", "json".`. Remaining `"summary"` strings are doc/test-fixture only.
- **Anchor+phase resolve end-to-end** (not dormant): full-pipeline + force returns resolve real bounds; cached/memo/purged degenerate returns resolve to absent section (honest, documented in-code) per ADR-006 as-built.
- **#4848 single reader**: retrieval reuses `snapshot()` via `take_transcripts_for_feature`; no new buffer reader; R-11 no-`transcript_hold` compile assertion present.

### 3. Interface implementation — PASS
**Evidence**:
- Three orthogonal non-destructive axes documented on `RetrospectiveParams` (`tools.rs:444-478`): `format` (render), `force` (recompute), `transcript` (scoped read-only retrieval); each doc'd as never purging (NG-6).
- Scoped read-only `transcript{ phase?, anchor?, match?, window? }` — observe `types.rs:691` `r#match` with `#[serde(rename="match")]`; `DEFAULT_WINDOW_MILLIS=120_000`, `DEFAULT_WINDOW_BLOCKS=3`, `Window::effective` applies defaults.
- `SessionLossInfo` / `TranscriptCandidate` / `TranscriptCandidatesSection` UNCHANGED — diff shows only NEW type additions (`TranscriptScope`, `Window`, `SessionSearchStatus`, `BoundsKind`, `ResolvedBounds`); no modification to the unchanged structs.
- Response-transient search status OUTSIDE `RetrospectiveReport`: `attach_search_status` appends a JSON content item to the built `CallToolResult` after memoization; `test_candidates_structurally_absent_from_memoized_report` proves the persisted type has no candidate slot.

### 4. Test-case alignment — WARN (carry-forward to Gate 3c)
**Evidence (well covered — impl-phase unit tests)**:
- R-01 per-loss INDETERMINATE matrix: `distill_scope_tests.rs:252` (elided/holes/Reconstructed/dropped individually), `:289` (OR-combined), `:312/:333/:355/:372`.
- R-05 fixed-offset clock/window boundaries: `distill_scope_tests.rs:93/105/119/129/158/172/184` — all fixed epochs; **no `now_ts`/`SystemTime::now`/`Utc::now`** in code (comment lines only).
- R-06 retention re-home no-wildcard: `server.rs:3544`.
- R-09 filters + `transcript:{}` ≡ `match:".*"` + AND-compose: `distill_handler.rs:755` (equivalence, non-empty guard against vacuous pass), `:794`, `:923` (intersection, not union), `:984` (phase self-bounding).
- retrospective-params / transcript-scope serde: `types.rs:2126+`.
- attach None/Some/Err: `distill_handler.rs:644/658/677`.
- 40+ new/migrated unit tests total; all in scope for the impl wave.

**Gaps — all tester-phase (Stage 3c) deliverables, NOT impl-phase**. The impl agent report explicitly scoped integration tests to Stage 3c, and ACCEPTANCE-MAP.md assigns AC-10/AC-19 verification to the tester:
- **AC-10** (≥80% token reduction, populated fixture, vacuity guard): no `test_token_reduction_ratio_populated_fixture` / `test_ac10_vacuity_guard` yet. ACCEPTANCE-MAP marks AC-10 PENDING (method: test), R-13 sc.1-3.
- **AC-19** (negative ownership boundary — no attribution/join/ledger field; no cross-`## Knowledge Stewardship` synthesis): no dedicated test. ACCEPTANCE-MAP.md:47-53 itself pre-flags this as lacking a risk scenario and instructs the tester to add it. `test_candidates_structurally_absent_from_memoized_report` is AC-06 (candidate leak), NOT AC-19.
- **R-12 / AC-11** summary→`ERROR_INVALID_PARAMS`: rejection CODE present at 4 loci and verified correct, but no behavioral test asserts the handler returns the error / exact message; only serde-deserialization unit tests exist (`tools.rs:6549+`). This is a breaking change — its behavioral + integration test (`test_tools.py::test_cycle_review_format_summary_invalid_params`) is a 3c deliverable.

These do not fail Gate 3b (they are not impl-wave artifacts), but Gate 3c **MUST** confirm they are added — see Carry-forward.

### 5. Code quality — PASS
**Evidence**: `cargo build -p unimatrix-server -p unimatrix-observe` and `cargo clippy … -D warnings` both `Finished` clean (0 errors/warnings). No `todo!()`/`unimplemented!()`/`TODO`/`FIXME`/placeholder in new prod code (the single `panic!` at `distill_handler.rs:1120` is inside a `#[cfg(test)]` lock-poison test). No `.unwrap()` in `distill_scope.rs` prod; resolver uses `u64::try_from(...).ok()?`.
**500-line note (WARN, non-blocking)**: New modules `distill_scope.rs` (395) and `distill_scope_tests.rs` (396) are correctly split under 500 per CON-7. Files over 500 lines are all **pre-existing monoliths** the feature merely edited (`tools.rs` 13047, `server.rs` 4170, `session.rs` 3666, `types.rs` 2315) — not introduced by crt-057; production-only code in `distill_handler.rs` is < 500 (its size is the crate's inline-`#[cfg(test)]` convention). Repo-wide monolith remediation is out of scope for this feature.

### 6. Security — PASS
**Evidence**:
- Regex DoS bounded: `MATCH_REGEX_SIZE_LIMIT = 1 MiB` size_limit + dfa_size_limit; oversized/invalid pattern → `ERROR_INVALID_PARAMS` (`validate_scope_regex` up front, `distill_scope.rs:42-68`). `regex` crate has no catastrophic backtracking.
- Input validation at the MCP boundary: `match` validated before the infallible retrieval; unresolved anchor/phase ids → absent section, never a full dump or panic.
- Deserialization hardening: dependency-free ISO-8601 parser is total — malformed/`None` ts → `None` (routes to byte-offset fallback), never panic/wrong-epoch; fully-corrupt snapshot degrades to zero candidates (`test_handler_fully_corrupt_snapshot_normal_response`).
- No hardcoded secrets; no path traversal (no file path handling in the new code); no shell/process invocation.
- `cargo audit`: 1 vulnerability — `rsa 0.9.10` RUSTSEC-2023-0071 (Marvin timing sidechannel, medium) transitive via `sqlx-mysql`. **Pre-existing, no fixed upgrade available, not introduced by crt-057** (feature adds no dependencies). Plus pre-existing `bincode` unmaintained warnings. WARN, non-blocking.

### 7. Knowledge stewardship — PASS
**Evidence**: `agents/crt-057-agent-4-server-report.md` §Knowledge Stewardship has both `Queried:` (context_search surfacing #4750/#4866/ADR-006 #5438 etc., all applied) and `Stored:` (entry #5439 orphan-delete-must-re-home pattern via /uni-store-pattern).

## Carry-forward to Gate 3c (MANDATORY — tester phase must close)

| Item | Risk/AC | What Gate 3c must verify |
|------|---------|--------------------------|
| Token-reduction test | AC-10 / R-13 | `tokens(default_markdown) ≤ 0.20 × tokens(transcript_full_json)` on a POPULATED fixture **with an explicit vacuity guard** (fixture must produce real candidates so the ratio isn't trivially satisfied by an empty buffer) |
| Ownership-boundary negative | AC-19 / NG-5 | Response schema carries no attribution/join/ledger/stewardship-synthesis field; no code path synthesizes across GH `## Knowledge Stewardship` blocks |
| Summary rejection behavioral | R-12 / AC-11 | Handler returns `ERROR_INVALID_PARAMS` with exact message for `format:"summary"` (unit + integration `test_tools.py`); ideally a four-loci source assertion |
| Fold four-site (soft) | R-07 | Post-`ReviewAggregateState` refactor the fold lands ONCE (`tools.rs:2538`), carried to the returns; only `retrieve_scoped_candidates`/`attach` are ×4 source-asserted. Confirm the four-returns fold parity is covered by `review_aggregates` tests; OVERVIEW's "fold-read ×4 assertion PRESERVED" wording is now an abstraction, not a literal ×4 call. |

## Rework Required
None blocking Gate 3b. The four carry-forward items above are Gate-3c (uni-tester) obligations, consistent with the delivery protocol phasing (impl wave → 3b unit tests; tester phase → 3c risk-coverage/integration tests) and the impl agent's stated Stage-3c scope boundary.
