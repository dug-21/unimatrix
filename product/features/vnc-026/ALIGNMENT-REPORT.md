# Alignment Report: vnc-026

> Reviewed: 2026-06-06
> Artifacts reviewed:
>   - product/features/vnc-026/architecture/ARCHITECTURE.md
>   - product/features/vnc-026/specification/SPECIFICATION.md
>   - product/features/vnc-026/RISK-TEST-STRATEGY.md
> Scope inputs: product/features/vnc-026/SCOPE.md, product/features/vnc-026/SCOPE-RISK-ASSESSMENT.md
> Vision source: product/PRODUCT-VISION.md + goal entries #4710 (personal-cloud), #4677 (self-learning), #4673 (proactive-delivery)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly advances goal:personal-cloud success criteria; honors single-edge-language and graceful-degradation principles |
| Milestone Fit | PASS | Exactly F3 of the ass-068 delivery path encoded in goal #4710; F4/F5/F6 work correctly deferred; RQ-8 exception scoped and human-approved |
| Scope Gaps | PASS | All 6 SCOPE goals, all 16 ACs, and all constraints traced into FR-01..26 / AC table / C-01..C-10 |
| Scope Additions | WARN | Health breadcrumb (ADR-005), OS-level CI runners, timeout structure — all risk-driven and documented, none scope-approved verbatim |
| Architecture Consistency | WARN | One stale cross-reference (R-14 gate note vs current FR-01); ownership-regex spaced-path defect candidate open; env-var naming pending F5 |
| Risk Completeness | PASS | All SR-01..SR-11 and A-1..A-4 traced; risk strategy adds genuinely new risks (R-14, R-20) with historical evidence |

**Verdict: PASS with 1 variance for human approval and 4 WARN-level notes.** No FAILs.

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Gap | — | None found. Every SCOPE goal (1–6), AC (01–16), constraint, and resolved question (RQ-1..RQ-8) is traceable to spec FRs/ACs and architecture components. |
| Addition | Health breadcrumb + stderr one-liner (ADR-005, FR-05, §2) | Not in SCOPE.md (scope says "exit 0, no stdout"). Explicitly invited by SCOPE-RISK SR-10 ("make the trade-off an explicit ADR"). Content-free, does not violate exit-0/no-stdout. Acceptable. |
| Addition | OS coverage in CI (Windows/macOS runners) | SCOPE AC-12 specifies only Node 18/20/22/24. Risk strategy R-14 demands Linux+macOS+Windows runners. Justified — Windows/macOS support is the feature's stated purpose ("unavailable on macOS/Windows" is the problem statement) — but expands CI surface beyond the scoped AC. |
| Addition | HTTP timeout structure (750/2,000/3,000 ms, overridable) | Architect-chosen detail (ADR-005 consequences); SCOPE only sets the 500 ms sync budget ceiling. Spec OQ-4 resolution. Within architect discretion. |
| Addition | Offset-file lifecycle (7-day prune, delete on SessionClose) and `crypto`/`os`/`process` built-ins beyond the four SCOPE lists | Detail-level; consistent with the "bounded client state" and "built-ins only" constraints' intent. |
| Simplification | AC-15: `transcript_delta` frames never queued (ADR-004 carve-out) | SCOPE AC-15's letter says fire-and-forget frames are enqueued on failure; deltas ARE fire-and-forget. Spec exempts them: failed deltas re-derive from the offset (the transcript file is the queue). Rationale: zero transcript bytes at rest — eliminates SR-06 rather than mitigating it; strictly better delivery (catch-up span). See variance 1. |

## Variances Requiring Approval

