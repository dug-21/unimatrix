# Agent Report — crt-057-agent-2-spec-v3 (Specification, v3 major rework)

**Task:** Major rework of crt-057 SPECIFICATION after the ass-091 redesign. Edit in place.
**Output:** `product/features/crt-057/specification/SPECIFICATION.md` (full replacement).
**GH:** posted spec-rework summary as a comment on #894.

## What changed

Full replacement of the boolean-era spec to the reworked SCOPE + ADR-001..ADR-006 (ass-091 design).

- Three orthogonal, non-destructive axes: `format` (render-only, `"summary"` dropped), `force` (durable
  recompute), `transcript: { phase?, anchor?, match?, window? }` (read-only, AND-composed, `snapshot()`, no new
  reader). Omit = summary only; `transcript:{}` ≡ `match:".*"` full dump under the existing cap.
- Fully non-destructive review — no purge verb. Eager review-purge removed from all four success returns;
  reclamation entirely on unchanged 24h-TTL / 64-cap / session-close backstops. Content-opaque fold read (#5030)
  stays as the sole review-seam side-effect, gated ×4.
- Loss propagation: per-session `matched` / `search_complete` / `elided_bytes` / `provenance`; INDETERMINATE
  no-match; anchor/phase defining bounds; `byte_offset` fallback for `ts:None`; `match` never a bare boolean.
- Clock normalization promoted to first-class interface + correctness AC.
- Window default ±120 000 ms / ±3 candidate blocks (AC-18, net-new).
- Ownership boundary (NG-5) asserted as FR-25 + negative AC-19.

## AC list (new / changed / removed)

- **New:** AC-18 (window default), AC-19 (ownership boundary — negative).
- **Repurposed/aligned:** SPEC AC-01..AC-17 aligned 1:1 with reworked SCOPE AC-01..AC-17, each with a
  verification method; AC-03 now "fully non-destructive / no purge verb"; AC-06 loss propagation / INDETERMINATE;
  AC-08 clock normalization; AC-12 four-site now gates only the fold read + memo-hit transcript parity.
- **Removed (boolean-era):** `include_transcript_candidates` FRs, purge-iff-flag (old AC-04), extractive/
  non-extractive states, force-vs-extract precedence (old FR-14), one-shot degraded-post-purge (old AC-06),
  honest-capture advisory (old AC-12), refuse-vs-warn premature-extraction WARN (old AC-13), old AC-19/AC-20
  (folded into AC-03/AC-17 + NFR-8).

Renumbering recorded in the SPEC closing block "FR/AC Renumbering & Retirement".

## Conflicts vs architecture

None. The spec conforms to ADR-001..ADR-006 and ARCHITECTURE §1–§12 exactly.

## Open questions surfaced (all deferred, non-blocking)

- OQ-1 live regex hit-rate / `ts:None` fraction (delivery-time experiment).
- OQ-2 anchor/phase caller-facing id representation (pseudocode detail).
- OQ-3 `Window` type shape + `r#match` serde-rename (pseudocode detail).
- OQ-4 `distill_before_purge` rename (delivery decision).

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` (crt-057) — surfaced #5434/#5438/#5435/#5436 (crt-057 ADR-002/006/
  003/004), #4848 (single content reader), #4850 (assembly-level attach), #5031 (crt-054 survival), #5037/#5042/
  #5051 (crt-055 fold), #5063/#5066 (phase bounds / aggregate wiring). Read reworked SCOPE + ADR-001..ADR-006 +
  ass-091 FINDINGS.
- Stored: nothing — specification is read-only tier; spec decisions feature-specific.
