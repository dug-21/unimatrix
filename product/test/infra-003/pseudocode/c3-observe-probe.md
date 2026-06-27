# C3 — Observe write surface, both directions (`obs-a`, `obs-b`)

> Source: ARCH C3, ADR-002, SPEC FR-02.2/AC-02, RISK R-04/R-09. SR-04.

## Purpose

Drive the two marked **observe** writes through the genuine production funnel
`parse_project_key → resolve_store → dispatch_request`, one per direction, over
the **one** cert-pinned bearer token (the slug is in the URL path, so one token
serves all four writes — FR-02.1). The marker rides `topic_signal`, which
round-trips verbatim into `observations.topic_signal` (`analytics.rs:539-554`,
`db.rs:865`). A `204` alone is **not** the verdict — each write is paired with its
C5 read-as-barrier positive control.

This component issues the **write** only. The barrier read and verdict are C5/C7.

## New Functions

### `observe_write(slug, marker)` → asserts HTTP 204; returns on success

```
observe_write(slug, marker):
    url := "https://localhost:${PORT}/v1/${slug}/observe"
    # Wire: HookRequest::RecordEvent { event: ImplantEvent }, serde tag "type".
    # Marker lands in ImplantEvent.topic_signal: Option<String> (wire.rs:267).
    # Build the JSON body with `node` (existing JSON-shaping idiom), NEVER string
    # interpolation, so the marker cannot break JSON quoting. Marker charset is
    # already constrained to [a-z0-9-] (R-12), but node-built JSON is the safe path.
    body := node-build({
        "type": "RecordEvent",
        "event": { ...minimal valid ImplantEvent...,
                   "topic_signal": marker }
    })

    code := curl -sS --cacert "$TMP/cert.pem" -o /dev/null -w '%{http_code}' \
                 -X POST url \
                 -H "Authorization: Bearer ${TOKEN}" \
                 -H "Content-Type: application/json" \
                 -d body
            on curl transport failure: infra_fail "observe POST to <url> failed (transport)"

    if code != "204":
        # A non-204 here is a transport/route fault for THIS write, not an
        # isolation property failure → INFRA (the write never genuinely landed).
        infra_fail "observe <slug> returned HTTP <code> (expected 204) —
                    write did not enter the funnel; INFRA, not RED"
    log "observe write <slug> accepted (204). marker=<marker>"
```

> The exact minimal `ImplantEvent` shape (required fields besides `topic_signal`)
> is taken from `wire.rs:109,129,267`; the posture-smoke uses a `SessionRegister`
> frame (`:454`) as a concrete example of a valid observe body. Tester confirms
> the smallest valid `RecordEvent` that persists a `topic_signal` row.

## State / Sequencing

No lifecycle state. Called by the C5 per-cell driver in strict per-store sequence:
`A-obs` write → C5 barrier; later `B-obs` write → C5 barrier. The two observe
writes target **different stores** (A vs B), so they are independent; the strict
sequencing that matters (C-08) is write-then-barrier per cell, owned by C5.

## Data Flow

| In | Out |
|----|-----|
| `slug` (`$SLUG_A`/`$SLUG_B`), `marker` (`$M_OBS_A`/`$M_OBS_B`), `$PORT`, `$TOKEN`, `$TMP/cert.pem` | server persists `observations.topic_signal = marker` in that slug's store; HTTP 204 to caller |

Marker → column mapping (load-bearing, R-09): `topic_signal` (payload) →
`observations.topic_signal TEXT`. C5/C6 query exactly this column. If a future
payload shape drops `topic_signal`, the spec-named fallback is
`observations.input` substring (SPEC OQ; not an ad-hoc guess) — flag, do not
silently switch.

## Error Handling

| Condition | Outcome |
|-----------|---------|
| curl transport failure | INFRA |
| HTTP != 204 | INFRA (write did not enter the funnel; never RED) |
| HTTP == 204 | return; C5 barrier decides PRESENT vs INFRA |

Note: a 204 does **not** prove the row landed in the right store, nor that it is
synced (writes are `tokio::spawn` fire-and-forget under `synchronous=NORMAL`). The
landing proof is exclusively the C5 marker-keyed read.

## Key Test Scenarios

1. `POST /v1/A/observe` with `M_OBS_A` returns 204; `POST /v1/B/observe` with
   `M_OBS_B` returns 204 (AC-02).
2. One bearer token authorizes both writes; the path selects the tenant (FR-02.1,
   #4950) — no per-slug write credential.
3. Marker round-trips verbatim into `observations.topic_signal` (verified by the
   C5 positive read; R-09 sc.2).
4. A non-204 observe response is INFRA, not RED (transport/route fault ≠ isolation
   failure).
5. JSON body is node-built so a marker (already `[a-z0-9-]`) never breaks quoting.
