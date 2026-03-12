## ADR-001: Hard STOP Gates for the /unimatrix-seed State Machine

### Context

`/unimatrix-seed` is a multi-turn conversational skill that must pause for human approval between each exploration level. SR-01 identifies this as a High/High risk: the model has no enforcement mechanism to stop it from advancing levels autonomously. The uni-init prototype failure (67 entries auto-generated, all deprecated) was caused exactly by this pattern — fully automated extraction with no human validation.

Skills are markdown instructions Claude follows. There is no runtime guard that prevents the model from continuing a skill without waiting for human input. The only reliable control is how the SKILL.md phrases each level transition.

### Decision

Model `/unimatrix-seed` as an explicit state machine with hard STOP gates. Each gate in SKILL.md uses mandatory stop phrasing that the model must respect:

```
**STOP. Present the entries above to the human for approval.
Do not proceed until the human responds.**

Ask: "Would you like to go deeper? Options: [A] module structure, [B] key conventions, [C] build/test workflow — or [N] done."
```

Key design rules:
1. Every level transition is a discrete yes/no decision point — not a soft "you could continue"
2. Gate phrasing uses STOP in bold at the start of the gate instruction
3. The approval menu is exhaustive — human picks from a closed list or says "done"
4. Depth is bounded to Level 0 + 2 opt-in levels. Level 3 does not exist — the skill instruction explicitly says "no further levels are available after Level 2"
5. Level 0 uses batch approval (2-4 entries, low risk). Level 1+ uses per-entry approval (higher stakes, explicit individual sign-off)

The state machine is documented in ARCHITECTURE.md Component 5 with explicit state transitions.

### Consequences

- Model instruction-following fidelity is improved by the explicit STOP phrasing, but not guaranteed — this is an inherent platform constraint (SR-03). Manual testing scenarios must verify each gate holds.
- Per-entry approval at Level 1+ increases conversation length but is required for quality control. This is the core design differentiator from the failed uni-init prototype.
- The bounded depth (Level 0 + 2 opt-ins) gives a clear contract: invoking `/unimatrix-seed` is a known-length conversation, not an open-ended exploration.
- Future enhancement: Level 0 could be skipped if seed has already been run — the EXISTING_CHECK state handles this by offering supplement-or-skip.
