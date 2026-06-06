# Pseudocode: config-knob (`infra/config.rs` + ctor wiring in `server.rs`/`main.rs`)

ADR: ADR-006. FRs: FR-10, NFR-04. Risk: R-11.

## Purpose

Add `transcript_buffer_max_bytes` beside `transcript_retention` (one transcript-policy
surface), validate the floor, give it the project-wins merge arm, and inject it into
`SessionRegistry` at the three production construction sites.

## Changes in `infra/config.rs`

### 1. Field on `RetentionConfig` (directly beside `transcript_retention` at `:~1561`)

```
/// Accumulated per-session transcript buffer cap in bytes (vnc-025 ADR-006).
/// Governs the ACCUMULATED in-memory buffer (the 64 KiB client soft cap bounds individual
/// deltas; the 1 MiB frame ceiling bounds individual events). Sibling of
/// transcript_retention — together they form the transcript-policy surface the enterprise
/// seam reads as a unit. No global aggregate cap (Constraint 11, human-accepted): worst
/// case is cap × concurrent sessions; the 4 h sweep_stale_sessions eviction is the backstop.
/// Evidence trigger to revisit: >32 concurrent registered sessions or >256 MiB resident
/// transcript memory.
/// Range: [65_536, usize::MAX]. Default: 4_194_304 (4 MiB).
#[serde(default = "default_transcript_buffer_max_bytes")]
pub transcript_buffer_max_bytes: usize,
```

### 2. Default fn + `Default` impl

```
fn default_transcript_buffer_max_bytes() -> usize { 4_194_304 }

// in impl Default for RetentionConfig:
transcript_buffer_max_bytes: default_transcript_buffer_max_bytes(),
```

(Keep the literal here and `DEFAULT_TRANSCRIPT_BUFFER_MAX_BYTES` in `session_transcript.rs`
equal — pin with a test, or have config reference the module constant; implementer's choice,
one source of truth preferred.)

### 3. `validate()` floor (in `RetentionConfig::validate`, same error pattern as siblings)

```
if self.transcript_buffer_max_bytes < 65_536 {
    return Err(ConfigError::RetentionFieldOutOfRange {
        path: path.to_path_buf(),
        field: "transcript_buffer_max_bytes",
        value: self.transcript_buffer_max_bytes.to_string(),
        reason: "must be >= 65536 (64 KiB — one max client delta)",
    });
}
// Rationale (ADR-006): a cap smaller than one delta makes every merge pathological
// ring-tail churn. Values between 64 KiB and the 12 KB tail window are legal.
```

### 4. Project-wins merge arm (`:~3376`, same per-field pattern as `transcript_retention`)

```
transcript_buffer_max_bytes: if project.retention.transcript_buffer_max_bytes
    != default.retention.transcript_buffer_max_bytes
{
    project.retention.transcript_buffer_max_bytes
} else {
    global.retention.transcript_buffer_max_bytes
},
```

## Construction-Site Wiring (all three production sites — R-11.4 grep gate)

| Site | Change |
|------|--------|
| `main.rs:645` | `SessionRegistry::with_transcript_cap(config.retention.transcript_buffer_max_bytes)` |
| `main.rs:1068` | same |
| `server.rs:335` | test-server ctor: keep `SessionRegistry::new()` (4 MiB default) — this site is the test constructor; main.rs daemon/stdio paths construct their own registry with the cap. If `server.rs:335` is reached by a production path, switch it to `with_transcript_cap` too — verify at implementation time which paths construct here (the brief lists all three sites as switching; follow the brief unless the test-only nature is confirmed in review) |

`SessionRegistry::new()` keeps the 4 MiB default — zero churn across existing test call sites
(ADR-006; ctor defined in registry-wiring.md).

## Error Handling

- Out-of-floor value aborts startup via the existing `validate()` → `ConfigError` path; error
  names the field and reason.
- Absent `[retention]` block / absent field → serde default applies (existing
  `#[serde(default)]` section behavior).

## Key Test Scenarios (R-11)

1. Serde: field absent → 4_194_304.
2. `validate()`: 65_535 rejected with field name in error; 65_536 accepted.
3. Project-wins: project sets non-default → overrides global; project default → global wins
   (mirror the existing `transcript_retention` merge tests).
4. Grep/review gate: all production `SessionRegistry` constructions use `with_transcript_cap`.
5. End-to-end cap chain (the wiring-gap catcher, R-11.5): construct registry with a 128 KiB
   cap from config → register session → stream past 128 KiB → overflow occurs at 128 KiB,
   not 4 MiB.
