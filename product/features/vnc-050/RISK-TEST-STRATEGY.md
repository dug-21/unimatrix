# Risk-Based Test Strategy: vnc-050

Evidence: #4970 (trust-boundary code must be proven at the real boundary; options-shape assertions are false green), #4965 (verify on secureConnect, gate the token write), #4416 (capability gate tests need a rejection test plus a success companion), #4321 (missing allowlist at a trust boundary is fix-before-merge), #4453 (one capability check point).

Rework 1: aligned to HUMAN-DECISIONS 2-5 (token mode is a capability ceiling, ADR-003; observe 403 + warn-once, ADR-004; tombstone revoke; bundle/env conflict list; `http.legacy_token_enabled`; backward compatibility FR-34..39).

## Risk Register

| ID | Risk | Sev | Lik | Priority |
|---|---|---|---|---|
| R-01 | The CA branch sends the bearer token or body before TLS verification, or inherits `rejectUnauthorized:false` / a `cert-pin.js` agent | High | Med | Critical |
| R-02 | Capability ceiling not enforced: a read token writes by claiming `agent_id:"human"` (or any Write holder). Causes: `BearerPrincipal` absent at the tool (rmcp path drops `Parts.extensions`), `ceiling_from` returns `None` instead of empty on an HTTP request without principal, a tool bypasses `require_cap`, or the intersection widens (read-write gains Admin) | High | Med | Critical |
| R-03 | The slug guard is bypassed on some route (SSE GET, DELETE, observe, `/v1/tools/...`, percent-encoded or trailing-slash paths), or runs after `resolve_store` | High | Med | Critical |
| R-04 | A read-token call still records learning: a `UsageContext` site defaults `learning_allowed:true`, the query_log insert is outside the predicate, or an unenumerated sink (briefing, helpful, implicit signals, a channel enqueued before the gate) | High | High | Critical |
| R-05 | Revocation is not live or not durable. The stamp `(mtime_ns,len,inode)` misses a change; a parse failure serves the last-good snapshot; an open MCP session keeps capability; the tombstone is dropped on a later rewrite, or a revoked label is re-mintable and inherits audit history | High | Med | Critical |
| R-06 | The constant-time scan exits early or uses a hash-keyed lookup, so timing leaks | Med | Low | Medium |
| R-07 | The MCP bridge and the observe path resolve credentials differently (precedence, trust, headers), so MCP works while telemetry is silently dropped | High | Med | Critical |
| R-08 | Extra headers are missing on one request type (SSE GET, DELETE, observe, `pingForInit`), or a reserved header can be overridden, or a CRLF/NUL value is accepted | High | Med | High |
| R-09 | A header value or token leaks to stderr, a log, an audit row, `--list` or an error body | High | Med | High |
| R-10 | Bundle `fp:null` widens decoder acceptance (empty string, wrong prefix accepted), or Rust and JS parity diverges, or `init --bundle` silently stores a broken credential | Med | Med | High |
| R-11 | The pinned branch regresses (AC-05): existing pin tests get edited, or the null/truthy branch selection flips for a stored `fingerprint:null` that used to error | High | Low | High |
| R-12 | Token-file concurrency. Two mints race and one is lost; a missing lock is left uncreated; a tmp file left behind; 0600 is not applied to the tmp file before rename | Med | Med | High |
| R-13 | The `token:` namespace is not reserved on every transport (stdio, UDS, HTTP), so a caller self-asserts `token:ci` and forges audit attribution, or `resolve_or_enroll` writes it into the agent table | Med | Med | High |
| R-14 | Flag-matrix gaps: an accepted-but-ignored combination (`--trust` with `--list`, `--mode` without `--headless`); revoking another slug's label succeeds; I/O happens before validation | Med | Med | Medium |
| R-15 | Legacy disable misbehaves. `UNIMATRIX_LEGACY_TOKEN_ENABLED` bad value does not fail boot; env does not override `http.legacy_token_enabled`; empty env treated as false; missing key not defaulting true; disabled body echoes the token; legacy affected by corrupt `tokens.json` | High | Low | High |
| R-16 | The `http_access` audit is wrong. A denied request is written to the target slug's log (which reveals the slug exists); the audit write blocks the request path; a long SSE session produces unbounded rows | Med | Med | Medium |
| R-17 | Read-token observe: server dispatches before the 403 (partial writes), or the client fails the job on 403, warns per event (log flood), stays silent, or the latch also suppresses read-write observe | High | Med | Critical |
| R-18 | Bundle/env conflict list drifts from FR-20: one of URL/MCP_URL/TOKEN/FINGERPRINT accepted with the bundle (silent override), or `UNIMATRIX_REMOTE_HEADERS` wrongly rejected (breaks Cloudflare Access) | Med | Med | High |
| R-20 | Residual gap (accepted): within a read-write (or legacy) token a caller claims any `agent_id`, including `human`, and gets that agent's non-Admin grants and attribution | Med | High | Accepted (documented, regression-pinned) |
| R-21 | Backward-compat break: new server rejects or alters behavior for an unmodified pre-vnc-050 client (pinned v:2 bundle, `remote.json`, env-var observe, legacy token); new client mis-reads old artifacts or rewrites `remote.json` | High | Med | Critical |
| R-22 | Residual (accepted, pre-existing PL-4 fail-open): omitted `agent_id` resolves to `anonymous`, which `resolve_or_enroll` auto-enrolls Restricted with Read+Write+Search under default `default_trust="permissive"` (`unimatrix-store/src/registry.rs:126-130`; config default `config.rs:447`). A read-write token with no `agent_id` therefore writes as `anonymous`. Not new: ceiling still bounds read tokens; `token:` ids never enrolled (HD 7.1) | Med | High | Accepted (documented, regression-pinned) |
| R-19 | Modularity growth in `tools.rs`, `server.rs` or `main.rs` beyond the stated line budget, or `usage.rs` crosses the cap | Low | Med | Low |

