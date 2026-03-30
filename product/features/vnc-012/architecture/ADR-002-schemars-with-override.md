## ADR-002: Preserve integer JSON Schema via #[schemars(with = "T")]

### Context

When a struct field uses `#[serde(deserialize_with = "fn")]`, schemars can no
longer infer the field's JSON Schema type — it falls back to an empty schema
(`{}`), which would change the advertised MCP tool schema and break
AC-10 (all affected fields must retain `type: integer` in the published schema).

Two schemars 1.x override mechanisms were evaluated:

- `#[schemars(schema_with = "fn_name")]` — calls a function
  `fn(&mut SchemaGenerator) -> Schema`. Required when the desired schema differs
  from any existing Rust type's schema. More verbose; requires a named function
  per field type.
- `#[schemars(with = "T")]` — tells schemars to generate the schema as if the
  field were type `T`. No function needed. `#[schemars(with = "i64")]` produces
  `{"type": "integer"}` with no additional constraints, identical to the
  baseline schema for a plain `i64` field. Verified against schemars 1.2.1
  source.

For `Option<i64>` fields, `#[schemars(with = "Option<i64>")]` emits the
nullable integer schema. For `evidence_limit: Option<usize>`,
`#[schemars(with = "Option<u64>")]` emits a non-negative integer schema
(adds `minimum: 0`), which is the correct semantic constraint for a count
field. This is a tighter schema than the unadorned `Option<usize>` baseline —
the scope explicitly notes this as acceptable (SR-01: `Option<u64>` adding
`minimum: 0` is expected for a count field, not a regression).

### Decision

Annotate each of the nine affected fields with the paired attribute:

| Field | serde attribute | schemars attribute |
|-------|----------------|--------------------|
| `GetParams.id` (i64) | `deserialize_with = "serde_util::deserialize_i64_or_string"` | `#[schemars(with = "i64")]` |
| `DeprecateParams.id` (i64) | same | same |
| `QuarantineParams.id` (i64) | same | same |
| `CorrectParams.original_id` (i64) | same | same |
| `LookupParams.id` (Option<i64>) | `deserialize_with = "serde_util::deserialize_opt_i64_or_string"` | `#[schemars(with = "Option<i64>")]` |
| `LookupParams.limit` (Option<i64>) | same | same |
| `SearchParams.k` (Option<i64>) | same | same |
| `BriefingParams.max_tokens` (Option<i64>) | same | same |
| `RetrospectiveParams.evidence_limit` (Option<usize>) | `deserialize_with = "serde_util::deserialize_opt_usize_or_string"` | `#[schemars(with = "Option<u64>")]` |

The schema snapshot test (see ADR-003) asserts `type: integer` appears for all
nine fields in the published tool list, locking in the AC-10 contract.

### Consequences

Easier:
- No new named functions required for schema generation — `with = "T"` is
  declarative and self-documenting at the call site.
- Schema output is identical to the pre-feature baseline for eight of the nine
  fields; `evidence_limit` gains a `minimum: 0` constraint that was semantically
  implied but not previously enforced.

Harder:
- The `#[schemars(with = "T")]` syntax is not validated by the Rust type system
  — a typo in `"Option<i64>"` silently generates an incorrect schema. The
  schema snapshot test (ADR-003) is the only guard against this.
- schemars 1.x is pinned at `"1"` (resolved to 1.2.1); a future schemars
  minor version that changes how `with = "T"` is resolved could alter the
  schema output. The snapshot test catches any such change.
