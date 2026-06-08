# C9 — Config Knobs

**Target source:** `unimatrix-server/src/infra/config.rs` — extend `RetentionConfig`
(beside `transcript_buffer_max_bytes` @ `:1561-1576`)
**Wave:** A/B — the two candidate-cap knobs are Wave A; the two hold knobs are Wave B.
**ADRs:** ADR-005 (per-cycle cap), ADR-008 (hold cap + TTL). **Risks:** R-02, R-15, R-18.
**AC:** AC-02, AC-10, AC-11. **Sequencing:** before C6/C7 (Wave A knobs), before C8 (Wave B knobs).

## Purpose

Add four config knobs to `RetentionConfig` using the SAME `serde(default)` / `validate()` / merge
pattern already used for `transcript_buffer_max_bytes`. No new pattern — extend the existing one.

## New Fields on `RetentionConfig`

```
// Wave A — candidate volume caps (FR-4)
#[serde(default = "default_session_cap_bytes")]
transcript_candidate_session_cap_bytes: usize,   // default 24 KB (24 * 1024) — pinned OQ-3 / FR-4

#[serde(default = "default_cycle_cap_bytes")]
transcript_candidate_cycle_cap_bytes: usize,     // default ~256 KB (ass-070 ~58 KB/6-session envelope)

// Wave A — fallback trigger threshold (ADR-006; RATIFIED as a config knob at Gate 3a)
#[serde(default = "default_fallback_hole_fraction")]
transcript_fallback_hole_fraction: f64,          // default 0.5 — C5/C6 fallback_triggered hole fraction

// Wave B — held-buffer bounds (ADR-008 / SR-01)
#[serde(default = "default_hold_max_sessions")]
transcript_hold_max_sessions: usize,             // default ~64 (held-count ceiling, R-02 memory bound)

#[serde(default = "default_hold_ttl_secs")]
transcript_hold_ttl_secs: u64,                   // default 86400 (24h independent TTL sweep, R-02)
```

Default fns:
```
default_session_cap_bytes()      -> 24 * 1024
default_cycle_cap_bytes()        -> 256 * 1024
default_fallback_hole_fraction() -> 0.5
default_hold_max_sessions()      -> 64
default_hold_ttl_secs()          -> 86400
```

> The fallback hole-fraction threshold (ADR-006, used by C5/C6 `fallback_triggered`) is RATIFIED at
> Gate 3a as a config knob (NOT a compile-time constant): `transcript_fallback_hole_fraction: f64`
> (default 0.5). It merges/`validate()`s with the same pattern as the other knobs (see below).

## `validate()` Extensions (R-18, AC-10)

Extend `RetentionConfig::validate()` (same place that rejects bad `transcript_buffer_max_bytes`):

```
fn validate(self) -> Result<(), ConfigError>:
    ...existing checks...
    if self.transcript_candidate_session_cap_bytes == 0: Err("session cap must be > 0")
    if self.transcript_candidate_cycle_cap_bytes == 0:   Err("cycle cap must be > 0")
    // sane ordering: a single session cannot exceed the cycle aggregate (warn or clamp; pin in delivery)
    if self.transcript_candidate_session_cap_bytes > self.transcript_candidate_cycle_cap_bytes:
        Err("session cap must not exceed cycle cap")
    // hole fraction is a ratio in [0.0, 1.0]
    if !(0.0..=1.0).contains(&self.transcript_fallback_hole_fraction):
        Err("fallback hole fraction must be in [0.0, 1.0]")
    if self.transcript_hold_max_sessions == 0:  Err("hold max sessions must be > 0")
    if self.transcript_hold_ttl_secs == 0:      Err("hold ttl must be > 0")
    // AC-10 enterprise seam: RetainDays is OSS-unreachable — REJECT it at validate()
    if matches!(self.transcript_retention, RetainDays(_)):
        Err("RetainDays retention is not supported in OSS")    // makes the C7 RetainDays arm dead
    Ok(())
```

The `RetainDays` rejection is what makes C7's `RetainDays(_)` arm structurally unreachable in OSS
(AC-10 / R-18) while the match stays exhaustive.

## Merge Pattern

Follow the existing `RetentionConfig` merge (env / file / defaults precedence) used for
`transcript_buffer_max_bytes`. The four new knobs merge identically — no special-casing.

## Data Flow

- **Consumers:** C6 (`transcript_candidate_session_cap_bytes` → C3/C5; `transcript_candidate_cycle_cap_bytes`
  → per-cycle aggregate cap; `hole_fraction` → fallback trigger), C7 (`transcript_retention`),
  C8 (`transcript_hold_max_sessions`, `transcript_hold_ttl_secs`).

## Error Handling

`validate()` returns `Err` on out-of-range / unsupported config (rejected at startup). No runtime panic.

## Key Test Scenarios

- Defaults applied when absent (serde default): 24 KB session, 256 KB cycle, 64 sessions, 86400s TTL.
- AC-10 / R-18: `RetainDays` config rejected at `validate()`.
- `validate()` rejects zero/invalid caps and TTL; rejects session cap > cycle cap.
- AC-02: both candidate caps independently tunable and enforced (feeds C3/C6 cap tests).
- AC-11: hold cap + TTL knobs drive the cap-eviction and TTL-sweep tests (R-02 boundary values).
