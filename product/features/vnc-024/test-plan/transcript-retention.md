# Test Plan — transcript-retention (Deliverable 4)

> Covers AC-13, AC-14. Risks R-09 (touchpoint missed / weak rejection, High), R-10 (TOML repr —
> mostly dissolved, Low), R-11 (PartialEq, Medium). Pseudocode: `pseudocode/transcript-retention.md`.
> File: `infra/config.rs` — `TranscriptRetention` enum + `transcript_retention` field on
> `RetentionConfig` threaded through **all four** touchpoints. **Config-only in F1** — no GC/purge code
> consumes it (that is the re-scoped #670). Correctness rests entirely on config load/validate/merge
> unit tests. **ADR-005: OSS `validate()` REJECTS `RetainDays(N)` with an enterprise-only error — this
> is NOT a range check.**

## Scope of this component

The enum `TranscriptRetention { PurgeOnCycleClose, RetainDays(u32) }` (derives
`Deserialize, Debug, Clone, PartialEq`) and its four `RetentionConfig` touchpoints:
1. struct field with `#[serde(default = "default_transcript_retention")]`
2. defaulter `default_transcript_retention()` + `Default for RetentionConfig`
3. `validate()` — OSS rejects `RetainDays`, accepts `PurgeOnCycleClose`
4. project-wins merge arm (with `!=`)

## R-09 — all four touchpoints exercised (AC-13, AC-14)

The "hidden site" failure mode (#4070/#2730): a literal construction or a merge arm is missed, so
absent-config or merge silently yields the wrong policy. Every touchpoint gets a test.

### Defaulter + Default impl (AC-13)
- **test_retention_absent_block_defaults_purge**: an **absent `[retention]`** section loads →
  `transcript_retention == PurgeOnCycleClose` (exercises both the `#[serde(default)]` defaulter and the
  `Default for RetentionConfig` impl).
- **test_retention_present_but_field_absent_defaults_purge** (Edge): a present `[retention]` block with
  `transcript_retention` **absent** → also `PurgeOnCycleClose`. (Absent-block vs present-but-field-absent
  both load the default.)
- **test_retention_compiled_default_is_purge**: the compiled `Default` value == `PurgeOnCycleClose`.

### validate() — REJECT RetainDays, ACCEPT PurgeOnCycleClose (AC-13, the updated obligation)
- **test_validate_rejects_retaindays_enterprise_only**: `validate()` rejects `RetainDays(N)` for **any
  `N` including `0`** with a clear **enterprise-only** error (e.g. `EnterpriseOnly { field: "transcript_retention" }`
  or a message **naming `RetainDays` as enterprise-only**) — **NOT a generic range/out-of-range error**.
  Test at least `N = 0` and one `N > 0` (e.g. `30`) — both rejected. There is **no range-check arm** in
  OSS; the only path through `RetainDays` is rejection.
- **test_validate_accepts_purge_on_cycle_close**: `validate()` accepts `PurgeOnCycleClose`
  unconditionally (always valid).
- Footgun closed: an operator who sets `RetainDays` is told **why** it failed, rather than believing
  durable retention is silently in effect when OSS has no encrypt-at-rest to honor it safely (principle 8
  / ass-069 in-memory-only).

### project-wins merge (AC-14)
- **test_retention_merge_project_wins**: distinct project + global `transcript_retention` values (project
  sets a non-default value it is *allowed* to set — i.e. `PurgeOnCycleClose` vs a differing global, or
  use whichever pair the merge logic compares) → **project wins** (mirrors the `config.rs:3307`
  per-field project-wins pattern, comparing with `!=`).
- **test_retention_merge_revalidated_rejects_retaindays** (#3905): assert the **merged result is
  re-validated** — a merged config that yields `RetainDays` is **still rejected** post-merge. Per-file
  `validate()` does not cover invariants that only emerge after the project-wins merge (Constraint 10).

### Hidden-site grep (R-09 coverage requirement)
- Reviewer/grep: search for any `RetentionConfig { .. }` **literal construction** missing the new
  `transcript_retention` field (#2730). Every construction site must set it (or rely on `..Default`).

## R-10 — TOML representation (mostly dissolved; AC-13)

Only the residual coverage — do **not** spend effort prettifying the rejected `RetainDays` tagged form.
- **test_purge_on_cycle_close_toml_deserializes**: `transcript_retention = "PurgeOnCycleClose"`
  deserializes to `TranscriptRetention::PurgeOnCycleClose` (serde externally-tagged default renders the
  unit variant as a bare string).
- **test_bare_u32_rejected**: a bare-`u32` (`transcript_retention = 30`) is **rejected**, not silently
  coerced — the enum (not a `u32`) is the only accepted shape (AC-13). This is the deserialization-level
  rejection; a TOML-supplied `RetainDays` that *does* parse is then rejected by `validate()` (R-09
  scenario 2), so no separate tagged-form coverage is required.

## R-11 — PartialEq on TranscriptRetention (AC-13/AC-14)

- **Compile-time**: the merge arm using `!=` builds → the derive is present. (Prefer **derive**, not a
  hand-impl, per #3437.)
- **Equality exercise** (covered transitively by the merge test): a real inequality is compared, e.g.
  `RetainDays(30) != RetainDays(31)`, `PurgeOnCycleClose != RetainDays(0)`, `RetainDays(30) == RetainDays(30)`.
  The merge test must exercise a **real inequality** so a broken `PartialEq` is caught (not an all-equal
  path that compares trivially).
- Reviewer confirms `PartialEq` is **derived** (not hand-rolled).

## R-08 (binding side, cross-ref)

The retention enum **binding** must carry **both** `PurgeOnCycleClose` and `RetainDays(u32)` variants
even though OSS `validate()` rejects `RetainDays` — the enum *shape* is the frozen F2/#670 enterprise
seam. That binding cross-check lives in `contract-fixtures.md` (R-08); noted here only so the two plans
agree the variant is **kept in the type**, **rejected in validate()**.

## Edge cases (from RISK strategy)

- `RetainDays(N)` for **any** `N` (incl. `0`) → rejected with the enterprise-only error (no range check).
- Absent `[retention]` block entirely **vs** present-but-`transcript_retention`-absent → both load
  `PurgeOnCycleClose`.
- Bare `u32` written for `transcript_retention` → config load rejects it.
- Merged config that resolves to `RetainDays` → still rejected after merge (#3905).

## Out of scope for this plan

- The retention enum's **TS binding** completeness (both variants present) → `contract-fixtures.md`
  (R-08). `transcript_retention` is config-only and **not** in the wire bindings set.
- Any GC/purge consumption of the field → re-scoped #670 (Constraint 6); no test asserts purge behavior.

## Self-check
- [ ] All four touchpoints tested: defaulter+Default (absent-block & present-but-absent), validate, merge.
- [ ] validate REJECTS RetainDays(N) for N=0 and N>0 with an **enterprise-only** error (NOT range), ACCEPTS PurgeOnCycleClose.
- [ ] Merge: project-wins AND merged result re-validated so merged RetainDays still rejected (#3905).
- [ ] R-10 residual: "PurgeOnCycleClose" deserializes; bare u32 rejected. No coverage for rejected RetainDays tagged form.
- [ ] R-11: PartialEq derived (not hand-impl); merge test exercises a real inequality.
- [ ] Reviewer/grep: no RetentionConfig literal construction missing the new field (#2730).
