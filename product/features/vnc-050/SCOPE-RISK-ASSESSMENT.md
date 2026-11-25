# Scope Risk Assessment: vnc-050

## User-Facing Entry Points & Behavioral Outcomes

| Entry point | Path-independent outcome |
|---|---|
| `unimatrix client-bundle <slug> --headless --mode read\|read-write --label L` | Emits a bundle; secret shown once; token listed under `--list` with label/mode/slug |
| `client-bundle <slug> --list` | Shows labels/modes/slugs, never secrets |
| `client-bundle <slug> --revoke L` | Next call with that bundle's token gets 401 |
| `UNIMATRIX_BUNDLE` set alone, bridge launched (CI / routine) | MCP `initialize` + `tools/call` succeed; observe events land; no `init`, no `remote.json` |
| `UNIMATRIX_REMOTE_MCP_URL` + `UNIMATRIX_REMOTE_TOKEN` | Same as above for MCP; one var alone exits non-zero, token not echoed |
| `UNIMATRIX_REMOTE_HEADERS` (JSON) | Headers visible at the proxy on every MCP + observe request; `Authorization`/`Mcp-Session-Id` override rejected loudly |
| `UNIMATRIX_REMOTE_FINGERPRINT` / bundle fingerprint | Pinned trust identical to today |
| No fingerprint, untrusted/mismatched cert | Connection refused before any token/body leaves the client |
| Read token calls write tool / observe / session write | Authorization error; learning/usage tables unchanged even for allowed reads |
| Token bound to A calls any `/v1/B/...` route | 403 |
| Legacy `{data_dir}/token` | Still all-slugs read-write unless operator disables it |
| curl / generic MCP client recipe | Recipe, as documented, works verbatim |

## Technology Risks

| ID | Risk | Sev | Lik | Recommendation |
|---|---|---|---|---|
| SR-01 | CA path reuses `cert-pin.js` agent and inherits `rejectUnauthorized:false`, or the token is written before the handshake check finishes | High | Med | Keep separate TLS branches; test with untrusted and hostname-mismatched certs, asserting zero bytes of the request reach the server |
| SR-02 | SHA-256 lookup on hashed tokens: a hash-keyed map lookup is not constant-time; plain digest compare loop may leak | Med | Med | Hash the presented token, then do a constant-time compare per entry (or prove hash-index is safe); document the choice as an ADR |
| SR-03 | Token table file concurrency: CLI mint/revoke while the server runs. Is revocation live or restart-only? | High | High | Decide on reload semantics up front; AC-16 implies live 401 after revoke |
| SR-04 | `UNIMATRIX_REMOTE_HEADERS` JSON parse failure / non-string values / CRLF injection | Med | Med | Fail loud on bad JSON; reject header names/values containing CR/LF; case-insensitive check on reserved names |

## Scope Boundary Risks

| ID | Risk | Sev | Lik | Recommendation |
|---|---|---|---|---|
| SR-05 | Drift to a second creation flow (a separate `token` command, server-side mint API, or a parallel headless bundle format/decoder) — violates human constraint | High | Med | `--headless` must be a flag on existing `client-bundle`; `UNIMATRIX_BUNDLE` must use the same decoder as `init --bundle`; treat any new command/decoder as a gate fail |
| SR-06 | Headless bundle format diverges from v:2 (new version, new fields) silently breaking `init --bundle` | Med | Med | Extend v:2 compatibly or version-bump with both decoders tested; `init --bundle` must accept a headless bundle or reject it loudly |
| SR-07 | OQ-6 read/write line: "read calls leave learning state unchanged" touches many side-effect sites (usage counters, co-access, implicit signals, briefing) | High | High | Enumerate every side-effect site of read tools; gate them on the identity capability, not per-call flags |
| SR-08 | AC-13/AC-14 need live infra; risk of shipping with them "deferred" and unproven | Med | High | Spec a scripted local equivalent (local CA + test server) for AC-13; mark operator-run parts explicitly |
| SR-09 | `--list`/`--revoke` operate on the server data dir; unclear whether `client-bundle` runs where the token table lives | Med | Med | Constrain the CLI to the server host (same as existing `client-bundle`); state it |

