# Alignment Report: vnc-005

> Reviewed: 2026-03-17
> Artifacts reviewed:
>   - product/features/vnc-005/architecture/ARCHITECTURE.md
>   - product/features/vnc-005/specification/SPECIFICATION.md
>   - product/features/vnc-005/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Agent: vnc-005-vision-guardian

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Directly implements W0-0 as defined; rationale and constraints match vision intent |
| Milestone Fit | PASS | W0-0 ("do this first") prerequisite; appropriately scoped to dev workspace only |
| Scope Gaps | PASS | All SCOPE.md goals, non-goals, and ACs are addressed across the three source docs |
| Scope Additions | WARN | Three items added beyond SCOPE.md: FR-12 (session cap at 32), FR-16/FR-17 accumulator data type mismatch with SCOPE, and FR-19/C-07 explicit UDS rate-limit exemption boundary |
| Architecture Consistency | PASS | Architecture resolves every scope risk (SR-01 through SR-09); ADRs documented; open questions acknowledged |
| Risk Completeness | PASS | 18 risks fully enumerated; all SCOPE-RISK-ASSESSMENT.md items traced; coverage matrix provided |

**Overall: PASS with one WARN.** The feature is well-aligned with the product vision and SCOPE.md. One warn-level scope addition and one minor data-model discrepancy between SCOPE and specification require human acknowledgment before delivery begins.

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | FR-12: Session concurrency cap (32 sessions) | SCOPE.md notes SR-09 as "Med/Low" with recommendation to "define maximum or document unbounded"; source docs resolve it as a hard cap of 32. This is the right call architecturally but is a scope addition — SCOPE.md did not mandate a specific number. |
| Addition | FR-19 / C-07: CallerId::UdsSession rate-limit exemption documented in code | SCOPE.md mentions this in security requirements ("document that this exemption is local-only and must never extend to HTTP transport") but does not make it a functional requirement with a test scenario. The specification elevates it to a formal constraint (C-07) with a code-comment gate requirement and the risk strategy adds R-07 with a test scenario. Alignment with W0-0 security requirements makes this correct — the elevation is conservative and appropriate. No human approval needed, but noting for visibility. |
| Simplification | Accumulator data model: SCOPE OQ-05 vs specification FR-16 | SCOPE.md OQ-05 resolves to `HashMap<feature_cycle, Vec<EntryRecord>>`. ARCHITECTURE.md Component 5 and spec FR-16 define the inner type as `Vec<EntryRecord>` in the domain model section but use `HashMap<u64, EntryAnalysis>` as the inner type in the architecture struct definition (`PendingEntriesAnalysis::buckets: HashMap<String, HashMap<u64, EntryAnalysis>>`). This is a refinement, not a contradiction — `EntryAnalysis` wraps `EntryRecord` — but the surface-level type names differ across SCOPE, specification, and architecture. Implementors should confirm the final inner type before coding to avoid a spec/implementation mismatch. |
| Simplification | Log rotation deferred (NFR-07) | SCOPE.md is silent on log rotation. SPECIFICATION.md explicitly defers it to Wave 2 (NFR-07). This is an acceptable simplification with documented rationale (dev-workspace scope). |
| Simplification | Connection cap: architecture leaves it as an open question; specification resolves it at 32 | ARCHITECTURE.md OQ-1 says "unbounded, revisit if reconnect storms reported." SPECIFICATION.md FR-12 sets 32 as a hard requirement with AC-20. The spec supersedes the architecture's open question, which is the correct direction of authority. The architecture document should be updated by the implementor to close OQ-1, but this is not a gate-blocking issue. |

---

## Variances Requiring Approval

No FAIL or VARIANCE classifications. One WARN item for human acknowledgment:

### WARN-01: Accumulator Inner Type Discrepancy

**What**: SCOPE.md OQ-05 resolves `pending_entries_analysis` as `HashMap<feature_cycle, Vec<EntryRecord>>`. ARCHITECTURE.md Component 5 defines it as `HashMap<String, HashMap<u64, EntryAnalysis>>` (two-level map with entry ID as inner key). SPECIFICATION.md FR-16 domain model says `HashMap<String, Vec<EntryRecord>>` — matching SCOPE — but the architecture's struct definition uses the two-level form.

**Why it matters**: If the implementor uses the architecture struct definition (two-level) as the ground truth, the retrospective drain semantics differ from the specification's domain model (single-level Vec). The two-level form is better (avoids duplicate entry IDs), but the specification's domain model section contradicts the architecture's struct definition. This ambiguity could produce an implementation that diverges from what AC-17/AC-18 tests exercise.

**Recommendation**: Before delivery begins, the architect should explicitly reconcile the inner type across SPECIFICATION.md §Domain Models and ARCHITECTURE.md §Component 5 and confirm which is authoritative. The two-level `HashMap<u64, EntryAnalysis>` inner structure is the stronger design — if that is the intent, the specification domain model section should be updated to match. Accept this warn by confirming the authoritative type.

