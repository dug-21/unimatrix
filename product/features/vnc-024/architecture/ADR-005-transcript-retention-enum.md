## ADR-005: transcript_retention Typed as a Purge-Policy Enum on RetentionConfig

### Context
ass-069 Q7 establishes "delete raw transcript on cycle close" as the OSS default of a new retention
knob, and names retention-as-policy the enterprise extension seam (retain-N-days, encrypt-at-rest,
data-residency) that must extend without re-architecture (goal #4710 — "extend, never re-architect").
The knob lands now, in F1, so the contract is frozen before #670 builds the buffer/purge lifecycle
that obeys it (SR-04). `RetentionConfig` (`config.rs:1499`) is the established home: each field has a
`#[serde(default = "...")]` defaulter, a `Default` impl (`:1551`), a range check in `validate()`
(`:1571`), and a per-field project-wins merge arm (`:3307-3329`). The struct derives `PartialEq` —
the merge arm compares with `!=`.

**What `transcript_retention` governs (scope, to prevent confusion).** It governs the **raw session
transcript — ephemeral working state**: the verbatim, possibly-secret-bearing conversation bytes the
client streams as `transcript_delta` (ADR-004). It does **not** govern retention of distilled
knowledge, observations, or the audit log — those are durable, sanitized, non-secret-bearing
artifacts with their own existing retention knobs. Conflating the two is the central footgun this
ADR closes.

The type choice is load-bearing. A bare `u32` day-count cannot encode the event-driven
purge-on-cycle-close default: `0` is ambiguous (delete immediately? never? on close?), and a future
encrypt-at-rest or data-residency policy has no numeric home — forcing an enterprise re-architecture
(A2). The enum is the correct, extensible shape, so it is **kept** as the enterprise seam.

**But `RetainDays(N)` is unsafe to *honor* in the OSS build.** `RetainDays(N)` implies durable
persistence of raw, secret-bearing transcript. That collides with PRODUCT-VISION principle 8 ("no
secrets in any database") and ass-069's in-memory-only decision, and it requires the
encrypt-at-rest / data-residency apparatus that only the enterprise build has. Treating `RetainDays`
as a benign accepted-and-ignored value is itself a footgun: an operator who sets it would believe
durable retention is in effect when OSS silently cannot provide it safely. OSS must therefore
**reject** it, not absorb it.

**No content secret-scanner exists to lean on (write-path reality).** Unimatrix has **no** reusable
content secret-redactor/scanner. Write-path defenses are *structural validation + metadata
sanitization*; content is stored verbatim and trusted (`hook.rs:2348`). Do **not** design any path
that assumes a reusable secret-redactor licenses persisting raw transcript. The architectural control
— accept-and-drop (ADR-004) + in-memory-ephemeral buffering (#670) + purge-on-cycle-close — **is** the
secrets guarantee. A scanner could only ever *supplement* distilled output; it can never replace this
control, and its absence is exactly why `RetainDays` cannot be honored in OSS.

### Decision
Add a `TranscriptRetention` enum and thread it through all four `RetentionConfig` touchpoints:

```rust
#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
pub enum TranscriptRetention {
    PurgeOnCycleClose,      // default — event-driven purge, no day count
    RetainDays(u32),        // enterprise: retain raw transcript N days
}
```

1. **Struct field** (`config.rs:1501`): `#[serde(default = "default_transcript_retention")]
   transcript_retention: TranscriptRetention`. `PartialEq` is mandatory (the merge `!=`).
2. **Defaulter + Default impl** (`:1541-1559`):
   `fn default_transcript_retention() -> TranscriptRetention { TranscriptRetention::PurgeOnCycleClose }`,
   and add `transcript_retention: default_transcript_retention()` to `Default for RetentionConfig`. An
   absent `[retention]` block (`#[serde(default)]` on the struct) still loads to `PurgeOnCycleClose`
   (AC-13).
3. **validate()** (`:1571`): in the OSS build, **reject** `RetainDays(_)` outright with a clear
   "enterprise-only" error (e.g. `ConfigError::RetentionFieldOutOfRange` /
   `EnterpriseOnly` with `field: "transcript_retention"`, message naming `RetainDays` as an
   enterprise-only policy). `PurgeOnCycleClose` is the **only** value OSS accepts and is always
   valid. The enum shape is retained as the enterprise seam, but the *day-count is not
   accepted-and-ignored* — it is a hard validation failure, so an operator cannot believe durable
   retention is in effect when OSS cannot safely provide it. (No range-check arm is needed in OSS,
   because the only path through `RetainDays` is rejection.)
4. **project-wins merge arm** (`:3307-3329`): add
   `transcript_retention: if project.retention.transcript_retention != default.retention.transcript_retention { project... } else { global... }` (AC-14).

The field is **config-only** in F1 — no GC or purge code consumes it; the crt-036 background GC
integration is the re-scoped #670's concern (Constraint 6). A bare `u32` is explicitly rejected in
review (AC-13, SR-04).

### Consequences
**Easier**: The enterprise retain/encrypt/residency seam is in place from F1 — a future policy adds
an enum variant (e.g. `EncryptRetain { days, key_id }`) and its validate/merge handling without
touching the field's type or the #670 buffer's interface. The OSS default is unambiguous and
event-driven. The pattern matches the four existing `RetentionConfig` fields exactly, so the merge
and validation infrastructure is reused, not invented.

**Harder**: The enum carries an enterprise variant that the OSS build deliberately cannot honor; the
"enterprise-only" rejection message must be clear enough that an operator understands *why* their
`RetainDays` config failed (not a generic range error). When the enterprise build lands, it replaces
the rejection arm with real retain/encrypt handling — the seam holds, but the OSS↔enterprise
divergence is now in `validate()` and must be kept in sync with the enum's variants.

**R-10 mostly dissolves.** The TOML-representation open question (how to prettify the tagged form of
`RetainDays`) is now largely moot: the only **live OSS value** is the unit variant
`transcript_retention = "PurgeOnCycleClose"`, which serde's externally-tagged default already renders
as a bare string. `RetainDays` is rejected at validate(), so **no design effort should be spent
prettifying the tagged form of a value OSS rejects.** Confirm only that `"PurgeOnCycleClose"`
deserializes; the `{ RetainDays = N }` TOML shape is an enterprise-build concern, not F1's.

No behavior consumes the field yet, so its correctness rests on config-load/validate/merge tests
(including a test asserting `RetainDays` is **rejected** in OSS) until #670 wires the purge lifecycle.

Cross-references: crt-036 (`RetentionConfig` + background GC seam this extends); ADR-004 (the
`transcript_delta` buffer #670 builds is what this policy governs); ass-069 Q4/Q7.
