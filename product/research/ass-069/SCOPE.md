# ASS-069: Client-Streamed Session Transcript — Fidelity, Attribution, and UDS-Path Simplification

## Question

Can the single JS/TS hook client stream the **full session transcript** to Unimatrix — continuously, over both UDS and HTTP — so the server holds the authoritative conversation during a delivery session, distills the meaningful bits at cycle review, and purges the raw transcript on session close? And given that authoritative log, **what in the existing UDS observation path becomes redundant or improvable** — including the heuristic session-registry population we currently run at ~90% accuracy? The non-negotiable constraint: every streamed transaction must remain correctly attributed to its originating session under concurrency, exactly as UDS transactions are today.

## Why It Matters

The personal-cloud goal (#4676) requires remote sessions to have the **same intelligence-pipeline fidelity** as local UDS sessions — explicitly including PreCompact restoration. Today the remote path degrades: the hook client reads only a 12KB transcript tail reactively at PreCompact, and remote clients skip it entirely (`transcript_excerpt: null`, ass-068 Q5). That is a real fidelity gap against a stated success criterion.

Client-streamed transcript closes it without session hosting. If the client ships transcript **deltas** on each hook event (fire-and-forget, delta-sized, off the sync hot path), the server accumulates the real conversation over **either** transport with **one** client. This is strategically larger than a fidelity patch:

1. **It may obsolete session hosting (ass-066) for the observation use case.** 066's "strictly superior fidelity" argument rested on in-memory full-conversation access. Client-streamed transcript gives the server the same full conversation without hosting the session — leaving 066's unique value as proactive/inter-turn *injection* (control), not observation. That is a smaller, far more deferrable prize, and it preserves the single-edge-language decision (no Python — confirmed out).

2. **It may simplify, not just extend, the existing path.** We currently reconstruct session identity and feature/phase attribution from process lineage (`SO_PEERCRED`, `/proc/{pid}/cmdline`) and majority-vote heuristics (`enrich_topic_signal()`, `check_eager_attribution()`), landing around 90% accurate. With the authoritative transcript in hand, some of that scaffolding may be retirable, and some of the 13 hook events may be redundant. The spike must look backward at the current path, not only forward at the new one.

3. **It is privacy- and enterprise-forward by construction.** Raw transcript is ephemeral working state; distilled knowledge is the durable artifact. Done right, "delete on cycle close" is the default of a *retention policy seam* the enterprise product extends — not a hardcode to be re-architected later.

The single gate that decides shippability is empirical and concurrency-shaped: **does per-session attribution survive multiple simultaneous sessions** when transactions arrive as streamed deltas rather than process-lineage-bound UDS connections? This must be measured, not argued.

## Bounded Questions

### Q1: Attribution under concurrency — the gate (PoC required)

Attribution is a **hard requirement**, not a quality axis. With multiple sessions running at once, every streamed transcript delta and every hook transaction must be attributed to the correct session — no cross-contamination, no races.

- How is session identity carried on the streamed path today's `SO_PEERCRED` / process-lineage mechanism cannot serve remotely (HTTP has no process lineage)? Candidate: authoritative `session_id` minted at `SessionRegister` and echoed on every subsequent frame/request.
- Build a PoC that drives **N concurrent sessions** (mixed local UDS + remote HTTP) producing interleaved deltas, and verify every transaction lands in the right `SessionRegistry` slot.
- Does the current `SessionRegistry` keying assume one-connection-per-session in a way that breaks when a single remote client multiplexes, or when deltas arrive out of order?
- Reliability interaction: most observation writes are **fire-and-forget**. Confirm attribution correctness does **not** depend on delivery guarantees — a dropped delta may lose content, but must never mis-attribute surviving content.

Output a clear **go/no-go on attribution correctness under concurrency**, with the keying/identity mechanism that guarantees it.

### Q2: Delta-streaming mechanism and bounding

- Transcript is append-only JSONL; the client has `transcript_path` on every event. Design the **delta** mechanism: per-session byte-offset tracking, ship-since-last-offset, server appends to a per-session buffer.
- Hot-path safety: deltas ride fire-and-forget events (not the 3-5 sync events). Confirm no regression to the ass-068 Q1 latency budget.
- **Volume bounding**: a large tool response yields a large delta. Define a max-delta / sampling / truncation guard so a multi-MB read is not shipped whole over remote HTTP. What is the right cap, and what is lost at the cap?
- Framing parity: UDS length-prefix vs HTTP body — both carry arbitrary-size deltas; confirm no 12KB-tail assumption leaks from the current PreCompact code.

### Q3: UDS-path simplification given the authoritative log *(Doug Q1)*

Look **backward** at the existing path with the full transcript now available:

- Which parts of the heuristic attribution stack (`enrich_topic_signal()`, `check_eager_attribution()` majority vote, process-lineage feature inference) become **redundant** when feature/phase/topic can be read from the authoritative conversation? What accuracy do we recover above today's ~90%?
- Are any of the **13 hook events redundant** once the transcript is streamed? Which signals are *only* derivable from a discrete hook (e.g., tool timing, rework markers) vs. now recoverable from the transcript itself? Produce a minimal-necessary hook set.
- What stays mandatory regardless (lifecycle boundaries, the sync injection events)?
- Net: a "before/after" of the observation surface — what the streamed transcript lets us **delete**, not just add.

### Q4: Ephemerality and secrets handling

- Raw transcript may contain secrets/keys (principle 8: no secrets in any database). Recommend buffer placement: in-memory `Arc<RwLock>` (principle 7) vs disk spill for crash recovery — and if disk, how "delete on close" genuinely scrubs.
- Session-close semantics: what triggers purge (SessionClose / Stop / cycle close), and what survives (distilled knowledge only).
- What the **audit log** records about a streamed/distilled session — that it happened and was purged, never the raw content (principle 2, append-only).

### Q5: Cycle-review distillation quality

- Feeding the **real** transcript into the existing retrospective pipeline (21 detection rules) vs. today's reconstructed observations — what is the quality uplift, and does any rule need rework to consume transcript-shaped input?
- What does "save the meaningful bits" extract concretely (decisions, rework, patterns, phase narrative), and where does that extraction run?

### Q6: Reconciliation with #670

#670 ("server-side session transcript buffer — iterative content accumulation from observation events") is the **reconstruction** approach; this spike is the **real-transcript** approach. Same buffer destination, different data source.

- Does client-streamed transcript **supersede**, **absorb**, or **complement** #670? (Likely: reuse #670's buffer/distill/purge machinery, replace its reconstruction data source with the streamed transcript.)
- Is there a fallback role for reconstruction when streaming is unavailable or capped?
- Recommend the disposition of #670 as a GitHub issue.

### Q7: Enterprise extension seams

The OSS product need not *meet* enterprise requirements but must *extend* to them without re-architecture (#4676 posture). For this design specifically:

- **Tenant dimension**: carry `(tenant, project)` keying from day one (OSS populates one tenant) so multi-tenant is a populated dimension, not a re-key.
- **Retention as policy**: "delete raw on cycle close" is the *default* of a retention-policy knob (enterprise sets retain-N-days / encrypt-at-rest / data-residency by config).
- **Capability gating**: the streaming/observe endpoint passes through the service-layer capability check (principle 3) behind the `BearerValidator` seam.
- **Known gap to surface, not solve — auditability**: most observation writes are fire-and-forget (low delivery guarantee). Enterprise audit will demand higher confidence and may require a different, acknowledged-and-delivered write path. This is **known and accepted** for OSS; the spike should *name* where the seam is insufficient for enterprise audit, not design the enterprise solution.

## Decision Criteria

Score the design against, in priority order:

1. **Attribution correctness under concurrency** *(gate — pass/fail, not weighted)* — every transaction attributed to the right session, always.
2. **Fidelity parity** — remote sessions reach local-UDS fidelity (closes the #4676 PreCompact-restoration gap).
3. **Simplification dividend** — how much of the existing heuristic path and hook set this lets us retire (Q3).
4. **Hot-path safety & reliability** — no sync-path regression; graceful degradation; fire-and-forget never mis-attributes.
5. **Secrets safety** — ephemeral raw transcript never persisted where it becomes an exposure surface.
6. **Maintenance** — single TS client, one buffer, reconciled with #670; no new language or runtime.
7. **Enterprise extension seams** — tenant, retention-policy, capability-gating present from day one; audit gap named.

## Approach

**Investigation + evaluation + targeted PoC.** Internal analysis of the current attribution stack (`SessionRegistry`, `enrich_topic_signal()`, `check_eager_attribution()`, process-lineage auth in `listener.rs`/`session.rs`) and the 13-event hook surface; the streamed-transcript design over the `wire.rs` contract. **A PoC is required for Q1** — concurrent-session attribution under interleaved streamed deltas is the gating input and cannot be settled from literature. Q2 delta-streaming and Q3 redundancy analysis may be argued from code + a small streaming harness.

**Breadth: `code`, thorough** — primarily internal codebase investigation, with the concurrency PoC as the empirical core.

**Confidence required: `actionable`** — specific enough to write a design brief and a chunked roadmap, with a clear go/no-go on attribution-under-concurrency.

## Constraints

- **Hard**: Per-session attribution must be correct under concurrency — multiple simultaneous sessions, each transaction attributed to its originating session. Parity with today's UDS attribution is the floor, not the target.
- **Hard**: Single edge language — JS/TS client only. No Python, no second client runtime (confirmed: 066's Python recommendation is rejected).
- **Hard**: Client and server communicate only via `wire.rs` JSON types; the streamed transcript rides the existing transport contract (length-prefixed UDS / JSON HTTP `/observe`).
- **Hard**: Graceful degradation — the hook process exits 0 regardless of server state; no stdout on failure (ass-064). A dropped delta loses content, never mis-attributes.
- **Hard**: No secrets in any database (principle 8); audit log append-only and complete (principle 2); in-memory hot path (principle 7).
- **Hard**: Full pipeline fidelity on both transports — no remote-vs-local signal loss.
- **Open — do not pre-decide**: Whether client-streamed transcript supersedes, absorbs, or complements #670. Whether the result is *more* or *less* observation surface than today (Q3 may shrink it).
- **Accepted, out of scope to solve**: Enterprise audit-confidence rework for fire-and-forget writes — named as a seam gap, not designed here.

## Dependencies / Prior Art

- **ass-068** — unified TS client architecture; this spike answers its Q5 transcript-reader question at higher ambition and feeds its Chunk 1/Chunk 5 roadmap.
- **ass-066** — session hosting; this spike may shrink its remaining value to injection-only. Out of scope to design here, but state the impact.
- **ass-064** — remote telemetry transport; 500ms sync budget, exit-0 degradation.
- **vnc-022** — `/observe` endpoint (shipped); `CompactPayload.transcript_excerpt` forward-compat seam.
- **#670** — server-side session transcript buffer (reconstruction approach); reconcile.
- **#4676** — personal-cloud goal; the PreCompact-fidelity success criterion this spike makes meetable.
- This codebase: `SessionRegistry` (`crates/unimatrix-server/src/infra/session.rs`), attribution machinery in `listener.rs`, `hook.rs` transcript reader (`extract_transcript_block`), the retrospective pipeline's 21 detection rules.

## Out of Scope

- **Implementing** the migration — this spike produces architecture + roadmap; delivery is separate sessions.
- **Session hosting / `unimatrix run` (ass-066)** — the design must not preclude injection-style hosting later, but designing it is separate.
- **Enterprise OAuth/RBAC/multi-tenant isolation/audit-confidence rework** — extension seams must exist; the enterprise implementation is a separate product.
- Server-side intelligence-pipeline scoring changes (the migration is wire-compatible by construction).

## What the Output Should Be

- **Go/no-go on attribution under concurrency** (Q1): the identity/keying mechanism, with PoC numbers across N concurrent mixed-transport sessions.
- **Streamed-transcript design** (Q2): delta mechanism, offset tracking, volume bounding, hot-path safety.
- **Simplification map** (Q3): before/after of the attribution stack and hook set — what the authoritative log lets us retire, with recovered-accuracy estimates and a minimal-necessary hook set.
- **Ephemerality + secrets design** (Q4) and **distillation quality** (Q5).
- **#670 disposition** (Q6) and **enterprise-seam checklist with named audit gap** (Q7).
- **Roadmap fit**: how this slots into ass-068's chunked migration.

## Known Constraints

- The hook fires on `PreToolUse`/`PostToolUse` many times per turn; cumulative overhead matters (ass-068 baseline: Node ~12ms/spawn, fire-and-forget dominant).
- Today's attribution is ~90% accurate via process-lineage + majority-vote heuristics — the bar to beat and possibly retire.
- Transcript is append-only JSONL with `transcript_path` available on every event; the 12KB tail is a self-imposed injection budget, not a transport limit.
- Most observation writes are fire-and-forget — adequate for OSS, insufficient for enterprise audit confidence (known, accepted).
