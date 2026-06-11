## ADR-006: Wave-to-Issue Mapping — Wave 1 = #726 + #725 (with shared C1/C2 contract fixtures), Wave 2 = #727 (resolves OQ-D)

### Context

OQ-D: confirm the wave-to-issue mapping. The SCOPE proposes Wave 1 = #726 (vnc-032, server serving) + #725 (nan-019, client init), and Wave 2 = #727 (vnc-033, multi-project routing). It also asks whether the architecture surfaces a finer cut — e.g. carving the C1/C2 connection-contract into its own first deliverable consumed by both server and client.

The umbrella exists precisely because the connection bundle (C1) and cert fingerprint (C2) are wire contracts shared by the server emitter and the client ingester. SR-02 mandates a single oracle plus cross-stack parity fixtures for C2, generated from the Rust side, never hand-written. If the server work (#726) and the client work (#725) each independently implement and test their half of C1/C2, the fixtures drift and the contract silently breaks at connect time — the exact failure mode the umbrella was created to prevent.

### Decision

Confirm the mapping with **one refinement**: the shared C1/C2 contract — wire form (ADR-001), fingerprint format + parity fixtures (ADR-002) — is a **shared sub-deliverable inside Wave 1**, authored once and consumed by both the server side (#726) and the client side (#725).

```
vnc-034 (umbrella)
├── Wave 1 — Single-project serving + client
│   ├── Shared contract sub-deliverable (NEW, build first within Wave 1)
│   │   ├── C1 bundle codec (Rust encoder + JS decoder, ADR-001)
│   │   └── C2 fingerprint + cross-stack parity fixtures from the Rust oracle (ADR-002, SR-02)
│   ├── #726 (vnc-032) — server serving: cert provision, client-bundle, container posture,
│   │                     resolve_store seam + DefaultResolver, /v1/tools alias
│   └── #725 (nan-019) — pure-JS client: init --remote, bundle ingest, cert pin, attach, size gate
└── Wave 2 — Multi-project routing
    └── #727 (vnc-033) — ProjectRouter resolver swap, [[projects]] config, slug lifecycle,
                          per-slug isolation, register/attach
```

The contract sub-deliverable does **not** get its own GH issue — it is a build-ordering rule within Wave 1: the C1 codec and the C2 fixtures land before either #726's `client-bundle` or #725's bundle ingestion depends on them. Both issues consume the same committed fixture corpus, so divergence fails CI rather than a user's connect.

`resolve_store` seam ownership sits with #726 (server-side, Wave 1) because it is the server's routing edge; #725 only consumes the route grammar (the `/v1/tools/...` alias and the `/v1/{slug}` attach shape from ADR-005).

### Consequences

- **Easier:** C1/C2 are implemented and parity-tested once; #726 and #725 cannot diverge on the wire contract (SR-02 closed by construction).
- **Easier:** The wave-to-issue mapping is confirmed as-is at the issue level — no re-cutting of the existing #725/#726/#727 issues; their SCOPEs are retained as input.
- **Easier:** Build order is explicit (contract → server + client → routing), so dependents build on a validated base (#3756).
- **Harder:** Wave 1 has an internal ordering constraint (contract before both halves) — a coordination cost the scrum master enforces, not a structural complication.

### Related

- ADR-001 (C1 wire form), ADR-002 (C2 fingerprint + fixtures): the contents of the shared sub-deliverable.
- ADR-003 (C4 seam) / ADR-005 (route alias): the `resolve_store` seam and route grammar #726 owns and #725 consumes.
- Wave 2 (#727) swaps the resolver impl behind the unchanged trait — the additive boundary (ADR-003 / ADR-005).
