## ADR-003: Phase Is a Soft Vocabulary Key — Governed by query_log Migration, Not Schema

### Context

The eval harness reads `query_log.phase` as a free-form `Option<String>`. The column type
is `TEXT` (not an enum type in the schema). The known values at time of writing are
`"design"`, `"delivery"`, and `"bugfix"` — the three session types defined by the project
protocol (`context_cycle` records the active session type at cycle start).

The critical governance question: when the vocabulary changes (a new session type is
introduced, or an existing type is renamed), what is the change mechanism?

Two options:
- **Schema change**: Introduce a database enum type or CHECK constraint on `query_log.phase`.
  All vocabulary additions require a schema migration.
- **Data migration**: The column stays `TEXT`. New values appear naturally in new rows as
  the protocol evolves. Retroactive labeling of old rows (if desired) is a `query_log` data
  migration — an UPDATE statement, not a schema change.

The question is whether the harness should validate phase values against a fixed set or
accept arbitrary strings.

Three options exist:

**Option A — Enum validation**: Define a `Phase` enum (`Design`, `Delivery`, `Bugfix`,
`Unknown`) in `eval/scenarios/types.rs`, parse `query_log.phase` strings into it during
extraction, and fail or warn on unrecognized values. Provides compile-time exhaustiveness
on match arms.

Drawback: a new session type (e.g., `"research"` for a future Assimilate session) requires
a code change in the eval harness before it can appear in the per-phase report. The harness
becomes a bottleneck for protocol evolution. Enum validation is appropriate for data that
the harness controls; it is not appropriate for data that originates from a separately-
evolving protocol layer.

**Option B — Free-form strings with no documentation constraint**: Accept any string,
display it verbatim, document nothing about the vocabulary. Easy to implement and future-
proof, but makes the report opaque to readers who don't know what values to expect.

**Option C — Free-form strings with closed documentation**: Accept any string at the
harness level (no validation), but document the known vocabulary explicitly in
`docs/testing/eval-harness.md`. State that the vocabulary is protocol-defined and may
evolve as session types are added; new values appear in the per-phase table automatically.
Treat `"(unset)"` as the display label for `None` — not a legal `query_log.phase` value.

This is the approach recommended by SCOPE.md RD-03 (Non-Goal: "No phase enum validation")
and is consistent with the design principle that the harness measures what the retrieval
pipeline does, not what the protocol layer enforces.

**Null label disambiguation (SR-04 resolution)**: SCOPE.md contains a wording conflict:
Goals §5 and AC-04/AC-05 say `"(none)"`, while Constraint 5 says `"(unset)"`. This ADR
resolves the conflict: `"(unset)"` is adopted as the canonical label. It is more precise
— `"(unset)"` communicates that the field exists but was not populated, whereas `"(none)"`
is ambiguous with a value that happened to be the string `"none"`. The implementation
must use `"(unset)"` uniformly across `compute_phase_stats`, `render_phase_section`, and
documentation.

### Decision

`query_log.phase` is a **soft vocabulary key**: the column is `TEXT` with no schema-level
enum or CHECK constraint. The eval harness does not validate values against a fixed set.
Phase values flow through the pipeline as `Option<String>`. The harness records and
displays whatever the `query_log` contains.

Vocabulary governance is **migration-based, not schema-based**:
- A new session type is introduced by using a new phase string in `context_cycle` calls.
  New rows carry the new value; the harness displays it automatically — no code change.
- Retroactive relabeling of old rows (e.g., renaming a session type) is a `query_log`
  data migration (`UPDATE query_log SET phase = 'new' WHERE phase = 'old'`), not a
  schema migration.
- No schema migration, no harness code change, and no ADR update is required when the
  vocabulary evolves.

The known vocabulary (`design`, `delivery`, `bugfix`) is documented in
`docs/testing/eval-harness.md` as the current set at the time of writing — not as a
fixed allowlist. Documentation states explicitly that new values appear automatically
and that retroactive labeling uses a data migration.

The display label for `None` phase values is `"(unset)"` in all code and documentation.
The `"(unset)"` group is sorted last in the per-phase table.

### Consequences

- A new session type (e.g., `"research"`) appears automatically in the eval report without
  any harness code change.
- Renaming a session type requires a `query_log` data migration to relabel historical rows.
  Without migration, old rows retain the original label and appear as a separate group in
  the report — which is informative, not catastrophic.
- The harness cannot detect a typo in `query_log.phase` (e.g., `"delivvery"` appears as
  its own row). This is acceptable: the harness is a measurement instrument, not a
  validator. `query_log.phase` is set by `context_cycle`, which follows the protocol.
- The `"(unset)"` label cannot collide with a real phase value (real values do not use
  parentheses).
- If phase validation or normalization is ever needed, it belongs at the `context_cycle`
  layer (where the value is written), not in the eval harness.
- Documentation in `eval-harness.md` must be updated when the vocabulary evolves; this
  is a documentation maintenance task, not an enforcement mechanism.
- The `compute_phase_stats` sort logic must implement: alphabetical ascending for named
  phases, `"(unset)"` unconditionally last (not alphabetically — it would sort before
  most strings due to the `(` character).
