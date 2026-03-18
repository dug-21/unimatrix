## ADR-003: Two-Level Config Merge — Replace, Not Extend

### Context

W0-3 introduces a two-level config hierarchy: global (`~/.unimatrix/config.toml`) and
per-project (`~/.unimatrix/{hash}/config.toml`). SCOPE.md states "per-project values
shadow global values, which shadow compiled defaults." SR-06 flags that the semantics
for list fields are ambiguous: does a per-project `categories` list replace or extend
the global list?

Two strategies exist:

**Option A — Replace (last-wins)**: A per-project config section replaces the entire
corresponding section from the global config. If per-project specifies `[knowledge]`,
all `[knowledge]` fields use per-project values; any not specified fall through to
`KnowledgeConfig::default()` (compiled defaults), not to the global config's values.
This is serde's natural `#[serde(default)]` behavior when deserializing the per-project
file independently.

A true three-level merge (compiled defaults → global → per-project field-by-field)
would require a custom merge function per struct or a reflective approach. List fields
pose the sharpest question: if the global config has `categories = ["outcome", "decision"]`
and the per-project config has `categories = ["incident", "runbook"]`, does the
per-project install get `["incident", "runbook"]` (replace) or
`["outcome", "decision", "incident", "runbook"]` (extend)?

**Option B — Section-level merge, field-level replace**: Parse both files into their
respective structs, then merge field-by-field, letting non-default per-project values
override global values. This is more complex to implement but gives true shadowing
semantics within a section.

**Option C — File-level replace**: If a per-project config file exists, it is the
sole config; the global file is not consulted. This is the simplest merge strategy
but is least useful — operators cannot share global defaults and override only
per-project values.

SR-06 directly says: "scalar fields shadow (last wins), list fields replace (not
append)." This confirms Option A is the intended semantic.

The rationale for replace-not-extend for list fields: list fields in this config
(`categories`, `boosted_categories`, `session_capabilities`) are complete policy
declarations, not additive sets. A per-project override of `categories` means "this
project uses exactly these categories," not "add these to the global set." An append
semantic would make it impossible for a per-project config to shrink the category
set, which is a valid use case (e.g., a read-only project that should accept only
`reference` and `decision` categories).

**Where the merge happens**: The merge runs in the startup path after
`ensure_data_directory()` returns `paths`. It is a pure function of two
`UnimatrixConfig` values. The implementation is:

```rust
fn merge_configs(global: UnimatrixConfig, project: UnimatrixConfig) -> UnimatrixConfig
```

where each sub-struct field replaces the global value if the per-project value is
not the compiled default (detected via `PartialEq` with `Default::default()`), and
the global value is kept otherwise.

A simpler implementation deserializes the global file, then deserializes the
per-project file over the same struct, letting serde's `#[serde(default)]` fill any
missing per-project fields from the struct's `Default` impl (not from the global).
Then a `merge()` function applies only non-default per-project values over global
values. This "default-aware merge" is the chosen approach.

### Decision

**Replace semantics for all field types**: per-project values override global values
field-by-field. A per-project field that is absent (not present in the TOML) falls
through to the global value; a field that is explicitly set overrides the global
value entirely. For list fields, the override replaces the entire list.

Implementation: load global config, load per-project config, run a
`KnowledgeConfig::merge(global, project)` per sub-struct where each field takes
the per-project value if it differs from `KnowledgeConfig::default()`, else
the global value. The merge is implemented sub-struct by sub-struct, not via a
blanket macro, to make the field-level semantics explicit and auditable.

**Merge location**: `load_config(global_dir, project_data_dir) -> Result<UnimatrixConfig>`
in `unimatrix-server/src/infra/config.rs`. Called from both `tokio_main_daemon` and
`tokio_main_stdio`, immediately after `ensure_data_directory()` returns `paths`.

**Validation timing**: Both config files are validated independently before merging.
A violation in either file aborts startup. The merged config is not re-validated
(merge preserves validity: it only picks between two already-validated values).

**`dirs::home_dir()` = None handling**: If `dirs::home_dir()` returns `None`, the
global config path is unresolvable; fall back to compiled defaults with a tracing
warning (do not abort). Per SCOPE assumptions, this is a container/CI concern
handled gracefully.

### Consequences

**Easier:**
- Replace semantics are predictable: operators know that setting a list field in
  per-project config fully controls that field. No "magic append" behavior to debug.
- Validation is straightforward: validate each file independently, then merge.
- The merge function is ~30 lines of explicit per-field code — readable, testable.
- Aligned with serde's natural deserialization model.

**Harder:**
- An operator who wants to add one category to the global list in a per-project
  config must repeat the full global list plus the addition. This is slightly
  verbose but unambiguous.
- The "non-default detection" merge requires `PartialEq` and `Default` on all
  sub-structs. Both are derivable and have no performance cost at startup.
