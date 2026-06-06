# parity-corpus — Rust Golden Generator + Layer 1/2 Suites (ADR-001)

## Purpose
The Rust hook is the oracle. An additive Rust dev-test generates committed goldens under
`packages/unimatrix/test/fixtures/parity/`; CI regenerates and diffs (zero-diff gate) and
must FAIL — not skip — if the generator does not run (R-20). The corpus is the F6
retirement evidence. Build FIRST (FR-22: goldens exist before client modules).

## Corpus Layout
```
packages/unimatrix/test/fixtures/parity/
  MANIFEST.json                       // see below — the R-02 completeness map
  {case-name}/
    stdin.json                        // raw stdin bytes fed to both clients (may be invalid JSON / empty)
    event.txt                         // argv event name (one line)
    transcript.jsonl                  // optional (SubagentStart tail cases)
    expected-request.json             // normalized build_request output (or marker for RecordEvents)
    expected-stdout.bin               // optional: golden stdout bytes for a fixture HookResponse
    response.json                     // optional: the HookResponse fed to the stdout layer
```

## Generator (additive Rust dev-test)

Location: `crates/unimatrix-server/src/uds/parity_corpus_gen.rs`, wired as
`#[cfg(test)] #[path = ...] mod parity_corpus_gen;` from `hook.rs` (same pattern as
`transcript_block_tests.rs`) — a same-crate test module so it can call the PRIVATE
oracle fns (`build_request`, `parse_hook_input`, `normalize_event_name`,
`format_injection`). NO production code changes (C-07).

```
#[test]
#[ignore = "corpus generator — run explicitly in CI / regen"]
fn generate_parity_corpus():
  out_dir = env UNIMATRIX_PARITY_DIR else panic!(
    "UNIMATRIX_PARITY_DIR not set — run via CI drift job or scripts/regen-parity.sh")
  for case in CASE_TABLE:                     // the ADR-001 mandatory inventory, in code
    write stdin.json / event.txt / optional transcript.jsonl (constructed content)
    input  = parse_hook_input(&stdin_bytes_as_str)        // oracle parse incl. defensive arm
    (canonical, provider) = normalize_event_name(&event)
    input.provider = Some(provider)
    effective = if canonical == "__unknown__" { raw } else { canonical }
    req = build_request(&effective, &input)
    req = apply SubagentStart fallback exactly as run() step 5b (uses transcript.jsonl path)
    json = serde_json::to_value(&req)
    normalize_volatile(&mut json)             // REQUIRED for a stable drift check:
      // every "timestamp" field (RecordEvent flatten + each RecordEvents element) → 0
      // any session_id matching ^ppid-\d+$ → "ppid-X"
    write expected-request.json (pretty, trailing newline)
    if case.response: write response.json + expected-stdout.bin where stdout bytes =
      reconstruction of write_stdout / write_stdout_subagent_inject using format_injection
      (pub(crate) — callable) + the verbatim envelope/println expressions, with a comment
      pinning to hook.rs:963-1006
      // (write_stdout writes to the real process stdout and cannot be captured
      //  in-process without refactor; the reconstruction uses the SAME serde_json
      //  serializer + the same expressions — cross-language drift risk preserved=0.
      //  Flagged as the one accepted oracle-indirection in the design.)
  write MANIFEST.json
```

### MANIFEST.json (R-02 / A-3 completeness map — gate-reviewed)
```
{ "generated_by": "parity_corpus_gen.rs",
  "case_count": N,
  "arms": { "<hook.rs arm or helper branch>": ["case-name", ...], ... } }
```
Every match arm / early return in `build_request`, `normalize_event_name`,
`build_cycle_event_or_fallthrough`, `is_bash_failure`, `extract_file_path`,
`extract_rework_events_for_multiedit`, and `transcript_block.rs` (`build_exchange_pairs`
branches, budget loop, truncate paths) maps to ≥1 named case. A Rust-side assertion in
the generator fails if an arm key has an empty case list (R-02 scenario 2 approximation;
a new arm without a manifest entry surfaces at corpus review).