---

## Detailed Findings

### Vision Alignment

vnc-005 is the direct implementation of W0-0 from PRODUCT-VISION.md. The vision states W0-0 is "do this first" and describes precisely the daemon mode, UDS transport, and bridge client pattern that this feature implements.

Key vision requirements checked against source documents:

| Vision Requirement | Source Doc Coverage | Status |
|---|---|---|
| `unimatrix serve --daemon` starts long-lived process | SPEC FR-01, AC-01; ARCH Component 1 | PASS |
| Claude Code connects via UDS instead of per-session stdio | SPEC FR-04, FR-09, FR-10; ARCH Components 2, 6 | PASS |
| Daemon survives client disconnect | SPEC FR-06, AC-04; ARCH Component 4 | PASS |
| Auto-start if no daemon running | SPEC FR-05, AC-05; ARCH Component 6 | PASS |
| PidGuard + flock one-daemon enforcement | SPEC FR-02, FR-08, AC-06, AC-07; ARCH references vnc-004 | PASS |
| UDS socket 0600 permissions | SPEC FR-13, AC-02; ARCH Security section; RTS R-06 | PASS |
| Stale PID check via is_unimatrix_process | SPEC FR-05 step 1, FR-08, AC-06; ARCH Component 6 | PASS |
| CallerId::UdsSession exemption boundary documented | SPEC FR-19, C-07; RTS R-07 | PASS |
| HTTP transport explicitly deferred (W2-2) | SPEC §NOT in Scope; SCOPE §Non-Goals | PASS |
| Dev workspace only; container/systemd deferred | SPEC C-11, NFR-07; SCOPE §Non-Goals | PASS |

The vision document's "Why UDS not HTTP" rationale (local only, no TLS management, HTTP is additive) is explicitly echoed in SCOPE.md's resolved design decision OQ-01, which also cites the W2-2 migration path. This is correctly disciplined.

The vision's three security requirements for W0-0 are all addressed:
- [High] 0600 permissions: SPEC FR-13, AC-02, ARCH Security, RTS R-06
- [Medium] Stale PID check: SPEC FR-05, FR-08, ARCH Component 6
- [Low] UdsSession exemption boundary documented: SPEC FR-19, C-07, RTS R-07

### Milestone Fit

W0-0 is the first item in Wave 0 ("Prerequisites — do first"). The feature makes no attempt to implement W0-1 (Two-Database Split), W0-2 (Session Identity), or W0-3 (Config Externalization). It does not implement any Wave 1 capabilities.

The feature correctly identifies itself as the prerequisite that "makes Wave 1+ intelligence features meaningful" (ARCH §System Overview). This is consistent with the vision's framing: "Without daemon mode, Wave 1 delivers the infrastructure for background intelligence but the background never actually runs between sessions."

The two-socket design decision (hook IPC on `unimatrix.sock`, MCP on `unimatrix-mcp.sock`) is explicitly forward-engineered for W2-2 migration: the MCP socket becomes the HTTP listener surface. This is milestone-disciplined foresight, not premature W2-2 implementation — it resolves a structural choice that would otherwise require W2-2 to untangle shared-socket discriminator logic.

No Wave 1, 2, or 3 capabilities are pre-built. The feature correctly terminates at W0-0 scope.

### Architecture Review

The architecture resolves all nine scope risks from SCOPE-RISK-ASSESSMENT.md:

| Scope Risk | Architecture Resolution | Quality |
|---|---|---|
| SR-01: transport-async-rw untested | ADR-003 confirms UnixStream wrapping; Clone already exists | Strong |
| SR-02: tokio+fork UB | ADR-001: spawn-new-process pattern, no fork after runtime | Definitive |
| SR-03: 5s timeout assumption | Open Question 2: 250ms polling, stderr error with log path | Adequate (left to implementor) |
| SR-04: default invocation behavioral change | Components 3, 6; bridge as default; stdio preserved | Strong |
| SR-05: accumulator eviction undefined | ADR-004: three eviction triggers, TTL 72h | Strong |
| SR-06: server clone + shutdown as coordinated refactor | ADR-002 + ADR-003 treated as single refactor | Strong |
| SR-07: graceful_shutdown decoupling | ADR-002: CancellationToken model, single shutdown call site | Strong |
| SR-08: stale MCP socket | Component 2: handle_stale_socket extended to unimatrix-mcp.sock | Strong |
| SR-09: per-session task backpressure | Open Question 1: left as "unbounded, revisit" (spec later resolves at 32) | Adequate |

One architecture open question (OQ-3: session task cancellation notification) remains unresolved. The architecture states "a 30-second join timeout is a reasonable implementation-time decision." This is acceptable for W0-0 scope — the behavior (drain naturally vs immediate cancel) is an implementation-time UX decision with no safety implication.

