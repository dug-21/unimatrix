# SPECIFICATION: vnc-050 — Headless Automation Access

GH #1010. Source: `product/features/vnc-050/SCOPE.md` (OQs resolved 2026-11-25), `SCOPE-RISK-ASSESSMENT.md`.

## Objective

Let non-interactive clients (GitHub Actions, Claude Code cloud routines, a Cloudflare-hosted site) attach to a personal-cloud Unimatrix server from one secret, using least-privilege credentials. Headless access **extends the existing setup flow** (`project register` → `client-bundle` → attach): `client-bundle --headless` mints a slug-bound `read` or `read-write` token, embeds it in a bundle, and the bridge/observe path consumes it from `UNIMATRIX_BUNDLE`. Pinning becomes optional (CA-validated TLS when no fingerprint is present), and the server enforces per-token mode and slug binding.

## Human Constraint (binding)

Extend the existing project register / client-bundle creation flow. No parallel or new creation mechanism: no `token` command, no server-side mint API or MCP tool, no second bundle format or decoder. Violation is a gate fail (SR-05).

## Domain Models / Ubiquitous Language

| Term | Definition |
|---|---|
| **Slug** | Operator-chosen project identifier; path segment 2 of `/v1/{slug}/...` (vnc-034 ADR-004 #4951). |
| **Bundle** | The v:2 client bundle produced by `client-bundle` (vnc-038 ADR-002 #5081): server-composed MCP URL, observe URL, token, optional fingerprint. |
| **Headless bundle** | A v:2-compatible bundle produced by `client-bundle <slug> --headless`; carries a scoped token. Decoded by the same decoder as `init --bundle`. |
| **Legacy token** | The existing `{data_dir}/token`. Implicit all-slugs, read-write. On by default; disableable by operator config. |
| **Scoped token** | A minted token with `label`, `mode`, `allowed_slugs`, stored as SHA-256 hash. Secret shown once at mint. |
| **Token table** | Server-side persistent store of scoped-token records (hash, label, mode, allowed_slugs, created_at, revoked state). Lives under the server data dir. |
| **Label** | Operator-supplied unique name for a scoped token; used for `--list`, `--revoke`, audit, and agent identity. Never secret. |
| **Mode** | `read` → capabilities {Read, Search}. `read-write` → {Read, Write, Search, SessionWrite}. |
| **Read/write line** | "Mutates knowledge or feeds learning" is write (OQ-6). |
| **Trust mode** | `pinned` (fingerprint present; vnc-039 ADR-001 behavior) or `ca` (no fingerprint; standard Node TLS, `rejectUnauthorized` true). |
| **Credential resolver** | Single JS module shared by MCP bridge and observe path that resolves URL(s), token, fingerprint, extra headers, from env and credential store. |
| **Env pair** | Low-level path: `UNIMATRIX_REMOTE_MCP_URL` + `UNIMATRIX_REMOTE_TOKEN` (MCP). `UNIMATRIX_REMOTE_URL` + `_TOKEN` keep their existing observe meaning. |
| **Extra headers** | `UNIMATRIX_REMOTE_HEADERS`: JSON object of caller-owned headers (e.g. Cloudflare Access). Not in the bundle. |

## Functional Requirements

### Baseline (Goal 1)
- **FR-01** A server test asserts the legacy token is accepted on every configured slug with Read and Write capability; `README`/docs state the baseline. (AC-01)

### client-bundle extension (Goals 2, 5)
- **FR-02** `unimatrix client-bundle <slug> --headless --mode read|read-write --label <name>` mints a scoped token bound to `<slug>`, persists its hash in the token table, and emits a bundle embedding the secret. The secret is printed only in this output. (AC-16)
- **FR-03** The headless bundle is v:2-compatible: same decoder, same field names for MCP URL, observe URL, token, fingerprint. Any new field is additive and ignored-safe for existing decoders. `init --bundle` accepts a headless bundle (or, if the architect decides otherwise, rejects it loudly with a named error — never silently mis-installs). (SR-06)
- **FR-04** Fingerprint inclusion follows the server cert: self-signed → bundle carries fingerprint; CA-issued (detected or operator-declared) → fingerprint omitted. (OQ-7)
- **FR-05** `client-bundle <slug> --list` prints label, mode, allowed_slugs, created_at, revoked state for tokens bound to `<slug>`, including revoked tombstones; never secrets or hashes.
- **FR-06** `client-bundle <slug> --revoke <label>` marks the token revoked as a tombstone: the record (hash, label, mode, allowed_slugs, created_at) is retained with revoked state; it is never deleted. The next request using that token receives 401 without server restart. (AC-16, AC-17, SR-03)
- **FR-06a** A label is never reusable: minting with a label held by any record, active or revoked, exits non-zero. Audit attribution for a revoked label stays bound to the original token. (AC-17)
- **FR-07** `--mode`, `--label` without `--headless` exit non-zero with a clear message. `--headless` without `--mode` or `--label` exits non-zero. Duplicate label (active or revoked, FR-06a) exits non-zero. `--revoke` of unknown label exits non-zero. Revoking an already-revoked label exits non-zero or is a no-op with a message (no state change). (SR-18)
- **FR-08** `client-bundle` minting/listing/revoking runs on the server host with access to the server data dir, same as the existing command. (SR-09)

### Server auth (Goal 5)
- **FR-09** The server loads multiple scoped tokens from the token table, each with mode and allowed_slugs. (AC-07)
- **FR-10** Validation: hash presented token with SHA-256, compare against each candidate hash in constant time (no early exit on the secret comparison). Legacy token compare remains constant-time. Choice recorded as ADR. (AC-10, SR-02)
- **FR-11** Unknown, malformed, revoked, or missing tokens → 401. When legacy token is disabled and presented, 401 body indicates the legacy token is disabled (without echoing it). (AC-10, SR-14)
- **FR-12** The resolved identity carries capability set (per Mode) and allowed_slugs. Legacy identity carries all-slugs.
- **FR-13** A single post-parse slug guard runs for every `/v1/{slug}/...` route (MCP, SSE, session, observe, any other) before `resolve_store`; slug not in allowed_slugs → 403. No default-store alias (`/v1/tools/...` treats `tools` as slug). (AC-09, SR-10)
- **FR-14** `read` tokens: search/lookup/get/briefing succeed; store/correct/deprecate/quarantine/edge/tag/enroll/cycle writes return an authorization error through the existing `Capability` write-gate (#4453), regardless of the tool-call `agent_id` claimed (including `human` or any registry agent holding Write). No parallel check. (AC-08, AC-25, SR-11)
- **FR-14b** Effective capabilities = registry grants(claimed `agent_id`) ∩ token mode set. Token mode set (ADR-003): `read` = {Read, Search}; `read-write` = {Read, Write, Search, SessionWrite}, never Admin; legacy token, stdio, and UDS = no ceiling (no behavior change vs. today). An HTTP request with no resolved principal = empty ceiling (deny). The token mode set is carried from the bearer-auth resolved identity into the tool context; the single `require_cap` check point tests the intersection. Intersection only narrows, never widens; an empty ceiling denies all capabilities. (AC-08, AC-25, AC-01, PL-4)
- **FR-14a** `read` tokens are rejected on observe (`/v1/{slug}/observe`) and session telemetry with 403, before the hook dispatcher runs; no observation, session, cycle, co-access, injection/query log, or extraction input is written. Rationale: observe feeds the full dispatcher and knowledge extraction. (AC-08, AC-18)
- **FR-15** Calls made with a `read` identity record no learning or usage side effects: usage counters, co-access, implicit signals, briefing/query logs feeding learning — including async/background queues. Gate is derived from effective capabilities (FR-14b, token ceiling) at one layer, regardless of claimed `agent_id`. Architect enumerates every side-effect site. (AC-15, SR-07, SR-17)
- **FR-16** `token:<label>` is the auth-layer audit identity only; it is never a tool-call `agent_id` and is never enrolled in the agent registry. The tool-call default `agent_id` when omitted is unchanged (`anonymous`). Audit records include the token label (or `legacy`) for every bearer call, independent of any caller-supplied `agent_id`; never the secret or hash. (AC-11, AC-27, SR-12)
- **FR-16a** Accepted residual gap (attribution only): within a `read-write` (or legacy) token, a caller may claim any tool-call `agent_id`, including `human`; `agent_id` stays self-asserted. Capability cannot exceed the token ceiling (FR-14b). Closure deferred to agent-identity work; no vnc-050 requirement targets it.
- **FR-17** Legacy-token disable is a single config setting with env override, following the `http.enabled` / `UNIMATRIX_HTTP_ENABLED` pattern `http.legacy_token_enabled` / `UNIMATRIX_LEGACY_TOKEN_ENABLED`. Default enabled. Env value overrides config file value; unrecognized env value fails at boot (non-zero exit, clear error). Part of the versioned config contract (PL-2). (AC-19)
- **FR-18** `/health` stays unauthenticated; no new unauthenticated route. (#5571)

### Client: credential resolution (Goals 2, 3)
- **FR-19** One credential resolver module is used by both `mcp-bridge.js` and the observe path. (SR-13, SR-15)
- **FR-20** Precedence: `UNIMATRIX_BUNDLE` → env pair → credential store (`remote.json`). `UNIMATRIX_BUNDLE` combined with any of the following is a terminal misconfiguration (exit non-zero, stderr names the conflicting variable, values not echoed):
  - `UNIMATRIX_REMOTE_MCP_URL`
  - `UNIMATRIX_REMOTE_URL`
  - `UNIMATRIX_REMOTE_TOKEN`
  - `UNIMATRIX_REMOTE_FINGERPRINT`

  `UNIMATRIX_BUNDLE` + `UNIMATRIX_REMOTE_HEADERS` is allowed (Cloudflare Access case); headers apply per FR-28. No other env var combination with the bundle is rejected. (SR-13)
- **FR-21** `UNIMATRIX_BUNDLE` alone: bridge completes `initialize` + `tools/call`, and observe events are delivered, with no `init` and no `remote.json`. (AC-16, SR-15)
- **FR-22** Env pair `UNIMATRIX_REMOTE_MCP_URL` + `UNIMATRIX_REMOTE_TOKEN` with no `remote.json`: bridge completes `initialize` and `tools/call`. (AC-02)
- **FR-23** Partial env pair (one half set, the other unset or empty per FR-40) → exit non-zero, clear stderr naming the missing or empty variable and the set one (e.g. "UNIMATRIX_REMOTE_TOKEN is empty; UNIMATRIX_REMOTE_URL is set"); token or any credential value never in stdout/stderr. Malformed `UNIMATRIX_BUNDLE` → same treatment. (AC-03, AC-29)
- **FR-24** `UNIMATRIX_REMOTE_FINGERPRINT` (format per vnc-034 ADR-002 #4948) pins the env-pair path only. With `UNIMATRIX_BUNDLE` set it is always a terminal misconfiguration per FR-20, whether or not it matches the bundle fingerprint. (SR-19)
- **FR-40** Every `UNIMATRIX_*` environment variable with an empty value is treated as unset for all resolution, precedence, conflict, and pairing rules (FR-17, FR-20, FR-23, FR-24, FR-28). No trimming: a whitespace-only value is non-empty. One pair half set + other empty → partial-pair error per FR-23, stderr naming the empty variable, no credential printed; both halves empty/unset → pair unset, credential store used. (AC-28, AC-29)

### Client: observe with read token
- **FR-24a** When observe receives 403 for a read-mode credential, the client emits exactly one warning per process (stderr, no token/header values), stops further observe sends for that process or treats subsequent 403s silently, and exits 0 / never fails the hook or job. MCP bridge operation is unaffected. (AC-18)

### Backward compatibility: new server × existing clients (Goal 8, human decision 4)
- **FR-34** An unmodified pre-vnc-050 client with an existing v:2 bundle carrying a pinned fingerprint and the legacy token attaches to the new server: `initialize`, `tools/call` (read and write) succeed. (AC-20)
- **FR-35** An existing `remote.json` written by pre-vnc-050 `init --bundle` (pinned, legacy token) works unchanged with both the old and new client against the new server; the new client reads it without migration or rewrite. (AC-21)
- **FR-36** The existing env-var observe path (`UNIMATRIX_REMOTE_URL` + `UNIMATRIX_REMOTE_TOKEN`, with or without `UNIMATRIX_REMOTE_FINGERPRINT`) delivers observe events to the new server with the legacy token, using both the old and new client. (AC-22)
- **FR-37** Legacy-token disabled (FR-17) against each FR-34–FR-36 client: request returns 401 with the disabled-token indication (FR-11); old client fails with its existing auth error, no hang or crash loop; new client observe path follows its existing non-fatal behavior. (AC-23)
- **FR-38** A new client given a pre-vnc-050 v:2 bundle (no new fields) via `UNIMATRIX_BUNDLE` or `init --bundle` attaches pinned exactly as before. (AC-24)
- **FR-39** Server-side: no existing route, response shape, status code, or header changes for legacy-token requests, except the FR-13 slug guard (legacy is all-slugs, so no new 403) and the FR-11 disabled body. (AC-20–AC-22)

### Client: TLS (Goal 3)
- **FR-25** No fingerprint → CA trust mode: Node default TLS, `rejectUnauthorized: true`, hostname verification on, built from the default `https` agent, not `cert-pin.js`. Untrusted cert or hostname mismatch → connection fails before any request bytes (headers, bearer token, body) are written. (AC-04, SR-01)
- **FR-26** Fingerprint present (store, bundle, or env) → existing pinned branch, byte-identical behavior; existing pin tests pass unmodified. (AC-05)
- **FR-27** Private CAs supported only via `NODE_EXTRA_CA_CERTS`; no custom CA setting.

### Client: extra headers (Goal 4)
- **FR-28** `UNIMATRIX_REMOTE_HEADERS` (JSON object of string→string) is applied to every MCP bridge request (initialize, calls, SSE GET, DELETE) and every observe POST. (AC-06, SR-16)
- **FR-29** Terminal misconfiguration (exit non-zero, values not echoed) on: invalid JSON, non-object, non-string values, names/values containing CR/LF, invalid header-name tokens, or case-insensitive `Authorization` / `Mcp-Session-Id`. (AC-06, SR-04)
- **FR-30** Header values are never logged, at any log level.

### Docs (Goals 6, 7)
- **FR-31** Direct-HTTPS recipe (curl + one generic MCP client) documented and exercised verbatim by a test or scripted smoke run. (AC-12)
- **FR-32** GitHub Actions example: read-only bundle searches and has a write rejected; a second read-write bundle writes only to its slug. A scripted local equivalent (local CA + test server, both tokens) runs in CI and is the only delivery-scope part. The live GH Actions run is a post-delivery operator check, never a delivery gate; vnc-050 adds no GH Actions workflow or infrastructure. (AC-13, SR-08)
- **FR-33** Tailscale findings doc: `tailscale serve`/Funnel with `*.ts.net` certs passing CA validation unpinned; GH Actions via `tailscale/github-action`; Cloudflare site reachability options. Gaps filed as GH issues and linked. Post-delivery operator check, never a delivery gate; no Tailscale work in vnc-050. C19 (#5792) remains unproven until the operator runs both this and the live AC-13 run. (AC-14)

## Non-Functional Requirements

- **NFR-01** No new npm runtime dependencies in the bridge/hook client.
- **NFR-02** Auth overhead per request: token table lookup is in-memory; revocation reflected on the next request (reload-on-change or per-request read — ADR). Added p99 latency < 1 ms at ≤ 100 tokens.
- **NFR-03** Token table file: 0600, written atomically (temp + rename) so a concurrent server read never sees a partial file. (SR-03)
- **NFR-04** Secrets: 32 random bytes from OS CSPRNG; stored only as SHA-256; never in logs, audit, `--list`, errors.
- **NFR-05** Back-compat: existing `{data_dir}/token` deployments and existing v:2 bundles/`remote.json` work with no config change.
- **NFR-06** No change to UDS trust model or `bridge.rs`.

## Acceptance Criteria

| AC | Criterion | Verification |
|---|---|---|
| AC-01 | Legacy token accepted on every slug with Read and Write; documented | Rust integration test over ≥2 slugs; doc review |
| AC-02 | `UNIMATRIX_REMOTE_MCP_URL`+`UNIMATRIX_REMOTE_TOKEN`, no `remote.json` → `initialize`+`tools/call` succeed against CA-trusted HTTPS | JS test with local CA via `NODE_EXTRA_CA_CERTS` |
| AC-03 | One var of the pair → non-zero exit, clear stderr, token absent from output | JS test asserting exit code and output scan |
| AC-04 | No fingerprint + untrusted or hostname-mismatched cert → rejected before any bytes of request/token | JS test with byte-capturing TLS server asserting zero application bytes received |
| AC-05 | Fingerprint present → pinned behavior unchanged; existing pin tests unchanged | Existing tests pass with no diff to test files |
| AC-06 | Extra headers on initialize, calls, SSE, observe; `Authorization`/`Mcp-Session-Id` override rejected; values not logged | JS test capturing headers per request type; log scan |
| AC-07 | Server loads multiple scoped tokens with mode and allowed_slugs | Rust test |
| AC-08 | `read` token: search/lookup/get/briefing OK; every write tool + observe → authorization error, for each claimed `agent_id` in {omitted, literal `token:<label>` string (rejected as reserved prefix / denied), registry Write-holder} | Rust table test: write tool × claimed agent_id |
| AC-09 | `read-write` token for A writes A, 403 on B for every route incl. `/v1/B/observe` | Rust route × token matrix test |
| AC-10 | Unknown/malformed/revoked → 401; constant-time compare; hashes at rest | Rust tests; file inspection test; ADR |
| AC-11 | Audit records token label for every bearer call, never secret | Rust test inspecting audit rows |
| AC-12 | curl / generic MCP recipe works verbatim | Scripted smoke run in test suite |
| AC-13 | Read-only bundle searches, write rejected; second token writes only its slug | Delivery: scripted local equivalent in CI. Post-delivery (not a gate): live GH Actions run, operator-executed |
| AC-14 | Tailscale findings doc covers listed items; gaps filed | Post-delivery operator check, not a delivery gate: doc review; issue links |
| AC-15 | `read` calls leave usage counters, co-access, implicit signals unchanged | Rust test snapshotting tables before/after (after async queues drain) |
| AC-16 | `client-bundle <slug> --headless --mode read` → bundle; `UNIMATRIX_BUNDLE` alone attaches (pinned or CA per bundle), no `init`/`remote.json`; `--revoke <label>` → 401 on next call without restart | End-to-end test: real server, CLI mint, bridge attach, observe delivery (read-write bundle), revoke, 401 |
| AC-17 | Revoked token retained as tombstone (hash only, revoked state); `--list` shows it; re-mint with same label exits non-zero; audit rows keep original attribution | Rust test: revoke, inspect table file, `--list`, re-mint attempt, audit query |
| AC-18 | `read` token on observe → 403, nothing written by dispatcher; client emits one warning, exit 0, job not failed | Rust test (table snapshot); JS test with multiple observe events asserting single warning and exit 0 |
| AC-19 | Legacy-token setting: default on; config off → 401; env override beats config both ways; unrecognized `UNIMATRIX_LEGACY_TOKEN_ENABLED` value → boot fails | Rust config test matrix |
| AC-20 | Old client + existing pinned v:2 bundle + legacy token → new server: initialize, read and write calls succeed | Compat test running the pre-vnc-050 client (pinned release artifact or git ref) against new server |
| AC-21 | Existing `remote.json` (pinned, legacy) works with old and new client against new server; file unchanged | Compat test with fixture `remote.json`; byte compare after run |
| AC-22 | Env-var observe path (`UNIMATRIX_REMOTE_URL`+`_TOKEN`, ± fingerprint) delivers to new server with old and new client | Compat test matrix: client {old,new} × fingerprint {set,unset-with-CA} |
| AC-23 | Legacy disabled → each AC-20–22 permutation gets 401 with disabled indication; no hang/crash loop; observe non-fatal | Compat test matrix with legacy disabled |
| AC-24 | New client + pre-vnc-050 v:2 bundle (fixture) via `UNIMATRIX_BUNDLE` and via `init --bundle` → pinned attach unchanged | JS test with fixture bundle |
| AC-25 | `read` token + tool-call `agent_id: "human"` → every write tool rejected with authorization error; no row written | Rust test |
| AC-26 | `read` token `context_search` (and briefing) → zero new usage and co-access rows after async queues drain | Rust test snapshotting usage/co-access tables |
| AC-27 | `read-write` token + omitted `agent_id` → call attributed `anonymous`; agent registry grants unchanged; no `token:` id ever present in the registry | Rust test: registry snapshot before/after, attribution check |
| AC-28 | `UNIMATRIX_BUNDLE` set + any FR-20 conflict var set to empty string → no conflict; bundle attaches | JS test per conflict var |
| AC-29 | URL/TOKEN pairs (`UNIMATRIX_REMOTE_MCP_URL`/`UNIMATRIX_REMOTE_TOKEN`, `UNIMATRIX_REMOTE_URL`/`_TOKEN`): one half set + other empty → non-zero exit per FR-23, stderr names the empty variable and the set one (e.g. "UNIMATRIX_REMOTE_TOKEN is empty; UNIMATRIX_REMOTE_URL is set"), no token or credential in output; both empty/unset → pair unset, credential store used | JS test per pair: set+empty (exit code, stderr text, output scan), empty+empty (store fallback) |

Supplementary (spec-derived, map to FRs): FR-07 flag misuse, FR-20 bundle + each of the four rejected env vars (four cases) and bundle + `UNIMATRIX_REMOTE_HEADERS` accepted, FR-29 header validation cases, FR-03 `init --bundle` on headless bundle — each needs a test.

Compat permutation matrix (AC-20–AC-24): client {pre-vnc-050, new} × credential source {v:2 pinned bundle via `init`, `remote.json`, env-var observe, `UNIMATRIX_BUNDLE` (new only)} × legacy token {enabled, disabled}. Every cell is a test case or marked N/A with reason in the test plan.

## User Workflows

1. **Operator mints CI credential (server host):** `project register <slug>` (existing) → `client-bundle <slug> --headless --mode read --label ci-search` → copies printed bundle into GH secret `UNIMATRIX_BUNDLE`.
2. **CI / cloud routine attaches:** job sets `UNIMATRIX_BUNDLE` (and optionally `UNIMATRIX_REMOTE_HEADERS`) → launches bridge → MCP tools work; observe events flow (read-write only; read token gets 403 on observe, client warns once and never fails the job — FR-24a).
3. **Operator audits/revokes:** `client-bundle <slug> --list` → `client-bundle <slug> --revoke ci-search` → CI's next call gets 401.
4. **Generic client:** curl/MCP client uses MCP URL + bearer token directly per recipe.
5. **Hardening:** operator disables legacy token once scoped tokens are in place.

## Constraints

- Pinned trust contract (vnc-039 ADR-001 #5115, vnc-034 ADR-002 #4948) must not regress; CA path never uses `rejectUnauthorized:false`.
- vnc-039 ADR-005 (#5119, bundle-only cloud MCP) is relaxed by this feature: env pair permitted. Architect supersedes/corrects the ADR.
- Pattern #5129 stands: pinned self-signed server is not registered as native http MCP.
- Bundle remains server-composed (#5081, #5543): client derives nothing route-shaped.
- Auth layer runs before slug parse; slug enforcement is a post-parse check; no default-store alias.
- Capability gating reuses existing `Capability` + write-gate (#4453).
- `/health` only unauthenticated route.
- AC-13 live run and AC-14 require operator infrastructure; both are post-delivery checks, never delivery gates. C19 (#5792) unproven until both run.

## Dependencies

- `crates/unimatrix-server`: `http/auth.rs` (`BearerValidator`, `StaticTokenValidator`), `http/token.rs`, `http/router.rs`, `router/seam.rs` (`SlugRouter`, `StoreResolver`), `http/public_url.rs`, audit log, usage/co-access/implicit-signal writers, `client-bundle` CLI and bundle encoder.
- `lib/hook-client`: `config.js`, `mcp-bridge.js`, `cert-pin.js`, bundle decoder used by `init --bundle`.
- Node stdlib `https`/`tls`/`crypto`; Rust `sha2`/`subtle` (or existing equivalents — no new crates if already present).
- Tailscale, Cloudflare Access (docs/validation only).

## NOT in Scope

- claude.ai connector OAuth; JWT/OIDC; token expiry/rotation automation.
- Closing `agent_id` attribution spoofing within a read-write/legacy token (residual gap, FR-16a; deferred to agent-identity work). Bearer-identity path stays dormant.
- Label reuse or hard deletion of token records.
- A separate `token` command, server mint API, MCP admin tool, or token UI.
- A new bundle format/version or second decoder.
- Per-tool/per-category ACLs beyond read vs read-write; multi-slug token minting beyond one slug per `client-bundle` invocation.
- Changes to UDS/local trust or `bridge.rs`.
- Tailscale/Cloudflare relay infrastructure; plain-HTTP listener.
- Custom CA config beyond `NODE_EXTRA_CA_CERTS`.
- Observe wire-format changes except shared header/TLS code.

## Open Questions

- **OQ-1 (architect):** Revocation propagation — file-watch reload vs per-request mtime check vs per-request read. Must satisfy "next request 401, no restart."
- **OQ-2 (architect):** CA-issued cert detection for FR-04 — auto-detect from cert chain, or explicit `--trust ca|pinned` flag on `client-bundle`. Flag would be an extension of the existing command, not a new flow.
- **OQ-3 (resolved, human 2026-11-25):** Server rejects (403); client accepts 403, warns once, never fails the job (FR-14a, FR-24a).
- **OQ-7 (architect/tester):** Source of the "old client" for AC-20–AC-23 — pinned last release artifact vs git ref checkout in CI.
- **OQ-4 (architect):** `init --bundle` on a headless bundle — accept (install into `remote.json`) or reject loudly? Spec permits either; must be tested.
- **OQ-5 (architect):** Label-derived agent_id format and whether scoped identities auto-enroll in agent registry.
- **OQ-6 (human):** One slug per token (scope wording "allowed_slugs" is plural, CLI is per-slug). Spec assumes single slug per mint; multi-slug left to table format only.