### Case inventory = ADR-001 mandatory list (verbatim, abridged here)
All 13 canonical events; 3 Gemini aliases; unknown-event passthrough; empty stdin;
malformed stdin; wrong-typed named field; missing session_id (ppid); missing cwd;
UserPromptSubmit empty/whitespace/4-word/5-word/long; PostToolUse Bash exit_code
0/nonzero/missing/non-integer × interrupted; Edit/Write/MultiEdit (normal/empty/missing/
non-array edits); non-rework tool; non-claude-code provider; PostToolUseFailure variants;
context_cycle bare/prefixed/near-miss/invalid-params/mcp_context-promotion/goal-overflow
multi-byte; SubagentStart prompt_snippet + transcript-tail variants; adversarial content
(control chars, emoji, lone-surrogate-adjacent, quotes/backslashes, U+2028/U+2029);
stdin exactly 1 MiB and 1 MiB + 1; unknown-extra-field preservation (flatten parity).

## CI Wiring (R-20 — fail, never skip)
```
job parity-drift:
  1. cargo test -p unimatrix-server --lib -- --ignored generate_parity_corpus
       with UNIMATRIX_PARITY_DIR=$PWD/packages/unimatrix/test/fixtures/parity
  2. grep the cargo output for "1 passed" (non-vacuity: 0 matched tests must FAIL the job)
  3. assert MANIFEST.json case_count > 0 AND file mtime changed during this job
  4. git diff --exit-code packages/unimatrix/test/fixtures/parity   (zero-diff gate)
```
A deliberate hook.rs behavior change = rerun generator locally
(`scripts/regen-parity.sh`), commit the diff — explicit and reviewable.

## JS Suites

### Layer 1 (node:test, extend packages/unimatrix/test/)
```
parity-request.test.js: for each corpus case:
  out = buildRequest pipeline (parseHookInput → normalize → buildRequest → SubagentStart fallback)
  normalizeVolatile(out)                      // same rules as the generator
  deepStrictEqual(out, expected-request.json) // structural — key order NOT asserted
parity-stdout.test.js: spawn the real client against a stub server that replays
  response.json with the right Content-Type; capture stdout; byte-compare (Buffer.equals)
  against expected-stdout.bin. Server-buffer pre-population for PreCompact cases is
  isolated behind ONE helper (test/helpers/buffer-prepopulate.js — SR-11/R-17).
contract.test extension (AC-14): client-built frames (incl. a delta frame) validate
  against crates/unimatrix-engine/bindings/fixtures/*.json incl. transcript_delta_payload.json.
```

### Layer 2 (integration, merged F2 server — C-08 satisfied)
```
- real server process; streamed deltas with injected drops across ≥10 spawns;
  final buffer content-equivalent to the transcript modulo elision markers.
- elision run (R-06): outage → >64 KiB growth → catch-up → next delta; assert the four
  pinned ADR-008 items via the Layer-2 helper: holes == [(last_offset, end − byteLen)],
  high_water == end, contiguous_tail crosses the seam post-catch-up, no NUL bytes served;
  session continues to a correct PreCompact restoration (W5-with-a-hole).
- AC-10/FR-26 concurrency: ≥8 interleaved sessions with drops; per-session byte tagging
  (ass-069 PoC method); each buffer holds only its own bytes; server keys are
  http-{session_id} (raw id on the wire — no client prefix).
```

### CI matrix (AC-12 + R-14)
Node 18/20/22/24 × {ubuntu, macos, windows} for the hook-client suite; zero-dep audit
(`package.json` has no `dependencies`); `lib/hook-client/` size check < 100 KB; the
parity-drift job (Linux only — goldens are platform-independent: generator normalizes
volatile fields and uses LF-pinned fixture bytes; `.gitattributes` marks
`test/fixtures/parity/** -text` to prevent CRLF mangling of golden bytes).

## Error Handling
- Generator panics loudly on any unwritable path/IO error — it never half-writes a corpus
  (write to temp dir, then move per-case dirs into place).
- JS suites treat a missing corpus dir as a hard failure (never skip — vacuous-pass
  guard, evidence #4452).

## Key Test Scenarios (meta)
1. Drift check catches a 1-byte golden edit (sanity canary committed as a test of the gate).
2. Generator run twice → byte-identical output (volatile normalization proven).
3. MANIFEST arm coverage reviewed at the test-plan gate: no `build_request` arm without
   a named case (R-02 coverage requirement).
4. AC-13 benchmark harness (≥50 iterations + warmup, server stubbed) measures the full
   spawn path incl. hash derivation, root walk, breadcrumb write; p50 ≤ ~12 ms,
   p95 ≤ 20 ms; results committed under product/features/vnc-026/testing/.
