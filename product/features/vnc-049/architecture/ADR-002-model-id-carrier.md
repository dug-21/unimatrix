## ADR-002: `model_id` carrier — new wire field + persisted column, plumbed plugin→ImplantEvent→observations

### Context
AC-06 (C18's raison d'être) requires local-model activity (e.g. `ollama`/`qwen3-coder`) to be stored
and **queryable as distinct** from a cloud-model event, through the real assembled path (SR-01, SR-10).
`ImplantEvent` carries `provider` but **no model field** (`wire.rs:251`), and the `observations` table
has no model column. OpenCode exposes the real backend `model:{providerID, modelID}` in-process on
`chat.message`/`chat.params`. SR-01 warns that "a model-carrier field is populated" is a tautology if
the local event never distinctly **lands and becomes queryable** via the assembled path.

### Decision
Plumb a first-class `model_id` from the plugin to a queryable column:

1. `HookInput.model_id: Option<String>` and `ImplantEvent.model_id: Option<String>`, both
   `#[serde(default, skip_serializing_if = "Option::is_none")]` (mirroring the existing `provider`
   field, `wire.rs:276`). Value format: `"<providerID>/<modelID>"`, e.g. `"ollama/qwen3-coder"`.
2. C1 plugin resolves in-process `model:{providerID, modelID}` and passes it as a new
   `--model <providerID>/<modelID>` CLI arg on the `Hook` subcommand; `hook::run()` populates
   `HookInput.model_id`, carried into `ImplantEvent.model_id` at every construction site.
3. Add nullable `model_id TEXT` to `observations` (same migration as ADR-001's `source_domain`).
   `listener.rs` insert sites bind it; read paths (`parse_observation_rows`, store SELECTs) surface it.
4. `source_domain` stays `"opencode"` (ADR-001); model distinctness is a **separate queryable field**,
   not a source_domain explosion (overloading `source_domain` per model would break its
   `^[a-z0-9_-]{1,64}$` domain-pack semantics).
5. Split-brain + lesson #5670: the `model_id` frame field must land at (a) Rust hook.rs extract+insert,
   (b) `normalize.js` port, (c) a `parity_corpus_uds.rs` case, (d) one test crossing JS-frame →
   listener → DB, asserting a local-model row is queryable distinct from a cloud-model row (SR-10).

### Consequences
Easier: AC-06 becomes a real end-path assertion (local `ollama/qwen3-coder` row queryable vs a
cloud-model row); enables C14 multi-LLM parity later; rides ADR-001's migration and plumbing seam at
marginal cost. Harder: another wire field = another split-brain surface (mitigated by the 4-layer
landing rule); ts-rs bindings regenerate and the drift gate (vnc-024 #4726) must pass; every
`ImplantEvent` construction site must set `model_id` (missed sites → NULL, canaried like provider).
Cross-references ADR-001 (shared migration/persistence) and ADR-003 (this is the E2 leg sized IN).
