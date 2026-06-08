# vnc-030 Test Plan — OVERVIEW

GH Issue: #699 · Stage 3a (Test Plan Design) · Rooted in RISK-TEST-STRATEGY.md (R-01..R-23) and ACCEPTANCE-MAP.md (AC-01..AC-10 + seam items). Dual stack: **JS** (`cd packages/unimatrix && npm test -- <name>`) for client components, **cargo** (`cargo test -p <crate>`) for engine/server/store.

## Test Strategy

vnc-030 makes attribution contractual. The test pyramid is risk-shaped, not coverage-shaped — every test traces to a numbered risk:

- **Unit (JS)** — `cycles.js` lifecycle + fail-open injection (R-02/R-03/R-22), `index.js` decoration/suppression/canary dispatch (R-05/R-06/R-09), `state.js` `bumpStampMiss` content-free RMW (R-03/R-19), worktree path routing (R-15).
- **Unit (cargo)** — `wire.rs` serde trio + tolerance (R-16), `infra/session.rs` FeatureSource precedence + two inversion flips + `apply_stamp` idempotency (R-04/R-13/R-17), `migration.rs` idempotence ×3 (R-11/R-20).
- **Integration (cargo, parity/listener layer)** — three-site round-trip (R-01), per-`topic_source`-value write site (R-12), never-declare floor regression (R-09), declared-beats-vote at close/sweep (R-04).
- **Integration (JS, live UDS + spawn)** — subagent root-id inheritance + canary fixtures (R-08/R-14/R-19), UDS-path stamp regression (R-23).
- **Seam / gate-blocking** — interception-seam survival (R-07), end-to-end round-trip at all 3 sites (R-01), UDS stamp parity (R-23), canary fixture set (R-19). See `seam-and-roundtrip.md`.
- **Manual/grep** — AC-07 accuracy sample, AC-09 protocol grep, FR-25 docstring grep, FR-26 #588 disposition, FR-27/R-21 #574 expiry, OQ-E canary-independence probe.

### Test infra is cumulative (binding)

