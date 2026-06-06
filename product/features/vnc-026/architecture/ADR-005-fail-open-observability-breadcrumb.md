## ADR-005: Fail-Open Stays, with a Content-Free Local Health Breadcrumb; init Ping Is the Only Loud Checkpoint

### Context

SR-10 (Medium/High): the exit-0/fail-open mandate (scope constraint, FR-03.7
inheritance) makes remote misconfiguration invisible — an expired token or wrong URL
means every remote session silently loses all learning, indefinitely. Evidence #4473:
warn+continue posture masks failure paths. The options span full silence (Rust hook
today: stderr only, usually unseen) to actively failing the hook (forbidden — the host
CLI must never see a failing hook). The trade-off must be explicit, not accidental.

### Decision

Three-part posture, all within the exit-0 contract:

1. **stderr one-liner on every failure** — `unimatrix: <class>: <message>` — matching the
   Rust hook's eprintln posture. Visible in Claude Code hook debug output for users who
   look; never blocks.

2. **Local health breadcrumb** — `~/.unimatrix/{hash}/hook-client/health.json`, written
   best-effort (atomic rename, failures swallowed) on every spawn that attempts a send:

   ```json
   {
     "last_success": 1765432100,
     "last_failure": 1765432190,
     "failure_class": "auth | connect | timeout | http_4xx | http_5xx",
     "consecutive_failures": 17,
     "queue_depth": 42,
     "url_host": "unimatrix.example.com"
   }
   ```

   Content-free: no token, no payloads, no transcript bytes, no full URL (host only).
   401/403 is classified `auth` specifically — the expired-token case SR-10 names — so
   the worst silent failure is the most precisely recorded one. Queue depth doubles as
   the growth observable (SR-05/SR-10 recommendation).

3. **`init --remote` Ping is the only loud checkpoint in F3.** Init builds a `Ping`
   HookRequest, POSTs it with the supplied token, and **fails init** with an actionable
   message on anything but Pong — wrong URL and bad token are caught at the moment a
   human is watching. After init, no hook spawn is ever loud.

Explicitly rejected for F3:
- **Failing or delaying hooks on misconfiguration** — violates the host-CLI contract.
- **Push/notification channels** — no daemon exists client-side; out of scope.
- **A `doctor`/status CLI surface** — deferred to F5 (#681), which owns init/UX
  unification; health.json is designed as its data source so F5 needs no new plumbing.

Residual risk, accepted and documented: a token that expires *after* init produces no
user-visible signal until someone inspects health.json, stderr, or notices missing
injections. The mitigation path (F5 surfacing, or an init re-run) is deliberate scope
sequencing, not an oversight.

HTTP timeouts are part of this posture: connect 750 ms, sync total 2,000 ms,
fire-and-forget total 3,000 ms (config-overridable via the settings.local.json block).
Sync expiry → no stdout, exit 0 — the host prompt proceeds without injection rather than
hanging on a WAN hiccup.

### Consequences

- Easier: misconfiguration is diagnosable in one file read; Layer 2 tests can assert
  breadcrumb transitions; F5 gets a ready-made health surface; queue growth has a
  numeric observable.
- Harder: one extra small write per failed spawn (bounded, atomic, best-effort);
  health.json is per-project state that the AC-13 perf measurement must include.
- The silence-after-init trade-off is now an auditable decision with a named owner (F5)
  rather than an emergent property.
