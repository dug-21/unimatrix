# vnc-031 Implementation Brief — Cross-Matcher-Group Stale Uni-Hook Prune in `mergeSettings`

> Install-surface only. Root-cause fix for #728; source-level subsumption of the nan-016 `PRUNE_FRAGMENT` script workaround.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-031/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-031/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/vnc-031/specification/SPECIFICATION.md |
| Architecture | product/features/vnc-031/architecture/ARCHITECTURE.md |
| ADR-001 (keep-target by entry identity) | product/features/vnc-031/architecture/ADR-001-keep-target-by-entry-identity.md |
| ADR-002 (cross-group prune generalization) | product/features/vnc-031/architecture/ADR-002-cross-group-prune-generalization.md |
| ADR-003 (retire dogfood prune, parity) | product/features/vnc-031/architecture/ADR-003-retire-dogfood-prune-parity.md |
| Risk-Based Test Strategy | product/features/vnc-031/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-031/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/vnc-031/ACCEPTANCE-MAP.md |

---

## ⛔ DELIVERY GATES — READ BEFORE ANY CODE LANDS

Three gates govern this feature. None is a design blocker; all are **delivery-time** obligations. The first two run **before** the prune/retire commits; the third gates the AC-09 fragment-deletion commit specifically.

### GATE A (pre-delivery) — Confirm replacement source is on the delivery base branch (OQ-B / R-14)
The lossless-prune justification rests on crt-052 **#706** (transcript-fed cycle-review distillation) **and** vnc-027 ADR-004 **#4811** (matcher-narrowing) being present on the **actual base branch this delivers from** — not merely "on `main` as of writing." If either is absent on the base, the prune removes broad-`"*"` `PreToolUse` telemetry with **no live replacement source**.
- **Action:** verify both are reachable on the delivery base (`git log` / `git merge-base` check), record the confirmation, before the prune lands.
- **Severity if skipped:** High — telemetry pruned with no replacement.

