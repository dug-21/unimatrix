## ADR-002: Pre-Resolve Histogram in Handler vs. Arc<SessionRegistry> on SearchService

Feature: crt-026 (WA-2 Session Context Enrichment)

### Context

`SearchService::search()` needs access to the session category histogram to compute
`phase_histogram_norm` for each candidate. There are two structural options:

**Option A — Add `Arc<SessionRegistry>` as a field on `SearchService`**:
`SearchService` holds the registry and calls `get_category_histogram(session_id)` inside
`search()` when `params.session_id` is `Some`. The handler only passes `session_id` via
`ServiceSearchParams`.

**Option B — Pre-resolve histogram in the tool handler**:
The tool handler (`context_search` in `mcp/tools.rs` and `handle_context_search` in
`uds/listener.rs`) calls `session_registry.get_category_histogram(sid)` synchronously
before constructing `ServiceSearchParams`. The resolved `HashMap<String, u32>` (or `None`
for empty/absent) is carried in `ServiceSearchParams.category_histogram`.
`SearchService` has no `SessionRegistry` dependency.

**Pattern reference**: crt-025 ADR-001 SR-07 established the pre-snapshot pattern: session
state is read synchronously before any `await` points to eliminate races with concurrent
session mutations. The histogram pre-resolution follows the same pattern.

**`SearchService` dependency surface**: Currently `SearchService` depends on `Store`,
`AsyncVectorStore`, `EmbedServiceHandle`, `AdaptationService`, `SecurityGateway`,
`ConfidenceStateHandle`, `EffectivenessStateHandle`, `TypedGraphStateHandle`, `RayonPool`,
`NliServiceHandle`. Adding `Arc<SessionRegistry>` would introduce an additional
infrastructure dependency, creating a coupling between the search ranking pipeline and
session lifecycle management.

**UDS path**: In `handle_context_search` (uds/listener.rs), `SessionRegistry` is already
passed as a direct reference (`&SessionRegistry`), not as an `Arc`. Pre-resolution works
identically on the UDS path without any wiring change to `SearchService`.

**WA-4a forward-compatibility risk (SR-07)**: WA-4a (proactive injection) resolves
candidates without a user query — the session context IS the retrieval anchor. WA-4a's
invocation path may not have a handler on the call stack that can pre-resolve the histogram
before constructing `ServiceSearchParams`. If this is the case, WA-4a will need
`Arc<SessionRegistry>` on `SearchService`, reopening this decision. This is a
forward-compatibility flag only; no code change is required in crt-026.

Unimatrix entry #3157 documents this decision context.

### Decision

Pre-resolve the histogram in the tool handler before constructing `ServiceSearchParams`
(Option B).

Concretely:
- `ServiceSearchParams` gains `session_id: Option<String>` and
  `category_histogram: Option<HashMap<String, u32>>`
- The `context_search` MCP handler and `handle_context_search` UDS handler each call
  `session_registry.get_category_histogram(sid)` synchronously, mapping an empty result to
  `None`
- `SearchService` receives the resolved data as plain struct fields; it holds no reference
  to `SessionRegistry`
- The pre-resolution must occur before any `await` point in the handler (SR-07 pattern)

**Forward-compatibility note**: If WA-4a (proactive injection) cannot use a handler-level
pre-resolution because it initiates retrieval without a tool call context, WA-4a must
supersede this ADR and add `Arc<SessionRegistry>` to `SearchService` at that time.

### Consequences

**Easier**:
- `SearchService` remains dependency-free of session infrastructure. It receives plain data
  and applies scoring logic — pure service, no lifecycle coupling.
- The pre-snapshot is race-free by construction: the histogram clone is taken once before
  any concurrent session mutation can occur during the search pipeline's async awaits.
- The UDS path (`handle_context_search`) already has `&SessionRegistry` in scope; no
  additional wiring is needed.
- Unit tests for `SearchService` can construct `ServiceSearchParams` with a direct
  `HashMap` — no mock `SessionRegistry` needed.

**Harder**:
- `ServiceSearchParams` grows by two fields. All existing construction sites must be
  updated (primarily the UDS construction block in `handle_context_search`).
- WA-4a may need to revisit this decision and accept `Arc<SessionRegistry>` on
  `SearchService` if its invocation model cannot pre-resolve the histogram at a handler level.
