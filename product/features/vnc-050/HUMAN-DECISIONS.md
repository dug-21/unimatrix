# vnc-050 Human Decisions (design review, relayed verbatim)

1. HTTP agent_id spoofing: DO NOT close in vnc-050. Accepted known gap; closed when OAuth is fully supported. Revise ADR-003 to remove the capability-fix scope (correct Unimatrix #5807), document as accepted gap, drop the R-02 "first delivery step" where it existed only to enforce this.

2. V-1: Reject. Read-only tokens cannot send observe events (ADR-004 stands). Client handling: accept the 403, emit one warning, never fail the job. Rationale: observe routes into the full hook dispatcher and writes beyond audit/telemetry (extraction tick creates knowledge entries from observations; helpful_count/signal_queue at session close; co_access/injection_log/query_log; cycle_events/cycle_tags; sessions.feature_cycle) — any other option lets a read-only credential poison responses.

3. V-2:
   a. Revoke: SPEC wins. Mark token "revoked" (tombstone, hash only), keep it; never allow label reuse. Rationale: AC-11 audit records token label per call; deletion orphans attribution and allows label reuse inheriting history.
   b. Bundle + env vars: reject bundle combined with URL, token, or fingerprint env vars (fail loudly). ALLOW bundle + UNIMATRIX_REMOTE_HEADERS (Cloudflare Access case). Write this explicit list into the spec.
   c. Legacy all-slugs token disable: config setting with env override, following the http.enabled / UNIMATRIX_HTTP_ENABLED pattern — single setting, supports PL-2 versioned config contract.

4. NEW: backward compatibility with existing clients is in scope. Add FR(s)/ACs and risk-test-strategy coverage with tests validating permutations of new server x existing client (existing bundles with pinned fingerprint, legacy token, existing env-var observe path, etc.).

Smaller questions (multi-slug defer, audit volume, --list --json): use reviewer/ADR recommendations unless blocking.

## Decision 5 (supersedes decision 1's descope effect) — Token mode is a capability CEILING

Do NOT accept the descope. Effective capabilities = intersection of (registry grants for declared agent_id) ∩ (token mode's allowed set).
- agent_id remains self-asserted attribution; the spoofing gap stays open/accepted (a read-write token can still claim "human"; closing that is future agent-identity work). Bearer-identity path in build_context_with_external_identity (server.rs:541) stays dormant.
- A read-only token can never write regardless of claimed agent_id. Intersection only narrows, never widens → safe now; matches PL-4 (fails closed; OSS default never widens).
- Reuse existing machinery: token capability set already in ResolvedIdentity from bearer auth → carry into ToolContext; require_cap (server.rs:631) checks both sets. One check point, no parallel system (#4453).
- The ceiling MUST also gate usage/co-access/learning recording for read-token calls, not just write tools.
Restores FR-14/AC-08, FR-15/AC-15, write-rejected half of FR-32/AC-13, #1010 done-when "write rejected", "site never holds a write-capable token".
Add: (1) ADR "Token mode is a capability ceiling, intersected with the registry. agent_id stays self-asserted. The intersection only narrows." (2) Risk entry stating residual gap: within a read-write token an agent can still claim any agent_id; accepted until agent-identity work lands.
OQ-7 (old-client source for compat tests): risk strategist picks.

## Decision 6 — Default attribution
Scoped-token tool call omitting agent_id: keep today's default attribution. Never enroll `token:` ids in the agent registry.

## Decision 7 (final; design does not reopen — small spec/brief edits only)

1. Omitted agent_id: keep today's default (anonymous); never enroll token: ids. Rationale: HD5 keeps attribution self-asserted; falling back to token:<label> would be identity replacement (#5807 rejected); http_access row already records token:<label>; avoids second identity namespace; fails closed.
   - FR-16: replace "distinct default agent_id derived from label" with: token:<label> is the auth-layer audit identity only; tool-call default agent_id unchanged.
   - New AC: read-write token + omitted agent_id -> attributed anonymous, registry grants unchanged, token: never appears in registry.
   - Check resolve_or_enroll: if it auto-enrolls anonymous with Write (PL-4 fail-open today), a RW token with no agent_id gets anonymous writes. Not a new hole — name it in the risk register.
2. Empty env vars: in SPEC, not ADR-006 (no ADR bullet). Every UNIMATRIX_* variable: empty value = unset (PL-2 versioned config contract; precedent config.js nonEmpty and ADR-001 empty UNIMATRIX_LEGACY_TOKEN_ENABLED = no override). No trimming.
   - New FR + two ACs: (a) empty env var alongside bundle -> no conflict; (b) empty half of URL/TOKEN pair -> pair is unset, not incomplete.
   - Fix IMPLEMENTATION-BRIEF line 76 to cite the new FR instead of ADR-006. ACCEPTANCE-MAP updated for new ACs.

## Decision 8 — Empty half of URL/TOKEN pair: fail loudly
Empty half = unset -> URL with empty token (or token with empty URL) is a partial pair -> error. Only when both halves are empty/unset is the pair unset and the credential store used. Rationale: setting URL states intent; silent fallback could connect to a different server/slug (breaks one-client-one-project; wrong-project writes irreversible); missing CI secrets typically arrive as empty strings — fail immediately with the cause.
Error message must name the empty variable, e.g. "UNIMATRIX_REMOTE_TOKEN is empty; UNIMATRIX_REMOTE_URL is set", and must never print the token or any credential.
Approved edits: fix AC-29 + R-18 scenario 4; brief line 76 -> FR-40; AC-27..29 into acceptance map.

## Decision 9 — Live AC-13 / AC-14 are post-delivery manual checks
AC-13 live GH Actions run and AC-14 Tailscale findings are POST-DELIVERY operator checks, not part of vnc-050 delivery and never a delivery gate. vnc-050 contains no GH Actions workflow/infrastructure and no Tailscale work; only the scripted local CI equivalent of AC-13 is in delivery scope. C19 (#5792) remains unproven until the operator runs both.
Update: SCOPE, SPEC (FR-32/FR-33), ACCEPTANCE-MAP (mark live AC-13 + AC-14 post-delivery), IMPLEMENTATION-BRIEF, #1010 body.

## Decision 10 — Observe-denied marker replaces ADR-004 per-process latch
APPROVED. Hook runs one process per event, so a per-process latch floods (R-17). Relaxing AC-18 rejected. Up-front skip rejected: ADR-005 keeps token mode out of the bundle (server-side authority), so the client must learn from the 403.
1. Latch only on 403 with body `token lacks SessionWrite`; other 403, 401, network failure unchanged, no marker.
2. Never store the secret: marker keyed on SHA-256 of credential; mode 0600 in client stateDir.
3. No expiry: mode immutable, labels not reused, revoked → 401; a new credential gets a fresh attempt.
4. Fail open toward retry: missing/corrupt/unreadable marker = not refused; marker write failure never changes exit code or output.
5. AC-18 tests: N events with read token → one warning, one 403; credential change resets; corrupt marker → one retry and rewrite; read-write credential never creates a marker.
Updated: ADR-004, ARCHITECTURE, SPEC FR-24a/AC-18, RTS R-17, ACCEPTANCE-MAP, IMPLEMENTATION-BRIEF, pseudocode C6c.

## Decision 11 — Global TLS override ignored on CA path
APPROVED. Unfingerprinted CA path passes `rejectUnauthorized: true` explicitly per connection, so `NODE_TLS_REJECT_UNAUTHORIZED=0` is ignored. Pinned behavior byte-identical. Rationale: the var lingers in CI/shells after proxy setup; honoring it silently disables verification, leaks the bearer token, and creates an undesigned third trust mode; it would break SR-01's guard. Supersedes "no `rejectUnauthorized` key on CA path". Test: var=0 + untrusted cert on CA path → refused, zero app bytes.
Updated: ADR-006, ARCHITECTURE, SPEC FR-25, RTS R-01/SR-01, SCOPE-RISK-ASSESSMENT SR-01, IMPLEMENTATION-BRIEF, pseudocode/test-plan C10.

## Decision 12 — Hook-client size cap raised to measured baseline
APPROVED. Hook-client size cap set to the measured post-feature line counts; that becomes the new PL-10 no-grow baseline.
