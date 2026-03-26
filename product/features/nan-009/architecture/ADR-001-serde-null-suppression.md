## ADR-001: Suppress Null Phase Emission in JSONL Output

### Context

`ScenarioContext.phase` is `Option<String>`. The field is absent from all pre-nan-009
scenario JSONL files. When serde serializes `Option::None` with only `#[serde(default)]`,
it emits an explicit `"phase":null` key in every JSONL record — including the entire
pre-existing scenario corpus. This changes the wire shape of every existing scenario file
retroactively, making diffs noisy, growing file sizes, and potentially confusing external
tooling that checks for key presence (SR-01 from the scope risk assessment).

Two options exist:

**Option A — Emit explicit null** (`#[serde(default)]` only): All records gain
`"phase":null`. Simple to read (phase is always present), but breaks the wire shape of
every pre-existing file and adds unnecessary bytes.

**Option B — Suppress null** (`#[serde(default, skip_serializing_if = "Option::is_none")]`):
Records without a phase value are written without the `"phase"` key, identical to
pre-nan-009 files. Records with a phase value include `"phase":"delivery"` etc. The reader
side uses `#[serde(default)]` to tolerate the absent key.

The scope risk assessment (SR-01) explicitly recommends Option B. The precedent is
established by ADR-001 nan-008 (category field) and pattern #3255.

One consequence of Option B is that the report-side `ScenarioResult` in `report/mod.rs`
must use `#[serde(default)]` (no `skip_serializing_if`) because the report module only
deserializes result JSON; it never re-serializes it. The `skip_serializing_if` annotation
is only needed on the writer-side types.

The scope's AC-02 and AC-03 already state that `null` must appear in runner output JSON
when phase is absent. This applies to `ScenarioResult.phase` in `runner/output.rs`, which
is serialized by `write_scenario_result`. Since runner always writes a `ScenarioResult`
with a definite `phase` field (either `Some(...)` or `None`), the writer-side
`ScenarioResult` does NOT use `skip_serializing_if` — it always emits the `phase` key,
including as `null`. The suppression applies only to the scenario JSONL context, not to
the result JSON.

Summary of annotation rules:
- `ScenarioContext.phase` in `types.rs`: `#[serde(default, skip_serializing_if = "Option::is_none")]`
- `ScenarioResult.phase` in `runner/output.rs`: no serde annotation (always emits)
- `ScenarioResult.phase` in `report/mod.rs`: `#[serde(default)]` only

### Decision

Apply `#[serde(default, skip_serializing_if = "Option::is_none")]` to
`ScenarioContext.phase` in `eval/scenarios/types.rs`. This suppresses null emission in
the scenario JSONL output, preserving backward compatibility with existing scenario files
by omitting the key entirely when phase is not set.

The `ScenarioResult.phase` field in `runner/output.rs` carries no `skip_serializing_if`
because result JSON always includes the phase key (consistent presence, even as null,
satisfies AC-02 and AC-03 and makes the field visible to the report reader).

The report module's `ScenarioResult.phase` in `report/mod.rs` carries
`#[serde(default)]` to tolerate both absent keys (pre-nan-009 result files) and explicit
`null` keys (result files from `eval run` on pre-col-028 corpora).

### Consequences

- Pre-nan-009 scenario JSONL files are byte-for-byte identical after re-extraction on a
  pre-col-028 corpus (phase key absent in both old and new output).
- New scenario JSONL files include `"phase":"delivery"` etc. only when the
  `query_log.phase` column is non-null.
- The existing test `ScenarioContext` with null phase serializes without a `phase` key —
  this is now the expected behavior (AC-09).
- AC-02 is satisfied: result JSON always carries the `phase` key.
- The report module correctly deserializes both old result files (no `phase` key) and new
  result files (`"phase":null` or `"phase":"delivery"`).
- Future fields on `ScenarioContext` that are optional and additive should follow the same
  pattern: `#[serde(default, skip_serializing_if = "Option::is_none")]`.