The architecture correctly identifies that `LifecycleHandles` needs `mcp_socket_guard: Option<SocketGuard>` and `mcp_acceptor_handle`. The drop ordering concern in the RTS Integration Risks section (PidGuard drops after SocketGuard) is a significant correctness invariant not explicitly stated in the architecture document — it is raised only in the RTS. This is acceptable as the RTS is authoritative for implementation constraints, but the architecture would benefit from a note on drop ordering in the Daemon Shutdown Sequence.

Log file growth (OQ-4) is acknowledged, deferred to a future nan-XXX logging feature, and explicitly covered in SPEC NFR-07. This is properly scoped.

### Specification Review

The specification is complete and tightly structured. All SCOPE.md acceptance criteria (AC-01 through AC-12) are present and expanded with explicit verification steps.

The specification adds eight acceptance criteria beyond the SCOPE.md twelve (AC-13 through AC-20). These additions are all defensible:
- AC-13 (hook IPC unaffected): covers SCOPE AC-10 at specification detail level
- AC-14 (concurrent sessions): covers SCOPE AC-11 with concrete verification
- AC-15 (bridge failure message): resolves SR-03 per SCOPE-RISK-ASSESSMENT.md recommendation
- AC-16 (stale MCP socket unlink): resolves SR-08 per SCOPE-RISK-ASSESSMENT.md recommendation
- AC-17/AC-18 (accumulator multi-session): resolves OQ-05
- AC-19 (fork before runtime): code-review gate for SR-02 / C-01
- AC-20 (session cap enforcement): resolves SR-09 with the FR-12 cap of 32

Constraint C-04 and C-05 are notable: they make the `UnimatrixServer` clone/Arc refactor and `graceful_shutdown` decoupling a joint gate requirement. This directly implements the SCOPE-RISK-ASSESSMENT.md SR-06 and SR-07 recommendations ("treat as a single coordinated refactor"). This is precisely correct.

FR-19 (CallerId::UdsSession exemption scope) is the one item that could be questioned as exceeding SCOPE.md. SCOPE.md says "document that this exemption is local-only and must never extend to HTTP transport callers" — a documentation requirement. FR-19 elevates this to a functional requirement with a code-comment gate. Given the vision's explicit concern about the W2-2 security boundary, this elevation is conservative and correct. It is noted as a scope addition (WARN-01 above) but does not require approval to proceed.

The specification's "NOT in Scope" section accurately mirrors SCOPE.md's Non-Goals with no omissions.

### Risk Strategy Review

The RISK-TEST-STRATEGY.md is the strongest of the three source documents. It:

1. Enumerates 18 risks, all credibly derived from the feature's scope and architecture
2. Provides 44+ test scenarios with specific observable assertions
3. Traces every scope risk to at least one architecture risk and a resolution path (Scope Risk Traceability table)
4. Cites specific Unimatrix knowledge entries (#81, #245, #300, #312, #731, #735) as evidence for risk severity decisions
5. Covers integration risks that are not captured as named risks (PidGuard drop ordering, accumulator drain rollback, ServiceLayer Arc construction guarantee)

The five Critical risks (R-01, R-02, R-03, R-04, R-12) represent the genuine failure modes of this architectural transformation:
- R-01 (Arc::try_unwrap) and R-03 (session EOF triggers shutdown) are the most likely first-delivery bugs — the RTS is right to elevate these to Critical based on historical evidence (entry #312, bugfix #92).
- R-12 (stdio regression) is correctly elevated to Critical; it is the kind of regression that passes smoke tests but breaks CI pipelines silently.

One gap in risk coverage: the RTS does not have a named risk for the drop-ordering invariant raised in the Integration Risks section (PidGuard must drop after SocketGuard). This is mentioned in prose but has no R-N identifier, no test scenario, and no coverage requirement. The RTS recommends it be treated as a code-review constraint. This is a minor process gap — if an implementor skips the integration risks prose and works only from the R-N register, this invariant is invisible. It does not change the PASS classification for Risk Completeness, but the implementor should be aware.

The Security Risks section correctly identifies `feature_cycle` key length as a potential DoS vector and recommends a 256-byte cap. This is not present in the specification as a functional requirement. If the team wants this enforced, it should be added to the specification as an FR or validation constraint before delivery begins.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns, scope addition patterns, milestone discipline — no results (category: pattern, topic: vision returned empty).
- Stored: nothing novel to store at this time. The scope additions and data-model discrepancy observed here are feature-specific to vnc-005's accumulator design evolution from SCOPE through architecture to specification. They do not yet constitute a cross-feature pattern. If a second vnc-phase feature shows the same type→subtype refinement gap between SCOPE and SPECIFICATION, that would warrant a stored pattern entry.
