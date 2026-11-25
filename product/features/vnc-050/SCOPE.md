# vnc-050: Headless Automation Access (env attach, CA-validated TLS, scoped tokens)

## Problem Statement
Non-interactive clients (GitHub Actions, Claude Code cloud routines, a Cloudflare-hosted site) cannot attach cleanly. The MCP bridge (`mcp-bridge.js`) only reads `~/.unimatrix/<projectHash>/remote.json` and hard-requires a pinned fingerprint. The server has one static token granting Read/Write/Search/SessionWrite across every slug. Putting that token in CI secrets or a website is a write-everything credential. Source: GH #1010.

## Goals
1. Document and test-lock the current baseline: the single token unlocks all slugs.
2. Headless attach extends the existing setup flow (`project register` → `client-bundle` → attach): `client-bundle <slug> --headless` emits a bundle a non-interactive client consumes from one secret (`UNIMATRIX_BUNDLE`) — no `remote.json`, no `init`. A raw URL/token env pair remains as the low-level path (curl / generic MCP client).
3. Pinning becomes optional. Without a fingerprint, the bridge validates the cert against standard CAs (normal Node TLS). With a fingerprint, today's pinned behavior is unchanged.
4. Callers can add extra request headers, such as Cloudflare Access `CF-Access-Client-Id` / `-Secret`.
5. Scoped tokens: multiple tokens, each `read` or `read-write`, each bound to `allowed_slugs`, minted/listed/revoked through `client-bundle`. Enforced server-side.
6. A documented direct-HTTPS MCP recipe (curl plus a generic MCP client).
7. Tailscale is validated only, as a post-delivery operator check (findings documented, gaps filed as follow-ups). No Tailscale work in vnc-050; never a delivery gate.

## Non-Goals
- claude.ai connector OAuth (separate spike).
- JWT/OIDC, token expiry/rotation automation, a token admin UI or MCP tool (unless chosen in OQ-3).
- Per-tool or per-category ACLs beyond read vs read-write.
- Changing the UDS/local trust model or the local bridge (`bridge.rs`).
- Building Tailscale or Cloudflare relay infrastructure.
- Changing the format of the hook-client observe path, except where it shares new header/TLS code.

