## ADR-002: Use simple session timeout instead of stdio health watchdog

### Context

When the MCP client disconnects (broken pipe, session restart), the server can linger as a zombie holding the database lock. The SCOPE.md proposed two approaches:

1. **Stdio health watchdog** — a background task that proactively detects broken stdout by writing to stderr or monitoring rmcp transport errors.
2. **Session timeout** — wrap the `running.waiting()` future with `tokio::time::timeout` to bound maximum session idle time.

### Decision

Use the simple session timeout approach. Wrap `running.waiting()` with `tokio::time::timeout(SESSION_IDLE_TIMEOUT, ...)` where `SESSION_IDLE_TIMEOUT` defaults to 30 minutes.

Rationale:
- The watchdog approach requires knowledge of rmcp internals (transport error detection, session health signals) that may not be exposed in rmcp 0.16's public API.
- The timeout approach requires zero rmcp knowledge — it simply bounds the outer future.
- Active sessions are not affected because rmcp's session future completes when the transport closes. The timeout only fires when the session has been silently dead for 30 minutes.
- The 30-minute default is conservative enough to avoid false positives in normal usage while short enough to prevent indefinite zombie linger.

### Consequences

- **Easier**: No rmcp internals to investigate. No background task to manage. Two lines of code change.
- **Easier**: No false positives from misinterpreting transport errors as broken pipes.
- **Harder**: Cannot detect broken pipes immediately — the server lingers for up to 30 minutes before releasing the database lock. Acceptable because the flock (Fix 4) allows a new instance to detect the existing one and wait/terminate.
- **Harder**: If a legitimate session is idle for 30+ minutes (e.g., developer leaves IDE open overnight), the server will shut down and the next tool call will fail. The client (Claude Code) will restart the server automatically.
