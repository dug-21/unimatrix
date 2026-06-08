# Agent Report — vnc-027-agent-3-parity-corpus-uds

Stage 3b, component 10 (parity corpus — UDS layer). Scope: unit-level fixtures +
corpus reconciliation only. Live-listener / cross-transport replay / FNF
truncation / AC-05 latency deferred to Stage 3c (not written).

## 1. Files modified / created

Rust generator (oracle):
- `crates/unimatrix-server/src/uds/parity_corpus_cases.rs` — removed retired
  `alias-before-tool` case + arm keys `normalize_event_name::alias::BeforeTool`,
  `topic_signal::pre_tool_use`.
- `crates/unimatrix-server/src/uds/parity_corpus_cases_b.rs` — removed retired
  `ptu-pre-non-cycle`, `cycle-near-miss`, `cycle-near-miss-suffixed`,
  `cycle-invalid-type`, `cycle-invalid-topic`, `cycle-missing-tool-input`; removed
  orphaned arm keys (`build_request::PreToolUse::no_promotion`,
  `cycle::not_context_cycle`, `cycle::near_miss_not_intercepted`,
  `cycle::missing_tool_input`, `cycle::validation_failed`); remapped the still-live
  `normalize_event_name::canonical::PreToolUse` arm onto `cycle-start-bare`.
- `crates/unimatrix-server/src/uds/parity_corpus_gen.rs` — wired the new UDS layer
  (emitted after the stale-dir prune so `uds-framing/` survives).
- `crates/unimatrix-server/src/uds/parity_corpus_uds.rs` — NEW: framing goldens
  (`write_frame` + `serialize_request`/`serialize_response`) and hash goldens
  (`compute_project_hash`), both drift-checked.

TS tests / fixtures:
- `packages/unimatrix/test/hook-client/parity-layer1.test.js` — pruned the R-01
  REQUIRED inventory (removed `alias-before-tool`, `cycle-near-miss`,
  `cycle-near-miss-suffixed`, `cycle-invalid-type`).
- `packages/unimatrix/test/hook-client/parity-uds-framing.test.js` — NEW (AC-01,
  R-18): write-direction `encodeFrame` vs committed wire.rs goldens + sync
  accept-injection parity + 1 MiB boundary both directions + >1 MiB reject;
  read-direction decode + `mapHookResponse` mapping.
- `packages/unimatrix/test/hook-client/parity-uds-sync-stdout.test.js` — NEW
  (AC-03, R-09): full UDS sync leg `mapHookResponse → writeSyncOutput` byte-equal
  to committed Rust stdout goldens (ContextSearch plain, SubagentStart envelope,
  BriefingContent verbatim, empty-injection Ack → silent, Ping/Pong → silent).
- `packages/unimatrix/test/hook-client/config.test.js` — added
  `test_symlinked_root_resolves_to_realpath_same_hash` (the ADR-007 §3 symlinked
  layout, the only healthy layout the config agent had not yet covered).

Regenerated corpus (`scripts/regen-parity.sh`):
- Deleted 7 retired case dirs; `MANIFEST.json` `case_count` 83 → 76, arms pruned.
- `project-hash-goldens.json` now generated through the corpus mechanism
  (only the `generated_by` header line changed — all 5 hashes byte-identical,
  confirming cross-language parity).
- NEW `uds-framing/` golden family (6 request + 7 response cases + MANIFEST).

## 2. Tests

- parity-layer1: **GREEN — 91 pass / 0 fail** (was 7 failing).
- Full hook-client suite (`node test/run-hook-client.js`): **535 pass / 0 fail**
  (536 total; 1 pre-existing lone-surrogate `todo`). Was 519 pass / 7 fail.
- New suites: parity-uds-framing 10/10; parity-uds-sync-stdout 5/5; config 49/49.
- Rust: `cargo test -p unimatrix-server --lib parity` — 9 pass / 0 fail (generator
  branch-coverage guard consistent after arm/case removal); generator `#[ignore]`.
- Regen is **idempotent** (two consecutive regens produce an identical tree → CI
  zero-diff drift gate holds). `cargo fmt`/`clippy` clean on the new file.

## 3. Issues / blockers

- **writeMcpJson failure triaged → PRE-EXISTING, unrelated, NOT fixed.**
  `test/init.test.js::test_creates_mcp_json_on_clean_project` expects `env: {}`
  but `lib/init.js` now writes `env: { LD_LIBRARY_PATH: ... }`. Introduced by
  commit `c1b10dd0` ("fix(init): set LD_LIBRARY_PATH in hooks…", #679), which is
  reachable from `origin/main`. It is in the `init` installer path, outside the
  parity/transport/UDS scope of vnc-027 and outside this component's files. Owner:
  whoever lands the next init touch (likely F5). No action taken.
- No other unexplained failures in the hook-client suite.

## 4. Confirmations

- ✅ 7 retired dirs removed; MANIFEST `case_count` 76 + arms pruned; R-01 REQUIRED
  inventory pruned. parity-layer1 GREEN.
- ✅ UDS framing fixtures committed (`uds-framing/`, Rust-generated, drift-checked),
  boundary at exactly 1,048,576 B both directions.
- ✅ Sync-trio stdout goldens exercised via `writeSyncOutput` (post-reduction set,
  FR-21; accepted divergences honored, FR-22).
- ✅ Hash-fixture corpus wired through the drift mechanism (ADR-007 §3); 5 healthy
  layouts + corrupt-worktree divergence covered (config.test.js); socketPath
  dirname == stateDir parent invariant held (pre-existing config tests).
- ✅ Size gate passes — fixtures live under `test/`, do NOT count against the
  client budget. Gate: **stripped 68907/100000, raw 112773/160000**.

Byte counts (working tree): non-boundary framing fixtures 2546 B; hash goldens
1024 B; each 1 MiB boundary fixture is repeated-`a` data that git zlib-packs to
~1.1 KB (negligible packfile cost despite the ~4 MiB working-tree size).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_get #4751/#4790 — surfaced
  the vnc-026 oracle/golden/drift corpus mechanism and the local regen procedure
  (UNIMATRIX_PARITY_DIR + scripts/regen-parity.sh, byte-exact goldens, .gitattributes
  eol=lf). Applied directly.
- Stored: see `/uni-store-pattern` below (corpus-case retirement requires arm-key
  reconciliation + post-prune emission of new generator subtrees).
