# Gate 3a Report: vnc-027

> Gate: 3a (Component Design Review)
> Date: 2026-06-08
> Result: PASS (3 WARNs)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS | 10 components in pseudocode/OVERVIEW match ARCHITECTURE Component Breakdown 1:1; interfaces traced to Integration Surface; all 7 ADRs cited and honored |
| 2. Specification coverage | PASS | FR-1..FR-32 each have pseudocode (FR-32 dogfood = documented post-merge); NFR-1..7 addressed; no scope additions (vnc-030/F5/F6 excluded) |
| 3. Risk coverage | PASS | R-01..R-18 all mapped to test scenarios; AC-01..AC-12 all covered; integration + edge + security risks present |
| 4. Interface consistency | PASS | Shared types (SendResult, ResolvedConfig.mode/socketPath, HookResponse::Text) consistent across OVERVIEW + components; accept set only at serialization; canonical-event keying coherent; no contradictions |
| 5. Knowledge stewardship | WARN | All Stage 3a producers + architect/risk-strategist comply; synthesizer (Session 1) report missing the block |

Three open questions adjudicated below: OQ1 CONSISTENT, OQ2 SURVIVES (wording clarification), OQ3 CORRECT.

## Detailed Findings

### Check 1 — Architecture alignment
**Status**: PASS
**Evidence**: pseudocode/OVERVIEW.md component table (rows 1–10) is a 1:1 match to ARCHITECTURE.md Component Breakdown. Interfaces match the architecture Integration Surface table: transport contract `post(config, frame, opts) -> Promise<SendResult>` (transport-uds.md), framing `4-byte BE u32 + JSON`, `MAX_PAYLOAD_SIZE=1_048_576` (wire-accept-text.md, transport-uds.md), socket path `~/.unimatrix/{projectHash}/unimatrix.sock` (config + ADR-007), SendResult mapping table reproduced verbatim from ADR-002 §2. All 7 ADRs are cited inline and their decisions reflected (e.g. ADR-001 shared injection core in listener-preformatted.md, ADR-003 flush-before-FIN in transport-uds.md). Authority order is explicitly stated (OVERVIEW: ADR > BRIEF > ARCHITECTURE > SPEC).

### Check 2 — Specification coverage
**Status**: PASS
**Evidence**: Every FR maps to a component: FR-1..4 → size-gate; FR-5..11 → transport-uds; FR-12..16 → config-transport-selection; FR-17..19 → wire-accept-text + listener-preformatted (FR-19 fallback explicitly rejected, preformatted chosen); FR-20..26 → parity-corpus-uds + transport-uds; FR-27..29 → build-request-sentinel + merge-settings-reduction; FR-30..31 → state-offset-rekey + index-dispatch; FR-32 → documented post-merge dogfood (correctly NOT coded). NFRs: latency (AC-05 perf job), size (gate), fail-open (every component error section), sync-path I/O (pruneOffsets FNF-only — index + state), platform/deps/compat all carried. No unrequested features in the pseudocode.

### Check 3 — Risk + AC coverage
**Status**: PASS
**Evidence**: OVERVIEW Risk→AC→Component table maps R-01..R-18; spot-verified each risk appears in at least one component test plan (R-01 transport-uds/parity FNF; R-02/R-03 size-gate; R-04 state; R-05 config/parity hash; R-06 transport-uds; R-07/R-08 wire/listener/parity frozen-binary; R-09 listener/parity sync; R-10 parity replay; R-11 sentinel/merge-settings; R-12 merge-settings/listener/parity lifecycle; R-13 config/transport/parity bounds; R-14 state/index; R-15 transport/parity latency; R-16/R-17 documented post-merge/unsupported; R-18 transport/parity caps). All 12 ACs have dedicated test-plan sections. Critical R-02 (merge ordering) is pinned both as design constraint and Gate-3c git-log audit. Integration risks, edge cases, security surfaces all carried into the test-plan OVERVIEW and component plans.

