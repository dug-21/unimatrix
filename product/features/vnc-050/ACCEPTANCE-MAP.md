# vnc-050 Acceptance Criteria Map

AC-01..AC-16 from SCOPE.md; AC-17..AC-29 added by SPECIFICATION.md (rework 1, HUMAN-DECISIONS 2-5).

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | Legacy single token accepted on every slug with Read and Write; documented | test | Rust integration test over ≥2 slugs, read + write via HTTP (R-15.4); grep docs for legacy-token baseline statement | PENDING |
| AC-02 | `UNIMATRIX_REMOTE_MCP_URL`+`_TOKEN`, no `remote.json` → `initialize`+`tools/call` succeed against CA-trusted HTTPS | test | Spawned `mcp-bridge.js`, clean HOME, local CA via `NODE_EXTRA_CA_CERTS` | PENDING |
| AC-03 | Partial env pair → non-zero exit, clear stderr, token absent from output | test | Spawn bridge with each var alone; assert exit code, stderr names missing var, sentinel token scan clean | PENDING |
| AC-04 | No fingerprint + untrusted or hostname-mismatched cert → rejected before any request/token bytes | test | Byte-capturing TLS server (untrusted; CA-trusted wrong host) for bridge, observe POST, `pingForInit`: zero app bytes; guard test for `rejectUnauthorized:false` (R-01) | PENDING |
| AC-05 | Fingerprint present (store/bundle/env) → pinned behavior byte-identical; existing pin tests unchanged | shell | `git diff main -- <existing pin test files>` empty; tests pass; each fp source selects pinned branch vs real self-signed server (R-11) | PENDING |
| AC-06 | Extra headers on initialize, calls, SSE, DELETE, observe; `Authorization`/reserved override rejected; values never logged | test | Capturing proxy per request type; reject matrix (R-08.2); stderr/log grep for sentinel value (R-09) | PENDING |
| AC-07 | Server loads multiple scoped tokens with mode and allowed_slugs | test | Rust test loading `tokens.json` with ≥2 entries of each mode; strict-parse rejects (unknown key, dup label, empty slugs) | PENDING |
| AC-08 | `read` token: search/lookup/get/briefing OK; every write tool + observe → authorization error for each claimed agent_id | test | Rust HTTP table test: write tool × agent_id {omitted, human, registry Write-holder, literal `token:<label>` string (rejected as reserved prefix / denied)} → error, DB unchanged; read-write companion succeeds (R-02.1/2) | PENDING |
| AC-09 | `read-write` token for A writes A; 403 on B for every route incl. `/v1/B/observe` | test | Route matrix {MCP POST, SSE GET, DELETE, observe} × {legacy, read@A, rw@A} × {A, B} + path variants (R-03) | PENDING |
| AC-10 | Unknown/malformed/revoked → 401; constant-time compare; hashes at rest | test | Rust 401 tests; `tokens.json` inspection (no plaintext); control-flow test match at index 0 and n-1, no HashMap/early break (R-06) | PENDING |
| AC-11 | Audit records token label (never secret) for every bearer call | test | Inspect `http_access` rows for legacy, scoped MCP, observe, denied (bound-slug log only); sentinel grep (R-16, R-09) | PENDING |
| AC-12 | curl / generic MCP client recipe works verbatim | shell | Scripted smoke extracts commands from docs file and runs them unmodified against local CA server | PENDING |
| AC-13 (delivery) | Read-only bundle searches, write rejected; second token writes only its slug — scripted local equivalent | shell | CI: scripted local CA + test server, read and read-write tokens | PENDING |
| AC-13 (live) | Same, via live GH Actions job | manual | Post-delivery operator check — not a delivery gate. vnc-050 adds no GH Actions workflow/infra. C19 (#5792) unproven until run | POST-DELIVERY |
| AC-14 | Tailscale findings doc (serve/Funnel `*.ts.net` CA unpinned, `tailscale/github-action`, Cloudflare reachability); gaps filed | manual | Post-delivery operator check — not a delivery gate. No Tailscale work in vnc-050. C19 (#5792) unproven until run | POST-DELIVERY |
| AC-15 | `read` calls leave usage counters, co-access, implicit signals unchanged | test | Snapshot tables, run search/lookup/get/briefing (helpful:true, agent_id:"human"), drain channels, compare; read-write and legacy companions change tables (R-04) | PENDING |
| AC-16 | `client-bundle --headless --mode read` → bundle; `UNIMATRIX_BUNDLE` alone attaches (pinned or CA); `--revoke` → 401 without restart | test | E2E: real server, CLI binary mint, spawned bridge (clean HOME, no remote.json), observe delivery with rw bundle, CLI revoke, next call 401 incl. open session (R-05.1/2) | PENDING |
| AC-17 | Revoked token kept as tombstone; `--list` shows it; same-label re-mint fails; audit attribution retained | test | Revoke, inspect `tokens.json` record, `--list`, re-mint exit non-zero, audit query; re-revoke/unknown → error, file byte-identical (R-05.4/5) | PENDING |
| AC-18 | `read` token observe → 403, nothing dispatched; client one warning, exit 0, job not failed | test | Rust: dispatcher spy zero calls, tables unchanged after extraction tick. JS: spawned hook, N≥3 events, one stderr warning, exit 0, exactly one 403 across N processes; credential change resets; corrupt marker → one retry + rewrite; rw no warning, no marker (R-17, HD 10) | PENDING |
| AC-19 | Legacy setting: default on; config off → 401; env beats config both ways; bad env value fails boot | test | Rust matrix config {absent,true,false} × env {unset,empty,true,false,1,0}; `yes`/`2`/`off`/`TRUE ` → boot error (R-15) | PENDING |
| AC-20 | Old client + pinned v:2 bundle + legacy token → new server: initialize, read, write OK | test | Compat harness with `@dug-21/unimatrix@0.11.3` tarball (R-21.1) | PENDING |
| AC-21 | Existing `remote.json` works with old and new client; file unchanged | test | Fixture `remote.json`; byte compare after both runs (R-21.2) | PENDING |
| AC-22 | Env-var observe path (± fingerprint) delivers with old and new client | test | Matrix client {old,new} × fp {set, unset-with-CA}; observe rows land (R-21.1) | PENDING |
| AC-23 | Legacy disabled → each AC-20..22 permutation 401 with disabled indication; no hang/crash loop; observe non-fatal | test | Compat matrix with legacy disabled, timeout-bounded (R-21.4) | PENDING |
| AC-24 | New client + pre-vnc-050 v:2 bundle via `UNIMATRIX_BUNDLE` and `init --bundle` → pinned attach unchanged | test | JS test with fixture bundle, pinned branch asserted (R-21.3) | PENDING |
| AC-25 | `read` token + `agent_id:"human"` → every write tool rejected; no row written | test | Rust HTTP test over all write tools; DB snapshot unchanged (R-02.1) | PENDING |
| AC-26 | `read` token `context_search`/briefing → zero new usage and co-access rows after queues drain | test | Rust table snapshot after channel drain; usage/co_access/query_log unchanged (R-04) | PENDING |
| AC-27 | `read-write` token + omitted agent_id → attributed `anonymous`; registry grants unchanged; no `token:` id in registry | test | Rust test: registry snapshot before/after, attribution check (R-22) | PENDING |
| AC-28 | `UNIMATRIX_BUNDLE` + any FR-20 conflict var set to empty string → no conflict; bundle attaches | test | JS test per conflict var (FR-40) | PENDING |
| AC-29 | URL/TOKEN pair: one half set + other empty → non-zero exit, stderr names empty and set var, no credential; both empty → store used | test | JS test per pair (MCP and observe): set+empty exit/stderr text/output scan; empty+empty store fallback (FR-23, FR-40, R-18.4) | PENDING |

## Supplementary (spec-required tests, map to FRs)

| Ref | Verification |
|-----|-------------|
| R-02 step 0 | First delivery step: `BearerPrincipal` present at tool in stateful HTTP session (else ADR-003 fallback) |
| FR-07 / R-14 | Flag-matrix table test: each invalid combo exit 2, token file mtime unchanged |
| FR-20 / R-18 | Bundle + each of 4 rejected vars → exit 1 naming var (bridge and hook); bundle + headers accepted; empty-string vars per AC-28/29 |
| FR-29 / R-08 | Header validation reject matrix |
| FR-03 / R-10 | Parity corpus `fp:null` accepted, malformed fp rejected (Rust and JS); `init --bundle` CA bundle stores `fingerprint:null` |
| FR-39 | Legacy-token response diff old vs new server empty except disabled body |
| R-12 | 20 parallel mints all present; 0600 on file and tmp; no tmp left |
| R-13 | `agent_id:"token:ci"` → `InvalidInput` on stdio, UDS, HTTP; no `token:` rows in agents table |
| R-20 | Regression pin: rw token + `human` non-Admin write succeeds; `http_access` records real label; rw + `human` Admin tool denied |
