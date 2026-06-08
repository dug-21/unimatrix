# Agent Report: vnc-027-agent-2-spec

## Artifact
- `product/features/vnc-027/specification/SPECIFICATION.md` — 32 FRs, 7 NFRs, 12 ACs (AC-01..AC-10 from SCOPE + AC-11/AC-12 from risk recommendations), domain model, 5 workflows.

## Key Decisions
1. **Parity bar split (SR-06)**: transport/framing parity full (FR-21); event-set parity explicitly NOT a goal — parity corpus covers post-reduction event set only. AC-03 references FR-21/FR-22 to resolve the self-contradiction.
2. **Accepted divergence register (SR-04)**: lone-surrogate (#4788) formally excepted from AC-03; corpus must avoid lone-surrogate inputs in byte-compare cases.
3. **OQ2 default = server-side preformatted** (FR-17), additive-only wire change (FR-18, AC-11); format_injection JS port is the fallback (FR-19) with size accounting.
4. **Socket lifecycle as observable contract** (§7.7): drain-before-close for FNF, full-read + stdout-flush before exit for sync; exact Node mechanics deferred to architecture.
5. **Added AC-11** (frozen-contract fixtures byte-unchanged, SR-03) and **AC-12** (HTTP regression guard for the FR-16 rekey, SR-08).
6. SubagentStop opt-in = settings key, default-off (OQ3); F5 owns UX (SR-09).

## Open Questions for Architect
- OQ2 wire shape (which optional HookResponse field, listener dispatch).
- Node drain/exit sequencing pattern (SR-01/SR-05) compatible with NFR-1.
- OQ5 worktree-cwd stderr dump → FR-15 fixture set (SR-12).
- Queue retention bound on the no-daemon path (SR-13).
- Drop-detector shape + rollback thresholds (SR-07).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — #4798 (sync-formatting asymmetry → FR-17–19), #4788 (lone-surrogate divergence → FR-22), #4780 (size-gate rework lesson → FR-4), #4743 (shared-core parity → OQ2 default).
