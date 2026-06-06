# Risk-Based Test Strategy: vnc-026

**Inputs**: SCOPE.md, ARCHITECTURE.md + ADR-001..008, SPECIFICATION.md (FR-01..26, AC-01..16),
SCOPE-RISK-ASSESSMENT.md (SR-01..SR-11, A-1..A-4).
**Historical evidence**: Unimatrix #4473 (warn+continue masks failure-path tests, vnc-017),
#4321 (trust-boundary input validation is fix-before-merge, vnc-013), #4452 (vacuous-pass
regression guards, vnc-016), #2984 (test expected-values copied from wrong input set),
#1203 (validate all files in one pass), #4703/#4720/#4726 (vnc-024), #4754 (ADR-004).

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `build-request.js` behavioral divergence from the Rust oracle on edge cases (rework extraction, MIN_QUERY_WORDS, context_cycle interception, JSONL tail-parse) | High | High | Critical |
| R-14 | Cross-platform stdin/path divergence: Windows/macOS stdin-fd, path-separator, and chmod quirks fail silently under fail-open (exit 0) — a total Windows failure would be invisible. Windows/macOS support is the feature's raison d'etre | High | High | Critical |
| R-02 | Parity corpus incompleteness — `hook.rs` behavior not represented by a corpus case is invisible to AC-01/AC-05/AC-14 (A-3) | High | Medium | High |
| R-04 | UTF-8 boundary trim bug in `delta.js` corrupts byte-offset accounting permanently (off-by-N persists in `last_offset` for the session's lifetime) | High | Medium | High |
| R-09 | Spawn-time config resolution silently fails: nested `.git` in monorepos, Claude Code rewriting `settings.local.json` and dropping the `unimatrix` key, partial env pair — remote session loses all learning with zero signal | Medium | High | High |
| R-10 | Post-init credential expiry is invisible by design (ADR-005); if the breadcrumb itself is wrong (misclassified failure, stale `queue_depth`, unwritable state dir) the only diagnostic surface lies | Medium | High | High |
| R-11 | Ownership-pattern regex false negative/positive in `merge-settings.js` → duplicated or orphaned hook entries on re-run, mode switch (local→remote), or paths containing spaces | Medium | High | High |
| R-03 | Stdout envelope byte divergence on adversarial content — `JSON.stringify` vs serde_json escaping of control chars / non-BMP / lone surrogates inside the literal template's one serializer call | High | Low | Medium |
| R-05 | Concurrent-spawn offset regression (last-writer-wins on `offsets/{key}.json`) re-ships spans; correctness depends entirely on F2 merge idempotence | Medium | Medium | Medium |
| R-06 | Elision frame geometry regression: ADR-008 pins elided frames end-anchored (`offset = file_len − bytes.length`) against merged F2 (PR #692); a span-start regression silently recreates the phantom hole and permanent PreCompact starvation | High | Low | Medium |
| R-07 | Delta livelock: a persistently failing delta (server 413 despite client guard, or auth failure) never advances the offset — identical failing send re-attempted on every FNF spawn forever | Medium | Medium | Medium |
| R-08 | Queue misbehavior: duplicate replay by concurrent spawns (delete-after-2xx race), poison-pill handling, drop-oldest eviction under burst, age-prune correctness | Medium | Medium | Medium |
| R-12 | RQ-8 local `HOOK_EVENTS` fix regresses existing local installs (blast radius beyond F3) | High | Low | Medium |
| R-13 | Spawn budget erosion: hash derivation + root walk + health.json write on every spawn; replay leakage onto the sync path; slow filesystems (A-1) | Medium | Medium | Medium |
| R-16 | Token leakage: argv, `settings.json`, breadcrumb, stderr one-liners, queued frames, or full URL in `health.json` | High | Low | Medium |
| R-17 | F2 (vnc-025, merged PR #692) semantic drift breaks the Layer-1 pre-population helper or the offset-bounded merge assumptions the client encodes (A-2, SR-11) | Medium | Medium | Medium |
| R-20 | Corpus-generator CI wiring failure: the Rust dev-test writes goldens into `packages/` via env var — if CI silently skips it, the drift check passes vacuously (evidence #4452, #3253) | Medium | Medium | Medium |
| R-15 | Sync stdout is server-controlled: a compromised/misbehaving server injects arbitrary text into the host CLI context; non-`text/plain` 200 must be dropped | Medium | Low | Low |
| R-18 | init Ping false confidence: Pong returned without actually exercising Bearer auth, or Pong JSON parsed loosely — init passes, every later spawn 401s | Medium | Low | Low |
| R-19 | ppid-fallback session collision: two sessions missing `session_id` under the same parent share `ppid-{N}` → shared offset file, cross-session delta attribution | Low | Low | Low |

## Risk-to-Scenario Mapping

### R-01: build-request.js parity divergence (Critical)
**Impact**: Wrong requests reach the server for some event shapes; remote learning silently degrades; F6 retirement evidence is invalid.
**Test Scenarios**:
1. Full ADR-001 mandatory edge-case inventory executed as Layer-1 corpus cases (13 events, Gemini aliases, unknown-event passthrough, empty/malformed stdin, ppid fallback, 4-vs-5-word boundary, Bash exit_code 0/non-zero/missing/non-integer, MultiEdit normal/empty/missing/non-array `edits`, PostToolUseFailure null/empty `extra`, context_cycle near-miss names, goal > MAX_GOAL_BYTES at a multi-byte boundary).
2. SubagentStart transcript-tail variants: malformed JSONL lines, window starting mid-line, multi-byte char split at the 12,000-byte window edge, thinking-only turns, tool_use/tool_result pairing, missing file, empty `transcript_path`.
3. CI drift check: regenerate goldens from the Rust oracle and diff — zero-diff required (R-20 guards this check itself).
**Coverage Requirement**: Every case in the ADR-001 inventory has a committed fixture + golden; structural JSON equality after volatile-field normalization (`timestamp`→0, `ppid-\d+`→`ppid-X`). No hand-written expected values (#2984).

### R-14: Cross-platform stdin/path divergence (Critical)
**Impact**: On Windows the client throws on every spawn before any network call; exit-0 + no stderr visibility means the install appears healthy while shipping nothing. macOS path/mode quirks degrade silently.
**Test Scenarios**:
1. FR-01 mandates `fs.readFileSync(0)` (the `/dev/stdin` discrepancy is resolved in the spec); unit test reads stdin via fd 0 with piped, empty, and >1 MiB inputs — on Windows runners too (fd-0 reads behave differently for pipe vs console stdin).
2. CI runs the hook-client suite on at least one Windows and one macOS runner in addition to Linux (the AC-12 Node 18/20/22/24 matrix must not be Linux-only).
3. Path-sensitive units exercised cross-platform: project-root walk with backslash separators, `os.homedir()` state-dir creation, `chmod` 0600/0700 no-op behavior on Windows (must not throw).
**Coverage Requirement**: Green suite on Linux + macOS + Windows for entry, config resolution, state dir, and queue modules; an explicit "spawn produces a POST on Windows" smoke test.

### R-02: Parity corpus incompleteness (High)
**Impact**: A `hook.rs` behavior with no corpus case ports wrong and no gate notices — the exact A-3 blind spot.
**Test Scenarios**:
1. Coverage audit: map every match arm / early-return in `hook.rs::build_request`, `normalize_event_name`, and `transcript_block.rs` to at least one corpus case; commit the mapping table as a corpus manifest (one-pass completeness check, #1203).
2. Rust-side branch coverage of the generator run: assert the generator exercises every `build_request` arm (fails if a new arm appears without a corpus case).
**Coverage Requirement**: Corpus manifest reviewed at the test-plan gate; no `build_request` arm without a named case.

### R-04: UTF-8 boundary trim corrupts offset accounting (High)
**Impact**: `last_offset` drifts from true byte position; every subsequent delta mis-aligns; F2 merge receives overlapping/garbled spans for the session's remainder.
**Test Scenarios**:
1. Transcript ending mid-2-byte, mid-3-byte, and mid-4-byte UTF-8 sequence: span end backs off to the last complete char; persisted offset equals bytes actually shipped; next spawn's span starts boundary-clean.
2. Span entirely inside one multi-byte char (file grew by 1-3 continuation bytes): ship nothing, offset unchanged.
3. Growth-replay sequence test: grow file in adversarial increments across 10 spawns; final server buffer byte-equals the transcript file (Layer 2).
4. Truncation + boundary interaction: 64 KiB cap landing mid-char at both the head-48 KiB and tail-12 KiB cut points.
**Coverage Requirement**: Property-style test over random multi-byte content proving `sum(shipped spans) == contiguous prefix` invariant; AC-06/AC-07 assertions on persisted offset values, not just POST presence.

### R-06: Elision frame geometry regression (Medium — re-graded under ADR-008)
**Re-grade rationale**: Originally High/Medium as an open client/server *disagreement* deferred until F2 merged. vnc-025 merged (PR #692) and ADR-008 pins the elided frame **end-anchored** against the merged `apply_delta`/`contiguous_tail` code (`crates/unimatrix-server/src/infra/session_transcript.rs`) — the disagreement no longer exists by design, and no F2 rework is needed. Residual risk is a client implementation regression: declaring the span-start offset (the originally spec'd shape) instead of the pinned formula. Severity stays **High** — a regression is silent and permanent per session (phantom unfillable hole, displaced tail bytes, `contiguous_tail` floors at `file_len`, the entire 61 KiB catch-up becomes unservable; ADR-008 Context). Likelihood drops **Medium → Low** — one pinned formula, semantics frozen against merged code, and the Layer-2 assertions below detect any regression deterministically. Priority: **Medium**.
**Impact**: A span-start regression yields `high_water < file_len` at apply time, tail bytes at wrong logical offsets, a phantom zero-filled hole on the next delta, and permanent PreCompact starvation — with no client-visible error.
**Test Scenarios**:
1. Integration (merged F2 server): outage → >64 KiB growth → catch-up delta with elision marker → next normal delta; assert all four pinned Layer-2 assertions below and that the post-elision delta extends contiguously at `file_len` (no further holes).
2. Frame-shape assertion (ADR-008): elided frame is end-anchored — declared `offset == file_len − bytes.length` (NOT the span start), `bytes = head(48 KiB) ++ marker ++ tail(12 KiB)`, frame ends exactly at `file_len`; client `last_offset` advances to `offset + bytes.length == file_len` (uniform ADR-004 rule, no truncation special case); AC-07 verifies no elided bytes are ever re-sent.
3. Escape-heavy oversized content (control-char dense, ~6x inflation): post-serialization size check triggers; frame < 1 MiB.
**Pinned Layer-2 helper assertions** (ADR-008, verified against merged `session_transcript.rs`):
- Hole forms **behind** the content at apply time: `holes == [(last_offset, file_len − bytes.length)]`, size `N_elided − marker_len`; server `elided_bytes` counter unchanged (client-side elision is invisible to it).
- `high_water == file_len` — server coverage and client `last_offset` agree.
- `contiguous_tail(12000)` returns pure client-tail bytes immediately after the elided frame (never zero-fill, never crossing the hole) and crosses the elision seam naturally once subsequent deltas extend at `file_len` (W5-with-a-hole).
- No NUL bytes are ever served (I4/FR-19) — zero-fill never escapes `contiguous_tail`.
**Coverage Requirement**: At least one Layer-2 run where elision occurs mid-session and the session continues to a correct PreCompact restoration (W5 with a hole), asserting all four pinned items.

### R-09: Silent config-resolution failure (High)
**Impact**: Remote sessions contribute nothing, indefinitely, with no signal — the highest-frequency real-world failure shape for fail-open clients (#4473).
**Test Scenarios** (ADR-006 obligations, all asserted on breadcrumb + stderr + exit 0 + no network):
1. Spawn from a subdirectory cwd; stdin `cwd` ≠ `process.cwd()`; stdin `cwd` empty.
2. Missing `settings.local.json`; file present without `unimatrix.remote`; malformed JSON in the file.
3. Env override beating a present file; exactly one env var of the pair set (`auth` class breadcrumb).
4. Nested-`.git` monorepo: nearest root wins; config identity and state-dir hash derive from the same root string (split-brain assertion).
5. Claude Code rewrite simulation: `settings.local.json` with the `unimatrix` key removed after init → next spawn degrades silently; init re-run restores it idempotently.
**Coverage Requirement**: Full FR-06 matrix automated; every no-config path proves no network attempt (transport spy).

### R-10: Breadcrumb wrong or missing — diagnosis surface lies (High)
**Impact**: ADR-005's entire SR-10 mitigation depends on `health.json` accuracy; a wrong breadcrumb is worse than none.
**Test Scenarios**:
1. Failure-class matrix: ECONNREFUSED→`connect`, timeout→`timeout`, 401/403→`auth`, 404/413→`http_4xx`, 500→`http_5xx`; `consecutive_failures` increments across spawns and resets on success; `queue_depth` matches actual queue file count.
2. Content-free assertion: breadcrumb never contains token, payload fragments, transcript bytes, or full URL (host only) — across all failure classes.
3. Breadcrumb write failure (read-only state dir): spawn still exits 0, no stdout, send still attempted.
4. Sync-trio failures also update the breadcrumb (spawns that attempt a send).
**Coverage Requirement**: Breadcrumb state-transition test driven through the W4 outage/recovery workflow; AC-09 failure matrix extended with breadcrumb assertions.

### R-11: Ownership-pattern merge corruption (High)
**Impact**: Re-runs duplicate hook entries (host fires the client twice per event — double deltas, double observations) or orphan/destroy user hooks.
**Test Scenarios** (AC-11 matrix):
1. Fresh config; re-run over own remote entries (recognized, replaced, not duplicated).
2. Config with old-style `unimatrix hook` entries → mode switch replaces them.
3. Config with foreign hooks, including a foreign `node` command that must NOT match the new pattern; install path containing spaces (the regex's `\S*` fails on spaced paths — `require.resolve` under `C:\Program Files\` or `~/My Projects/`).
4. Double-fire detection: after two init runs, count unimatrix entries per event == 1.
**Coverage Requirement**: Regex unit tests with a positive/negative command-string table; end-to-end re-run idempotency over all four config shapes.

### R-03, R-05, R-07, R-08, R-12, R-13, R-16, R-17, R-20 (Medium)
- **R-03**: ADR-001 adversarial-content cases (control chars, emoji, lone-surrogate-adjacent, embedded quotes/backslashes) byte-compared against Rust stdout goldens; envelope produced only via the literal template (grep-gate: no `JSON.stringify` of envelope objects in `transform.js`).
- **R-05**: two concurrent spawns of one session with interleaved offset read/write; assert worst case is a re-shipped span and final server buffer is correct (F2 dedupe); offset file atomic-rename verified (no partial JSON ever observable).
- **R-07**: stub server permanently 413/401 on delta path only → assert offset never advances, no queue file appears, carrying events unaffected, breadcrumb records the class, and per-spawn cost stays bounded (one fstat + one failed POST — no growth).
- **R-08**: queue lifecycle (AC-15) plus: corrupt frame deleted and replay continues (poison-pill); 501-file and >5 MiB enqueue triggers drop-oldest; >24 h frames pruned unreplayed; two concurrent recovering spawns may double-send a frame — assert server tolerates the duplicate observation; replay budget 32 frames/256 KiB enforced, stop-at-first-failure leaves remainder.
- **R-12**: FR-21 regression test over fresh AND pre-existing local configs; full 9-event local set written and recognized on re-run; assert the diff to local mode is list+matchers only (no behavioral change elsewhere — SR-07 blast-radius gate).
- **R-13**: AC-13 benchmark includes hash derivation, root walk, AND health.json write; separate assertion that sync spawns perform zero queue I/O (fs spy on `queue/`) and zero delta I/O except the RQ-6 tail read (FR-09 narrowed wording).
- **R-16**: argv assertion (no token in hook command), `settings.json` content scan, breadcrumb/stderr scan for the token string across the full failure matrix, queued frame content scan (no `Authorization`), settings.local.json mode 0600 + gitignore warning path.
- **R-17**: buffer pre-population isolated behind one test helper (FR-23); contract tests pinned to committed fixtures, not vnc-025 internals; vnc-025 is merged (PR #692) — Layer 2 must run against the merged server before delivery gates (C-08).
- **R-20**: CI must fail (not skip) if the corpus generator does not run — assert generator output timestamp/marker in the drift-check job; goldens regenerated in CI from the workspace, diffed against committed.

### R-15, R-18, R-19 (Low)
- **R-15**: 200 with `Content-Type: application/json` (or absent) on the sync path → no stdout; oversized 200 body handling; stdout written verbatim only for `text/plain` (defensive-classification test).
- **R-18**: init Ping with a wrong token → init fails loud with `auth` message (proves Ping exercises Bearer auth, not just reachability); non-Pong 200 JSON → init fails; Pong parse is strict.
- **R-19**: two stdin payloads missing `session_id` with the same `process.ppid` → document shared offset behavior; assert parity with Rust (same fallback) and that sanitized session keys prevent path traversal (`../`, `/`, 65+ chars → hashed key).

## Integration Risks

| Boundary | Risk | Scenario |
|---|---|---|
| client ↔ F1 `/observe` | Header/negotiation mismatch: missing `Accept: text/plain` on one sync arm → raw JSON printed to the host (the exact #4703 failure) | Per-sync-event header assertion; raw-JSON-on-stdout canary test |
| client ↔ F1 wire types | Hand-drift from bindings (C-01) | AC-14 round-trip vs `bindings/fixtures/*.json` incl. `transcript_delta_payload.json` |
| client ↔ F2 merge | Offset/hole/idempotence assumptions (R-05, R-06, R-17) | Layer-2 drop/elision/concurrency runs against a real F2 server |
| client ↔ server namespacing | Client accidentally prefixing `http-` (double-prefix) | Assert raw `session_id` on the wire; server-side buffer key check in Layer 2 |
| `init.js` ↔ `merge-settings.js` | `mergeSettings` generalization to `commandSource` breaks the local call site | Back-compat wrapper test: existing local init flow byte-identical settings output |
| client ↔ host CLI | Stdout contract: anything unexpected on stdout corrupts the host (a stray `console.log`, the stderr one-liner leaking to stdout) | Every test asserts stdout bytes exactly (empty or golden); no logging framework on the stdout fd |
| queue dir ↔ Rust queue | Cross-format reads if dirs ever collide | Assert distinct paths (`event-queue/` vs `hook-client/queue/`); no shared files |

## Edge Cases

- Empty stdin / EOF-immediately / stdin exactly 1 MiB / stdin 1 MiB + 1 (cap behavior parity with Rust).
- Transcript file: zero-length; deleted between fstat and read (TOCTOU — read throws → ship nothing, offset unchanged); replaced by a shorter file (`file_len < last_offset` → reset, ship nothing — A-4/FR-11); replaced by an identical-length different file (undetectable — document as accepted).
- Offset file: corrupt JSON (treat as 0? must be defined — re-ship from 0 is safe via idempotent merge; negative or non-numeric offset must not throw); offset file for a session past 7-day prune mid-session.
- `transcript_path` pointing at a directory, a non-existent path, or a huge non-JSONL binary file (delta path is content-opaque — must still cap and ship or skip without throwing).
- Queue: same-ms same-pid enqueue collisions (`seq` bump); state dir on a full disk (every queue/offset/breadcrumb write fails — spawn still exits 0); `~` unresolvable (no HOME env).
- URL forms: trailing slash, `http://` vs `https://`, port, path prefix (`https://host/base` + `/observe`), IPv6 literal.
- Sync 200 with empty body → no stdout (silent-skip parity); 200 body exactly at/over MAX_INJECTION_BYTES (server already gates — client prints verbatim).
- Node version skew: `fetch` not used (built-ins only), but `fs.readFileSync(0)`, `process.ppid`, `Promise.allSettled` semantics identical 18→24 (CI matrix is the guard).

## Security Risks

| Surface | Untrusted input | Damage potential | Blast radius | Coverage |
|---|---|---|---|---|
| Hook stdin (host CLI, semi-trusted) | `session_id`, `cwd`, `transcript_path` | Path traversal via session_id into state dir filenames; arbitrary-file read via `transcript_path` shipped to the configured server (exfiltration if stdin is attacker-influenced) | Client state dir; bytes sent only to the user-configured server | Sanitized session keys (`^[A-Za-z0-9_-]{1,64}$` else hashed) — ADR-003 test; traversal corpus (`../../`, absolute paths, null bytes); document that `transcript_path` reads are trusted-host scope (matches Rust hook posture, #4321 trust-boundary lens) |
| Server responses (sync path) | 200 text body | Prompt injection into the host CLI context; host-envelope escaping break via crafted text in the SubagentStart template | Host conversation | R-15 scenarios; `JSON.stringify` escaping fuzz on the inner scalar (quotes, backslashes, control chars, U+2028/U+2029) |
| Bearer token | — | Leakage via argv, checked-in files, breadcrumb, stderr, queue frames | Remote server account | R-16 scenarios; NFR-10 |
| Queue frames at rest | tool_input/tool_response excerpts | Secrets readable by other local users | Local machine | 0600/0700 mode tests; 24 h prune test; assert no `transcript_delta` file ever appears in `queue/` (ADR-004 — the load-bearing at-rest guarantee) |
| `settings.local.json` | — | Token committed to git | Repo history | init gitignore warning test; mode 0600 |

## Failure Modes

Expected behavior when things go wrong (all under C-05: exit 0, no stdout on failure):

| Failure | Required behavior | Verified by |
|---|---|---|
| Server unreachable / timeout / non-2xx | Sync: silent no-injection within timeout budget (no hang). FNF: enqueue (non-delta), breadcrumb, stderr one-liner | AC-09 matrix + timeout-expiry timing test |
| Delta send failure | Offset non-advance, no queue file, carrying event unaffected (independent `Promise.allSettled` outcomes) | AC-15 delta arm; R-07 |
| Missing/broken config | No network, breadcrumb, exit 0 | R-09 matrix |
| State dir unwritable / disk full | Send still attempted; queue/offset/breadcrumb failures swallowed | R-10 scenario 3; edge cases |
| Corrupt queue frame | Deleted, replay continues | FR-15 poison-pill test |
| Transcript rewritten/truncated | Offset reset to `file_len`, ship nothing, never a negative span | FR-11 test |
| Oversized everything (stdin, delta, queue) | Cap, elide, or drop-oldest — never throw, never exceed 1 MiB body | AC-07; FR-14 |
| init failures | The ONE loud path: bad URL/token/non-Pong → actionable error, non-zero init exit | R-18; FR-19 |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 | R-01, R-02, R-20 | ADR-001 Rust-oracle goldens + CI drift check; residual risks are corpus completeness (R-02) and the drift check running at all (R-20) |
| SR-02 | R-03 | ADR-002 literal templates; residual: inner-scalar escaping on adversarial content, golden-covered |
| SR-03 | R-08, R-13 | ADR-003 replay bounds (32 frames/256 KiB, FNF-only); verified by sync-path fs-spy + bounded-replay tests |
| SR-04 | R-06, R-07 | Cap-on-raw + post-serialization assert (ARCHITECTURE delta mechanics); elision geometry pinned end-anchored by ADR-008 against merged F2 (R-06 re-graded Medium); residual: 413 livelock (R-07) |
| SR-05 | R-08 | ADR-003 full mini-spec (bounds, O_EXCL, drop-oldest, prune); scenarios cover concurrency and poison frames |
| SR-06 | R-16 (residual) | ADR-004 eliminates delta content at rest; residual queue posture (0600, 24 h prune) tested under R-16 |
| SR-07 | R-12 | RQ-8 confined to list + matchers; AC-16 regression over pre-existing configs |
| SR-08 | R-11 | Extended ownership pattern; re-run matrix incl. old-style entries; new finding: spaced-path regex gap |
| SR-09 | R-09 | ADR-006 root-anchored single-location resolution; full FR-06 matrix incl. Claude Code key-drop case |
| SR-10 | R-10 | ADR-005 breadcrumb + init Ping; residual: breadcrumb accuracy itself, now first-class tested |
| SR-11 | R-17 | F2 consumed at wire level only; single pre-population helper; Layer-2 re-run post-#670-merge |
| A-1 | R-13 | AC-13 measures the full per-spawn path on the reference env; cwd-keying fallback only on measured failure |
| A-2 | R-17 | Delivery gate C-08 (vnc-025 merge before gates) |
| A-3 | R-02 | Corpus manifest mapping every `build_request` arm to a case |
| A-4 | R-04 (adjacent), Failure Modes | FR-11 rewrite guard; tested directly |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-14) | ~30 corpus cases (ADR-001 inventory) + 5 cross-platform scenarios; CI on Linux/macOS/Windows |
| High | 5 (R-02, R-04, R-09, R-10, R-11) | ~22 scenarios: corpus manifest audit, UTF-8 boundary property test, FR-06 config matrix (8), breadcrumb transition matrix (6), merge re-run matrix (4) |
| Medium | 10 (R-03, R-05, R-06, R-07, R-08, R-12, R-13, R-16, R-17, R-20) | ~25 scenarios: adversarial-content goldens, concurrency/offset races, elision Layer-2 run with pinned ADR-008 assertions, livelock stub, queue lifecycle + bounds, AC-16 regression, AC-13 benchmark, token-leak scans, CI drift-check non-vacuity |
| Low | 3 (R-15, R-18, R-19) | ~7 scenarios: content-type defense, init auth-Ping, ppid collision + key sanitization |

**Gate-blocking notes for the tester / leader**:
1. RESOLVED — FR-01 now mandates `fs.readFileSync(0)`; the `/dev/stdin` form is gone from the spec. Still standing from this note: AC-12's CI matrix must add OS coverage (Linux/macOS/Windows), not only Node versions (R-14 scenario 2).
2. R-11 contains a design defect candidate: the ownership regex `\S*` cannot match install paths containing spaces — confirm `require.resolve` output shapes on Windows/macOS before freezing the pattern.
3. RESOLVED via ADR-008 — vnc-025 merged (PR #692) and elision semantics are pinned end-anchored against the merged buffer (`session_transcript.rs`); no F2 rework. The Layer-2 helper must assert the four pinned items under R-06: hole forms behind the content at `(last_offset, file_len − bytes.length)`; `high_water == file_len`; `contiguous_tail` crosses the elision seam; no NUL bytes ever served.
