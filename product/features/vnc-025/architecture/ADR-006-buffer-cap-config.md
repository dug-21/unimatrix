## ADR-006: `transcript_buffer_max_bytes` Lives on `RetentionConfig`, Injected into `SessionRegistry` at Construction

### Context

Resolved decision 1 fixed the value (4 MiB default, ring-tail) and the placement intent
(beside `transcript_retention` — the two halves of one transcript-policy surface the
enterprise seam reads as a unit). What remains architectural is the plumbing: `SessionRegistry`
is constructed with no config in three production sites (`server.rs:335`,
`main.rs:645/:1068`) and dozens of test sites call `SessionRegistry::new()`; the buffer (in
`infra/session_transcript.rs`) needs the cap at merge time, and threading config through every
`apply_transcript_delta` call would smear a policy value across dispatch code.

### Decision

- New field on `RetentionConfig` (`infra/config.rs`, directly beside `transcript_retention`
  at `:1561`): `pub transcript_buffer_max_bytes: usize`, serde default `4_194_304` (4 MiB),
  documented as governing the *accumulated* per-session buffer (the 64 KiB client cap bounds
  individual deltas; the 1 MiB frame ceiling bounds individual events).
- `validate()` rejects values `< 65_536` (64 KiB — one max client delta): a cap smaller than
  one delta makes every merge a pathological ring-tail churn; values between 64 KiB and the
  12 KB tail window are legal (PreCompact still works).
- Cap injection at construction: `SessionRegistry::with_transcript_cap(max_bytes: usize)`;
  the registry stores it and passes it to each `TranscriptBuffer` created in
  `register_session`. `SessionRegistry::new()` keeps the 4 MiB default — zero churn across
  existing test call sites. The three production sites switch to
  `with_transcript_cap(cfg.retention.transcript_buffer_max_bytes)`.
- The project-wins merge arm (`config.rs:3376`, vnc-024 ADR-005 pattern) gains the matching
  per-field arm for the new knob.
- Aggregate posture (Constraint 11, SR-06 — human-accepted): no global cap. Worst case is
  cap × concurrent sessions (tens of MiB at personal-cloud scale); the 4 h
  `sweep_stale_sessions` eviction is the backstop. Evidence trigger to revisit: sustained
  >32 concurrent registered sessions or observed resident transcript memory >256 MiB.

### Consequences

- Easier: policy reads as one config block (retention + cap) — the enterprise seam audits one
  section; tests need no config plumbing; the cap is immutable per registry lifetime (no
  mid-session resize semantics to define).
- Harder: changing the cap requires a server restart — acceptable, config is read at boot
  everywhere else too.
- Cross-references: ADR-002 (how the cap is enforced), ADR-004 (the retention enum gate),
  vnc-024 ADR-005 / #4721 (placement precedent and project-wins merge).
