# Agent Report: vnc-026-agent-2-spec (Specification Writer)

## Deliverable
- `product/features/vnc-026/specification/SPECIFICATION.md` — 26 FRs, 10 NFRs, all 16 ACs from SCOPE.md with verification methods, domain model, 5 workflows, 10 constraints, 5 open questions for the architect.

## Key Decisions
- SR-02 baked into FR-04: SubagentStart `hookSpecificOutput` envelope must be a literal template string, not generic JSON serialization; committed fixtures are the only serialization authority.
- SR-04 baked into FR-08/AC-07: the 64 KiB delta cap is defined against the serialized frame with a mandatory post-serialization size check (escape-inflation worst case covered in verification).
- SR-05/SR-06 expanded into FR-14–FR-16: queue gets explicit bounds (entries/bytes/age), drop-oldest eviction, single-writer O_EXCL-per-frame strategy, age-based expiry + purge-on-replay, and documented plaintext at-rest posture — exact values delegated to architect (OQ-3).
- SR-01 enforced via FR-22: parity corpus (Rust hook as oracle, adversarial cases included) is required before client modules are implemented.
- FR-09 clarifies the AC-08 "no transcript I/O on sync path" vs RQ-6 (SubagentStart JSONL tail-parse) apparent conflict: the tail-parse is query derivation matching Rust behavior, not delta streaming.
- A-4 guard specified as FR-11 (never read a negative span); reset semantics left to architect (OQ-2).
- Remote event set enumerated concretely (9 events: existing 7 + PreCompact + PostToolUseFailure), verified against current `init.js`/`merge-settings.js` source.

## Open Questions (for Architect)
- OQ-1: SR-10 observability ADR (silence vs local breadcrumb under fail-open).
- OQ-2: offset reset semantics on transcript shrink.
- OQ-3: queue bound values + locking scheme.
- OQ-4: timeout values + env-var names (align with F5 #681).
- OQ-5: replay batch sizing for slow filesystems.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced vnc-024 ADRs #4714/#4720/#4726 and vnc-025 ADRs #4741/#4743; incorporated as frozen-surface dependencies and constraints C-01/C-07.

## Self-Check
- All AC-01..AC-16 present with verification methods: yes.
- All RQ-1..RQ-8 reflected as binding requirements: yes.
- All SR-01..SR-11 + A-1..A-4 addressed in FRs, constraints, verification, or OQs: yes.
- No TBD/placeholder sections; unknowns flagged as OQ-1..OQ-5: yes.
- Output confined to `specification/` (+ this report): yes.

---

# Reconciliation Pass (second run, same agent ID)

Spec aligned to ADR-003..007 (architecture written in parallel). No redesign — fold-in only.

## Sections changed
- §2 Domain Models: state dir → `~/.unimatrix/{hash}/hook-client/` layout (0600/0700); event-queue term gains the delta exemption; new "Health breadcrumb" term (ADR-005).
- FR-03: timeouts 750 / 2,000 / 3,000 ms connect / sync / fire-and-forget, config-overridable (ADR-005).
- FR-05: stderr one-liner + content-free health.json breadcrumb with failure classes; init Ping sole loud checkpoint (ADR-005).
- FR-06: ADR-006 resolution order — `UNIMATRIX_REMOTE_URL`/`UNIMATRIX_REMOTE_TOKEN` > root-anchored `settings.local.json` `unimatrix.remote`; root walk identical to state-dir hashing; partial env pair = misconfiguration; SR-09 test obligations.
- FR-07: offset advances to boundary-trimmed shipped-span end (or `file_len` on elision); concurrent second POST, independent outcomes (ADR-004/ADR-007).
- FR-11: rewrite guard = reset to `file_len`, ship nothing (traces to ass-069 Q1); atomic-rename offsets; sanitized session keys.
- FR-12: explicit ADR-004 carve-out — `transcript_delta` never queued; failure = offset-non-advance + re-derive next spawn; accepted losses recorded.
- FR-14: ADR-003 values — O_EXCL one-frame-per-file `{ts_ms}-{pid}-{seq}.json`, 500 files / 5 MiB / 24 h drop-oldest, 0600/0700.
- FR-15: replay cap 32 frames / 256 KiB per spawn, stop-at-first-failure, poison-pill deletion.
- FR-16: at-rest lifecycle — only non-delta frames queued; 24 h prune; offset files 7 d / deleted on successful SessionClose.
- FR-18: settings.local.json write details (0600, merge-preserving, gitignore warning).
- AC-15: carve-out in verification — delta failure asserted via offset-non-advance + no queue file, not queue presence; bounded replay values.
- NFR-09 / C-09: state footprint updated (hook-client subdir, health.json, no transcript bytes at rest).
- §6 W4: deltas not enqueued; bounded replay; 24 h expiry; catch-up span semantics.
- §10: OQ-1..5 marked resolved with resolving ADRs (table); replaced by OQ-6.

## Remaining genuine open question
- OQ-6: env-var names canonical for F3, to be confirmed against F5 (#681) before delivery (caveat shared with ADR-006).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced vnc-026 ADR entries #4754 (ADR-004), #4753 (ADR-003), #4757 (ADR-007), plus vnc-025 #4740/#4741 confirming the idempotent-merge basis for the FR-11 reset rule. No storage (read-only tier).

---

# Defect Fix Pass (third run, same agent ID) — R-14

Risk strategist flagged R-14 (Critical): FR-01 specified `fs.readFileSync('/dev/stdin')`, which
throws on Windows (a target platform); fail-open makes the failure invisible. ARCHITECTURE.md
already specified `fs.readFileSync(0)`.

## Changes
- FR-01 (§3.1, lines 48-51): stdin read changed to `fs.readFileSync(0)` (fd 0), with explicit
  "never `'/dev/stdin'`" warning citing R-14. Now matches ARCHITECTURE.md.
- §5 (after AC-16 table): verification note added — stdin reading (FR-01) must be covered on
  Linux/macOS/Windows; AC-12 CI matrix covers Node versions, OS coverage comes from the risk
  strategy's R-14 scenario.
- Verified no other `/dev/stdin` occurrence remains in the spec.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — no entries directly relevant to the fd-0 stdin fix; no storage (read-only tier).