### GATE B (pre-delivery) — nan-016 harness/runbook assertion update is in-scope here (OQ-5 / FR-16)
The nan-016 dogfood effect-harness + RUNBOOK that document the surviving stale `"*"` group (#4930) are updated **in this feature** (AC-09), not as a nan-016 follow-up. Per the spec finding, those harness assertions are already a **clean-state** assertion (nan-016 added the script-level prune). So this is an **assertion-keep / attribution-repoint + script-retire**, NOT a fresh harness rewrite. Do not let the scope balloon into a nan-016 test rewrite (OQ-C, R-13).
- **Action:** repoint the clean-state attribution from the script prune to `mergeSettings`-alone; **preserve the negative control** (it must still `assert.throws` on an unpruned post-state — reconstruct that no-prune state via a path that does NOT include Step 3c). Do not delete the control.

### GATE C (binding, blocks AC-09) — Prove parity on REAL legacy input BEFORE deleting the script prune (ADR-003 / SR-04 / R-04 / #4938)
**Do not delete `PRUNE_FRAGMENT` until the source prune is proven to subsume every case the script's whole-shell-token matcher handled — on REAL legacy-shaped input, never a pre-narrowed seed.** A green `merge-settings.test.js` does **not** satisfy this gate; the proof must run on a settings file carrying a genuine `"*"` Rust `PreToolUse` uni hook plus `.bak`/old-client-dir uni hooks.

Parity case table (P1–P8) — every row must be GREEN on real legacy input for the correct arm before the fragment-deletion commit:

| # | Legacy-shaped input | Required Step 3c outcome | Arm |
|---|---|---|---|
| P1 | Stale `"*"` `PreToolUse` Rust uni hook | Pruned; fresh entry under `PRETOOLUSE_CYCLE_MATCHER` kept | promote (object) |
| P2 | Stale node-client uni hook from a prior install | Pruned | rollback (string) |
| P3 | `.../index.js.bak` uni hook (different whole token) | Pruned (uni-owned, not kept object — unconditional) | both |
| P4 | Old-client-dir uni hook (`dogfood-client-OLD/...`) | Pruned | promote |
| P5 | Rollback `LD_LIBRARY_PATH=<dir>` genuine just-written command | **Kept** (it IS the keep-target by identity; no dirname heuristic) | rollback |
| P6 | Quoted spaced-path target (the #4931 tokenizer bug) | **Kept** by object identity; quoting irrelevant | both |
| P7 | Foreign hook alongside stale uni hooks | Preserved byte-for-byte | both |
| P8 | A matcher group emptied solely of uni hooks | Group dropped, event key retained | both |

**Ordering gate:** P1–P8 demonstrated GREEN → only then the commit that deletes `PRUNE_FRAGMENT`. Deletion must not precede the parity proof.

---

## Goal

Make `packages/unimatrix/lib/merge-settings.js#mergeSettings` prune stale Unimatrix-owned hook entries across **all** matcher groups — not only the managed `EVENT_MATCHERS[event]` group — for each **managed (registered-this-run) event**, so a legacy `"*"` → narrowed-matcher migration is clean for every consumer (`init`, `init --remote`, dogfood switchover). Then retire the bespoke prune in `scripts/dogfood-switchover.sh` so promote/rollback rely on shipped `mergeSettings` alone (one battle-tested ownership-aware path). Install-surface only — no Rust, server, transport, signature, or ownership-classification changes.

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| mergeSettings (Step 3c cross-group prune) | pseudocode/merge-settings-step3c.md | test-plan/merge-settings-step3c.md |
| dogfood-switchover.sh (prune retire) | pseudocode/dogfood-switchover-retire.md | test-plan/dogfood-switchover-retire.md |
| dogfood-effect harness (assertion repoint) | pseudocode/dogfood-effect-harness.md | test-plan/dogfood-effect-harness.md |

Note: pseudocode and test-plan files are produced in Session 2 Stage 3a. Component names above are derived from the architecture; actual file paths are filled during delivery.

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Keep-target identification | Object identity (`Object.is` / strict `===`) against the `newHookEntry` Step 3 placed/mutated — **never** a command-string compare. Captured into `keptEntryByEvent[event]`. Closes SR-01 by construction. | OQ-1 / SR-01 | architecture/ADR-001-keep-target-by-entry-identity.md |
| Prune rule | Prune **every** uni-owned entry outside the managed group **unconditionally** — even one whose command coincidentally equals the fresh command (different object). Enforces single-owned-entry-per-event. | OQ-2 | architecture/ADR-002-cross-group-prune-generalization.md |
| Event subset | **Managed (registered) events only** (new Step 3c). Step 3b opt-out covers `HOOK_EVENTS \ events`. Union = all, intersection = empty. | OQ-3 / SR-03 | architecture/ADR-002-cross-group-prune-generalization.md |
| Ownership reconciliation (behavioral change) | Any `isUnimatrixHook === true` entry under a non-managed matcher is reconcilable Unimatrix-owned state → pruned (survive → pruned). Human-approved; the one genuine behavioral change. | OQ-4 (human) | architecture/ADR-002-cross-group-prune-generalization.md |
| Script-retire & parity | Retire `PRUNE_FRAGMENT`; promote/rollback call `mergeSettings(..., {dryRun})` owning the write. Gated on P1–P8 parity proof on REAL legacy input (GATE C). | OQ-5 / Goal 4 / SR-04 | architecture/ADR-003-retire-dogfood-prune-parity.md |
| Signature | Unchanged. No `targetToken`, no prune-hint, no new parameter. Both call arms back-compat. | OQ-1 / FR-08 | architecture/ADR-001 (consequence) |

## Files to Create/Modify

| File | Change |
|------|--------|
| packages/unimatrix/lib/merge-settings.js | Add **Step 3c** (after Step 3, before Step 3b): capture `keptEntryByEvent[event]` in Step 3; cross-group prune all uni entries except the kept object for each registered event; drop emptied groups; retain event key; emit cross-matcher action. |
| packages/unimatrix/test/merge-settings.test.js | Extend existing fixtures/helpers with cross-group regression cases (AC-01..AC-08). No isolated scaffolding. |
| scripts/dogfood-switchover.sh | Retire `PRUNE_FRAGMENT` / `pruneStaleUniHooks` / `commandReferencesTarget` / `shellTokens` / `emitAndWrite`; promote/rollback call `mergeSettings(..., {dryRun})` owning the write. Keep `--dry-run`, exit codes, completeness checks, env-only param passing. **GATE C blocks this deletion.** |
| packages/unimatrix/test/dogfood-effect.test.js | Repoint clean-state attribution from script-prune to `mergeSettings`-alone; **preserve negative control** (GATE B). Update RUNBOOK/test-plan assertion(s) that document the surviving `"*"` delta. |

**No `lib/init.js` logic change** — both consumers inherit Step 3c identically through `mergeSettings`.

## Data Structures

```
keptEntryByEvent: Record<event, newHookEntry>   // NEW, function-local to mergeSettings
                                                // object-reference map; the kept entry per managed event
newHookEntry:     { type: "command", command: string }   // the kept object, captured by reference (Step 3)
matcher group:    { matcher: string, hooks: HookEntry[] } // one entry in content.hooks[event]
```

## Function Signatures (all unchanged — additive only)

```
mergeSettings(filePath, commandSource, options) -> { actions: string[], content: object }
  // signature UNCHANGED (OQ-1); commandSource accepts string (init) or {events, commandForEvent} (initRemote)
isUnimatrixHook(entry) -> boolean              // UNCHANGED (Non-Goal 2) — sole ownership signal
pruneUnimatrixEvent(content, event, actions) -> void  // UNCHANGED (Step 3b); cleanup shape reused inline in 3c
EVENT_MATCHERS[event] -> string                // UNCHANGED; PreToolUse -> PRETOOLUSE_CYCLE_MATCHER
```

Step 3c shape (from ARCHITECTURE; keep-test is **identity**, not command compare):

```
// per registered event, after Step 3 has placed newHookEntry and set keptEntryByEvent[event]
group.hooks = group.hooks.filter(hook => !(isUnimatrixHook(hook) && hook !== kept));
// then drop emptied groups; event key stays (managed group holds kept)
content.hooks[event] = eventArray.filter(g => g && Array.isArray(g.hooks) && g.hooks.length > 0);
```

### Action-String Contract

| Condition | Action string |
|---|---|
| Cross-group stale removal (Step 3c) | `Removed stale unimatrix hook: <event> (cross-matcher migration)` — **NEW**, substring-disjoint from below |
| Opt-out removal (Step 3b) | `Removed unimatrix hook: <event> (opt-out)` — unchanged |
| Managed repoint (Step 3) | `Updated hook: <event>` / `Added hook: <event>` — unchanged |
| Dedup within managed group (Step 3) | `Removed duplicate unimatrix hook for <event>` — unchanged |

Under `{dryRun:true}` every action is `[dry-run] `-prefixed by the existing Step 4 map. One action per group that lost ≥1 uni entry.

## Constraints

- **Surface containment (NFR-01):** only the four file areas above. No `lib/init.js` logic change. No other files.
- **Ownership identification fixed:** reuse `isUnimatrixHook` / `UNIMATRIX_PATTERNS` unchanged (Non-Goal 2). Prune correctness is fully bounded by `isUnimatrixHook` precision — a uni-**looking**-but-unclassified near-miss MUST survive (SR-02, R-07).
- **No new foreign-hook pruning, ever** — strictly scoped by `isUnimatrixHook`, identical to `pruneUnimatrixEvent` (Non-Goal 3).
- **Back-compat call shapes:** both `mergeSettings(fp, binaryString, {dryRun})` and `mergeSettings(fp, {events, commandForEvent}, {dryRun})` keep working with no signature break (FR-08, OQ-1).
- **vnc-027 adjacency:** `EVENT_MATCHERS` / `PRETOOLUSE_CYCLE_MATCHER` untouched; the retained cycle frame is the keep-target (never pruned); SubagentStop opt-in/opt-out byte-unchanged (SR-06, R-10).
- **Event partition is load-bearing:** Step 3c = `events`; Step 3b = `HOOK_EVENTS \ events`; union all, intersection empty (SR-03).
- **Fail-loud non-zero invariant:** a managed event must NEVER end with zero uni hooks. AC-02 asserts `count !== 0` with a self-identifying message (SR-01, FR-03).
- **Idempotency:** `deepStrictEqual` across runs, including stale-`"*"`-on-first-run (FR-12, SR-07).
- **Minimal change / reuse:** reuse the existing `pruneUnimatrixEvent` emptied-group/empty-event cleanup shape; do not introduce parallel machinery (NFR-03). `merge-settings.js` is outside the C-04 `lib/hook-client` size gate.
- **Output format stable:** 2-space-indented JSON + trailing newline unchanged (NFR-05).
- **Test discipline:** extend existing fixtures/helpers (`tempSettingsPath`, `writeSettings`, `writeOptIn`, `seedWith*`, `expectedLocalCommand`); test infrastructure is cumulative (NFR-06).
- **No daemon / no network / no Rust** — pure in-memory transform plus the existing single write (NFR-04).

## Dependencies

- **Replacement observation source — SHIPPED:** crt-052 #706 (transcript-fed cycle-review distillation) + vnc-027 ADR-004 #4811 (matcher-narrowing), merged on `main`. **GATE A** verifies both are on the delivery base branch before the prune lands.
- **Depended-on, NOT modified (server-side Rust, out of scope):** `context_cycle_review` and its observation/transcript consumption (`crates/unimatrix-server`, `crates/unimatrix-observe`) — Non-Goal 5.
- **Existing primitives reused (all in `lib/merge-settings.js`):** `isUnimatrixHook`, `UNIMATRIX_PATTERNS`, `EVENT_MATCHERS`, `PRETOOLUSE_CYCLE_MATCHER`, `normalizeCommandSource`, `buildHookClientCommand`, `pruneUnimatrixEvent` cleanup shape.
- **Consumers exercised:** `lib/init.js#init` (string arm) + `#initRemote` (object arm); `scripts/dogfood-switchover.sh`; `packages/unimatrix/test/dogfood-effect.test.js` and the nan-016 RUNBOOK / test-plan docs.
- **Test harness:** `node --test` (built-in `node:test`), `assert`; existing fixtures in `merge-settings.test.js`.

## NOT in Scope

- Changing `EVENT_MATCHERS`, the matcher-narrowing decision, or the SubagentStop opt-in matrix (vnc-027 ADR-004).
- Changing `isUnimatrixHook` or `UNIMATRIX_PATTERNS`.
- Pruning foreign (non-uni) hooks under any circumstance.
- Changing the opt-out prune (`pruneUnimatrixEvent`) behavior, scope, or action strings.
- Any `mergeSettings` signature change, new parameter, or `targetToken` hint.
- Re-litigating which events register (`init`/`initRemote` pass full `HOOK_EVENTS`; SubagentStop default-off filtering unchanged).
- Any client-side / Rust-binary / server / transport / daemon / network change.
- Modifying the replacement observation/analysis path (`context_cycle_review`, distillation, reconstruction).
- Any `lib/init.js` logic change beyond inheriting `mergeSettings`' new internal behavior.
- A nan-016 harness test rewrite (the change is an assertion-attribution repoint, GATE B / OQ-C).

## Alignment Status

**All six vision checks PASS — no variances requiring approval** (ALIGNMENT-REPORT.md, reviewed 2026-06-10):

| Check | Status |
|-------|--------|
| Vision Alignment | PASS — advances `personal-cloud` (#4934); one battle-tested ownership-aware path |
| Milestone Fit | PASS — root-cause completion beside the active OSS-cloud track; adds no future-milestone capability |
| Scope Gaps | PASS — all four Goals + AC-01..AC-10 covered |
| Scope Additions | PASS — no capability beyond SCOPE |
| Architecture Consistency | PASS — ADR-001/002/003 trace to SR-01..SR-07 and the Open Questions |
| Risk Completeness | PASS — R-01..R-15 map every SR + the OQ-B dependency |

Two SCOPE-originated items carried forward for delivery sign-off (NOT new variances):
1. **OQ-4 behavioral change (survive → pruned)** — already human-approved; consistent with the project-wide ownership model. Noted for traceability.
2. **OQ-B / R-14 delivery-base dependency** — a delivery-time verification (GATE A above), correctly deferred — not a design variance.
