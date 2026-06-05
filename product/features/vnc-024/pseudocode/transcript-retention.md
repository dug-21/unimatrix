# Component 5 — transcript_retention Enum (Deliverable 4)

> ADR-005. `TranscriptRetention` enum on `RetentionConfig`, threaded through ALL FOUR touchpoints.
> **OSS `validate()` REJECTS `RetainDays(N)` as enterprise-only** (not accepted-and-ignored); only
> `PurgeOnCycleClose` is accepted. The enum shape is the load-bearing enterprise seam; a bare `u32`
> is rejected. Config-only in F1 — no GC consumes it. Merged config is re-validated (Constraint 10).

## Purpose

Freeze the raw-transcript retention knob now so #670's purge lifecycle inherits it. The field governs
the **raw session transcript (ephemeral working state)** — the verbatim, possibly-secret-bearing
`transcript_delta` bytes — NOT distilled knowledge, observations, or the audit log (those have their
own knobs). OSS cannot durably persist raw secret-bearing transcript (no encrypt-at-rest; principle
8 / ass-069 in-memory-only), so `RetainDays` is a hard validation failure, not a silent accept.

## Files (all in `crates/unimatrix-server/src/infra/config.rs`)

| Touchpoint | Anchor | Change |
|------------|--------|--------|
| Enum def | near :1490 (before `RetentionConfig`) | new `TranscriptRetention` enum |
| Struct field | :1501 (`RetentionConfig`) | add `transcript_retention` field |
| Defaulter | :1541-1549 (with the other `default_*` fns) | `default_transcript_retention()` |
| `Default` impl | :1551-1559 | add `transcript_retention: default_transcript_retention()` |
| `validate()` arm | :1571-1602 (before `Ok(())`) | reject `RetainDays`, accept `PurgeOnCycleClose` |
| project-wins merge | :3307-3329 (`retention:` block) | add `transcript_retention` merge arm |

## Enum definition (near :1490)