Extend, never re-scaffold:
- **JS** — `tempStateDir()` (state.test.js), `freshProject()`/`childStateDir()`/`startStubServer` (index.test.js), `startRealServer`/`resolveServerBinary` + `transportUds` (parity-layer2-uds.test.js), adversarial bytes via `String.fromCharCode` (#4769). New tracker fixtures live in the existing `test/hook-client/` files where the component already has a suite; new files only for the three NEW concerns (`cycles.test.js`, `state-canary.test.js`, `uds-stamp-regression.test.js`, `seam-survival.test.js`).
- **Platform guard (binding, lesson #4832)** — every test that binds/connects a real UDS socket (live UDS round-trip, canary integration on the live daemon) MUST carry `const IS_WINDOWS = process.platform === "win32"` + `{ skip: IS_WINDOWS }` on the describe/it. Pure offline byte-compare suites (HTTP↔UDS `encodeFrame` parity that never opens a socket) stay **unguarded** so Windows keeps coverage. The Stage 3c tester and Gate 3c validator must reason about the GitHub Actions matrix (windows-latest × every Node), not just the local Linux dev OS.
- **cargo** — extend the col-017 `topic_signal` serde block in `wire.rs` (the 5-test pattern: none-absent / some-present / null-tolerant / serialize-none-omits / serialize-some-includes); extend the migration idempotence test family in `migration.rs`; extend listener record-path integration tests.

## AC ↔ Risk ↔ Test Mapping

| AC | Risks | Component test plan | Primary tests |
|----|-------|---------------------|---------------|
| AC-01 | R-02, R-03, R-08, R-22 | cycles.md | `cycles.test.js` lifecycle + multi-turn + crash-resume + prune; fail-open injection |
| AC-02 | R-16 | wire-cycle-stamp.md | `wire.rs` serde trio + tolerance + ts-rs 7th export + `git diff --exit-code bindings/` |
| AC-03 | R-05 | index-decoration.md | `index.test.js` suppression strip present/absent; CYCLE_* keeps topic_signal |
| AC-04 | R-01, R-04, R-09, R-13, R-17 | feature-source.md, listener-stamp-read.md | FeatureSource decision tree; sweep+close inversion flips; never-declare floor; round-trip ×3 |
| AC-05 | R-11, R-12, R-20 | topic-source-migration.md, listener-stamp-read.md | migration idempotence ×3; one integration case per topic_source value; grep-audit |
| AC-06 | R-08, R-14, R-19 | state-canary.md, index-decoration.md | canary fixture set (positive/negative/forward-compat); subagent root-id integration; `stamp_miss == 0` |
| AC-07 | R-09, R-20 | feature-source.md (manual) | declared-session accuracy sample; multi-shape never-declare fallback; post-migration distribution window |
| AC-08 | R-15 | cycles.md (worktree) | worktree `.git`-file regression; no-raw-cwd-hash grep/assert |
| AC-09 | — | protocol-redeclaration.md | grep re-declaration line in all 3 protocols |
| AC-10 | R-23, R-06, R-16 | uds-stamp-regression.md | UDS `encodeFrame` carries `cycle_stamp` byte-equivalent to HTTP body; replay |
| Seam: interception survival | R-07 | seam-and-roundtrip.md | CYCLE_START → tracker+stamp (not sentinel); non-cycle → sentinel, no touch |
| Seam: 3-site round-trip | R-01 | seam-and-roundtrip.md | per-site declared row + batch N→N |
| FR-25 docstring | — | docstring-driveby.md | grep corrected filter description |

## Cross-Component Test Dependencies

1. **Client strip ⇄ server skip-tally (R-05/R-04)** — the client's `cycle_stamp`-present-strips-topic_signal contract must exactly match the server's stamp-present-skips-enrich/tally branch. Test both ends: JS `index.test.js` asserts the outgoing frame shape; cargo listener integration asserts the server consumes it and the vote tally does not grow.
2. **Three-site lockstep (R-01)** — the shared `apply_stamp_to_row` helper collapses three read sites; the round-trip AC must still assert **per site independently** (single ~:719, single ~:861, batch ~:1042). Field-exists-on-struct is insufficient evidence (#3486 lesson).
3. **Wire seam both directions (R-16)** — new-server/old-Rust-hook (steady state, `hook.rs` untouched) and old-server/new-client (deploy window) both tested. Serde tolerance in `wire.rs`; spawn-level in parity-uds.
4. **Decoration upstream of transport fork (R-23/AC-10)** — decoration mutates in-memory `request` before `selectTransport` (`index.js:410`); UDS regression proves the stamp survives `transport-uds.encodeFrame` byte-equivalent to the HTTP body. Cross-transport equivalence is the integration seam.
5. **Migration version uniqueness (R-11)** — `CURRENT_SCHEMA_VERSION = 28` must be unique against the rebased main at delivery (crt-052 also touches schema). Delivery-time check, not just unit idempotence.

## Integration Harness Plan (infra-001)

The infra-001 Rust MCP-protocol harness (`product/test/infra-001/`, USAGE-PROTOCOL.md) exercises the compiled `unimatrix-server` through MCP JSON-RPC. **vnc-030's server changes are MCP-visible** (a new wire field on `ImplantEvent`, a new `observations.topic_source` column, precedence-chain behavior at record/close/sweep), so the harness applies.

### Suite selection (per the suite-selection table)

vnc-030 touches: server tool logic, store/retrieval, schema/storage, confidence-adjacent attribution. Run:

| Suite | Why it applies to vnc-030 |
|-------|---------------------------|
| `smoke` (`-m smoke`) | **Mandatory minimum gate** — one critical path per capability; must pass before any vnc-030 work is accepted. |
| `protocol` | Wire field is additive (frozen-F1); MCP handshake + tool-discovery must stay byte-stable (R-16). |
| `tools` | Record-path tools (`context_cycle` + observation record) gain the stamp + topic_source — every param/response format. |
| `lifecycle` | Multi-step store→search, restart persistence — the stamp's cross-restart resilience (R-08) and the declared-beats-vote close/sweep flow (R-04) are lifecycle-shaped. |
| `volume` | Schema change (`topic_source` ALTER) + contradiction/attribution scan at scale — migration must not regress at hundreds of entries. |

`confidence`, `contradiction`, `security`, `edge_cases` are run if smoke surfaces adjacency; not required by the feature's change surface, but security input-validation (topic content injection, session_id traversal) is covered by **dedicated client unit tests** (see `cycles.md` security section) rather than the harness.

### Existing-suite coverage vs gaps

- **Covered by existing suites** — MCP handshake/discovery stability (protocol), generic observation record + search round-trip (tools/lifecycle), migration-on-restart persistence (lifecycle), scale (volume).
- **Gaps requiring NEW integration tests** (MCP-visible behavior no existing suite validates):
  1. **A stamped event records `topic_source='declared'` and skips the vote tally** — new lifecycle-flow test (the contract is new). Plan addition to `suites/test_lifecycle.py` (`test_stamped_event_attributes_declared`) OR cargo listener-layer integration if the stamp field is not reachable through the public MCP tool surface (delivery confirms which — see Open Questions).
  2. **`topic_source` column present + per-value population** — new tools/lifecycle test asserting the column exists post-migration and carries the expected value per write path. Plan addition to `suites/test_lifecycle.py`.
  3. **Declared feature beats contradicting vote at close/sweep** — new lifecycle test (`test_declared_survives_vote_at_close`). Plan addition to `suites/test_lifecycle.py`.

  These are MCP-surface assertions; the **byte-level wire** (`cycle_stamp` serde) and the **three-site lockstep** are covered by cargo unit/integration where the read sites are visible at source — the harness validates the user-visible result, the cargo tests validate the internal lockstep. Where the stamp is only emitted by the TS client (not constructible via a raw MCP tool call), the round-trip is validated by the JS parity-UDS live-daemon suite, not infra-001.

- **NOT planning new integration tests for**: pure internal precedence logic with no MCP-visible delta (covered by cargo unit), suppression strip (client-only, JS unit), canary RMW (client-only, JS unit). No infra-001 **infrastructure** change is needed; if one were (e.g., a fixture to inject a raw stamped frame), file a GH Issue per USAGE-PROTOCOL — do not modify the harness in this PR.

### Running (Stage 3c)

```bash
cargo build --release
cd product/test/infra-001
python -m pytest suites/ -v -m smoke --timeout=60          # MANDATORY gate
python -m pytest suites/test_lifecycle.py -v --timeout=60   # stamp + topic_source + close/sweep
python -m pytest suites/test_protocol.py suites/test_tools.py suites/test_volume.py -v --timeout=60
```

### Failure triage (Stage 3c, non-negotiable)

Per USAGE-PROTOCOL decision tree: (1) caused by vnc-030 → fix code, re-run, document; (2) pre-existing/unrelated → file GH Issue, mark `@pytest.mark.xfail(reason="Pre-existing: GH#NNN — …")`, continue; (3) bad assertion → fix test, document. **Never** fix unrelated integration failures in the vnc-030 PR. The cross-platform lesson (#4832) applies: a UDS-binding test failing only on windows-latest is a missing skip-guard, not a feature defect — add the guard, do not xfail.

## Smoke Gate Statement

`python -m pytest suites/ -v -m smoke --timeout=60` is the mandatory minimum gate for Stage 3c and MUST pass before the RISK-COVERAGE-REPORT is written. The interception-seam-survival test (R-07, `seam-and-roundtrip.md`) is the **second** gate and runs **before any vnc-030 server-work validation** — if the seam is dead, the entire mechanism is inert regardless of server correctness.

## Test Conventions

- JS: `describe`/`it` from `node:test`, name `test_{component}_{behavior}` or descriptive `it("…")` matching the existing files.
- cargo: `#[test]` / `#[tokio::test]`, name `{struct}_{scenario}_{expected}`, extend the existing serde/migration blocks.
- Arrange/Act/Assert; deterministic (no wall-clock flakiness — prune tests inject `updated` timestamps; canary tests assert exact counts).
- Pinned CLI **claude 2.1.167** named in test-module doc comments for every test that rests on `--resume` id-reuse or depth-1 inheritance (R-08/NFR-08).

## Knowledge Stewardship
- Queried: `context_briefing` + `context_search` (decision/vnc-030) — surfaced #4834 (ADR-007 seam contracts, gate-blocking seam-survival anchors), #4837 (ADR-002 stamp decoration), #4816 (ADR-004 precedence), #4832 (cross-platform UDS skip-guard lesson — binding on UDS test design), #4831/#4821 (additive wire-enum-field blast-radius pattern). Applied: seam-survival anchors pinned to real `file:line`; UDS tests carry win32 guards; wire tests extend the col-017 serde block.
- Stored: deferred to Stage 3c — no novel test-infra pattern discovered at plan time (governing patterns #3486/#4372/#4092/#924/#4832 already exist). Re-evaluate at retro if the contractual-write-time-field-across-N-read-sites shape recurs in crt-052.
