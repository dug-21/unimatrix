# Gate 3b Report: vnc-026

> Gate: 3b (Code Review)
> Date: 2026-06-07
> Result: PASS
> Branch: feature/vnc-026

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | index/delta/state/transform/config/transport trace to validated pseudocode + ADRs; all documented deviations (1-6) traced and adjudicated |
| 2. Architecture compliance | PASS | ADR-001..008 all honored; component boundaries maintained; C-07 holds (server change is test-only) |
| 3. Interface implementation | PASS | HookRequest union, delta payload, envelope template, mergeSettings commandSource — all match Architecture integration surface |
| 4. Test case alignment | PASS | 421 hook-client tests (419 pass / 1 todo / 1 skip), Layer 1 + Layer 2 + contract + benchmark suites map to component test plans |
| 5. Code quality | PASS | cargo build clean; no TODO/FIXME/unimplemented in production; no `.unwrap()` in non-test code; all source files <= 500 lines |
| 6. Security | PASS | No hardcoded secrets; session-key sanitization (traversal defense); body guards; text/plain content-type guard; token never logged; cargo audit only pre-existing transitive advisory |
| 7. Risk coverage (test plans -> risks) | PASS | R-01..R-20 mapped to tests/CI; Critical R-01/R-14 and the four pinned ADR-008 R-06 assertions exercised |
| 8. Knowledge stewardship | PASS (1 WARN) | All implementation agents have `## Knowledge Stewardship` blocks with Queried/Stored entries; agent-10 documents an MCP-unavailable environmental blocker (WARN) |

## Detailed Findings

### Check 1 — Pseudocode fidelity
**Status**: PASS
**Evidence**:
- `index.js` mirrors `hook.rs::run()` step-for-step: read stdin (fd 0, 1 MiB cap) -> defensive parse -> normalize -> resolve cwd/root/config -> buildRequest -> SubagentStart fallback -> sync/FNF dispatch -> exit 0. Sync/FNF split matches `hook.rs:244-251`.
- `delta.js` implements end-anchored elision (`offset = effectiveEnd - byteLength(bytes)`), uniform offset advance (`offset + byteLen`), rewrite guard (`file_len < last_offset` -> reset, ship nothing), UTF-8 boundary trim, post-serialization body guard.
- `transform.js` emits envelopes from literal templates with the sole serializer call on the inner scalar.
- All 6 documented deviations adjudicated as acceptable (see Adjudications below).

### Check 2 — Architecture compliance
**Status**: PASS
**Evidence**:
- ADR-001 (Rust-oracle corpus): generator is a `#[cfg(test)]` module in `hook.rs`; 83-case corpus committed; drift check passes with zero diff (83 cases, MANIFEST fresh).
- ADR-002 (literal-template envelopes): `transform.js renderEnvelope` — verified no whole-envelope serialization.
- ADR-003 (state + queue mini-spec): 0700/0600 modes, O_EXCL enqueue, bounds, prune in `state.js`/`queue.js`.
- ADR-004 (deltas never queued): `index.js:266` enqueues only the carrying frame; `delta.js` re-derives on failure.
- ADR-005 (fail-open + breadcrumb + timeouts 750/2000/3000): `transport-http.js` / `state.js`.
- ADR-006 (config precedence): `config.js resolve()` — env > single root-anchored file; partial pair = misconfig; same root feeds hash.
- ADR-007 (separate concurrent POST): `index.js runFireAndForget` uses `Promise.allSettled`.
- ADR-008 (end-anchored elision): `delta.js buildElidedFrame`.
**C-07 (no server production changes)**: the only `crates/` non-test change is a 7-line `#[cfg(test)]`-gated module include in `hook.rs`; all `parity_corpus_*.rs` files are children of that test module. Zero Cargo.lock/Cargo.toml change on this branch. The Rust addition is additive, test-only.

### Check 3 — Interface implementation
**Status**: PASS
**Evidence**: HookRequest discriminated union, `transcript_delta` payload `{offset, bytes}`, SubagentStart `hookSpecificOutput` template, raw `session_id` on the wire (no `http-` prefix), `mergeSettings(filePath, commandSource, options)` with legacy-string back-compat wrapper — all match the Architecture Integration Surface table.

### Check 4 — Test case alignment
**Status**: PASS
**Evidence**: `node test/run-hook-client.js`: tests 421, pass 419, fail 0, skipped 1, todo 1. Layer 2 (real F2 server): 8/8 pass including AC-05 drops, AC-07 elision (four pinned ADR-008 items), AC-10 concurrency. Contract round-trip, benchmark, size, zero-dep suites all present and aligned to per-component test plans.

### Check 5 — Code quality
**Status**: PASS
**Evidence**:
- `cargo build --workspace`: Finished (warnings only, pre-existing).
- No `TODO`/`FIXME`/`unimplemented!`/`todo!` in production JS or the additive Rust.
- No `.unwrap()` in non-test code (JS modules are wrapped, never-throw; Rust additions are test-only where unwrap is acceptable).
- File sizes: largest JS is `build-request-tools.js` (452) and `init.js` (493); largest Rust is `parity_corpus_gen.rs` (500, exactly at limit). All <= 500.

