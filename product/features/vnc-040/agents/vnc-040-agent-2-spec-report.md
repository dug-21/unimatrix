# Agent Report — vnc-040-agent-2-spec

## Task
Author SPECIFICATION.md for vnc-040 (per-slug config overlay resolution, C6 / Feature A of #785)
from the APPROVED SCOPE.md, turning AC-01…AC-09 and the 10-input verdict table into testable
functional/non-functional requirements with verification methods and domain models.

## Output
- `product/features/vnc-040/specification/SPECIFICATION.md`

## Coverage
- All 9 scope ACs present. AC-08 split into AC-08a (per-file) and AC-08b (post-merge cross-field
  re-validation) per SR-01 — added FR-07 mandating `validate_config(&merged)`, citing #3905.
- SR-04: AC-02 specified as a hard byte-for-byte regression sentinel across all 10 inputs for the
  no-file path (FR-08, NFR-02); `None` arm must pass global `Arc`s unchanged, never re-derive.
- SR-03/SR-07: AC-04 makes both model-parity assertions testable (NLI + embedding, one of each at
  N≥2), incl. that a per-slug `[embedding]` override neither loads nor describes a second embedding
  model (FR-04 construction proof + FR-05 whole-section lock).
- 10-input verdict reproduced as a closed checklist requirement (FR-11 / AC-07).
- Ubiquitous language defined: overlay, fallthrough, merged-config, model invariant, `[embedding]`
  lock, hash-pin carve-out, the seam.

## Key Decisions
- Split scope AC-08 into AC-08a/AC-08b so the SR-01 cross-field re-validation is a first-class,
  separately-verifiable criterion rather than folded into per-file validation.
- Verification methods labeled B (behavioral N=2 model-free per #5172) / U (unit) / C (construction
  review) / R (byte-equality regression) so the tester can map directly.
- Carried A1/A2/A3 assumptions forward; flagged SR-02 (`merge_configs` inline `InferenceConfig`
  literal re-audit, #4070) explicitly to the architect.

## Open Questions (for architect)
1. **SR-02 re-audit:** confirm the existing `merge_configs` `inference` arm (incl. the inline
   `InferenceConfig {…}` literal, #4070) handles the global→per-slug pairing identically to
   global→project before reuse. If not, the per-key merge (FR-02) needs care.
2. **`[embedding]` lock mechanism:** architect to decide whether the whole-section global-wins lock
   (FR-05) lives inside `merge_configs` or as a post-merge overwrite at the seam — both satisfy the
   spec; the choice affects whether `merge_configs` gains an `[embedding]` arm.
3. **Post-merge validation placement (FR-07/AC-08b):** confirm `validate_config` already covers the
   relevant cross-field sum invariants (fusion/PPR weights) or whether the merged-config check needs
   an additional assertion.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — #2395, #4655/#4649, #5172, #5079, #5165, #5086; plus
  #5196 via #4655 edges (whole-section lock).
- No storage (read-only tier).

---

# Revision — Design-Gate Correction (full call-site verdict surface)

## Defect addressed
The closed verdict checklist (FR-11 / AC-07) claimed completeness over "10 inputs", but
`build_project_server` carries two more config-relevant call-site inputs the spec omitted:
`permissive` and `instructions`. The checklist's whole purpose (crt-056 AC-1) is to prevent silent
drops — and it had dropped two. Surgical edit, not regeneration.

## What changed

### Functional requirements
- **FR-11 reframed** "Closed 10-input verdict" → "Closed full-call-site verdict": now requires every
  config-relevant call-site input (~12: 9 crt-056 params + `embed_handle` + `permissive` +
  `instructions`) plus `[embedding]` to have an explicit verdict row, re-derived from the live
  signature.
- **FR-14 added** — `server.instructions` PER-SLUG OVERLAYABLE: merged value threaded to each slug's
  ServiceLayer; absent ⇒ global underlies. Replaces today's global-only fan-out (`main.rs:687` →
  `main.rs:1095`). #785 names it a per-slug knob.
- **FR-15 added** — `permissive` GLOBAL-LOCKED: daemon permission posture, passed unconditionally,
  never from a merged config.
- **FR-03 annotated** — clarified instructions is overlayable via FR-14 (separate string arg), not
  one of the 6 crt-056 `Arc`s.

### Verdict checklist table
- Retitled "10-Input" → "Full Call-Site Verdict Checklist".
- Added row **P** (`permissive`, GLOBAL-LOCKED) and row **I** (`instructions`, OVERLAYABLE per-slug).

### Acceptance criteria
- **AC-02 strengthened** — fallthrough arm now asserts `Arc::ptr_eq` (machine-checked same-allocation,
  matching crt-056 AC-2) on the 3 global handles, not just value/byte-equality. Method R + C → R + U.
- **AC-07 amended** — asserts the full surface incl. `permissive` (global-locked) and `instructions`
  (overlayable) rows.
- **AC-10 added** — behavioral: slug A instructions ≠ slug B; global underlies when unset. Verify B + U.

### Documented limitation (non-blocker)
- New "Known Limitation" subsection: a per-slug file setting a GLOBAL-only section (`[server.tls]`,
  transport, `[embedding]`) is silently ignored at the seam — only the `*_sha256` pin warns (AC-05).
  Mitigation: Feature B's annotated seed. Seam-level warn = optional future enhancement (out of scope).

### Housekeeping
- Dropped stale "10-input" language in Objective, NFR-02, A3, Ubiquitous Language; added the
  `Arc::ptr_eq` reference to NFR-02.
- Added `permissive` (process-posture lock) and `server.instructions` (overlayable) to Constraints.
- Added domain terms (`server.instructions` overlay, `permissive` flag); renamed "10-input verdict"
  → "Call-site verdict".

## Verification-method labels
Kept consistent (B / U / C / R). AC-02 moved review-only C → machine-checked U; AC-10 = B + U.

## Open question (for architect)
- Where does the global `server.instructions` value live at the per-slug loop — already a
  global-resolved `UnimatrixConfig` field that `merge_configs` covers, or a separate `instructions`
  arg sourced outside the config struct (`main.rs:687`)? FR-14's "merged config's `server.instructions`"
  assumes the former; if instructions is passed outside the merged `UnimatrixConfig`, FR-14 needs a
  small plumbing change at the seam (route the global instructions through the merged config before
  threading), not just a thread-through. Architect to confirm against the live call shape.

## Knowledge Stewardship (revision)
- Queried: mcp__unimatrix__context_briefing — surfaced #5198 (vnc-040 ADR-002: model invariants and
  byte-for-byte fallthrough hold BY CONSTRUCTION), underpinning the AC-02 `Arc::ptr_eq` strengthening
  and the AC-07 by-construction `permissive` lock. No new patterns stored (read-only tier).