```
/// Retention policy for the RAW SESSION TRANSCRIPT — ephemeral working state: the verbatim,
/// possibly-secret-bearing conversation bytes streamed as transcript_delta (vnc-024 ADR-004/ADR-005).
/// Does NOT govern distilled knowledge, observations, or the audit log (those have their own knobs).
#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
pub enum TranscriptRetention {
    /// Default — event-driven purge on cycle/session close. The ONLY value OSS honors.
    PurgeOnCycleClose,
    /// Enterprise seam — retain raw transcript N days. REJECTED by OSS validate() (enterprise-only:
    /// durable secret-bearing persistence needs encrypt-at-rest/residency the OSS build lacks).
    RetainDays(u32),
}
```
- `PartialEq` is **mandatory** — the merge arm compares with `!=` (R-11). Derive it, do not hand-roll
  (#3437).
- `Debug, Clone` match the existing `RetentionConfig` field conventions.
- Externally-tagged serde default: `transcript_retention = "PurgeOnCycleClose"` deserializes to the
  unit variant. The tagged `{ RetainDays = N }` TOML form is an enterprise-build concern — do NOT
  spend effort prettifying a value OSS rejects (R-10, mostly dissolved). A bare `u32`
  (`transcript_retention = 30`) fails to deserialize into the enum → rejected (AC-13 / R-10).

## Struct field (:1501, inside `RetentionConfig`)

```
/// Raw-transcript retention policy (vnc-024). OSS honors only PurgeOnCycleClose; RetainDays is
/// rejected by validate() as enterprise-only. Default: PurgeOnCycleClose.
#[serde(default = "default_transcript_retention")]
pub transcript_retention: TranscriptRetention,
```
`RetentionConfig` already has `#[serde(default)]` at the struct level (:1500), so an absent
`[retention]` block loads every field to its defaulter — including `PurgeOnCycleClose` (AC-13).

## Defaulter (:1541-1549, alongside the existing `default_*` fns)

```
fn default_transcript_retention() -> TranscriptRetention {
    TranscriptRetention::PurgeOnCycleClose
}
```

## Default impl (:1551-1559)

```
impl Default for RetentionConfig {
    fn default() -> Self {
        RetentionConfig {
            activity_detail_retention_cycles: default_activity_detail_retention_cycles(),
            audit_log_retention_days: default_audit_log_retention_days(),
            max_cycles_per_tick: default_max_cycles_per_tick(),
            transcript_retention: default_transcript_retention(),   // ADD — R-09 "hidden site"
        }
    }
}
```
> R-09 / #2730: grep for every `RetentionConfig { .. }` literal construction; each must include the
> new field or fail to compile. The compiler enforces this for non-`..` literals — rely on it.

## validate() arm (:1571-1602, before the closing `Ok(())`)

OSS REJECTS `RetainDays(_)` for ANY `N` (including 0) — no range check; rejection is the only path
through the variant (ADR-005). `PurgeOnCycleClose` is always valid.

```
// existing range checks for the three numeric fields stay UNCHANGED ...

// ===== vnc-024 ADR-005: OSS rejects RetainDays as enterprise-only =====
MATCH self.transcript_retention:
    TranscriptRetention::PurgeOnCycleClose -> {}   // always valid in OSS
    TranscriptRetention::RetainDays(_) ->
        RETURN Err(ConfigError::RetentionFieldOutOfRange {
            path: path.to_path_buf(),
            field: "transcript_retention",
            value: "RetainDays",
            reason: "RetainDays is an enterprise-only policy; the OSS build cannot durably retain \
                     raw transcript (no encrypt-at-rest). Use PurgeOnCycleClose.",
        })
// ======================================================================
Ok(())
```
- Reuse the existing `ConfigError::RetentionFieldOutOfRange { path, field, value, reason }` variant
  (ARCHITECTURE Integration Surface) — no new error type needed. The `reason` MUST name `RetainDays`
  as enterprise-only, NOT a generic range error (R-09 scenario 2 / failure-mode table), so an
  operator understands *why* it failed rather than believing durable retention is silently in effect.
- If delivery prefers a dedicated `ConfigError::EnterpriseOnly { field }` variant, that also
  satisfies AC-13 provided the message names `RetainDays` as enterprise-only. Either is acceptable;
  reusing `RetentionFieldOutOfRange` avoids a new error variant.

## project-wins merge arm (:3307-3329, inside the `retention: RetentionConfig { ... }` block)

```
retention: RetentionConfig {
    activity_detail_retention_cycles: <existing project-wins arm>,
    audit_log_retention_days:         <existing project-wins arm>,
    max_cycles_per_tick:              <existing project-wins arm>,
    // ===== vnc-024 ADR-005: per-field project-wins merge (R-09) =====
    transcript_retention: if project.retention.transcript_retention
                             != default.retention.transcript_retention {
        project.retention.transcript_retention.clone()
    } else {
        global.retention.transcript_retention.clone()
    },
},
```
- `!=` requires the derived `PartialEq` (R-11); `.clone()` requires the derived `Clone`.
- Mirrors the three existing retention merge arms exactly (pattern reuse, not invention).

## Merge re-validation (Constraint 10 / #3905) — R-09 scenario 3

Per-file `validate()` does not cover invariants that emerge only after the project-wins merge. The
merged `RetentionConfig` (or the merged top-level config) MUST be re-validated so a merged
`RetainDays` is still rejected.

```
// wherever the project-wins merge result is finalized:
LET merged = merge_project_wins(global, project, default)
merged.retention.validate(path)?        // re-validate — a merged RetainDays is still rejected
```
> Delivery confirms the existing call site that validates post-merge config; if none re-validates
> `retention`, add the call. This is the #3905 lesson: validate the MERGED result, not just each file.

## Data flow

```
Load:     absent [retention]  ─► #[serde(default)] ─► default_transcript_retention() ─► PurgeOnCycleClose
Validate: startup validate()  ─► PurgeOnCycleClose => Ok ; RetainDays(_) => Err(enterprise-only)
Merge:    (global, project)   ─► project-wins arm picks transcript_retention
Re-val:   merged config       ─► validate() again ─► merged RetainDays still rejected
```
No GC/purge code consumes the field in F1 (Constraint 6). Not in the wire bindings set (config-only).

## State machine

None. Pure config value with load → validate → merge → re-validate lifecycle (above).

## Error handling

| Condition | Behavior |
|-----------|----------|
| Absent `[retention]` / present-but-field-absent | loads `PurgeOnCycleClose` (defaulter) |
| `transcript_retention = "PurgeOnCycleClose"` | deserializes to unit variant; `validate()` Ok |
| `RetainDays(N)` for any N (incl. 0) | `validate()` Err — enterprise-only, naming `RetainDays` |
| bare `u32` (`transcript_retention = 30`) | serde deserialize fails → rejected (enum is the only shape) |
| merged `RetainDays` (one source sets it) | merge picks it, re-validation rejects it (#3905) |

## Constraints honored

- **Enum, not `u32`** (Constraint 6 / AC-13): bare `u32` rejected; enum is the enterprise seam.
- **OSS rejects `RetainDays`** (ADR-005): hard validation failure, not accept-and-ignore.
- **All four touchpoints** (R-09): field + defaulter + `Default` impl + `validate()` + merge (and
  re-validation). A missed site is the #4070/#2730 failure mode — the compiler catches the literal
  construction; the tests catch the validate/merge arms.
- **No secret-scanner reliance** (Constraint 9): `RetainDays` rejection IS part of the secrets
  posture — no path may assume a redactor licenses durable raw-transcript retention.

## Key test scenarios (hints — full plan in test-plan/transcript-retention.md)

- **AC-13 default**: absent `[retention]` (and present-but-field-absent) → `PurgeOnCycleClose`.
- **AC-13 reject (R-09)**: `validate()` rejects `RetainDays(N)` (test N=0 and N>0) with an
  enterprise-only error naming `RetainDays` — NOT a generic range error; accepts `PurgeOnCycleClose`.
- **AC-13 shape (R-10)**: `"PurgeOnCycleClose"` deserializes to the unit variant; bare `u32` rejected.
- **AC-14 merge (R-09/R-11)**: project sets `PurgeOnCycleClose`-vs-a-different value, global differs →
  project wins; the `!=` arm exercises a real inequality (proves `PartialEq`).
- **Re-validation (#3905)**: a merged `RetainDays` is still rejected after merge.
- **Literal-construction grep (R-09/#2730)**: no `RetentionConfig { .. }` literal omits the new field.

## Open questions / gaps

- **validate() error variant choice**: reuse `ConfigError::RetentionFieldOutOfRange` (no new
  variant) vs add `ConfigError::EnterpriseOnly { field }`. Both satisfy AC-13 if the message names
  `RetainDays` as enterprise-only. Delivery picks one; pseudocode uses the reuse path. Non-blocking.
- **Post-merge re-validation call site**: delivery confirms the existing finalize point that calls
  `retention.validate()`; if absent, add it (Constraint 10). Flagged, not assumed.
