# vnc-027 Pseudocode — OVERVIEW

TS UDS hook client + hook-set reduction (F4a). GH #680. Stage 3a pseudocode.
Authority order when wording conflicts: **ADR-001..ADR-007 (with the ADR-004 /
ADR-006 amendments) > IMPLEMENTATION-BRIEF > ARCHITECTURE > SPECIFICATION**.
Integration surface names are traced to the existing source files (cited inline);
nothing here invents an interface.

## Components (10 + this overview)

| # | File | Lang | Status | Pseudocode |
|---|------|------|--------|-----------|
| 1 | `test/check-hook-client-size.js` | JS | REWRITE (first commit) | size-gate.md |
| 2 | `crates/unimatrix-engine/src/wire.rs` | Rust | MOD additive | wire-accept-text.md |
| 3 | `uds/listener.rs` + `http/router/observe.rs` (+ `uds/hook.rs` mechanical) | Rust | MOD | listener-preformatted.md |
| 4 | `lib/hook-client/transport-uds.js` | JS | NEW | transport-uds.md |
| 5 | `lib/hook-client/config.js` | JS | MOD | config-transport-selection.md |
| 6 | `lib/hook-client/index.js` | JS | MOD | index-dispatch.md |
| 7 | `lib/hook-client/build-request-tools.js` | JS | MOD | build-request-sentinel.md |
| 8 | `lib/merge-settings.js` | JS | MOD | merge-settings-reduction.md |
| 9 | `lib/hook-client/state.js` | JS | MOD | state-offset-rekey.md |
| 10 | Parity corpus (UDS layer, test files) | JS+fixtures | NEW | parity-corpus-uds.md |

## Shared types (defined once here)

```
SendResult = {                       // transport-http.js:5-7 contract (oracle)
  ok: boolean,
  status: number,                    // HTTP-equivalent; UDS uses 0/200/204/code
  contentType: string | null,
  body: Buffer | null,
  failureClass: null | "auth" | "connect" | "timeout" | "http_4xx" | "http_5xx"
}                                    // NO new failureClass values (ADR-002 §2)

ResolvedConfig (ok:true) = {         // config.js::ok(); NEW fields starred
  ok: true, source, projectRoot, projectHash, stateDir, urlHost,
  mode: "http" | "uds",             // * NEW (ADR-002 §3)
  // http: url, token, timeouts
  // uds:  socketPath                // * NEW = ~/.unimatrix/{projectHash}/unimatrix.sock (ADR-007)
}
ResolvedConfig (ok:false) = { ok:false, reason: "partial_env"|"malformed", projectRoot, projectHash, stateDir }
                                     // "missing" reason RETIRED (ADR-002 §3): no-remote → mode:"uds"

HookRequest frame (JSON)            // transport-agnostic; queue stores raw, NO accept
HookResponse (wire, serde tag "type"): Pong | Ack | Error{code,message}
                                   | Entries{items,total_tokens} | BriefingContent{content,token_count}
                                   | Text{body}              // * NEW variant (ADR-001 §3)
```

Wire frame: 4-byte big-endian u32 length prefix + JSON. `MAX_PAYLOAD_SIZE =
1_048_576`. Write rejects payload > cap; read rejects declared length 0 or > cap
**before allocating** (wire.rs:16,349,372 is the byte authority).

NEW request fields (ADR-001 §1): `ContextSearch.accept: Option<String>` and
`CompactPayload.accept: Option<String>`, both
`#[serde(default, skip_serializing_if = "Option::is_none")]`. Set ONLY by
transport-uds at serialization time, value `"text/plain"`. Builders and queued
frames never carry it.

## Data flow (per spawn — unchanged shape, NEW points starred)

