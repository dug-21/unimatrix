## ADR-003: Retire the Dogfood Script Prune — Parity Argument and Single Behavior for Both Consumers

### Context

nan-016 shipped a script-level workaround in `scripts/dogfood-switchover.sh`
(`PRUNE_FRAGMENT` / `pruneStaleUniHooks` / `commandReferencesTarget`) because
`lib/merge-settings.js` was frozen (C-8). The root-cause fix (ADR-002) makes
that workaround redundant. Goal 4 retires it so promote/rollback rely on shipped
`mergeSettings` alone (nan-016 ADR-003 #4926: one battle-tested code path).

SR-04 (High/Med) is the gate: if the source prune does not subsume **every** case
the script's whole-shell-token matcher handled, retiring the script regresses
promote/rollback even though `mergeSettings` tests pass. The #4938 discipline:
enumerate what the primitive does and does not manage; prove parity on **real
legacy-shaped** input, never a pre-narrowed seed. SR-05 additionally requires one
behavior for both `mergeSettings` call arms — no per-consumer divergence.

### Decision

**Parity argument — each script case maps to a source behavior:**

| Script case (`pruneStaleUniHooks`) | Source (Step 3c) behavior | Subsumed? |
|---|---|---|
| Stale `"*"` `PreToolUse` uni hook (target token absent) | Different object than `keptEntryByEvent`; pruned unconditionally | Yes |
| `.../index.js.bak` uni hook (different whole token) | Not the kept object; pruned | Yes |
| Old-client-dir uni hook (`dogfood-client-OLD/...`) | Not the kept object; pruned | Yes |
| Rollback `LD_LIBRARY_PATH=<dir>` dirname-level match (keep) | The kept object IS the just-written legacy entry (string arm, ADR-001); kept by identity — no dirname heuristic needed | Yes |
| Quoted spaced-path target kept (the #4931 bug) | Kept by object identity; quoting is irrelevant — no tokenizer | Yes (strictly safer) |
| Foreign hook preserved | `isUnimatrixHook` scope unchanged; foreign never a candidate | Yes |
| Emptied group / event-key cleanup | Reused `pruneUnimatrixEvent` cleanup shape | Yes |

The script's `commandReferencesTarget` existed **only** because the script ran
*after* `mergeSettings` in a separate process and had to reconstruct the keep
target from a `targetToken`, with a quote-aware tokenizer (#4931) and a rollback
dirname special-case. Step 3c holds the kept object reference (ADR-001), so every
one of those heuristics collapses to a single identity test that is strictly more
correct: the script's whole-token matcher could be fooled by command-shape edge
cases; object identity cannot. The source prune therefore **subsumes** the script
prune on the union of its cases, including the rollback dirname match and the
`.bak`/old-dir tokens.

**Both call arms, one behavior (SR-05):** Step 3c runs inside `mergeSettings`,
after `normalizeCommandSource`. The string arm (`init` local / `rollback`) and
the object arm (`initRemote` / `promote`) both flow through the same Step 3
(producing `keptEntryByEvent`) and the same Step 3c. There is no per-consumer
branch. AC-06 exercises both arms on real legacy-shaped input.

**Retire mechanics (AC-09):** `dogfood-switchover.sh` `run_promote`/`run_rollback`
collapse to a plain `mergeSettings(..., { dryRun })` that owns its own write —
matching `initRemote`'s call shape (nan-016 ADR-003 #4926). Delete
`PRUNE_FRAGMENT`, `pruneStaleUniHooks`, `commandReferencesTarget`, `shellTokens`,
`emitAndWrite`, and the `{ dryRun: true }`-then-bespoke-write pattern. The
script's own `--dry-run`, exit codes, and completeness checks stay.

**Parity proof obligation (binding, SR-04 / #4938):** retire only after the
source prune is proven on REAL legacy-shaped input — a settings file carrying a
genuine `"*"` Rust `PreToolUse` uni hook plus `.bak`/old-dir uni hooks — not a
pre-narrowed seed. The nan-016 effect-harness/runbook assertion that "one stale
`"*"` group survives" (#4930, #4932) inverts to assert a **clean** migration. Do
not delete the script prune until this passes (Design Recommendation 2).

### Consequences

Easier: one ownership-aware path; promote/rollback inherit the clean migration
for free; no quote-aware tokenizer to maintain; the #4931 spaced-path failure
mode is gone by construction. Plain `init` on an affected project now migrates
cleanly with no per-consumer reimplementation.

Harder: the parity proof must run on real legacy input before AC-09 deletion —
a hard gate, not a formality (#4938). The harness/runbook assertion inversion is
in scope here (AC-09); OQ-5 confirms it is not a nan-016 follow-up.

Cross-references: ADR-001, ADR-002; nan-016 ADR-003 (#4926); #4931, #4932,
#4936, #4938; SR-04, SR-05; OQ-5.