### Check 4 — Interface consistency
**Status**: PASS
**Evidence**: OVERVIEW shared types match per-component usage — SendResult (no new failureClass) = ADR-002 table = transport-uds mapHookResponse; ResolvedConfig gains `mode` + `socketPath` (config) consumed by index selectTransport; `HookResponse::Text { body }` defined once (wire-accept-text) and consumed identically by listener + transport. Critical invariants are coherent across files: (a) `accept` is injected ONLY at transport-uds serialization, never by builders, never stored in the queue — stated identically in OVERVIEW, wire-accept-text, transport-uds, ADR-001 §2; (b) FNF success → status 0 → transform writes no stdout (status!==200) — coherent index/transform/transport; (c) age-prune-only authority of ADR-006 over spec FR-30/AC-10 stated identically in OVERVIEW, state, index, brief, ACCEPTANCE-MAP; (d) Text-only-to-accept-callers coupling consistent in wire/listener/transport. No contradictions found.

### Check 5 — Knowledge stewardship
**Status**: WARN
**Evidence**: Active-storage agents comply — architect #4802–#4808 (+amend #4810/#4811 via context_correct), risk-strategist #4809, scope-risk #4800, researcher #4798. Read-only Stage 3a producers comply — pseudocode (Queried #4806/#4802/#4803/#4798/#2582/#300/#4743), testplan (Queried + "nothing novel to store -- {reason}"). spec/spec-amend/vision-guardian all have blocks.
**Issue (WARN)**: `agents/vnc-027-synthesizer-report.md` has NO `## Knowledge Stewardship` block. The synthesizer is a Session 1 compilation agent (produced IMPLEMENTATION-BRIEF + ACCEPTANCE-MAP), outside the strict Stage 3a design-phase set Gate 3a enforces; all design-phase (pseudocode, test-plan) and source-doc (architect, risk-strategist) agents comply. Recorded as WARN, not REWORKABLE FAIL. Minor: the pseudocode read-only report satisfies the Queried requirement but uses "Deviations: none" in place of an explicit "Stored / nothing novel" line — acceptable for a read-only agent.

## Adjudication of the Three Open Questions

### OQ1 — merge-settings opt-OUT pruning of stale entries
**Verdict: CONSISTENT / APPROVED.**
The "additive-only" constraint is scoped to the **frozen F1 WIRE contract** (spec §7.1, FR-18, AC-11, NFR-7): wire.rs struct fields, `HookResponse` variants, ts-rs bindings — i.e. the serialized protocol. `merge-settings.js` writes `.claude/settings.json`, a generated install artifact, not wire frames; it is categorically outside that constraint. AC-08 requires an "opt-in key on/off/non-boolean matrix" — for "off" to be observable after a prior "on", a previously-registered Unimatrix SubagentStop entry must be removable. The pseudocode (merge-settings-reduction.md §"Removal on opt-out") correctly scopes pruning to **Unimatrix-owned entries only** via the existing `isUnimatrixHook` ownership logic, leaving PreToolUse (still registered, matcher narrowed) and user/third-party hooks untouched. This is install-surface idempotency already implied by ADR-004 §5's dedup/ownership model. Test coverage exists (merge-settings test plan `test_subagentstop_key_false_absent`; pseudocode scenario 3 "Opt-out removal"). Non-blocking recommendation: the architect may add one sentence to ADR-004 §2 explicitly confirming opt-out strips Unimatrix-owned SubagentStop entries, so the bidirectional behavior is sourced in the ADR rather than emergent in pseudocode.

