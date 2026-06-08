# Agent Report: vnc-026-retro-architect

Retrospective knowledge extraction for shipped feature vnc-026 (TS HTTP hook client, F3, #679/#696).

## 0. Stewardship Review (~27 cycle entries assessed)

- **ADRs #4751–#4759**: all follow Context/Decision/Consequences; #4754→#4759 correction chain clean (deprecated, superseded_by set).
- **#4778→#4783 supersession**: recorded cleanly — #4778 deprecated, `superseded_by: 4783`, single chain, no duplication; #4783 carries `caused_by_feature:vnc-024`.
- **#4775 CORRECTED → #4788**: contained the invalidated claim "the wire carries no Entries-vs-other signal" (divergence 2 was fixed client-side via the format_injection header dispatch, per #4783 and shipped transform.js). Corrected to keep the Layer-1 reconstruction technique and the still-open lone-surrogate divergence, mark the over-wrap FIXED.
- **#4780 vs #4786** (both size-gate): complementary, not duplicates — #4780 = rework lesson (trim comment prose only, keep oracle line anchors, never minify/raise limit); #4786 = current state (budget exhausted at 99,997/100,000). Both kept.
- **#4766–#4774, #4776–#4777, #4779, #4781–#4782, #4784–#4785**: assessed against category templates; all well-formed and accurate. No other corrections or deprecations needed.

## 1. Patterns

- **NEW #4789** — Cross-language parity-port pipeline (oracle-generated committed goldens + MANIFEST arm-coverage guard + non-vacuous CI drift check + Layer 1/Layer 2 split). Generalizes ADR-001 for future ports; edge Supports→#4751.
- **UPDATED #4775 → #4788** — stale divergence claim corrected (see above).
- **SKIPPED**: spawn-level JS testing of a real client against in-process stub servers as a standalone pattern — the load-bearing gotchas are already stored (#4768 stub-server/timeout tricks, #4774 async-spawn-not-spawnSync); a structural umbrella entry would add lookup noise, not knowledge.
- **Verification of followed patterns**: #4726 (ts-rs drift check), #1201 (ownership prefix-match), #588 (.git dir-vs-file), #4452 (drift-check vacuity) were all applied during vnc-026 and held up — still accurate, no corrections.

## 2. Procedures

- **NEW #4790** — Hook-client parity suites local procedure: `scripts/regen-parity.sh` (UNIMATRIX_PARITY_DIR + `cargo test ... generate_parity_corpus -- --ignored`, commit goldens with the Rust change), `npm run test:hook-client` (Layer 1), `cargo build --release` then `npm run test:hook-client:layer2` (Layer 2 spawns the real server), `cargo test -p unimatrix-server --lib parity` for the guards; gotchas cross-referenced (#4782 gitattributes, #4784 realpath, #4786 size gate). Replaces the prior CI-job-only mental model — future F4/F5/F6 devs need this locally.
- **#4781** (pre-existing failure outside owned suites → GH Issue, no xfail) confirmed accurate, carried forward.

## 3. ADR Status

| ADR | Entry | Status |
|---|---|---|
| ADR-001 corpus | #4751 | VALIDATED — 83 cases / 104 arms, caught 2 real divergences pre-gate |
| ADR-002 literal templates | #4752 | VALIDATED — with #4783's clarification: governs envelope SERIALIZATION, not the wrap decision (header-keyed dispatch is contract-keyed, not a forbidden heuristic) |
| ADR-003 state/queue | #4753 | VALIDATED — mini-spec implemented verbatim |
| ADR-004 deltas never queued | #4759 | VALIDATED — amended AC-15 honored end to end |
| ADR-005 fail-open/breadcrumb | #4755 | VALIDATED |
| ADR-006 config resolution | #4756 | **FLAGGED FOR AMENDMENT (human approval required)** — INCOMPLETE: claims Rust-hook parity while specifying the init.js walk that lacked `resolve_git_file`; zero worktree mentions; this silence is the root cause of rework item 4 (the gates had nothing to check against). Shipped code now ports `resolve_git_file`. Amendment must: mandate the gitdir-chase, name project.rs as the verified oracle, and enumerate BOTH consumers of projectRoot (state hash + config anchor). NOT superseded — awaiting human approval. |
| ADR-007 separate POST | #4757 | VALIDATED |
| ADR-008 end-anchored elision | #4758 | VALIDATED with minor wording amendment candidate: implementation anchors at `effectiveEnd` (= file_len backed off ≤3 B when the file ends mid-UTF-8-char), per Gate-3a WARN A; ADR literal says `file_len`. Flag for a one-line amendment, not supersession. |
| ADR-003 **vnc-024** (#4714, text allowlist) | — | **FLAGGED FOR AMENDMENT (human approval required)** — per #4783: the mandatory `--- Unimatrix Context ---\n` format_injection header is now a load-bearing wire discriminator the client depends on, but the ADR never specifies it as contract. Amendment should pin the header as a structural invariant of Entries on the text/plain wire. |

## 4. Lessons

- **NEW #4791** — A divergence accepted below the spec/ADR level is invisible to every gate; promote it into the documents gates check, and never write tests that assert the divergence (edge Supports→#4785).
- **NEW #4792** — Synthetic test credentials must not use real-provider prefixes (`sk-`); scanners flag branch history forever; build prefix-shaped input at runtime if needed.
- Carried as evidence (already stored, confirmed high quality): #4783 (wire-discriminator correction), #4785 (enumerate ALL consumers), #4780/#4786 (size gate), #4782 (gitattributes eol-without-text), #4784 (macOS realpath).

## 5. Retrospective Findings (report-only, not stored)

- **bash_for_search 692 calls (+1.6σ), 23% of Bash** — recurring despite the CLAUDE.md rule; storing another lesson will not change behavior. Human action: consider a PreToolUse hook that rejects `grep|rg|find|ls` via Bash in agent sessions.
- **cold_restart (354-min gap, 26 re-reads) / session_timeout (5.9 h, 13 h)** — session-management hygiene; human action only.
- **compile_cycles 19 + Bash-failure cluster during corpus-generator dev** — recommendation "batch field additions before compiling" is sound but generic; not stored. The 500-line-exactly `parity_corpus_gen.rs` suggests the generator was fighting the file-size limit too — consider allowing test-only generators a split earlier.
- **sleep_workarounds ×2** — use `run_in_background` + Monitor; harness guidance, human/protocol action.
- **Outliers**: 27 knowledge entries (+4.2σ) — heavy stewardship CONFIRMED high quality (only 1 of 27 needed correction, supersession chains clean). friction (+3.4σ) and tool calls (+2.3σ) consistent with a 12-component, 5-wave feature; post-completion work 0% and zero permission friction are positive.
- **Budget watch**: hook-client payload gate is exhausted (99,997/100,000 B, #4786). Any F4+ hook-client work needs a human decision first: raise the gate (needs an ADR superseding the NFR-03 rationale) or accept trim-to-add forever.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search ("vnc-026...", k=20) + context_lookup (topic vnc-026) + per-entry context_get over #4766..#4786 — 27 cycle entries assessed; #4778→#4783 supersession verified clean; mcp__unimatrix__context_search pattern/procedure sweeps for packages/unimatrix + crates/unimatrix-server — followed patterns #4726/#1201/#588/#4452 verified still accurate.
- Stored: entry #4789 "Cross-language parity-port pipeline" (pattern), #4790 "Hook-client parity suites local regen/run procedure" (procedure), #4791 "Divergence accepted below spec/ADR level is invisible to gates" (lesson), #4792 "Synthetic test credentials must not match real-provider prefixes" (lesson); corrected #4775→#4788 (stale wire-indistinguishable claim).