1. **AC-15 carve-out — `transcript_delta` frames are never written to the disk queue (ADR-004)**
   - **What**: SCOPE.md AC-15 (binding, human-approved with RQ-1) states "On send failure, fire-and-forget frames are enqueued to the disk event queue." SPECIFICATION FR-12/AC-15 exempt `transcript_delta` frames: on delta-send failure the offset does not advance and the next spawn re-derives `[last_offset, file_len)`. The architecture's own Open Question 2 flagged this as a wording conflict for "spec / leader" — the spec folded it in, but no record shows the human approving the changed AC letter.
   - **Why it matters**: It alters a binding acceptance criterion. It is, however, a strict improvement against the vision: SR-06 (raw conversation bytes, potentially secrets, unencrypted at rest) is eliminated rather than mitigated, consistent with architectural principle 8's spirit (secrets posture) and goal #4710's degradation contract ("content loss, never mis-attribution"). Delivery quality is equal or better (re-derived spans are fresher and dedup is free via F2's idempotent merge).
   - **Recommendation**: **Accept.** Record the acceptance (or amend SCOPE.md AC-15) so the delivery gate doesn't trip on the letter of the original AC.

## Detailed Findings

### Vision Alignment

vnc-026 is a near-literal implementation of goal #4710 (Individual developer-friendly deployment) success criteria:

- "No local binary required for remote clients — hook events POST to /observe endpoint, injection content returned in response" → SCOPE Goals 1–2, FR-01..04.
- "Remote sessions have same intelligence pipeline fidelity as local UDS sessions — achieved via client-streamed transcript deltas (ass-069), NOT session hosting" → SCOPE Goal 3, FR-07..11, W5 (the #4676 PreCompact closure).
- "Single edge language — JS/TS hook client only" → zero-dependency CommonJS client; no second runtime introduced.
- "One container, one bearer token, one command" → `npx @dug-21/unimatrix init --remote <url> --token <tok>` (FR-17..20).
- The known audit-confidence gap is correctly preserved: SCOPE Non-Goals and spec §9 both name "Enterprise acknowledged-delivery / at-least-once audit path (ass-069 Q7)" as out of scope — matching the goal entry's "accepted and deferred to the enterprise product."

It also advances goal:self-learning (the spec's stated objective: "the self-learning pipeline... today loses all conversational context for remote sessions") and goal:proactive-delivery (server-formatted sync injections over HTTP).

Architectural principles: #5 graceful degradation is the feature's organizing constraint (fail-open, defined fallbacks at every layer — FR-05, Failure Modes table); #6 "client is an adapter, not infrastructure" is honored (stateless except two bounded artifacts); #8 no-secrets posture extends to the client edge (RQ-3, NFR-10, R-16). Principles 1–4, 7 are server-side and untouched (C-07 forbids server changes) — N/A by construction.

### Milestone Fit

Goal #4710 encodes the delivery path: "F3 vnc-026 TS HTTP client + streaming (#679, remote MVP, closes PreCompact gap)." The documents stay inside that boundary. Non-goals correctly defer UDS transport (F4 #680), init unification (F5 #681), hook.rs retirement (F6 #682), sidechain capture (ass-071), and distillation (crt-052 #689). The one cross-milestone exception — RQ-8's local `HOOK_EVENTS` fix — is explicitly human-approved in SCOPE.md, blast-radius-limited (C-10, SR-07, R-12), and removed from F5's deliverables to avoid duplication. The parity corpus doubling as F6 retirement evidence is forward-looking without building F6 itself. No future-milestone capability is built early.

### Architecture Review

ARCHITECTURE.md is consistent with SCOPE and traces every SR to a disposition. The transport seam ("a module, not an abstraction layer") leaves F4 room without building it — good milestone discipline. Three consistency items (all WARN, none vision violations):

1. **Stale cross-reference**: RISK-TEST-STRATEGY gate-note 1 says "FR-01 specifies `fs.readFileSync('/dev/stdin')`" — but the current SPECIFICATION FR-01 already specifies `fs.readFileSync(0)` and explicitly forbids `'/dev/stdin'` "(R-14)". The spec defect the risk strategy gates on is already resolved; the gate note is stale. Leader should confirm the resolution and not treat gate-note 1 as open. (This is the exact failure shape of stored pattern #3337 — testers asserting against superseded doc wording.)
2. **Ownership regex spaced-path gap** (gate-note 2): the architecture freezes `/(^|\s|\/)node\s+\S*\/hook-client\/index\.js\s/` in its Integration Surface table, but `\S*` cannot match install paths containing spaces (`C:\Program Files\`, `~/My Projects/`). Risk strategy flags it as a design defect candidate. Must be resolved before the pattern freezes — affects AC-11.
3. **Env-var naming pending F5** (architecture OQ-3 / spec OQ-6): `UNIMATRIX_REMOTE_URL`/`UNIMATRIX_REMOTE_TOKEN` must be confirmed against #681 before delivery. Tracked in both docs; carry-forward item, not a blocker.

### Specification Review

The spec covers all 16 SCOPE ACs verbatim in §5 with verification methods, and resolves the architecture's two AC-wording open questions: FR-09 narrows AC-08 ("the SubagentStart fallback query reads the JSONL tail per FR-02/RQ-6 — that is query derivation, not delta streaming") — a faithful narrowing since the Rust hook does the same read, preserving parity; and AC-15 reflects the ADR-004 carve-out (variance 1 above). FR coverage is complete against all 6 SCOPE goals; §9 NOT-in-scope mirrors SCOPE Non-Goals item-for-item. The queue mini-spec (FR-14..16) delivers SCOPE-RISK design recommendation 2 exactly (bounds, eviction, locking, at-rest posture, lifecycle).

### Risk Strategy Review

Full SR-01..SR-11 / A-1..A-4 traceability table present. The strategy adds material new risks beyond the scope assessment, each with historical Unimatrix evidence: R-14 (Windows stdin — Critical, given fail-open makes total failure invisible), R-20 (vacuous CI drift check, evidence #4452), R-15 (server-controlled stdout as a prompt-injection surface — relevant to the vision's trust posture), R-19 (ppid session collision). Coverage proportions match priority (Critical: ~35 scenarios; Low: ~7). Gate-note 3 (pinning F2 elision-hole merge semantics with vnc-025 before gates) is a necessary cross-feature coordination item consistent with C-08's delivery gate; flag to the leader since vnc-025 is still in flight.

## Knowledge Stewardship
- Queried: /uni-query-patterns (context_search, topic `vision`, category `pattern`) — found #3337 (architecture/spec wording divergence misleads testers; directly applicable to the stale R-14 gate note), #2298, #4617 (not applicable).
- Stored: nothing novel to store — the one observed misalignment shape (doc cross-reference staleness across sequentially produced artifacts) is already captured by pattern #3337; the AC-15 carve-out variance is feature-specific and does not generalize.