### OQ2 — `accept` appended at struct end → additive ts-rs diff vs AC-11 "byte-unchanged"
**Verdict: SURVIVES, with a wording clarification (WARN).**
AC-11 bundles two distinct guarantees that must be read separately:
1. **Wire serialization bytes** of every existing frame: fully byte-unchanged. `accept` is added at struct end with `#[serde(default, skip_serializing_if = "Option::is_none")]` and `accept: None` at hook.rs construction sites, so a `None`/absent field is wire-identical to the pre-feature frame. `scripts/regen-parity.sh` on the JSON parity goldens yields a true zero diff. This is the real frozen-contract proof and it holds.
2. **ts-rs bindings**: these DO change — additively (new optional field + new `Text` variant). The literal AC-11/ACCEPTANCE-MAP phrasing ("ts-rs bindings pass byte-unchanged" / "run UNMODIFIED") is imprecise. The architecture (line 33: "ts-rs bindings regenerate additively"), ADR-001 consequences, and the wire-accept-text pseudocode + test plan all interpret it correctly: **existing type shapes are byte-unchanged; new members are added; the binding regenerated, committed, and the drift CHECK passes** (the check SCRIPT is unmodified, the bindings FILE is additively updated). The implementing artifacts handle this correctly, so AC-11 is satisfiable.
**WARN / recommendation**: the tester's AC-11 assertion must permit the additive binding diff (commit regenerated bindings; assert no existing shape changed) rather than asserting a literal zero-diff on the bindings file. Suggest the spec-writer soften AC-11/ACCEPTANCE-MAP wording from "ts-rs bindings pass byte-unchanged" to "ts-rs bindings change additively only (existing shapes unchanged); drift check green after regen." Documentation clarity only — does not block design.

### OQ3 — FR-16 deletion keyed on `canonical` (exactly "TaskCompleted"), never effectiveEvent/request.type
**Verdict: CORRECT.**
index-dispatch.md keys the delete branch on `canonicalEvent === "TaskCompleted"` and explicitly states: "Use the canonical value, NOT `effectiveEvent` (which falls back to rawEvent on UNKNOWN) and NOT `request.type`." It carries an inline note that a `Stop` spawn is also a SessionClose frame but its `canonical` is "Stop", so the delete does not fire — exactly ADR-006 §3 ("keyed by canonical event name, never frame type; Stop and TaskCompleted are both SessionClose frames"). `canonical` is sourced from `normalize.normalizeEventName(rawEvent)` and passed explicitly into `runFireAndForget`. Test coverage pins both the positive (state `test_taskcompleted_deletes_offset`) and the assertable negative (state `test_stop_must_not_delete_offset`; index `test_sessionclose_delete_removed`, `test_canonical_event_flag_passed_to_fire_and_forget`). The retained-but-unreachable branch is documented and unit-pinned, satisfying the FR-22-by-analogy "never silent dead code" rule. Age-prune-only is correctly the sole effective mechanism (pruneOffsets wired on the FNF path).

## Merge Sequencing Verification
**Status**: PASS (design-level).
At Gate 3a there is no code yet, so the check is whether the DESIGN bindingly encodes the size-gate-first constraint. It does: pseudocode/OVERVIEW §"Merge sequencing (BINDING)" lists size-gate as LITERAL FIRST COMMIT; size-gate.md, IMPLEMENTATION-BRIEF §"Merge Sequencing", and the test-plan OVERVIEW all repeat it; the git-log audit is correctly deferred to Gate 3c as a process check, additionally backstopped by the gate's embedded self-test running green on commit 1. The cross-feature dependency (vnc-030 depends on the redefinition) is carried.

## WARNs (non-blocking)

| # | Issue | Suggested Owner | Action |
|---|-------|-----------------|--------|
| W1 | synthesizer report lacks `## Knowledge Stewardship` block | uni-synthesizer (Session 1) | Add block on next touch; Session-1 agent outside strict Stage 3a set, so not blocking |
| W2 | AC-11/ACCEPTANCE-MAP "ts-rs bindings pass byte-unchanged" is imprecise (bindings change additively) | uni-spec-writer | Reword to "additive-only; existing shapes unchanged; drift check green after regen"; ensure Stage 3c tester asserts additive diff, not literal zero-diff on bindings file |
| W3 | ADR-004 silent on opt-out stripping of stale SubagentStop entries (pseudocode adds it correctly per AC-08) | uni-architect | Optional one-line ADR-004 §2 confirmation that opt-out strips Unimatrix-owned entries |

No FAIL items. No scope concerns.