## Integration Risks

| ID | Risk | Sev | Lik | Recommendation |
|---|---|---|---|---|
| SR-10 | Slug enforcement after auth layer: some route (observe, SSE, session, `/v1/tools` parsed as slug) skips the post-parse check | High | Med | Single enforcement point covering every `/v1/{slug}/...` route; table-driven test over all routes x tokens |
| SR-11 | Read identity omits Write/SessionWrite but a tool path checks a different gate, or bypasses `Capability` (#4453) | High | Med | Reuse existing write-gate; test every write tool with a read token |
| SR-12 | Legacy token identity (`http-bearer`) and new labeled identities collide in audit/agent registry; enrollment/trust per agent_id becomes ambiguous | Med | Med | Define agent_id per token (e.g. label-derived) and audit label field |
| SR-13 | Hook-client observe path and MCP bridge resolve config differently (`UNIMATRIX_REMOTE_URL` vs `_MCP_URL`, bundle vs env precedence) | Med | High | One shared resolver module for both paths; define precedence: bundle vs env pair vs store, and conflict = fail loud |
| SR-14 | Legacy-token disable option breaks existing local clients/hooks on upgrade | Med | Low | Default on (per OQ-5); clear 401 message hinting at disabled legacy token |

## Path-Divergence Risks

| ID | Entry point | Divergence | Recommendation |
|---|---|---|---|
| SR-15 | `UNIMATRIX_BUNDLE` | MCP bridge honors it but observe/hook path ignores it (or vice versa); CI sees MCP work, telemetry silently dropped | Both paths consume the bundle; test each from the env var |
| SR-16 | `UNIMATRIX_REMOTE_HEADERS` | Applied on initialize/calls but not SSE stream or observe POST; Cloudflare Access blocks those silently | Test header presence on every request type |
| SR-17 | Read token on read tools | Tool returns success but side effects recorded via a background/async path (usage queue) | Assert table state after calls, not just responses |
| SR-18 | `--mode`/`--label` on client-bundle without `--headless` | Flags accepted and ignored | Reject or define; no silent no-op |
| SR-19 | Bundle fingerprint present while env `UNIMATRIX_REMOTE_FINGERPRINT` also set | One silently wins | Define precedence or fail loud on conflict |

## Assumptions

- Goals 2 / OQ-1: existing `client-bundle` and bundle decoder can be extended without breaking v:2 consumers.
- Background (auth.rs): `BearerValidator` seam can return per-token slug sets and capabilities without router API changes.
- OQ-7: CA-issued certs (`*.ts.net`) validate under stock Node TLS; private CAs via `NODE_EXTRA_CA_CERTS` only.
- OQ-6: all learning side effects of read tools are identifiable and gateable at one layer.
- Constraints: `client-bundle` runs with access to the server's data dir.

## Design Recommendations

1. SR-05/SR-06/SR-13: one credential resolver and one bundle decoder shared by `init --bundle`, bridge and observe path; `--headless` is a flag, not a flow.
2. SR-01: CA branch built from default `https` agent with explicit `rejectUnauthorized: true`; global TLS override (`NODE_TLS_REJECT_UNAUTHORIZED=0`) is ignored by design; pin branch untouched; byte-level "no token before verify" test, plus var=0 + untrusted cert on CA path → refused, zero app bytes.
3. SR-10/SR-11: single post-parse slug guard + existing Capability gate; route x token matrix test.
4. SR-03: specify live revocation (reload-on-change or per-request read) with ADR.
5. SR-07/SR-17: inventory read-tool side effects; gate by identity; assert DB state.
6. SR-18/SR-19/SR-04: every accepted-but-unusable input fails loud.
