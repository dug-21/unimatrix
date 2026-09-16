# C8 — Parity corpus (OpenCode arm) — Agent Report

**Agent:** vnc-049-agent-3-c8-parity-corpus (uni-rust-dev) · **Commit:** 0defe435

## Outcome: DONE (col-022 split-brain guard live) — with one PRE-EXISTING blocker flagged

## Files (commit 0defe435)
- `crates/unimatrix-server/src/uds/parity_corpus_opencode.rs` — NEW. Generator emitting `opencode-arm-goldens.json` from the shipped `hook/opencode.rs` arm oracle.
- `crates/unimatrix-server/src/uds/parity_corpus_gen.rs` — MODIFIED. Registers `opencode_arm` module + calls `opencode_arm::generate(&out_dir)` in `generate_parity_corpus` (alongside `uds_layer::generate`).
- `packages/unimatrix/test/fixtures/parity/opencode-arm-goldens.json` — NEW. Committed golden (oracle-generated, byte-stable).
- `packages/unimatrix/test/hook-client/parity-opencode-arm.test.js` — NEW. JS consumer suite (the JS-arm drift sentinel).

## Target-file correction (Stage 3a resolved)
The brief named `parity_corpus_uds.rs`; the test plan corrected toward `parity_corpus_cases*.rs` + `parity_corpus_gen.rs`. **Neither receives the opencode cases as `Case::new` request-dir goldens.** Verified against the tree: opencode's arm is unreachable through the stdin→request pipeline, so I used a **flat arm-level golden** (the `project-hash-goldens.json` precedent), generated from the arm functions and wired into `generate_parity_corpus`. Rationale below.

## Why flat arm-goldens, not `Case::new` request goldens
- The corpus request pipeline (`oracle_request` Rust / `pipelineRequest` JS) stamps provider only via the **inference path** (`normalize_event_name` on `event.txt`). For canonical names both sides infer `claude-code`; the opencode arm (`is_opencode_event_alias`=false today) is never reached. A canonical-name `Case` would just re-test claude-code.
- The JS hook-client has **no `--provider`/`--model` flag** — the OpenCode plugin shells to the Rust binary, not the JS client (#5754). So `provider="opencode"` and `model_id` are unreachable through `buildRequest`; the arm functions are the honest parity surface. Adding a hint path to `pipelineRequest` would fabricate a non-existent production JS code path (rejected per #5754).
- Consequently I did NOT add `build_request::<Event>::opencode` arm keys to `all_arm_keys()` (would break `assert_coverage` with no reachable case).

## Coverage (all computed from the shipped arm — no hand-written expectations)
- All **7 canonical events** (SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, SubagentStart, PreCompact, Stop) → `normalize` → `(canonical, "opencode")` + alias-guard verdict.
- Raw OpenCode source names (session.created, chat.message, tool.execute.before/after, session.idle, experimental.session.compacting) → `__unknown__` sentinel (NFR-01 no-silent-coerce).
- **provider** stamp `"opencode"` both sides (AC-02c). **KNOWN_PROVIDERS** membership + order parity (#5754 gotcha 1).
- **model_id** carrier validation parity (OQ-5, R-15, #5754 gotcha 2): valid `<provider>/<model>` shapes, charset rejects, 129-char over-cap, empty, and the trailing-`\n` trap JS `$`-without-`m` correctly rejects.
- **SubagentStart** present as a canonical-event case (observe-only; no injection leg encoded, per R-16). The agent-on-`extra.agent_type` frame is exercised by the provider-agnostic existing `sas-*` request goldens; the opencode arm covers name canonicalization.
- Alias-guard #5751 property: inference path still stamps `claude-code` for the 7 shared names (guard must stay false).

## Tests
- **Rust** (`cargo test -p unimatrix-server --lib -- opencode parity_corpus`): **28 passed, 0 failed, 1 ignored** (the `#[ignore]` generator). `test_generator_branch_coverage` green → `assert_coverage` intact.
- **JS** (`node test/run-hook-client.js`): **829 passed, 1 skipped, 0 failed** — includes the new `parity-opencode-arm.test.js` (36 assertions).
- **Drift sentinel (R-03.2) verified by mutation:** mutating `normalize.js::canonicalizeOpencode` → JS suite RED; revert → green. Golden is byte-stable across regeneration (zero-diff drift-gate safe). Rust-arm direction guarded by `check-parity-drift.sh` regen+diff and opencode.rs's own unit tests.
- **File boundary respected:** did NOT modify `opencode.rs` or `normalize.js` (arms). The Rust↔JS arms ARE in parity (all 36 JS assertions green against the Rust-oracle golden).

## BLOCKER (pre-existing, OUT of C8 scope — for SM routing)
`scripts/check-parity-drift.sh` fails, but **only** on `sas-tail-multibyte-window-edge/expected-request.json` (a SubagentStart transcript-tail case, unrelated to opencode). My `opencode-arm-goldens.json` is zero-diff.
- **Confirmed pre-existing:** reproduced on a clean (stashed) tree without any C8 change. The vnc-049 branch never touched `transcript_block.rs`, the sas case builder (`parity_corpus_transcripts.rs`), or that fixture (verified via `git log merge-base..HEAD`). Likely latent drift present at the merge-base.
- **Symptom:** the committed golden expects a `ContextSearch` (successful transcript-tail extraction); the current oracle produces the `RecordEvent` fallback (empty extraction) for the multibyte-window-edge transcript.
- **Recommendation:** route to the owner of SubagentStart transcript extraction to investigate whether this is a real extraction regression or a stale golden needing regen+commit. I did NOT regenerate/commit it (out of scope; could mask a real regression). The col-022 opencode guard itself is green.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + `context_search` (pattern/decision) + `context_get` #5670/#5754/#4751 — surfaced the decisive #5754 (JS client has no --provider flag; opencode plugin shells to Rust binary) and the project-hash-goldens flat-golden precedent.
- Stored: entry #5757 "OpenCode-style provider arm needs a FLAT arm-level parity golden, not a stdin-request case dir" via `context_store` (pattern, topic unimatrix-server).
