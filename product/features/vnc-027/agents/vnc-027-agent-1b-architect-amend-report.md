# Agent Report: vnc-027-agent-1b-architect-amend

Targeted ADR amendments resolving RISK-TEST-STRATEGY R-04 and R-12. No other design changes.

## Code Verification (performed independently)

- `TaskCompleted` absent from `merge-settings.js HOOK_EVENTS` (lines 29-39) and from `.claude/settings.json` — host never spawns the hook with it. R-04 claim confirmed.
- `build-request.js:59-66`: `Stop` and `TaskCompleted` share one case building a **SessionClose** frame — ADR-006's original RecordEvent claim was wrong. Confirmed.
- `crates/unimatrix-server/src/uds/listener.rs:2919`: `"SubagentStop" | _ => (None, None, None, None)` all-None fallthrough — no server lifecycle depends on SubagentStop. R-12 evidence confirmed.

## R-04 Decision: Option (a) — age-prune-only as the effective deletion mechanism

Rationale:
- AC-10's "TaskCompleted and/or age-prune" wording explicitly permits age-prune-only; uni-zero review pre-endorsed it as the honest fallback.
- Option (b) — registering TaskCompleted in HOOK_EVENTS — would add a hook event to serve an offset-cleanup optimization, directly contradicting this feature's hook-set-reduction goal (ADR-004), while buying nothing the 7-day prune does not provide.
- The actual FR-16 defect (per-turn re-stream) is fixed by removing the SessionClose delete alone; age-prune bounds residue.
- TaskCompleted branch retained only as a zero-cost forward provision (one equality check on the canonical event name — never frame type, since Stop is also a SessionClose frame), pinned by unit test plus the Stop-must-not-delete assertable negative. Documented as unreachable, not silent.
- Frame-type claim corrected in both Context and Decision.

## R-12 Resolution

ADR-004 decision point 2 now states explicitly: no server lifecycle (session close, buffer finalization) depends on SubagentStop, citing listener.rs:2919; the no-SubagentStop lifecycle test converts this to an asserted contract.

## Amended Files

- `product/features/vnc-027/architecture/ADR-006-offset-delete-rekey.md` (retitled; Context, Decision, Consequences amended)
- `product/features/vnc-027/architecture/ADR-004-hook-set-reduction.md` (decision point 2 amended)

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- returned #4807/#4806 (vnc-027 ADRs), #4809 (hook-client pattern: verify event registration before keying behavior to it — directly applicable, cross-referenced in amended ADR-006), prior vnc-0xx decisions; applied #4809.
- Stored: entry #4810 "ADR-006: FR-16 Offset Delete — SessionClose Delete Removed; Age-Prune Is the Effective Mechanism (TaskCompleted Branch Retained but Unregistered)" via context_correct superseding #4807; entry #4811 (ADR-004 amended, R-12 independence statement) via context_correct superseding #4805. Both category decision, topic vnc-027, tags [adr, vnc-027].