## Background Research
- **Server auth** `crates/unimatrix-server/src/http/auth.rs`: `StaticTokenValidator` does a constant-time compare against a 32-byte token from `{data_dir}/token` (`http/token.rs`, 0600, O_EXCL). On success it returns a fixed `ResolvedIdentity{agent_id:"http-bearer", Restricted, [Read,Write,Search,SessionWrite]}`. It contains no slug binding. The `BearerValidator` trait already exists as an extension seam (FR-14).
- **Slug routing** (`http/router.rs`, `router/seam.rs`): the slug comes from path segment 2 (`/v1/{slug}/...`), and `SlugRouter` resolves it through `StoreResolver`. Auth runs as a tower layer before routing, so slug binding has to check the request path (or a slug carried in the identity) after the slug is parsed.
- **Hook client** `lib/hook-client/config.js`: it **already** honors `UNIMATRIX_REMOTE_URL`/`_TOKEN` (observe path). That path is unpinned, and an env pair with only one variable set is a terminal misconfiguration. This is the precedent the MCP bridge should follow.
- **MCP bridge** `lib/hook-client/mcp-bridge.js`: reads only the credential store. It rejects a missing `mcp_url`/token and **refuses a null fingerprint** ("cloud MCP requires a v:2 bundle"). `cert-pin.js` sets `rejectUnauthorized:false` and checks the fingerprint by hand on `secureConnect`. That logic has to stay on the pinned branch only.
- **Unimatrix ADRs**: vnc-039 ADR-001 (#5115, fail-loud pinned bridge), ADR-005 (#5119, cloud MCP is bundle-only; this feature relaxes it), vnc-034 ADR-002 (#4948, fingerprint format), vnc-049 ADR-008 (#5745, external_identity seam), pattern #5129 (do not register a pinned self-signed server as native http MCP), #4453 (write_capable flag pattern), #5571 (every port authenticated).
- `UNIMATRIX_PUBLIC_URL` (`http/public_url.rs`) sets the cert SANs and bundle URLs. This matters for Tailscale and proxy host names.

## Proposed Approach
- **Bridge**: resolution order is env pair, then credential store. The env path has no fingerprint, so it uses CA-validated TLS. An optional `UNIMATRIX_REMOTE_FINGERPRINT` pins. Extra headers come from `UNIMATRIX_REMOTE_HEADERS` (format TBD, OQ-2). They are never logged, and `Authorization`/`Mcp-Session-Id` cannot be overridden.
- **Server**: replace or extend `StaticTokenValidator` with a token-table validator. Each entry holds a hashed secret, mode, `allowed_slugs`, and a label. The identity carries the capability set (read omits Write/SessionWrite) and the allowed slugs. The router rejects slugs outside `allowed_slugs` with 403 before `resolve_store`. The existing `{data_dir}/token` stays as the implicit all-slugs read-write token for back-compat.
- **Docs**: direct-HTTPS recipe; GH Actions example (docs only; no GH Actions workflow/infra in vnc-050). Tailscale findings are post-delivery.

## Acceptance Criteria
- AC-01: A test asserts that the legacy single token is accepted on every configured slug with Read and Write. The behavior is documented.
- AC-02: With `UNIMATRIX_REMOTE_URL`+`UNIMATRIX_REMOTE_TOKEN` set and no `remote.json`, `mcp-bridge.js` completes `initialize` and `tools/call` against a CA-trusted HTTPS server.
- AC-03: An env pair with only one variable set exits non-zero with a clear stderr message, and the token never appears in output.
- AC-04: Without a fingerprint, a cert that is untrusted or has a hostname mismatch is rejected before any request body (or bearer token) is written.
- AC-05: With a fingerprint (store or env), pinned behavior is byte-identical to today. Existing pin tests pass unchanged.
- AC-06: Configured extra headers appear on every bridge request (initialize, calls, SSE). Attempts to override `Authorization` are rejected. Header values are never logged.
- AC-07: The server loads multiple scoped tokens, each with mode `read|read-write` and `allowed_slugs`.
- AC-08: A `read` token can search/lookup/get. Write tools (store/correct/deprecate/quarantine/edge/cycle writes, observe) return an authorization error.
- AC-09: A `read-write` token bound to slug A writes to A and gets 403 on slug B for every route, including `/v1/B/observe`.
- AC-10: Unknown or malformed tokens get 401. Token comparison stays constant-time. Secrets are stored at rest as hashes (or per OQ-4).
- AC-11: Audit logs record a token label (never the secret) for every bearer call.
- AC-12: A documented curl / generic-MCP-client recipe is exercised by a test or a scripted smoke run.
- AC-13: A GH Actions job attaches with a read-only token, searches successfully, and has a write rejected. A second token writes only to its permitted slug. Delivery scope: scripted local CI equivalent only; the live GH Actions run is a post-delivery operator check, never a delivery gate.
- AC-14: The Tailscale findings doc covers `tailscale serve`/Funnel with `*.ts.net` certs passing CA validation unpinned, GH Actions via `tailscale/github-action`, and Cloudflare site reachability options. Gaps are filed as issues. Post-delivery operator check; not in delivery scope, never a delivery gate.
- AC-15: Calls made with a `read` token (search/lookup/get/briefing) leave learning and usage state (usage counters, co-access, implicit signals) unchanged.
- AC-16: `client-bundle <slug> --headless --mode read` emits a bundle; with it set as `UNIMATRIX_BUNDLE` alone, the bridge attaches (pinned or CA, per bundle) with no `init` and no `remote.json`. `--revoke <label>` makes that bundle's token get 401.

## Constraints
- The pinned path's trust contract (vnc-039 ADR-001 and vnc-034 ADR-002) must not regress. The CA path must not inherit `rejectUnauthorized:false`.
- Pure-JS bridge with no new npm runtime dependencies (existing pattern).
- Auth runs before slug parsing, so slug enforcement needs a post-parse check. No default-store alias may exist (`/v1/tools/...` parses `tools` as a slug).
- `/health` stays unauthenticated (ADR-002). No new unauthenticated surface (#5571).
- Back-compat: existing deployments using `{data_dir}/token` and v:2 bundles keep working.
- Capability gating must reuse the existing `Capability` and write-gate machinery (#4453), not add parallel checks.
- Live AC-13 and AC-14 need a live server and a tailnet; they are post-delivery operator checks, never delivery gates. C19 (#5792) stays unproven until the operator runs both.

## Open Questions — Resolved (uni-zero, 2026-11-25)
Principle: headless access extends the existing personal-cloud setup flow; it is not a second one.
- OQ-1/OQ-3 (resolved together): the bundle is the headless credential. `unimatrix client-bundle <slug> --headless --mode read|read-write --label <name>` mints a slug-bound scoped token and embeds it. The client reads `UNIMATRIX_BUNDLE` (same decoder as `init --bundle`), deriving MCP and observe URLs, token, and optional fingerprint. Revocation in scope: `client-bundle <slug> --revoke <label>` and `--list`. No separate `token` command. Low-level fallback: `UNIMATRIX_REMOTE_MCP_URL` + `UNIMATRIX_REMOTE_TOKEN` (new MCP var — `UNIMATRIX_REMOTE_URL` keeps its observe meaning); a partial pair is a terminal misconfiguration.
- OQ-2: `UNIMATRIX_REMOTE_HEADERS` (JSON object), separate from the bundle (caller-owned, e.g. Cloudflare Access). Applied on both bridge and observe paths. `Authorization`/`Mcp-Session-Id` not overridable; values never logged.
- OQ-4: hashed at rest (SHA-256; tokens are 32 random bytes). Secret shown once, at mint. Legacy `{data_dir}/token` unchanged.
- OQ-5: legacy all-slugs token on by default (back-compat); operator config option disables it. Docs recommend disabling once scoped tokens are in use.
- OQ-6: read/write line = "mutates knowledge or feeds learning." Read tokens: search/lookup/get/briefing allowed; observe/session telemetry (`SessionWrite`) denied; usage/co-access side effects of read calls NOT recorded (SLN1 poisoning guard). New AC: read-token calls leave learning/usage tables unchanged.
- OQ-7: trust mode follows the server cert. Self-signed → bundle carries fingerprint → pinned (unchanged). CA-issued (e.g. `*.ts.net`) → bundle omits fingerprint → standard CA validation. `NODE_EXTRA_CA_CERTS` covers private CAs; no custom setting.
- OQ-8: deferred; validate only (AC-14), follow-up if a gap is found. No plain-HTTP listener in vnc-050.

## Tracking
GH #1010
