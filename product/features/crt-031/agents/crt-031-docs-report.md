# crt-031 Documentation Agent Report

## Summary

README.md review for feature crt-031 (Category Lifecycle Policy — Pinned vs Adaptive). Primary artifacts read: `SCOPE.md`, `SPECIFICATION.md`.

---

## Sections Reviewed

### `[knowledge]` config block (lines 239–252)

Already contains both new fields as stated in the spawn prompt:

```toml
boosted_categories = ["lesson-learned"]
adaptive_categories = ["lesson-learned"]
```

Both fields have accurate inline comments. The `adaptive_categories` comment correctly names the field's purpose (automated lifecycle management, retention, auto-deprecation), names the prerequisite issue (#409), and states the default. No edit required here.

### Configuration section intro — list fields enumeration (line 216)

The per-project config override paragraph listed `categories`, `boosted_categories`, `session_capabilities` as the list fields that replace entirely. `adaptive_categories` is also a list field with project-overrides-global semantics (SPEC FR-10). It was absent from this enumeration.

**Edit applied:** Added `adaptive_categories` to the list field enumeration.

Before:
> List fields (`categories`, `boosted_categories`, `session_capabilities`) replace the global list entirely

After:
> List fields (`categories`, `boosted_categories`, `adaptive_categories`, `session_capabilities`) replace the global list entirely

### MCP Tool Reference — `context_status` row (line 342)

SPEC FR-11 specifies that `context_status` output now includes `category_lifecycle: Vec<(String, String)>` — per-category lifecycle labels (`"adaptive"` or `"pinned"`). The existing `context_status` description listed the metrics it exposes but omitted this new output field.

**Edit applied:** Added lifecycle label mention to the `context_status` description.

Before:
> Shows entry counts, distributions, correction chains, coherence score, security metrics, and graph cohesion metrics (...)

After:
> Shows entry counts, distributions, correction chains, coherence score, security metrics, graph cohesion metrics (...), and per-category lifecycle labels (adaptive vs pinned).

### Knowledge Categories section (lines 379–396)

No changes required. The section describes category semantics, not lifecycle policy. The lifecycle distinction (adaptive vs pinned) is an operator configuration concern documented in the `[knowledge]` config block, not a change to what the categories mean.

### Tips for Maximum Value (lines 187–208)

No changes required. The lifecycle policy does not introduce a new operational constraint for users beyond what is captured in the config block. The `adaptive_categories` field has a safe default (`["lesson-learned"]`) and config-omit behavior is documented.

### Security Model (lines 495–527)

No changes required. This feature makes no security model changes.

### Architecture Overview (lines 432–492)

No changes required. This feature adds no new crates, storage tables, or transport changes. It is config-only at the schema level (SCOPE.md Non-Goals: "Does NOT add database schema changes").

---

## README Sections Modified

1. **Configuration** (line 216) — added `adaptive_categories` to the list-fields enumeration.
2. **MCP Tool Reference** (line 342) — added per-category lifecycle labels to `context_status` description.

## Sections Requiring No Change

- `[knowledge]` config block — already complete per spawn prompt.
- Knowledge Categories — not affected.
- Tips for Maximum Value — not affected.
- Security Model — not affected.
- Architecture Overview — not affected.
- CLI Reference — not affected.
- Skills Reference — not affected.

## Artifact Traceability

| Edit | Artifact reference |
|------|--------------------|
| `adaptive_categories` added to list-fields enumeration | SPEC FR-10 (merge_configs, project-overrides-global), SCOPE.md Goal 5 / Constraint "list fields" |
| `context_status` description updated | SPEC FR-11 (`category_lifecycle` field, both summary and JSON), AC-09 |
