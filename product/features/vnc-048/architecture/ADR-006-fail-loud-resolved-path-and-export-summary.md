## ADR-006: Close the silence — fail-loud naming the fully-resolved absolute path, plus an export stderr count summary

### Context
The bug cost the operator time because both directions were *silent*: export emitted an audit-rows-only file and reported success; import restored into a store nothing routes to and reported success. The `--skip-quarantined`/`audit_log` asymmetry means an audit-rows-only export is a *legitimate* output (Non-Goal — correctly staying as-is), which is exactly what made the empty export look successful (SR-08). Every new accept-but-inert path must be closed by **loud failure that names the resolved path**, not by a heuristic. The single most likely operator mistake in the host-bind-mount shape (SR-11/C-7) is a base miss that resolves the host `$HOME`'s `.unimatrix` instead of the container's — indistinguishable from a typo *unless* the message prints the resolved path.

### Decision
Two complementary mechanisms, no heuristic:

1. **Fail-loud naming the fully-resolved absolute path (SR-11/C-7, AC-03).** Every accept-but-inert path errors with the resolved absolute path and the next action, never a silent no-op:
   - Missing store at the resolved path (both commands) → names `{base}/<slug>/unimatrix.db` and "register the slug / check `--project-dir`".
   - Host bind-mount base miss → surfaces *as* the missing-store error naming the **host** path actually tried; the printed path is what distinguishes a base miss from a typo (identical posture to `project list/register` today; one line of help text, AC-07 — no new mechanism).
   - Reserved/charset-invalid slug (AC-04), live-PID (ADR-003), non-empty audit (ADR-005) each already name their resolved path + remedy.
   Where the base is absolute (all production shapes), the printed path is absolute; the `_with_base` test hook prints the base as given — the point is naming the exact path tried.
2. **Export stderr count summary (AC-06, export-only).** After a successful export, print one line to stderr: `exported N entries, M audit rows → <path>`. This is not a behavior change — it reports what already happened — and `exported 0 entries` is self-diagnosing for every future cause of a sparse export, including the correctly-retained `--skip-quarantined`/`audit_log` asymmetry. No summary is added to import (OQ-4): import already prints per-table counts via `print_summary`.

**Rejected:** a sibling-slug-dir scan that guesses the operator "meant" a nearby slug. It is a heuristic, reads as scope creep, and cannot distinguish a slug dir from a path-hash dir (#4972). Reporting what happened beats guessing what was meant.

### Consequences
Easier: every silent-success path from the original bug becomes a loud, self-diagnosing failure or a visible count; a base miss is diagnosable from the printed path alone. The operator sees "exported 0 entries" the instant a resolve is wrong, closing the class of failure that shipped this bug — without touching the audit filter (Non-Goal).

Harder: export gains stderr output (accepted — stderr, not stdout, so piping the JSONL is unaffected). The fail-loud messages must be maintained to always carry the resolved path; a future refactor that drops the path from an error message silently re-opens SR-11 — an AC-03 test asserting the resolved path appears in the error guards this.