```
stdin → parseHookInput → normalize → resolveCwd
  → config.resolve(cwd)              # * yields mode + (socketPath | url/token)
  → buildRequest                     # * PreToolUse non-cycle → null sentinel
  → if request === null: return (exit 0, no transport, no stdout)   # *
  → transport = config.mode === "uds" ? transportUds : transportHttp  # * selected once
  → FNF:  pruneOffsets → queue.prune → queue.replay(config, transport.post)  # * pruneOffsets wired
          → carrying transport.post (+ delta transport.post)
          → on carrying.ok && canonicalEvent==="TaskCompleted": deleteOffset  # * rekeyed
  → sync: transport.post(config, frame, {sync:true}) → transform.writeSyncOutput
```

UDS sync exchange (ADR-001 + ADR-003), transport-uds injects `accept:"text/plain"`:

```
client                                   daemon (listener.rs handle_connection)
connect(socketPath); arm 40ms deadline   auth (peer-cred) → read_exact(4) → len
end(frame{...,"accept":"text/plain"})     → read_exact(len) → deserialize HookRequest
  (write+FIN, flush guaranteed)           → wants_text = accept_of(&req)==Some("text/plain")  # pre-dispatch
read 4B header + body  ◄───────────────   → dispatch_request(req) (unchanged)
destroy(); map per ADR-002 §2             → if wants_text: convert Entries/BriefingContent via
                                            shared injection core → Text{body} (or Ack if empty);
                                            Ack/Error/Pong stay JSON (hard allowlist)
```

FNF: `connect → end(frame) → resolve on 'finish'; no read`. Server EPIPE writing
its Ack to the FIN'd socket is expected and DEBUG-classified (#3448).

## Merge sequencing (BINDING — SR-02 / R-02 Critical)

1. **size-gate rewrite — LITERAL FIRST COMMIT** (client at 99,997/100,000 raw;
   3-byte headroom). vnc-030 also depends on this redefinition. Merge order must
   be auditable in `git log`.
2. Wire additions (wire-accept-text + listener-preformatted) — server side,
   independently testable; AC-11 proves F1 fixtures byte-unchanged.
3. transport-uds + config mode + index selection + parity-corpus UDS layer.
4. Hook-set reduction (build-request-sentinel + merge-settings-reduction) +
   FR-16 rekey (state-offset-rekey + index wiring).
5. Dogfood switchover (post-merge, drop detector — not an F4a gate item).

## Cross-cutting contracts every component must honor

- **Frozen F1 wire contract — additive only**: `skip_serializing_if` optionals,
  no renames/removals, no `deny_unknown_fields` (AC-11).
- **Text-only-to-accept-callers coupling** (ADR-001 §6, R-08): `Text` is returned
  ONLY to a caller that sent `accept`. The frozen Rust hook never sends `accept`,
  so it never receives `Text` and never fails to deserialize. This coupling is the
  ONLY protection; it is exact, not approximate.
- **age-prune-only** (ADR-006, authoritative over spec FR-30/AC-10 "and/or"):
  SessionClose delete removed; 7-day `pruneOffsets` is the sole effective
  mechanism. `TaskCompleted` branch retained, keyed by **canonical event name,
  never frame type**, unreachable-but-unit-tested, NOT in HOOK_EVENTS.
- **Fail-open (NFR-3)**: never throw to host; exit 0 always; no stdout on failure;
  no secrets in stderr/breadcrumbs; every fs/network call wrapped.
- **No new npm deps**: Node `net`/`fs`/`path`/`crypto` core only. UDS Unix-only —
  document, don't shim.

## Sequencing constraints for Stage 3b implementers

- size-gate stands alone (no deps) and must merge before any client byte grows.
- wire-accept-text must land before listener-preformatted (the listener references
  the new variant) and before transport-uds end-to-end round-trip tests can pass.
- config-transport-selection must land before index-dispatch can select a UDS
  transport; transport-uds must exist before index injects it.
- state-offset-rekey (deleteOffset semantics + pruneOffsets already exists) pairs
  with the index-dispatch wiring change in the same merge step (4).
