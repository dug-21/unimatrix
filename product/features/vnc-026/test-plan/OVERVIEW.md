# vnc-026 Test Plan Overview — TS HTTP Hook Client + Client-Streamed Transcript Deltas

**Inputs**: RISK-TEST-STRATEGY.md (R-01..R-20), SPECIFICATION.md (FR-01..26, AC-01..16),
ACCEPTANCE-MAP.md, ADR-001..008, IMPLEMENTATION-BRIEF.md Delivery Notes (binding gate decisions).

## Binding Gate Decisions (honored throughout; do NOT reopen)

1. **AC-15 amended**: `transcript_delta` frames are NEVER queued. AC-15 delta arm asserts
   offset-non-advance + NO queue file — never queue presence (Delivery Notes 1, ADR-004).
2. **Timeouts accepted**: 750/2,000/3,000 ms. NFR-02's 500 ms is the normal-operation budget;
   2,000 ms is the degraded-path deadline. Not a conflict; no test flags it.
3. **Gate-note 1 CLOSED**: FR-01 mandates `fs.readFileSync(0)`. The fd-0-on-Windows test
   obligation stands under R-14.
4. **AC-12 expanded**: CI matrix = Node 18/20/22/24 × Linux/macOS/Windows OS runners (R-14).
5. **Layer-2 helper** asserts the four pinned ADR-008 server-state items (see Layer 2 below).
6. **ass-071 freebie**: opportunistic debug dump of a raw SubagentStop stdin payload during
   delivery test runs — advisory only, commits to nothing (see Cross-Component Obligations).

## Test Layers

