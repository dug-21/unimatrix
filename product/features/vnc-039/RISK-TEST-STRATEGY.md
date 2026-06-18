# Risk-Based Test Strategy: vnc-039

> Pure-JS stdio→HTTPS MCP bridge (Scope A; #774 MERGED (PR #778, 913b78cb) — live cloud validation now available) + out-of-tree credential relocation (Scope B, independent, lands first).
> Inputs: SCOPE.md, ARCHITECTURE.md, ADR-001..005, SPECIFICATION.md (FR-01..27 / AC-01..11), SCOPE-RISK-ASSESSMENT.md (SR-01..09), ass-080/FINDINGS.md.
> Historical grounding: lesson #4970 (vnc-034 F1 — trust-boundary false-green; the SR-02 precedent and the live-handshake test recipe), pattern #4153/#4373 (schema-bump test cascade — SR-07), lesson #4796 (gates must not assert CI/unrun ACs as fact — SR-04).

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | **Silent token leak**: bridge flushes the bearer (`Authorization: Bearer`) before the leaf fingerprint matches `fp` — or the pin is dead code over a live handshake (vnc-034 F1 class). | High | Med | **Critical** |
| R-02 | **Per-socket re-pin gap**: persistent Streamable-HTTP session reuses a connection-pool agent or a second TLS socket that flushes a body before `verifyPeerFingerprint` runs on *that* socket. | High | Med | **Critical** |
| R-03 | **Green-on-stub stops short of live (#774 MERGED, PR #778)**: live cloud validation is now available; the hazard is delivery treating the fast stub tier as terminal instead of running the now-actionable live validation against the real endpoint. | Med | Med | High |
| R-04 | **SSE-parse correctness**: ~90-LoC `text/event-stream` parser drops/duplicates/mis-boundaries `data:` records → plausible-but-broken JSON-RPC results. | High | High | **Critical** |
| R-05 | **`Mcp-Session-Id` replay correctness**: session id not captured from `initialize` headers, or not replayed on a subsequent request → 2nd+ call 400/session-not-found. | High | High | **Critical** |
| R-06 | **Schema mismatch — a current break, not latent**: file-mode remote observe reads `unimatrix.remote.url` (never written) and never reads `fingerprint`, so it falls through to UDS today; remote observe does **not** run over HTTPS now and would be **unpinned** when wired. `config.pinnedFp` unpopulated. | High | High | **Critical** |
| R-07 | **Two-key resolution failure**: bridge and hook client index the store by different keys (slug vs `projectHash`) → one consumer silently resolves nothing. | High | Low | High |
| R-08 | **Scope B coupled to Scope A / cloud**: Scope B coverage transitively requires a reachable cloud or Scope A code → B loses its independence and cannot land first. (#774 merged (PR #778) removes the cloud-unreachability driver, but Scope B must stay `[no-cloud]` regardless.) | Med | Med | High |
| R-09 | **Token leaks to a loggable surface**: token appears in `printSummary()`, stdout/stderr, `.mcp.json`, the pin-mismatch error, or an exception trace. | High | Low | High |
| R-10 | **`.mcp.json` write clobbers/duplicates**: non-idempotent or non-merge-preserving write removes a co-resident MCP server or double-writes `unimatrix`; `--dry-run` writes. | Med | Med | Med |
| R-11 | **Legacy path silent skip**: legacy `--remote`/`--token` cloud-MCP request produces no bridge AND no signal → dead `context_*` surface with no diagnosis. | Med | Low | Med |
| R-12 | **Credential left in repo tree**: relocation leaves a token-bearing file (stale `.claude/settings.local.json`, partial write) stageable by `git add -A`. | High | Low | High |
| R-13 | **Store read error mishandled**: malformed JSON / unknown `schema_version` / ENOENT mapped to the wrong posture (e.g. bridge fails-open, or hook client throws instead of UDS-falling-through). | Med | Med | Med |
| R-14 | **Hybrid flip-bar undefined at delivery**: the SSE/session correctness overrun threshold is left to vibe → late, panic pivot importing the 91-pkg tree. | Med | Med | Med |
| R-15 | **Bundle-only boundary leak on legacy**: the universal store-write writes a legacy credential with `fingerprint: null` but the hook client then pins (or fails) on it. | Med | Low | Low |
| R-16 | **stdio framing split**: partial-line buffering drops or splices JSON-RPC messages under chunked stdin. | Med | Low | Med |
| R-17 | **Unstable session/attribution identity**: the bridge sends a non-stable `Mcp-Session-Id` or `clientInfo.name` across requests within one bridge session → the server (which keys audit attribution on them — vnc-014/#4708) bleeds attribution across sessions, undercutting 1-client:1-project integrity. | High | Med | High |

---

## Risk-to-Scenario Mapping

### R-01: Silent token leak — bearer flushed before pin matches (SR-02, ADR-001, FR-11/FR-12)
**Severity**: High · **Likelihood**: Med · **Impact**: The cleartext bearer reaches a mismatched/MITM server. This is the exact failure vnc-034 F1 shipped through three green gates (lesson #4970): the pin was DEAD CODE over a live handshake, and shape assertions passed while the boundary never executed.

**Test Scenarios** (the live-handshake recipe from #4970 — NOT shape assertions):
1. **Live good-pin**: stand up a real `https.createServer` with a self-signed leaf; compute its `fp`; drive a full MCP lifecycle through the bridge; assert it connects and round-trips.
2. **Live wrong-pin**: same server, bridge configured with a *different* `fp`; assert (a) the connection is rejected/destroyed, (b) **the capturing test server received NO `Authorization` header and NO request body** (token never crossed the boundary), (c) a loud diagnosable expected-vs-presented error to stderr + non-zero exit.
3. **Negative-control**: assert the test would FAIL if `applyCertPin` were a no-op — i.e. the wrong-pin server, if the pin were dead, *would* have received the token. (Guards against the assertion itself being vacuous.)
4. **No-shape-only rule**: a test that only asserts `rejectUnauthorized===false` / `verifyPeerFingerprint is a function` does NOT satisfy this risk and must be rejected at gate.

**Coverage Requirement**: A real `https.createServer`-backed test proving good-pin connects AND wrong-pin rejects with the token provably never written to the wire (capturing server asserts zero `Authorization` received). This is a **non-negotiable acceptance criterion** (AC-04), and the bridge is routed to **fresh-context security review even when gates are green** (the #4970 second-order lesson: same suite, same blind spot).

### R-02: Per-socket re-pin gap on the persistent connection (SR-02, ADR-001 §3.2, Assumption A3)
**Severity**: High · **Likelihood**: Med · **Impact**: The observe path is single-shot/one-socket; the bridge is persistent. If a connection-pool agent or a fresh socket flushes a body before `secureConnect`→`verifyPeerFingerprint` runs on *that* socket, the token leaks on socket #2 even though socket #1 pinned correctly — the "TLS reused at ~0 LoC" assumption silently breaks.

**Test Scenarios**:
1. **Second-request re-pin**: with a live good-pin server, drive `initialize` then `tools/call`; assert each opened TLS socket ran `verifyPeerFingerprint` before its first body byte (instrument the cert-pin seam or count `secureConnect` verifications vs sockets).
2. **No-pool assertion**: assert the bridge does not construct an `https.Agent` with `keepAlive`/pooling that could dispatch on an unverified socket (static + behavioral: force a new socket mid-session and confirm it re-pins).
3. **Mid-session cert swap**: server presents the correct leaf on socket #1 and a wrong leaf on socket #2; assert socket #2 is rejected and no token-bearing body is flushed on it.

**Coverage Requirement**: Every TLS socket the bridge opens within a session is proven to re-run the pin before its first body byte; keep-alive reuse only on an already-pinned socket.

### R-03: Green-on-stub must not stand in for live — #774 MERGED, live is now actionable (SR-04, #774/PR #778)
**Severity**: Med · **Likelihood**: Med · **Impact**: #774 merged (PR #778, commit 913b78cb, closed COMPLETED 2026-06-18) — rmcp `allowed_hosts` is now wired from `UNIMATRIX_PUBLIC_URL`, so remote MCP requests no longer 403 and live cloud validation is **available now**. The hazard is no longer "the stub is the terminal validation because the cloud is unreachable"; it is delivery **stopping at green-on-stub when live is now available**. The stub stays as a fast first tier; the live run against the real `/v1/{slug}` endpoint is a **delivery gate** (lesson #4796: never assert an unrun AC as executed fact — and now there is no #774 excuse not to run it).

**Test Scenarios**:
1. **Stub contract pinning (fast first tier)**: the stub's wire behavior (200 on `initialize` + `Mcp-Session-Id` response header; replay-required 4xx when the header is absent on a follow-up; both content types) is pinned to *observed* rmcp behavior — capture a real `initialize` POST response (the SCOPE-noted raw POST → 200) and encode its headers/framing as the stub's golden contract, with a fixture-provenance comment citing the captured response. The stub is the fast tier, not the terminal tier.
2. **Live-validation IS the gate (no longer deferred)**: every Scope-A AC (AC-03/04/05/06/09) must be live-validated against the real cloud endpoint before the coverage report may mark it validated; the gate must NOT report a Scope-A AC as live-validated on stub evidence alone. Any AC not yet live-run is tagged `pending-live-run`, not `not-validated-live (#774)` — the #774 block is gone.
3. **Active post-#774 live-validation checklist** (now that #774 has merged — IMMEDIATELY ACTIONABLE in delivery, run against the real cloud `/v1/{slug}` endpoint, in order):
   1. **FIRST: validate the rmcp 1.7.0 session-id handshake — live** — confirm whether the session id is **server-minted and returned** (in the `initialize` response `Mcp-Session-Id` header) vs **client-minted**, and that the bridge replays the server's value verbatim. This is the bridge's **hardest seam** and is **unpinnable from unimatrix-server alone** (rmcp owns the handshake), so it is the first thing exercised live; this is now a concrete delivery gate, not a deferred item. Reconcile the observed behavior back into the stub contract (R-03) and the `http-session`/R-05 + R-17 assertions.
   2. re-run AC-03/04/05/06 live: initialize→session→tools/list→tools/call round-trip, both response framings, and the live good-pin handshake against the real leaf.
   3. reconcile any stub/real divergence into the stub contract.

**Coverage Requirement**: Stub framing is provably derived from a captured rmcp response (not invented) AND every Scope-A AC is live-validated against the real cloud endpoint before being reported validated; the post-#774 live-validation checklist is **active/in-delivery** (now that #774 has merged) and referenced from the AC map; the session-id handshake (item #1) is run live first.

### R-04: SSE (`text/event-stream`) parse correctness (SR-01, FR-09, ADR-001 unit `sse-parse`)
**Severity**: High · **Likelihood**: High · **Impact**: ~90 LoC of hand-rolled SSE parsing; subtle errors (record-boundary on blank line, multi-line `data:` concatenation, CRLF vs LF, `id:`/`event:` handling, a `data:` split across TCP chunks) yield plausible-but-wrong JSON-RPC results — silent corruption, not a crash.

> **SSE-skip probe (FIRST delivery experiment — can RETIRE this risk entirely; now RUNNABLE LIVE).** With #774 merged (PR #778), the probe is **no longer blocked** — it runs **live against the real cloud endpoint early in delivery**, giving a **definitive** answer rather than a stub-derived guess. Before building the SSE parser, run the probe: send every request with `Accept: application/json` **only** (no `text/event-stream`) and exercise the full lifecycle (initialize→tools/list→tools/call, incl. a large/streaming-prone result) against the **live rmcp endpoint**. If the server returns **JSON for every response across the full lifecycle**, the ~90-LoC SSE parser is **DROPPED**, **R-04 evaporates** (the hardest correctness surface retires), and the hybrid flip-bar's SSE component (~90 LoC + the SSE half of the ~50-LoC dispatch) **drops out of the ~260-LoC surface**. The probe is the first delivery experiment precisely because it can remove the Critical SSE-parse risk before any of it is built — and now it resolves definitively live, not against a stub. If the live probe shows the server ever emits `text/event-stream`, R-04 stands in full and the scenarios below apply. The hybrid flip-bar still runs **after** the probe resolves.

**Test Scenarios** (`sse-parse` as a separately-testable unit with its own fixtures — SR-01):
1. Single `data:` line → one JSON-RPC object.
2. Multi-line `data:` (RFC-style concatenation with `\n`) → one reassembled payload.
3. Multiple events in one stream separated by blank lines → N JSON-RPC objects in order.
4. `event:`/`id:` lines present and ignored-or-honored per spec; `Last-Event-ID` carried.
5. **Chunk-boundary fuzz**: feed the same SSE byte stream split at every offset (incl. mid-`data:` and mid-record-boundary); assert identical parsed output regardless of split.
6. CRLF and bare-LF line endings both parse.
7. 1 MiB body guard enforced on the SSE path (reused `transport-http.js` constant).

**Coverage Requirement**: `sse-parse` unit tests cover single/multi-line/multi-event/chunk-split/CRLF cases with a golden corpus; parser output is byte-split-invariant.

### R-05: `Mcp-Session-Id` capture & replay correctness (SR-01, FR-02/FR-03, ADR-001 unit `http-session`)
**Severity**: High · **Likelihood**: High · **Impact**: If the bridge fails to capture the session UUID from the `initialize` *response headers*, or omits it on a later request, every post-initialize call fails (session-not-found) — the bridge appears to "half work" (initialize succeeds, tools fail).

**Test Scenarios** (`http-session` as a separately-testable unit — SR-01):
1. Capture: `initialize` response carries `Mcp-Session-Id`; assert it is retained.
2. Replay: assert `tools/list` and `tools/call` requests carry the captured `Mcp-Session-Id` request header verbatim.
3. Absent-on-initialize: server returns no session header → assert defined behavior (per spec: proceed sessionless vs fail-loud — pin to observed rmcp, R-03).
4. Distinct-from-tool-param: assert the transport `Mcp-Session-Id` is not conflated with any tool-param session_id (entry #4708).
5. Teardown: on stdin EOF, assert a best-effort `DELETE` with the session header.

**Coverage Requirement**: Capture-then-replay proven across the full lifecycle (initialize→tools/list→tools/call); replay header asserted present and equal on every post-initialize request.

### R-06: Schema mismatch fixed — a current break, not a latent risk (SR-07, FR-23, ADR-004)
**Severity**: High · **Likelihood**: High · **Impact**: This is a **break today**, not a latent hazard. The file-mode remote observe path reads `unimatrix.remote.url` — a key that is **never written** — and never reads `fingerprint`, so it falls through to UDS today; remote observe does **not** run over HTTPS now, and the moment it were wired off the existing read it would run **unpinned** (`config.pinnedFp` unset). The writer emits `{mcp_url, observe_url, token, fingerprint}`. If the relocation faithfully ports the existing read, the break survives and the observe path stays UDS-fallthrough / unpinned (a security regression hiding behind "no behavior change").

**Test Scenarios**:
1. **Canonical-read regression**: seed `~/.unimatrix/<projectHash>/credentials.json` with the canonical schema; assert the hook client's file-mode `resolve()` returns `observe_url` (not `url`) as the post target AND **`pinnedFp` is populated from `fingerprint`** (the load-bearing regression assertion — AC-08c).
2. **File-mode remote observe runs over pinned HTTPS post-fix** (the break-fix proof, not just config population): stand up a **local pinned `https.createServer`** with a self-signed leaf; compute its `fp`; seed the store with that `fingerprint`; drive the file-mode observe POST and assert it **actually runs over HTTPS to `observe_url` and is pinned** — i.e. it connects on good-pin (it could not before; it fell through to UDS) AND the request reaches the https server. Wrong-pin → connect-class failure → fail-open exit 0 (unchanged observe posture). Asserting `pinnedFp` is populated is necessary but NOT sufficient; coverage must prove the request transits pinned HTTPS.
3. **No-UDS-fallthrough regression**: with a valid file-mode remote credential present, assert observe does NOT silently fall through to UDS (the current break) — it targets `observe_url`.
4. **Old-key absence**: assert the new store has no `url` key and the hook client never reads `url`.
5. **Both consumers, one schema**: assert the bridge reads `mcp_url`/token/`fingerprint` and the hook client reads `observe_url`/token/`fingerprint`/`timeouts` from the *same* file — no per-consumer dialect.
6. **Schema-version cascade** (pattern #4153/#4373): adding/bumping `schema_version` updates the read, the write, and the test assertions together; unknown version → terminal diagnosable read error (R-13).

**Coverage Requirement**: A regression test proving file-mode remote observe **actually runs over pinned HTTPS** against a local pinned https server post-fix (good-pin connects, wrong-pin connect-class fails) — `pinnedFp` populated is necessary but not sufficient — plus the no-UDS-fallthrough regression and a both-consumers-one-schema resolution test. Faithful-port of the old break is a gate failure.

### R-07: Two-key store resolution failure (SR-08, FR-26, ADR-003)
**Severity**: High · **Likelihood**: Low (architecture fixed key = `projectHash`) · **Impact**: If write-key and read-keys ever diverge (slug vs `projectHash`), one consumer silently resolves nothing — dead surface, no error.

**Test Scenarios**:
1. **One-derivation**: assert `init` write-key, bridge read-key, and hook-client read-key are all `computeProjectHash(projectRoot)` from the one shared export (they cannot disagree).
2. **Round-trip by hash**: write store for project P; assert both consumers read it back keyed by P's `projectHash`.
3. **Slug-is-payload**: assert the slug is read only from inside `mcp_url` (posted verbatim) and is never used as a store key.
4. **Wrong-key miss**: read with a different `projectHash` → `null` (ENOENT) → defined fall-through, not a crash.

**Coverage Requirement**: Write and both reads proven to resolve from one store by one `projectHash`-derived key.

### R-08: Scope B independence from Scope A / cloud (SR-05, AC-11, ADR-005)
**Severity**: Med · **Likelihood**: Med · **Impact**: Scope B is the de-risking lever (lands first). #774 merged (PR #778) so the cloud is now reachable, but Scope B's independence is still load-bearing: if any Scope B test transitively needs the bridge or a reachable cloud, B loses its lands-first property and cannot merge ahead of Scope A.

**Test Scenarios**:
1. **No-cloud Scope B suite**: the entire Scope B test set (store write, mode 0600, no-secret-in-tree, both-consumers-resolve) runs green with no cloud reachable and the bridge module absent/un-spawned.
2. **B-without-A**: hook-client resolution + observe pinning tested without invoking `mcp-bridge.js`.
3. **Merge-order assertion (process)**: coverage report explicitly states Scope B is mergeable independently of Scope A (Scope B is `[no-cloud]`; #774 is merged (PR #778) so Scope A is now live-validatable, but B must not depend on A or the cloud).

**Coverage Requirement**: Scope B coverage is fully `[no-cloud]` and has zero dependency on Scope A code or a reachable endpoint.

### R-09: Token to a loggable surface (SR — NFR-06, FR-12/FR-17, AC-09, NFR-03)
**Severity**: High · **Likelihood**: Low · **Impact**: A leaked token in logs/`.mcp.json`/error text defeats the whole relocation.

**Test Scenarios**:
1. Capture stdout/stderr + `printSummary()` during remote `init` and a bridge run (incl. the pin-mismatch path) → assert the token string appears in none.
2. Assert `.mcp.json` contains no token, no `mcp_url`, no `fp` — only `command`/`args:[<bridge path>, <projectHash>]`/`env:{}` (AC-09, FR-17).
3. Assert the pin-mismatch error message (expected-vs-presented) contains no token (FR-12).
4. Assert thrown bundle/store errors carry token-free messages.

**Coverage Requirement**: Grep-all-surfaces-for-token test across init, bridge happy path, and bridge mismatch path — token absent everywhere.

### R-10: `.mcp.json` write idempotency / merge-preservation (SR-09, FR-15/FR-16, AC-07)
**Severity**: Med · **Likelihood**: Med · **Impact**: Clobbering a co-resident MCP server or duplicating `unimatrix` on re-`init`, or writing under `--dry-run`, is a regression.

**Test Scenarios**:
1. Pre-seed `.mcp.json` with a co-resident server; run `init` twice; assert `unimatrix` not duplicated and the co-resident server preserved.
2. `--dry-run` → no write, intended change reported.
3. Malformed existing `.mcp.json` → throws (mirrors `writeMcpJson`), does not silently overwrite.

**Coverage Requirement**: Re-`init` idempotency + co-resident preservation + dry-run no-write, extending the existing `writeMcpJson` fixtures (cumulative, NFR-08).

### R-11: Legacy path loud-not-silent (SR-06, FR-18/FR-19, AC-10)
**Severity**: Med · **Likelihood**: Low · **Impact**: A silent skip leaves users with a dead `context_*` surface and no diagnosis.

**Test Scenarios**:
1. Run `init --remote/--token`; assert `.mcp.json` has no `unimatrix` MCP entry.
2. Assert the **exact** unsupported-message text and the command exit behavior (wording + exit are testable, per SR-06 — not prose).
3. Assert the legacy observe path still works unchanged.

**Coverage Requirement**: Deterministic message text + exit asserted; no bridge wired on legacy.

### R-12: Credential left in the repo tree (SR — AC-08, FR-20/FR-27)
**Severity**: High · **Likelihood**: Low · **Impact**: A stageable token-bearing file re-opens the commit-leak vector the feature exists to close.

**Test Scenarios**:
1. After remote `init`, assert the store file exists out-of-tree at mode 0600 and `git status --porcelain` / `git add -A` dry-run lists no token-bearing path.
2. Assert `.claude/settings.local.json` contains no `unimatrix.remote` credential after `init`.
3. **Stale-creds migration**: pre-seed a legacy `.claude/settings.local.json` with `unimatrix.remote`; run bundle `init`; assert the subtree is deleted (merge-preserving — other keys survive) and no in-tree token remains (FR-27).
4. Partial-write/crash: a failed store write does not leave a token-bearing temp in the tree.

**Coverage Requirement**: Post-`init` repo tree is provably free of any token-bearing path, including after migration from a legacy in-tree file.

### R-13: Store read-error posture (SR — error boundaries §9, FR-23, ADR-004)
**Severity**: Med · **Likelihood**: Med · **Impact**: Wrong posture per origin: bridge must fail-loud on malformed/unknown-version (never fail-open unpinned); hook client must terminal-`malformed` on parse error but UDS-fall-through on ENOENT/incomplete.

**Test Scenarios**:
1. Malformed JSON → bridge exits non-zero loud; hook client → terminal `malformed`.
2. Unknown `schema_version` → terminal diagnosable read failure (not a silent skip) for both.
3. ENOENT → bridge exits with "no credential for project"; hook client → UDS fall-through (exit 0).
4. Incomplete entry (missing `fingerprint` for a bundle credential) → defined posture, not an unpinned silent run.

**Coverage Requirement**: Each read-error class mapped to its specified posture per consumer; bridge never fails open unpinned.

### R-14: Hybrid flip-bar definition (SR-03, OQ-1, NFR-07) — the delivery checkpoint
**Severity**: Med · **Likelihood**: Med · **Impact**: An undefined threshold turns a possible pivot into a late, panic import of the 91-pkg/25 MB tree.

**Test/Process Scenarios**: see the **Hybrid Flip-Bar Checkpoint** section below — this is a pre-thresholded delivery gate, not a runtime test.

**Coverage Requirement**: The flip-bar threshold and check are written and agreed before the `sse-parse`+`http-session` units are implemented.

### R-15: Bundle-only boundary — legacy credential pin posture (SR-06, ADR-005 §6)
**Severity**: Med · **Likelihood**: Low · **Impact**: The universal store-write writes legacy credentials with `fingerprint: null`; the hook client must keep legacy unpinned (preserve today's behavior), not pin-or-fail on null.

**Test Scenarios**:
1. Seed a legacy entry with `fingerprint: null`; assert hook client resolves it with `pinnedFp` unset → observe posts unpinned (unchanged legacy behavior).
2. Assert a bundle entry (`fingerprint` present) DOES populate `pinnedFp`.

**Coverage Requirement**: `null` fingerprint → unpinned legacy; present fingerprint → pinned bundle. No crash on null.

### R-16: stdio framing under chunked input (FR-07, ADR-001 unit `stdio-frame`)
**Severity**: Med · **Likelihood**: Low · **Impact**: Partial-line buffering bugs drop or splice JSON-RPC messages when stdin arrives in arbitrary chunks.

**Test Scenarios**:
1. One message split across multiple stdin chunks → one parsed message.
2. Multiple messages in one chunk → N parsed messages in order.
3. Chunk boundary exactly on the newline → no empty/dropped message.

**Coverage Requirement**: `stdio-frame` unit is byte-split-invariant on read and newline-frames on write.

### R-17: Session/attribution identity stability (FR-02/FR-03, vnc-014/#4708, ADR-001 unit `http-session`)
**Severity**: High · **Likelihood**: Med · **Impact**: The server keys **audit attribution** on the transport `Mcp-Session-Id` and on `clientInfo.name` (vnc-014/#4708). If the bridge mints a fresh/unstable `Mcp-Session-Id` per request, or sends a varying `clientInfo.name` across calls within one bridge session, the server attributes calls from the same client/session to different identities → cross-session attribution bleed, undercutting the 1-client:1-project integrity basis (memory: one-client-one-project rationale). This is distinct from R-05 (capture/replay *correctness*): here the captured value must also be **stable** and the client identity must not drift.

**Test Scenarios**:
1. **Stable session id across requests**: drive `initialize`→`tools/list`→`tools/call` within one bridge process; assert the **same** `Mcp-Session-Id` value is sent on every post-initialize request (not regenerated, not blank-then-refilled).
2. **Stable `clientInfo.name`**: assert the `initialize` `clientInfo.name` the bridge advertises is a fixed, stable identifier and does not vary across bridge invocations for the same project (deterministic, not random/per-spawn).
3. **No per-request minting**: assert the bridge never mints its own `Mcp-Session-Id` on a follow-up request when the server returned one on `initialize` (it replays the server-minted value verbatim — ties to R-05 scenario 3 and the #774 handshake item below).
4. **Attribution-bleed negative**: two distinct bridge sessions for two distinct projects do not collide on session id or `clientInfo.name` (no shared/global mutable identity).

**Coverage Requirement**: Within one bridge session the `Mcp-Session-Id` and `clientInfo.name` sent to the server are proven **identical across all requests**; across distinct project sessions they are proven distinct — so the server's audit attribution cannot bleed.

---

## Integration Risks

- **Store ↔ both consumers (R-06, R-07, R-13)**: one file, one `projectHash` key, one schema read by the bridge and the hook client. The highest-leverage integration seam — a divergence here silently disables one surface. Covered by the both-consumers-one-schema resolution test.
- **Bridge ↔ cert-pin seam (R-01, R-02)**: the bridge reuses `cert-pin.js`/`transport-http.js` pinned-flush. The integration risk is the *persistent-connection* divergence (per-socket re-pin) the single-shot observe path never exercised.
- **Bridge ↔ Streamable-HTTP transport (R-04, R-05, R-03)**: session-header capture/replay and dual framing against a stub pinned to rmcp behavior — the novel surface; stub is the fast first tier and live validation is now available (#774 merged, PR #778), so the live run is a delivery gate, not a deferred follow-up.
- **`init` ↔ `.mcp.json`/store write (R-09, R-10, R-12)**: idempotent merge-preserving `.mcp.json` write + out-of-tree store write + legacy-subtree cleanup, all in `initRemote()`.

## Edge Cases

- SSE `data:` split across TCP chunks; mid-record-boundary split; CRLF vs LF (R-04).
- `Mcp-Session-Id` absent on `initialize` (R-05).
- Second TLS socket mid-session / mid-session cert swap (R-02).
- Malformed / unknown-`schema_version` / ENOENT / incomplete store entry (R-13).
- `fingerprint: null` legacy credential (R-15).
- No `homedir` → store path null (same posture as existing `socketPathFor`).
- Malformed existing `.mcp.json` (R-10).
- 1 MiB body guard on both `application/json` and SSE paths (R-04).

## Security Risks

The bridge accepts **two untrusted inputs**: (1) the cloud's TLS leaf certificate, (2) the Streamable-HTTP response bytes (`application/json`/SSE). It holds **one high-value secret**: the bearer token.

- **Untrusted input — the cloud leaf cert**: the entire trust model is leaf-fingerprint pinning over a `rejectUnauthorized:false` handshake. **Blast radius if mis-pinned: the cleartext bearer is handed to an attacker-controlled server** (R-01, R-02). Mitigation is the live good-pin/wrong-pin handshake test proving the token never crosses on mismatch (#4970 recipe) + fresh-context security review. This is the single most important security control in the feature.
- **Untrusted input — response bytes**: malformed SSE / oversized bodies must not hang or corrupt; 1 MiB guard + chunk-split-invariant parser (R-04). The bridge is a translator, not an evaluator — it does not execute response content, bounding injection risk to JSON-RPC payload pass-through.
- **Secret at rest**: cleartext-at-rest is *accepted by decision* (NFR-05; the key would co-locate with ciphertext). The hardened risks are **cleartext-in-the-repo** (R-12, removed by out-of-tree relocation + mode 0600) and **token-to-logs** (R-09). No path-traversal surface: the store path is derived from `homedir()` + a SHA-256 `projectHash`, not from user-supplied strings.
- **No token on the command line / in `.mcp.json`** (FR-17, AC-09): a committable `.mcp.json` carries only the bridge path + `projectHash`. The bridge reads the secret from the out-of-tree store at spawn time.

## Failure Modes

| Condition | Expected behavior | Posture |
|-----------|-------------------|---------|
| Bridge cert-pin mismatch | socket destroyed **before** any token byte; loud expected-vs-presented stderr; non-zero exit | fail-loud / fail-closed (R-01) |
| Observe-path cert-pin mismatch | connect-class failure → breadcrumb → exit 0 | fail-open (unchanged) (R-06) |
| Store malformed / unknown version | bridge: loud non-zero exit; hook client: terminal `malformed` | loud (R-13) |
| Store ENOENT | bridge: "no credential for project" loud; hook client: UDS fall-through | bridge-loud / hook fail-open (R-13) |
| Legacy cloud-MCP request | loud deterministic unsupported message, no bridge wired | loud (R-11) |
| Streamable-HTTP 4xx/5xx | surfaced as JSON-RPC error on stdout | per-request (R-05) |
| Store write failure during `init` | throw → init exits 1 (creds must persist); no token-bearing partial in tree | loud (R-12) |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (SSE/session ~260-LoC correctness) | R-04, R-05, R-14, R-17 | ADR-001 carves five separately-testable units; `sse-parse` + `http-session` get dedicated fixtures; the FIRST-delivery SSE-skip probe (now runnable **live** — #774 merged, PR #778 — so it resolves definitively, not against a stub) can retire R-04 and shrink the surface; `http-session` also carries the R-17 identity-stability assertions (server-minted session id replayed verbatim, stable `clientInfo.name`); flip-bar runs AFTER the probe resolves (below). |
| SR-02 (cert-pin trust boundary, vnc-034 F1 class) | R-01, R-02, R-09 | Live `https.createServer` good/wrong-pin handshake test proving token-never-on-wire (#4970 recipe) as AC-04; per-socket re-pin contract (ADR-001 §3.2); fresh-context security review even on green. |
| SR-03 (zero-dep DIY bet / hybrid fallback) | R-14 | Pre-agreed flip-bar threshold (below); 30-min SDK re-check at delivery (ass-080 out-of-scope discovery). |
| SR-04 (#774 stub-only → now MERGED (PR #778); hazard reframed to stopping at green-on-stub) | R-03 | #774 merged (PR #778, 913b78cb) — live cloud validation now available. Stub contract pinned to a captured rmcp `initialize` response is the fast first tier; the **active** post-#774 live-validation checklist (session-id handshake first) is a delivery gate — every Scope-A AC live-validated before being reported validated. |
| SR-05 (two-scope / cross-issue coupling) | R-08 | ADR-005 component boundary makes Scope B fully `[no-cloud]`; B lands first; B coverage cannot depend on A or a reachable cloud (independence still required even though #774 merged (PR #778) made the cloud reachable). |
| SR-06 (legacy silent-skip) | R-11, R-15 | AC-10 deterministic message text + exit asserted; legacy `fingerprint: null` stays unpinned (no pin-or-fail on null). |
| SR-07 (pre-existing schema mismatch) | R-06, R-13 | ADR-004 single canonical schema; regression test asserting `pinnedFp` now populated; faithful-port is a gate failure; schema-version cascade (pattern #4153/#4373). |
| SR-08 (slug vs projectHash keying) | R-07 | ADR-003 fixes key = `projectHash` via one shared derivation; slug is payload inside `mcp_url`; one-derivation + round-trip tests. |
| SR-09 (`.mcp.json` idempotency) | R-10 | AC-07 re-`init` idempotency + co-resident preservation + dry-run, extending `writeMcpJson` fixtures. |
| (no SR — surfaced at architecture-risk from vnc-014/#4708) | R-17 | Server keys audit attribution on `Mcp-Session-Id` + `clientInfo.name`; `http-session` proves stable identity across requests within a bridge session and distinct identity across project sessions, protecting the 1-client:1-project integrity basis. Live-validated first now that #774 has merged (PR #778) — server-minted-id handshake checklist item #1, an active delivery gate. |

---

## Hybrid Flip-Bar Checkpoint (SR-01 / SR-03 / OQ-1 / NFR-07)

**Purpose**: a concrete, pre-thresholded delivery checkpoint deciding whether to abandon DIY and adopt the SDK-transport-behind-custom-fetch hybrid. Set BEFORE implementing `sse-parse` + `http-session` (the ~260-LoC correctness surface), so the pivot is data-driven, not panic-driven.

**Run AFTER the SSE-skip probe resolves (R-04).** The SSE-skip probe is the FIRST delivery experiment; the flip-bar is evaluated only after it lands, because a passing probe **removes the SSE component entirely** — dropping ~90 LoC of `sse-parse` plus the SSE half of the ~50-LoC dispatch from the ~260-LoC surface, leaving roughly `http-session` ~120 + JSON-only dispatch. A flip decision taken before the probe would threshold against a surface that may not exist. If the probe passes, R-04 evaporates, the LoC-overrun and correctness-instability conditions below are evaluated against the reduced (no-SSE) surface, and the chunk-split-invariance correctness condition no longer has an SSE component to fail on.

**Checkpoint timing**: evaluated once, **after the SSE-skip probe resolves**, when the surviving correctness units reach first-green against the rmcp-pinned stub (R-03, R-05, and R-04 only if the probe shows SSE is required). With SSE required: `sse-parse` ~90 + `http-session` ~120 + SSE/JSON dispatch ~50 = ~260 LoC. With the probe passing: ~`http-session` 120 + JSON-only dispatch, no `sse-parse`.

**FLIP to hybrid only if ALL of the following hold** (high bar — a flip imports the 91-pkg/25 MB tree AND still requires a hand-written custom-fetch + Node→Web `Response` adapter):
1. **LoC overrun**: the combined `sse-parse`+`http-session`+dispatch surface exceeds **~520 LoC** (≈2× the ~260 estimate) when first-green — the documented NFR-07 trigger.
2. **Correctness instability**: the chunk-split-invariance (R-04) or capture/replay (R-05) tests remain red after a focused fix pass, OR require non-stdlib parsing machinery to pass.
3. **30-min SDK re-check is positive**: at delivery, re-check whether a slimmer client-only `@modelcontextprotocol/sdk` subpackage now install-tree-shakes the server/OAuth deps (ass-080 out-of-scope discovery #2). A flip is only sane if the dep cost has materially dropped.

**DO NOT FLIP if** any of: the overrun is under ~520 LoC; the correctness tests are green; or the SDK still pulls the full 91-pkg/server-laden tree (the cert-pin path still demands the hand-written custom-fetch either way — ass-080 gate: the SDK gives zero leverage on TLS pinning).

**On flip**: the cert-pin pinned-flush contract (R-01, R-02) is unchanged — it lives in the injected custom-fetch regardless. The flip trades correctness risk for supply-chain cost; AC-02 (zero-dep) is then knowingly waived and must be re-approved by the human.

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 5 (R-01, R-02, R-04*, R-05, R-06) | ~21 |
| High | 6 (R-03, R-07, R-08, R-09, R-12, R-17) | ~20 |
| Medium | 5 (R-10, R-11, R-13, R-14, R-16) | ~13 |
| Low | 1 (R-15) | ~2 |

\* R-04 (SSE-parse) is **retired entirely if the FIRST-delivery SSE-skip probe passes** (`Accept: application/json` yields JSON across the full lifecycle) — dropping the Critical count to 5 and removing the SSE component from the flip-bar surface.

**Non-negotiable acceptance criteria** (gate must verify these by name, not by green suite — lesson #4970/#2758):
- AC-04 live good/wrong-pin handshake proving the token never crosses on mismatch (R-01) — plus fresh-context security review even on green.
- Per-socket re-pin proven across the persistent session (R-02).
- `pinnedFp`-populated regression on the file path (R-06).
- Stub framing provably derived from a captured rmcp response + every Scope-A AC live-validated against the real cloud endpoint before being reported validated; the active post-#774 (merged, PR #778) live-validation checklist run, session-id handshake first (R-03).
- Scope B suite fully `[no-cloud]`, mergeable independently of Scope A (R-08).

## Knowledge Stewardship
- Queried: context_search for cert-pin/trust-boundary lessons (#4970 — the SR-02 precedent + the live-handshake test recipe, directly drives R-01/R-02 design and AC-04), schema-bump test-cascade patterns (#4153/#4373 — R-06), unrun-AC gate lessons (#4796 — R-03); context_get on #4970 for the full recipe.
- Stored: nothing novel to store -- the cross-feature risk pattern "trust-boundary code needs a live-boundary test, not a shape assertion" already exists as lesson #4970 (active, edge-linked to ADR-001 vnc-039 #5108). No 2nd-feature pattern beyond it yet to warrant a new entry; the schema-bump cascade is already pattern #4153/#4373.