### Check 6 — Security
**Status**: PASS
**Evidence**:
- No hardcoded secrets; token resolved from env/settings.local.json, never in argv/checked-in files (FR-18/C-06).
- Boundary validation: defensive stdin parse (lone-surrogate parity, type checks), session-key sanitization (`^[A-Za-z0-9_-]{1,64}$` else sha256 prefix) defeats path traversal into the state dir.
- No path traversal in file ops; positioned reads bounded; rewrite/TOCTOU guarded (ship nothing, never negative span).
- Deserialization never panics (every fs/JSON call wrapped, degrades to default).
- Response handling: `text/plain` content-type guard (R-15) prevents server-injected non-text into host stdout; token/full-URL never logged (R-16).
- `cargo audit`: 1 vulnerability — RUSTSEC-2023-0071 (rsa Marvin Attack, medium, no upstream fix), transitive via `sqlx-mysql`. PRE-EXISTING (zero dependency change on this branch; F3 adds zero Rust runtime deps). Remaining entries are unmaintained-crate warnings. Not introduced by and not actionable within vnc-026.

### Check 7 — Risk coverage (test plans address Risk-Based Test Strategy)
**Status**: PASS
**Evidence**: Risk tags R-01..R-19 referenced across the hook-client suite; R-12/R-17/R-20 covered by merge-settings/Layer-2/CI-drift respectively. Critical risks: R-01 (parity corpus 83 cases + drift check), R-14 (CI matrix Node 18/20/22/24 x Linux/macOS/Windows). R-06 four pinned ADR-008 Layer-2 assertions exercised and passing. R-20 drift check has three explicit non-vacuity guards (generator-ran, MANIFEST mtime advanced + case_count>0, zero git diff). CI adds 4 jobs: hook-client-matrix, zero-dep+size audit, parity-drift, Layer 2 integration.

### Check 8 — Knowledge stewardship compliance
**Status**: PASS (1 WARN)
**Evidence**: All 13 reviewed implementation/rework agent reports (agents 3-10, 14, 16, 17, 19, 20) contain a `## Knowledge Stewardship` block with `Queried:` evidence and a `Stored:` or "nothing novel to store -- {reason}" entry. Stored patterns: #4766, #4767, #4768, #4769, #4770, #4774, #4776, #4777, #4779, #4780.
**WARN**: agent-10 (build-request) reports MCP tools "NOT AVAILABLE" in its session — block present with a documented reason and the intended-store pattern captured for a future steward. Honest documentation of an environmental blocker; non-blocking.

## Adjudication of Documented Deviations

| # | Deviation | Disposition |
|---|-----------|-------------|
| 1 | `stdout-subagent-non-entries-fallback` parity todo | ACCEPTED — marked as visible `node:test` todo (suite fail 0); root cause is an F1/F2 wire-contract limitation (text/plain erases Entries-vs-BriefingContent), remedy is server-side (C-07 out of scope); client implements ADR-002 letter, pinned by `transform.test.js`; cross-refs lesson #4778. WARN-acceptable, not a FAIL. |
| 2 | Elision anchored at effectiveEnd | ACCEPTED — implemented per pseudocode (`delta.js buildElidedFrame`), Gate 3a WARN resolution. |
| 3 | state.js sanitizes session keys internally; delta passes raw session_id | ACCEPTED — confirmed `maybeSendDelta` passes raw id; `state.offsetPath` sanitizes (pattern #4772). |
| 4 | `queue.queueDepth` used | ACCEPTED — `index.js` calls `queue.queueDepth(config.stateDir)`; pseudocode's `state.queueDepthFast` did not exist. |
| 5 | Size-gate comment trim (8c0c1878) | ACCEPTED — behavior-preserving; code still traces to pseudocode. |
| 6 | AC-13 full-spawn p50 25.7 ms | ACCEPTED — client-work p50 0.07 ms (~100x margin under NFR-01's measured path); overage is Node cold-start (~11.6 ms) in arm64 container; documented in `testing/ac-13-benchmark-results.json`. WARN-acceptable. |

## Out-of-Scope Pre-Existing Failure (not a gate blocker)
`test_creates_mcp_json_on_clean_project` (init.test.js) fails in the full package suite. Confirmed: file untouched by this branch (no diff vs main), predates the feature (nan-004/#221), exercises the local-binary `.mcp.json` path which F3 does not modify. Documented and out of scope for vnc-026.

## Observations (non-blocking WARNs)
- Payload size 97.7 KB is close to the 100 KB ceiling (NFR-03); CI size check guards it but headroom is small for future additions.
- AC-13 spawn target should be read against the isolated client-work measurement, not raw spawn wall-time, on CI-class hardware (already documented in the artifact).

## Rework Required
None.
