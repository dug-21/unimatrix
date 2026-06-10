## ADR-004: Switchover Does No Daemon Lifecycle Management; Relies on Client Fail-Open

### Context
The TS hook client connects over UDS to a local daemon at `~/.unimatrix/{hash}/
unimatrix.sock` (derived from the project-root hash, #4923). C-7 mandates fail-open hook
posture: hooks exit 0 with empty stdout even when the daemon is absent. SR-08 warns that a
switchover that assumes/manages the daemon, or emits a command that can exit non-zero, could
break the host session. An operator might also wrongly assume the switchover starts the
daemon.

### Decision
The switchover (and install) scripts perform **no** daemon start/stop/probe/lifecycle
management (OQ-1d resolved). They only rewrite settings and freeze files. Correctness of the
fail-open posture is inherited from the unmodified client (`lib/hook-client/index.js`
guarantees exit-0/empty-stdout on every path, including config-miss and connect failure) and
is **asserted by effect**: the harness re-fires the emitted hook command against a scratch
project root whose hash has no live socket (daemon-absent) and asserts exit 0 / empty stdout.
The runbook states explicitly that the operator is responsible for the daemon and that hooks
fail-open if it is down.

### Consequences
Easier: the switchover is safe to run regardless of daemon state; no race between settings
flip and daemon readiness; smaller, auditable scripts.
Harder: the runbook must be explicit that a *silent* fail-open during the soak (daemon down)
looks like success at the hook layer — F6's soak observability (not nan-016) must catch a
down daemon. nan-016 only guarantees the hook never breaks the session.

Related: ADR-003 (emits the command), ADR-005 (re-fired-hook daemon-absent assertion).
Honors C-7. Cites #4923.