| Layer | Infra | Scope |
|---|---|---|
| Unit | `node:test` under `packages/unimatrix/test/` (cumulative — extend existing suites) | Per-module behavior, failure paths, bounds, security posture |
| Layer 1 parity (AC-01/AC-04/AC-05) | `node:test` + committed corpus `test/fixtures/parity/` | `buildRequest` structural equality vs Rust goldens; stdout byte-identical vs `expected-stdout.bin`; deterministic buffer pre-population behind ONE helper (SR-11) |
| Layer 2 integration (AC-05/AC-06/AC-07/AC-10) | `node:test` spawning the **merged F2** `unimatrix-server` (PR #692) HTTP `/observe` | Streamed deltas, injected drops, elision mid-session, ≥8 interleaved sessions, PreCompact restoration, four pinned ADR-008 assertions |
| Rust oracle generator | `cargo test` dev-test in `unimatrix-server` | Generates corpus goldens; branch-coverage assertion over every `build_request` arm (R-02) |
| CI gates (AC-12) | GitHub Actions | Node × OS matrix; zero-dep audit; <100 KB; corpus drift check that FAILS (not skips) when the generator is absent (R-20) |
| Benchmark (AC-13) | shell harness, ≥50 iterations + warmup | Full spawn path incl. hash derivation, root walk, health.json write; results committed to `product/features/vnc-026/testing/` |

**Stub server**: a shared `node:test` helper (`test/helpers/stub-server.js`, new, cumulative) —
local `http` server with scriptable responses (status, Content-Type, body, delay, refuse) and a
request log (method/path/headers/body). Used by transport, transform, delta, queue, index,
init-remote plans. **Transport spy**: config/no-network assertions use the stub's request log —
zero requests received is the proof.

## Risk-to-Test Mapping

| Risk | Priority | Plan file(s) | Core scenarios |
|---|---|---|---|
| R-01 | Critical | build-request, transcript, parity-corpus | Full ADR-001 inventory as Layer 1 cases; SubagentStart tail variants; CI drift check |
| R-14 | Critical | index, config, state, queue | fd-0 stdin on Windows (piped/empty/>1 MiB); OS-matrix CI; backslash root walk; chmod no-op; "spawn produces a POST on Windows" smoke |
| R-02 | High | parity-corpus | Corpus manifest mapping every `build_request` arm; Rust-side branch-coverage assertion |
| R-04 | High | delta | Mid-2/3/4-byte trim; span-inside-one-char; 10-spawn adversarial growth; property test `sum(shipped) == contiguous prefix` |
| R-09 | High | config | Full FR-06 matrix (8 cases) incl. nested-.git, key-drop rewrite, partial env pair; no-network proof per case |
| R-10 | High | state, transport-http | Failure-class matrix → breadcrumb; content-free scan; read-only state dir; W4 transition test |
| R-11 | High | init-remote | Regex pos/neg table incl. spaced paths (open gate note); 4-shape re-run matrix; double-fire count |
| R-03 | Medium | transform, parity-corpus | Adversarial-content byte goldens; grep-gate on transform.js |
| R-05 | Medium | delta, state | Concurrent-spawn offset race; atomic rename; F2 dedupe end-state |
| R-06 | Medium | delta (Layer 2) | End-anchored frame shape; elision-mid-session run; four pinned assertions; post-elision contiguity |
| R-07 | Medium | delta | Permanent 413/401 delta stub → no advance, no queue file, bounded per-spawn cost |
| R-08 | Medium | queue | Lifecycle, poison-pill, drop-oldest (501 files / >5 MiB), 24 h prune, concurrent double-send, 32/256 KiB budget |
| R-12 | Medium | init-remote | FR-21 over fresh + pre-existing local configs; diff confined to list+matchers |
| R-13 | Medium | index, queue, delta | AC-13 benchmark; sync-path fs spy (zero queue I/O, zero delta I/O except RQ-6 tail read) |
| R-16 | Medium | state, queue, init-remote, transport-http | Token-leak scans: argv, settings.json, breadcrumb, stderr, queued frames, url_host-only |
| R-17 | Medium | delta (Layer 2), parity-corpus | Pre-population behind one helper; pinned to committed fixtures; Layer 2 vs merged server |
| R-20 | Medium | parity-corpus | Drift-check non-vacuity: CI fails when generator doesn't run |
| R-15 | Low | transport-http, transform | Non-text/plain 200 → no stdout; oversized 200 body |
| R-18 | Low | init-remote | Wrong-token Ping → loud auth failure; strict Pong parse |
| R-19 | Low | build-request, state | ppid collision documented; key sanitization traversal corpus |

## AC Coverage Map

| AC | Plan file(s) | AC | Plan file(s) |
|---|---|---|---|
| AC-01 | build-request, parity-corpus | AC-09 | transport-http, delta, state |
| AC-02 | transport-http, index | AC-10 | delta (Layer 2) |
| AC-03 | transport-http, transform | AC-11 | init-remote |
| AC-04 | transform, parity-corpus | AC-12 | parity-corpus (CI section) |
| AC-05 | parity-corpus, delta (Layer 2) | AC-13 | index (benchmark) |
| AC-06 | delta | AC-14 | build-request (contract round-trip) |
| AC-07 | delta | AC-15 (amended) | queue, delta |
| AC-08 | index, delta | AC-16 | init-remote |

## Cross-Component Test Dependencies

- **Stub server helper** precedes transport-http, transform, delta, queue, index, init-remote suites.
- **Parity corpus + manifest** precede Layer 1 suites (FR-22: goldens generated BEFORE client modules).
- **F2 pre-population helper** (ONE helper, SR-11) precedes Layer 1 PreCompact and all Layer 2 runs.
- **Layer 2** requires `cargo build --release` of the merged server (C-08 satisfied).
- **stdout contract**: every test in every suite asserts stdout bytes exactly (empty or golden) —
  the client↔host boundary is checked everywhere, not in one place.
- **ass-071 freebie**: the Layer 2 harness, when it drives a SubagentStop event, writes the raw
  stdin payload to a debug artifact under `product/features/vnc-026/testing/` (advisory; no assertion).
- **Unknown-field parity** (ass-071 carry-in): build-request plan includes the `extra`-flatten
  preservation case — unknown stdin fields survive into the request unreordered/undropped.

## Integration Harness Plan (infra-001)

**Suites applicable**: `smoke` only (mandatory minimum gate per "any change at all").

**Rationale**: vnc-026 makes **zero server-side production changes** (C-07). The only Rust change
is an additive dev-test (corpus generator) in `unimatrix-server`, which cannot alter MCP-visible
behavior. The infra-001 harness exercises the MCP JSON-RPC stdio interface; this feature's surface
is the HTTP `/observe` endpoint consumed by a Node client — outside infra-001's scope. Stage 3c runs:

1. `cargo test --workspace 2>&1 | tail -30` (includes the new generator dev-test)
2. `cd product/test/infra-001 && python -m pytest suites/ -v -m smoke --timeout=60` — regression
   gate proving the dev-test addition didn't disturb the binary.

**No new infra-001 tests planned**: hook-client integration behavior (deltas, drops, elision,
concurrency, parity) lives in the feature's own `node:test` Layer 2 suites under
`packages/unimatrix/test/` per C-04/NFR-06 (cumulative `node:test` infra). Adding an HTTP-hook-client
dimension to infra-001 would be harness infrastructure change — out of scope; if a durable
HTTP-client harness is wanted later, file a GH Issue.

**Failure triage**: per USAGE-PROTOCOL.md — feature-caused → fix; pre-existing → GH Issue + xfail;
bad assertion → fix test. Never fix unrelated failures in this PR.

## Gate-Blocking Items for Stage 3c

1. Smoke suite green (mandatory).
2. AC-15 evaluated against the AMENDED letter only.
3. Ownership-regex spaced-path fix (WARN 2) must land before the AC-11 pattern table freezes —
   init-remote plan carries the obligation.
4. AC-12 CI must include OS runners AND the drift-check job must fail-not-skip (R-20).
