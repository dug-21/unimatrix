# Test Plan: parity-corpus (Rust generator + Layer 1/2 suites + CI gates)

ADR-001 (Rust hook is the oracle). Risks: R-01, R-02, R-03, R-20; AC-01, AC-04, AC-05, AC-12.

## Corpus Structure

`packages/unimatrix/test/fixtures/parity/{case-name}/`:
- `stdin.json` (+ optional `transcript.jsonl`) → `expected-request.json` (+ `expected-stdout.bin` for stdout-layer cases with a fixture `HookResponse`).
- `MANIFEST.md` (committed): table mapping **every** `build_request` match arm / early return in `hook.rs::build_request`, `normalize_event_name`, and `transcript_block.rs` to ≥1 named case (R-02; one-pass completeness, #1203). Reviewed at the test-plan gate.

## Rust Generator (additive dev-test in `unimatrix-server`)

- `parity_corpus_generator` dev-test: reads case `stdin.json` inputs, runs the real `normalize_event_name` + `build_request` (+ SubagentStart fallback) and `write_stdout*` against fixture HookResponses, writes goldens into `packages/` via env-var path (`UNIMATRIX_PARITY_OUT`), run from workspace root in CI.
- `test_generator_branch_coverage` — Rust-side assertion that the generator exercised every `build_request` arm; FAILS if a new arm appears without a corpus case (R-02 scenario 2).
- No production code change (C-07); `cargo test --workspace` runs it locally.

## Comparison Rules (Layer 1 runner)

- Requests: structural JSON equality after volatile-field normalization — `timestamp` → 0, session ids matching `ppid-\d+` → `ppid-X`. Nothing else normalized (unknown-field preservation must survive comparison — ass-071 carry-in).
- Stdout: **byte-identical** vs `expected-stdout.bin` (AC-04/AC-05 Layer 1).
- No hand-written expected values anywhere (#2984) — goldens only from the generator.

## Layer 1 Suite (AC-01 / AC-04 / AC-05)

- `parity-layer1.test.js` — iterates every corpus case: run `buildRequest` (+ transform for stdout cases) and compare per the rules above. PreCompact cases compared against an identically pre-populated F2 buffer via the ONE pre-population helper (SR-11).
- Case inventory: the full ADR-001 mandatory edge-case inventory (enumerated in build-request.md, transcript.md, normalize.md, transform.md — those files own per-area details; the manifest is the master list).

## Layer 2 Suite (AC-05 / AC-10)

Owned by delta.md (elision/drops/concurrency runs vs the merged F2 server). This plan owns the
shared helper: spawn `target/release/unimatrix-server` (HTTP mode), pre-populate/inspect buffer
state through wire-level calls + committed fixtures only — never vnc-025 internals (R-17). Helper
exposes the four ADR-008 pinned-assertion checks used by delta.md.

## CI Gates (AC-12, R-14, R-20)

- **Matrix**: Node 18/20/22/24 × {ubuntu, macos, windows} (R-14 approved expansion). Full `node:test` hook-client suite green on all cells.
- **Drift check (R-20 — must FAIL, not skip)**: job runs the Rust generator from the workspace, then `git diff --exit-code packages/unimatrix/test/fixtures/parity/`. Non-vacuity guards: (a) generator writes a run-marker (timestamp/case-count file) the job asserts exists and is fresh; (b) the job fails if the generator test was filtered out / didn't execute (assert cargo test reports ≥1 test run for the generator name); (c) deliberately corrupting one golden in a meta-test (or one-time verification) proves the diff gate trips (#4452 vacuous-pass lesson).
- **Zero-dep audit**: `package.json` has no `dependencies`; require-graph scan of `lib/hook-client/` resolves only Node built-ins.
- **Size check**: `lib/hook-client/` total < 100 KB.
- **Benchmark job** (AC-13): see index.md; results artifact committed under `product/features/vnc-026/testing/`.

## Maintenance Contract

- The corpus is F6 retirement evidence: cases are never deleted/thinned; a deliberate hook.rs
  behavior change regenerates goldens as an explicit, reviewable diff.
