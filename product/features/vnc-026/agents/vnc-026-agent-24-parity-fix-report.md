# Agent Report: vnc-026-agent-24-parity-fix

## Mandate
Investigate + fix the `stdout-subagent-non-entries-fallback` client/oracle parity divergence flagged as manifesting on macOS/Windows CI (PR #696, issue #679).

## Investigation Findings

### 1. The macOS/Windows CI failures were NOT the parity todo
Run 27094022619 (latest on feature/vnc-026), all 8 macOS/Windows hook-client cells failed, all 4 ubuntu cells passed. The todo case behaved as a TODO uniformly on every platform (`not ok ... # TODO`, counted under `# todo 1`, never a hard fail). The actual per-platform failures:

- **All Windows cells** (`# fail 1`): `test_transform_source_is_ascii_safe` (transform.test.js). Root cause: windows-latest runners ship Git with `core.autocrlf=true`; checkout converts LF→CRLF, injecting 0x0D bytes the byte-scan rejected. Checkout artifact, not a source defect. **Fixed here** (test made EOL-agnostic; see below). The concurrent CI agent's `.gitattributes` fix addresses the checkout side — the two fixes are complementary, no file overlap.
- **All macOS cells** (`# fail 1`): `test_replay_precedes_carrying_event` (index.test.js:564) — 1 POST instead of 2: the pre-seeded queue frame is not found by the spawned child. Strongly consistent with the macOS `/var` → `/private/var` tmpdir symlink causing a `compute_project_hash` mismatch between the test's `childStateDir()` and the child's own resolution (cf. pattern #4766: hash AFTER path.resolve normalization — `path.resolve` does not dereference symlinks, but the child's `cwd`/HOME may arrive dereferenced). **Not fixed — outside my file ownership (index.test.js)**. Needs a follow-up: canonicalize via `fs.realpathSync` on both sides of that test, or seed the queue by globbing the actual state dir the child created.
- **Drift-check job**: exit 101 during corpus regeneration; **passes locally** (83 cases, zero drift). **Layer 2 job**: `rust-lld: unable to find library -lonnxruntime` — CI environment/link issue, ci.yml agent's lane.

### 2. The wire-discriminator conclusion (lesson #4778) was WRONG
Re-examined cold:
- `observe_response_to_http` (crates/unimatrix-server/src/http/router/observe.rs:30-46): both Entries and BriefingContent → 200 text/plain, no distinguishing header. Confirmed.
- BUT `format_injection` (crates/unimatrix-server/src/uds/hook.rs) **unconditionally** prepends `--- Unimatrix Context ---\n` to every Entries body; empty/over-budget → `None` → **204, never a headerless 200**. The header is a structural invariant of the Entries variant on the wire.
- BriefingContent (`format_index_table`, crates/unimatrix-server/src/services/index_briefing.rs:41 + mcp/response/briefing.rs:52-64) **always** starts with the fixed `CONTEXT_GET_INSTRUCTION` constant. Disjoint constants.
- ADR-002 adjudication: its letter governs envelope **serialization** (literal templates, never object serialization). It says nothing about the wrap/plain decision. `grep -r heuristic product/features/vnc-026/` → zero hits; "never invent a heuristic" was agent-18's paraphrase, not ADR text. Header dispatch is contract-keyed (mandatory first bytes of the single formatting truth), not content sniffing.
- Safety: misclassification requires a BriefingContent beginning with the exact header line — impossible in production, fail-safe if contrived (wrapped body still injects). A server header change breaks Layer 1 goldens loudly (ADR-001 drift check).

## Fix Applied (client-side; zero server changes — C-07 honored)

1. **lib/hook-client/transform.js**: `renderEnvelope` now wraps iff `reqSource === "SubagentStart" && text.startsWith(INJECTION_HEADER)`; else plain path — exact mirror of the oracle's enum match in `write_stdout_subagent_inject_response`. `INJECTION_HEADER` exported. Literal template untouched (ADR-002 enforcement test still passes: 1 `JSON.stringify(text)`, 1 `hookSpecificOutput`, verbatim prefix, 1 stdout write site).
2. **test/hook-client/parity-layer1.test.js**: `STDOUT_TODO` emptied — `stdout-subagent-non-entries-fallback` is now a passing byte-identical assertion. `wireBodyFromGolden` discriminates on the envelope literal prefix (envelope golden → extract inner scalar; plain golden → strip one newline) — non-vacuous: a wrong wrap/plain decision still diverges byte-wise.
3. **test/hook-client/transform.test.js**: `test_subagent_always_wraps_documented_wire_divergence` replaced by `test_subagent_header_dispatch_mirrors_oracle_enum_match` (pins: header → envelope, non-header → oracle plain bytes, bare header w/o newline → plain, non-SubagentStart never wraps). Envelope/escaping tests now use header-prefixed bodies (faithful Entries wire shape; escaper coverage unchanged). Added `test_subagent_200_briefing_body_writes_plain`. `test_transform_source_is_ascii_safe` tolerates CR **only** when followed by LF (CRLF checkout artifact; raw line breaks are syntax errors inside JS string literals, so guard strength is unchanged — bare CR still rejected). Verified by simulating a CRLF working tree: 25/25 pass.

## Verification
- `npm run test:hook-client`: **422 tests, 421 pass, 0 fail, 0 todo, 1 skipped** (pre-existing platform-conditional `test_root_walk_windows_separators`). Former todo now passes.
- `node test/check-hook-client-size.js`: **PASS** (99.1 KB / 100 KB; transform.js 3,937 B).
- Local drift check: zero drift (no goldens touched).
- All files ≤500 lines (transform.js 83, transform.test.js 432, parity-layer1.test.js 418).

## Commit
`482edf11` — `fix(parity): SubagentStart wrap/plain dispatch keyed on format_injection header — resolves stdout-subagent-non-entries-fallback todo; EOL-tolerant ascii guard (#679)` — scoped to the three owned files only. Not pushed (leader pushes).

## Open Items for the Leader
1. macOS `test_replay_precedes_carrying_event` (index.test.js) — real remaining red on macOS cells; not in my ownership. Likely `/var`→`/private/var` realpath mismatch in state-dir derivation between test and child.
2. Drift-check CI exit 101 + Layer 2 `-lonnxruntime` link failure — CI env issues, ci.yml agent's lane.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced lesson #4778 (prior disposition), ADR-001 #4751, ADR-002 #4752, wire-body reconstruction pattern #4775; all directly informed the investigation.
- Stored: entry #4783 "SubagentStart text/plain wire DOES carry an Entries-vs-BriefingContent discriminator: the mandatory format_injection header" via context_correct superseding #4778 (the prior lesson's disposition was wrong; tagged caused_by_feature:vnc-024 — ADR-003's two-variant text allowlist should have specified the wire discriminator at design time).