## Risk-to-Scenario Mapping

### R-01 CA branch leaks before verification (Critical)
Impact: the token is exfiltrated to a MITM, which compromises the slug.
1. Run a byte-capturing raw TLS server with an untrusted self-signed cert. The bridge runs with no fingerprint. Assert zero application bytes are received, the exit code is 1, and stderr contains no token.
2. Repeat with a cert signed by a trusted CA (`NODE_EXTRA_CA_CERTS`) but a hostname mismatch. Assert zero bytes.
3. Repeat scenarios 1 and 2 for the observe POST (`transport-http.js`) and for `pingForInit`.
4. Add a guard test: build an unpinned request with options that contain `rejectUnauthorized:false`. The guard must throw.
Coverage: the real socket boundary on both transports (#4970). Shape-only assertions do not count.

### R-02 Capability ceiling (Critical)
0. **First delivery step** (gates all other ceiling work): over real HTTP in a stateful session, assert `BearerPrincipal` is present in the tool's `RequestContext` for POST (initialize, then `tools/call`). If absent, implement the `McpAdapter::call` re-insert fallback (ADR-003) and re-run before continuing.
1. Table test over real HTTP: read token × every write tool (`store`, `correct`, `deprecate`, `quarantine`, `edge`, `tag`, `enroll`, cycle writes) × claimed `agent_id` {omitted, `human`, registry Write-holder, `token:<label>`}. Every cell returns `CapabilityDenied`; DB unchanged (AC-08, AC-25).
2. Read-write companion: same calls succeed (#4416); read-write + `human` on an Admin tool is denied (ceiling never contains Admin).
3. Empty ceiling: an HTTP request that reaches the tool with `Parts` but no `BearerPrincipal` (test hook removes it) is denied for Read and Write. stdio and UDS (`None`) behave exactly as today.
4. Narrow-only: a registry agent lacking Write under a read-write token is still denied Write (intersection, not union).
5. Static check: every `tools.rs` capability check goes through `require_cap(ctx, ..)`; no direct registry calls bypass it.
Coverage: driven through the HTTP transport, never by calling `require_cap` directly.

### R-03 Slug guard bypass (Critical)
1. Run the matrix {MCP POST, SSE GET, DELETE session, observe POST} × {legacy, read@A, rw@A} × {A, B}. Expected: B returns 403 for both scoped tokens, and legacy passes.
2. Path variants: `/v1/B/`, `/v1/B`, `/v1/%42/...`, `/v1//B`, `/v1/tools/...` (slug `tools`), and case variants. Each is either a 403 or the existing 400/404, never a 200.
3. The 403 body is identical whether or not slug B exists.

### R-04 Learning side effects from read tokens (Critical)
1. Snapshot access/helpful counts, `co_access`, `feature_entries`, `query_log`, `agents`, `sessions`, `observations`, signal_queue, implicit signals.
2. Read token runs search, lookup, get and briefing with `helpful:true`, `feature` set and claimed `agent_id:"human"` (claim must not re-enable learning).
3. Drain usage/background channels; every table identical. Audit rows (`http_access`, tool audit) ARE written.
4. Read-write companion: the same batch changes the tables (gate not vacuous).
5. Legacy token and stdio: learning still recorded (`None` ceiling → true).
6. Grep-level inventory: every MCP `UsageContext` construction in `tools.rs` sets `learning_allowed` from `ctx`; no `Default` that yields true on an MCP path. Any sink outside ADR-004 gets its own test.

### R-05 Revocation not live and tombstone (Critical)
1. Against a running server: mint, call (200), revoke via the CLI process, call (401). No restart.
2. With an MCP session open across a revoke, the next `tools/call` is denied.
3. Two writes within the same mtime tick with equal length are still detected (inode changes on rename). Write a test that forces this case.
4. Tombstone: after revoke, `tokens.json` retains the record (hash, label, mode, slugs, created_at, `revoked_at`); a later mint of another label preserves the tombstone; re-minting the revoked label exits non-zero; `--list` shows it revoked; audit rows for the label still resolve to the original token (AC-17).
5. Revoking an already-revoked or unknown label: non-zero (or no-op message), file byte-identical.
6. Corrupt `tokens.json`: scoped calls return 401, the legacy token still returns 200, and a revoked token does not come back to life. Fix the file and confirm recovery.
7. Delete `tokens.json`: scoped calls return 401 and nothing panics.

### R-06 Timing (Medium)
1. A code-level test or review confirms there is no `HashMap` keyed by hash and no early `break` in the scan.
2. A unit test: a match at index 0 and at index n-1 both resolve correctly with no short-circuit. Check the control flow, not wall-clock timing.

### R-07 Bridge and observe resolve differently (Critical)
1. Run the full precedence matrix through `resolveRemote`: bundle only, env pair only, store only, bundle plus each env var, a partial pair, and a fingerprint without a URL.
2. Repeat each case end-to-end through `mcp-bridge.js` and through `config.js`. Both must reach the same outcome.
3. With `UNIMATRIX_BUNDLE` only, an observe event arrives at the bundle's `observe_url` using the bundle's trust.

### R-08 Headers (High)
1. A capturing proxy asserts the headers are present on initialize, POST, SSE GET, DELETE, observe POST and `pingForInit`.
2. Each of these inputs is rejected with exit 1 and the header name in the message: invalid JSON, an array, a non-string value, CR, LF, NUL, more than 8 KiB, more than 32 entries, a bad name token, each reserved name in mixed case, and case-duplicate names.
3. As a second line of defense, the reserved headers are still the bridge's own even if the parser is bypassed.

### R-09 Leakage (High)
Use sentinel token and header values. Capture stdout, stderr, debug logs, audit rows, `tokens.json`, `--list` output and all 401/403 bodies across every error path. Grep for the sentinels; there must be zero hits. The `tokens.json` sha256 never appears outside the file.

### R-10 Bundle decoder (High)
1. Add to the parity corpus: `fp:null` is accepted; `""`, `"sha256:"`, uppercase hex, a missing key and an extra key are each rejected by both Rust and JS.
2. `init --bundle` with a CA bundle stores `fingerprint:null`, and the bridge then uses CA trust.

### R-11 Pinned regression (High)
1. `git diff` on the existing pin test files is empty, and they pass.
2. Bundle fingerprint, env fingerprint and stored fingerprint each select the pinned branch, proven against a real self-signed server.

### R-12 Token-file concurrency (High)
1. Run 20 parallel mints. All 20 are present and the file parses.
2. The server reads during a write and never sees a partial file.
3. `tokens.json` and the tmp file are both 0600, and no tmp file remains after a successful write.

### R-13 Reserved namespace (High)
1. Over stdio, UDS, HTTP-legacy and HTTP-scoped, `agent_id:"token:ci"` returns `InvalidInput`.
2. The agents table has no `token:` rows after a scoped session.

### R-14 Flag matrix (Medium)
A table test covers every invalid combination. Each exits 2, and no file is touched (assert the file's mtime is unchanged). `--revoke` of a label bound to another slug is an error.

### R-15 Legacy disable (High)
1. `UNIMATRIX_LEGACY_TOKEN_ENABLED` ∈ {`yes`, `TRUE `, `2`, `off`} → non-zero boot with a clear error naming the variable; `true|1|false|0` accepted; empty = no override.
2. Matrix config {absent, true, false} × env {unset, true, false}: env wins; absent key defaults true (existing config files parse unchanged, PL-2).
3. Disabled → legacy token gets 401 "legacy token disabled", no echo; scoped tokens unaffected.
4. Enabled/default → legacy works on 2+ slugs with Read and Write (AC-01); corrupt `tokens.json` does not affect legacy.

### R-16 Audit (Medium)
1. Every bearer call (legacy, scoped, observe) writes an `http_access` row containing the label.
2. A 403 for B is written to A's log, and B's log is unchanged.
3. A failure of the audit sink does not fail the request.

### R-17 Read-token observe (Critical)
1. Server: read token on `/v1/{slug}/observe` and every session telemetry route → 403 `token lacks SessionWrite`; a dispatcher spy records zero invocations; observations, sessions, signal_queue, cycle_events, cycle_tags, co_access, injection_log, query_log and entries unchanged after an extraction tick and channel drain.
2. Client: spawn the real hook process with a read bundle, emit N≥3 hook events. Exactly one warning line on stderr; hook exit code and stdout identical to a no-remote run; after the first 403 no further observe POSTs reach a capturing proxy.
3. Non-403 failures (500, connection refused) keep existing behavior (no latch).
4. Read-write bundle: observe flows; no warning (latch not triggered).

### R-18 Bundle/env conflict list (High)
1. `UNIMATRIX_BUNDLE` + each of `UNIMATRIX_REMOTE_MCP_URL`, `UNIMATRIX_REMOTE_URL`, `UNIMATRIX_REMOTE_TOKEN`, `UNIMATRIX_REMOTE_FINGERPRINT`: exit non-zero, stderr names the variable, value not echoed; through both the bridge and the hook.
2. `UNIMATRIX_BUNDLE` + `UNIMATRIX_REMOTE_HEADERS`: accepted; headers present on every request.
3. Empty value = unset for every `UNIMATRIX_*` var (HD 7.2, no trimming): `UNIMATRIX_BUNDLE` + `UNIMATRIX_REMOTE_TOKEN=""` (and each other conflict var empty) -> no conflict, bundle used; bridge and hook.
4. Empty half of the URL/TOKEN pair (HD 8, fail-loud): `UNIMATRIX_REMOTE_URL` set + `UNIMATRIX_REMOTE_TOKEN=""`, and reverse -> partial-pair error naming the empty variable; output asserted to contain no token/credential bytes. Both empty -> pair unset, credential store used.

### R-20 Residual agent_id gap (Accepted)
Not mitigated in vnc-050 (FR-16a). Pin the documented behavior so a later change is deliberate: read-write token + `agent_id:"human"` writes, attributed `human` in the tool audit while `http_access` records the real `token_label`. Test asserts the label is always present so forensics can tie the claim to the credential. Closure: agent-identity/OAuth work.

### R-22 Omitted agent_id as anonymous (Accepted)
1. Read-write token, tool call with `agent_id` omitted -> write succeeds, attributed `anonymous`; `http_access` records `token:<label>`; registry grants unchanged; no `token:` id appears in the agent table.
2. Read token, `agent_id` omitted, write tool -> denied by ceiling despite `anonymous` holding Write.
3. No token (stdio/UDS), `agent_id` supplied -> that id resolved/enrolled in registry as today.

### R-21 Backward compatibility (Critical)
Old-client source (OQ-7): see below. Matrix (spec FR-34..39): client {pre-vnc-050, new} × credential {v:2 pinned bundle via `init --bundle`, existing `remote.json`, env-var observe `UNIMATRIX_REMOTE_URL`+`_TOKEN` ± fingerprint, `UNIMATRIX_BUNDLE` (new only; old = N/A, var unknown)} × legacy {enabled, disabled}. Every cell a test or N/A with reason.
1. Enabled: old client initialize + read + write tool calls succeed; observe events land in DB (AC-20, AC-22).
2. `remote.json` fixture written by the old `init --bundle`: byte-identical after old and new client runs (AC-21).
3. New client + fixture pre-vnc-050 v:2 bundle via `UNIMATRIX_BUNDLE` and `init --bundle`: pinned branch, no CA path (AC-24).
4. Disabled: every cell → 401 with disabled indication; old client exits with its existing auth error within a timeout (no hang, no restart loop); observe non-fatal (AC-23).
5. FR-39: record legacy-token responses (status, headers, body shape) for a fixed request set on old vs new server; diff empty except the disabled body.

**OQ-7 resolved: old client = published release artifact `@dug-21/unimatrix@0.11.3` (exact version, `npm pack` tarball with integrity hash, cached in CI).** Why: it is what installed users actually run, including the packaged `files` layout and bin wiring that a source checkout would not exercise; it is immutable, whereas a git ref can be re-tagged and includes unreleased tree state. Fallback when the registry is unreachable: `git archive v0.11.3 packages/unimatrix` (same tag the release was built from), flagged in the test output. Bump the pinned version when vnc-050 ships so the matrix always tests N-1.

### R-19 Modularity (Low)
The gate measures the net line delta for the four over-cap files.

## Behavioral-Outcome Coverage (from the scope behavioral lens)

| Outcome | Entry point | Required scenario (drives the real entry point) |
|---|---|---|
| Bundle emitted; secret shown once; listed | `unimatrix client-bundle <slug> --headless --mode M --label L` (binary) | Run the CLI binary. stdout is the blob only, and the secret appears nowhere else. `--list` then shows L, M and the slug |
| Labels shown, no secrets | `client-bundle <slug> --list` (binary) | Run the binary after 2 mints. stderr has the table, stdout is empty, and there are no secret or hash sentinels |
| Next call is 401 | `client-bundle <slug> --revoke L` (binary) | AC-16 end-to-end: a real server, the bridge attached via the bundle, the CLI revoke, then the next bridge call fails with 401 |
| MCP and observe work; no init, no `remote.json` | `UNIMATRIX_BUNDLE` + launch `mcp-bridge.js` | Spawn the bridge process with a clean HOME. `initialize` and `tools/call` succeed, an observe event lands in the DB, and no `remote.json` exists afterwards. Run once pinned and once CA |
| Env pair works; partial pair exits non-zero with no token | `UNIMATRIX_REMOTE_MCP_URL` + `_TOKEN`, launch the bridge | Spawn the bridge with the pair, using a CA via `NODE_EXTRA_CA_CERTS`, and confirm calls succeed. Spawn it with each var alone: exit is non-zero and the output scan for the token is clean |
| Headers present on every request; reserved names rejected | `UNIMATRIX_REMOTE_HEADERS` + launch the bridge and hook | A spawned bridge and hook run through a capturing proxy; assert the headers on every request type. Spawn with `{"authorization":..}`: exit 1, no value printed |
| Pinned trust unchanged | Fingerprint via bundle or `UNIMATRIX_REMOTE_FINGERPRINT` | Spawn the bridge against a self-signed server. A matching fp connects; a mismatched fp is refused with zero bytes sent |
| Refused before any token leaves | No fp, bad cert | Spawn the bridge against the byte-capturing server (untrusted, then hostname mismatch). Zero bytes received |
| Authorization error regardless of claimed agent_id; tables unchanged | A read token calls write tools (with `agent_id:"human"`), observe and session writes | Through the bridge spawned with a read bundle: each write tool with `agent_id:"human"` is denied; reads served; DB snapshot unchanged after search, lookup, get, briefing |
| Observe denied; job not failed; one warning | Hook process with a read bundle in a CI-like job | Spawn hook for N events: exit 0, one stderr warning, zero DB observation rows, no POSTs after the first 403 |
| Revoked label stays visible, never reusable | `client-bundle <slug> --revoke L` then mint `--label L` | Binary: revoke, `--list` shows L revoked, re-mint L exits non-zero |
| Existing clients keep working | Pre-vnc-050 client (0.11.3) with pinned bundle / `remote.json` / env observe + legacy token | R-21 matrix against the new server binary |
| 403 | A token bound to A calls `/v1/B/...` | curl with the A token against every B route (POST, SSE GET, DELETE, observe). All return 403 |
| Legacy still works on all slugs | Legacy `{data_dir}/token` | curl and the bridge with the legacy token on 2 slugs, doing a read and a write. Then restart with `UNIMATRIX_LEGACY_TOKEN_ENABLED=false` (and separately `http.legacy_token_enabled=false`): 401 with the disabled message; `=yes` fails boot |
| Recipe works verbatim | Documented curl / generic MCP client | A scripted smoke test extracts the commands from the docs file and executes them unmodified against a local CA server |

Seam tests (`resolveRemote`, `slug_guard::check`, `require_cap`) do not discharge any row above.

## Integration Risks
- rmcp `Parts.extensions` propagation (verified in rmcp 1.7.0 source, ADR-003) must be proven by test as delivery step 1; an rmcp upgrade could silently drop it, so the test stays in the suite permanently. Absent principal must yield empty ceiling, never `None`.
- Observe ceiling (auth-layer `ResolvedIdentity`) and tool ceiling (`cap_ceiling::ceiling_from`) must derive from the same mode → capability table; a divergence lets observe and tools disagree on read-write.
- `require_cap` and `records_learning` must read the same `ctx.cap_ceiling` (#4453); a second source re-opens R-04.
- Slugs provisioned after boot: ceiling and slug guard apply identically (test covers it).
- The fire-and-forget usage channel: the gate must sit before enqueue, otherwise drain timing hides failures (R-04).

## Edge Cases
- Empty `tokens` array; a missing file; a label at 63 characters; a label with uppercase or a leading `-`; a duplicate label; `allowed_slugs` empty or naming an unregistered slug.
- A presented token that is the wrong length, not hex, or uppercase hex; `Bearer` with extra spaces; a missing `Authorization` header.
- A mint immediately followed by a request within the same mtime tick.
- An empty `UNIMATRIX_REMOTE_HEADERS` string or `{}`; a header name colliding with a proxy header; non-ASCII values.
- A bundle with `fp:null` used against a self-signed server: must fail loudly at attach.
- A label minted for A is revoked while being used by an open SSE stream.

## Security Risks
- **Untrusted inputs:** the bearer token (any network client); the URL path (slug guard); `tokens.json` (local file, which could be corrupt); `UNIMATRIX_BUNDLE` and `UNIMATRIX_REMOTE_HEADERS` (CI secrets, possibly malformed); self-asserted `agent_id`; certificates presented by the server.
- **Damage:**
  - Token leak before verification (R-01).
  - Cross-slug access (R-03).
  - Learning poisoning from public or CI read tokens (R-04).
  - Header injection or request smuggling via CRLF (R-08).
  - Privilege escalation via claimed `agent_id` under a read token (R-02); audit forgery via `token:` impersonation (R-13).
  - Residual: attribution spoofing within a read-write token (R-20, accepted).
  - Slug enumeration via 403/404 differences (R-03.3, R-16.2).
- **Blast radius:**
  - A leaked scoped token reaches one slug at its mode until revoked.
  - A leaked legacy token reaches all slugs read-write, which is why the disable option needs a test.
  - A corrupt token file fails closed for scoped tokens only.
- **Deserialization:** `tokens.json` uses strict unknown-key rejection. The bundle uses an exact key set.

## Failure Modes
| Failure | Expected behavior |
|---|---|
| Bad or revoked token | 401, no echo |
| Slug outside the binding | 403, generic body, audited to the bound slug |
| Corrupt `tokens.json` | Scoped tokens get 401; one error log per stamp; legacy and local unaffected |
| TLS verification failure | Socket destroyed, zero bytes sent, exit 1, no token in output |
| Client misconfiguration | Exit 1, token-free stderr naming the variable |
| CLI misuse | Exit 2, no I/O |
| Audit sink failure | Request proceeds |
| Bad `UNIMATRIX_LEGACY_TOKEN_ENABLED` | Boot error, non-zero exit |
| Read token on write tool | `CapabilityDenied`, no DB change, audit row written |
| Read token on observe | Server 403 pre-dispatch; client one warning, job unaffected |
| HTTP request without principal at tool | Empty ceiling, deny |
| Re-mint revoked label | Exit non-zero, file unchanged |

## Scope Risk Traceability
| Scope Risk | Arch Risk | Resolution |
|---|---|---|
| SR-01 | R-01 | ADR-006 separate CA branch plus guard; zero-byte tests |
| SR-02 | R-06 | ADR-001 full scan with `ct_eq` |
| SR-03 | R-05, R-12 | ADR-001 per-request stat reload, fail closed, flock with tmp+rename |
| SR-04 | R-08 | ADR-007 strict validation |
| SR-05 | R-10 | ADR-005: `--headless` flag, single decoder |
| SR-06 | R-10 | v:2 kept; nullable fp; parity corpus |
| SR-07 | R-04 | ADR-004 sink gating plus inventory obligation |
| SR-08 | — | Scripted local CA smoke test; live AC-13/14 operator-run (accepted) |
| SR-09 | R-14 | CLI is server-host only (ADR-001 consequence) |
| SR-10 | R-03 | ADR-002 single guard before the split |
| SR-11 | R-02, R-20, R-22 | ADR-003 capability ceiling (intersection); residual attribution gap accepted (FR-16a) |
| SR-12 | R-13, R-16, R-22 | `token:<label>` is auth-layer audit identity only; tool default agent_id unchanged (HD 7.1) |
| SR-13 | R-07 | ADR-006 shared resolver |
| SR-14 | R-15, R-21 | `http.legacy_token_enabled` default true + env override; compat matrix with legacy disabled |
| SR-15 | R-07 | Both consumers use C9 |
| SR-16 | R-08 | Headers applied on every request type |
| SR-17 | R-04, R-17 | Ceiling-derived learning gate; observe 403 pre-dispatch; client warn-once |
| SR-18 | R-14 | Flag matrix, exit 2 |
| SR-19 | R-07, R-18 | FR-20 explicit conflict list; headers allowed with bundle |

## Coverage Summary
| Priority | Risks | Required scenarios |
|---|---|---|
| Critical | 8 (R-01, 02, 03, 04, 05, 07, 17, 21) | 44 |
| High | 8 (R-08, 09, 10, 11, 12, 13, 15, 18) | 20 |
| Medium | 3 (R-06, 14, 16) | 6 |
| Low | 1 (R-19) | 1 |
| Accepted | 2 (R-20, R-22) | 4 (regression pins) |
| Behavioral outcomes | 15 | 15 |
