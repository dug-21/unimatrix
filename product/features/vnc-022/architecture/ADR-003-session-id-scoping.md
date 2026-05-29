## ADR-003: Session ID Scoped by Transport Prefix

### Context

SR-03 (HIGH/Low): Client-generated `session_id` values are trusted after format validation (`sanitize_session_id`: max 128 chars, alphanumeric + `-_`). In a single-user personal cloud deployment (Day 1), collision risk is negligible — one user, UUID-based session IDs.

However, the architecture must not create a security hazard that becomes exploitable when multi-user deployments arrive (W2-3 OAuth). If two users share the same Unimatrix instance (enterprise), their client-generated session IDs could collide, allowing one user's events to modify another's session state in `SessionRegistry`.

Additionally, UDS and HTTP sessions should never collide — they are different transport domains with different trust models.

Options:
- **(A) No scoping**: Trust client session IDs globally. Works for single-user. Silent collision risk in multi-user.
- **(B) Per-token scoping via hash prefix**: Prefix session_id with a transport+identity marker before it enters `dispatch_request`. Transparent to the pipeline.
- **(C) Server-assigned session IDs**: Server generates session IDs, returns them to clients. Breaks the existing client contract (Claude Code generates its own session IDs).

### Decision

Option (B) with Day 1 simplification: The `/observe` handler prefixes client-supplied session IDs with `http:` before passing to `dispatch_request`.

Day 1 (single-user, static token):
```
Client sends: session_id = "abc-123-def"
Handler passes: session_id = "http:abc-123-def"
```

UDS path (unchanged): session_id passes through unmodified (no prefix).

This ensures:
- HTTP session `"abc-123-def"` and UDS session `"abc-123-def"` map to different `SessionRegistry` entries
- All HTTP sessions share the `http:` prefix (single user, so no per-token differentiation needed yet)

W2-3 evolution (multi-user, OAuth): When `ResolvedIdentity.agent_id` carries the OAuth subject, the prefix becomes `http:{subject_hash}:` — providing per-user isolation without changing `dispatch_request` or `SessionRegistry`.

The prefix is applied in the `/observe` handler (C3), before calling `dispatch_request`. The `sanitize_session_id` check runs on the full prefixed value. The prefix characters (`h`, `t`, `p`, `:`) are all within the allowed character set (alphanumeric + `-_` — note: `:` is not in the allowed set, so the prefix must use only allowed characters). Revised: use `http-` as prefix instead of `http:` since the colon is not in the allowed character set.

Final format: `http-{client_session_id}`
W2-3 format: `http-{subject_hash_8chars}-{client_session_id}`

### Consequences

- Zero changes to `dispatch_request`, `SessionRegistry`, or `sanitize_session_id` — they see a string that passes validation.
- UDS and HTTP session namespaces are disjoint. A local hook and a remote hook for the same logical session are tracked separately. This is correct — they are different transport paths with different injection histories.
- Day 1 cost is minimal: one `format!("http-{session_id}")` in the handler.
- The 128-char limit on session_id accommodates the prefix: `http-` is 5 chars, leaving 123 for the client ID. Claude Code session IDs are UUIDs (36 chars). Ample room.
- Multi-user evolution requires only changing the prefix construction, not the pipeline.
