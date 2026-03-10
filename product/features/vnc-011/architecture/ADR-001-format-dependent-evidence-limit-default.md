## ADR-001: Format-Dependent evidence_limit Default

### Context

SCOPE.md states `evidence_limit` default changes from 3 to 0. The Scope Risk Assessment (SR-03) flags this as a high-severity behavioral change: any existing automation relying on evidence in JSON responses will silently lose data. The `evidence_limit` parameter currently defaults to 3 via `params.evidence_limit.unwrap_or(3)` in the handler, and applies to the JSON serialization path using clone-and-truncate (col-010b ADR-001).

The markdown formatter does not use `evidence_limit` at all -- it selects exactly k=3 examples per collapsed finding group (ADR-002), independent of the parameter. Changing the JSON default to 0 would make JSON responses return all evidence by default, which is the opposite of the SCOPE intent (reduce noise). The SCOPE intent is: markdown is the low-noise default, JSON is the full-data fallback.

Two options:
1. **Global default change to 0**: `evidence_limit` defaults to 0 (unlimited) for both formats. JSON consumers who relied on truncation get all evidence. Markdown consumers are unaffected (formatter controls its own selection).
2. **Format-dependent default**: `evidence_limit` defaults to 0 for JSON (consistent with "JSON is the full data" philosophy), 3 for JSON if not specified (preserving backward compatibility). But this is confusing -- the same parameter would mean different things.

Option 1 is actually correct given the architecture: markdown has its own selection logic (k=3 per group), so `evidence_limit` only matters for JSON. Changing JSON's default to 0 (unlimited) means JSON consumers get the complete report -- which is what `format: "json"` semantically promises. Consumers who want truncation can pass `evidence_limit: 3` explicitly.

### Decision

Change `evidence_limit` default from 3 to 0 globally (for the JSON path). The markdown path ignores `evidence_limit` entirely.

Concretely:
- `params.evidence_limit.unwrap_or(3)` becomes `params.evidence_limit.unwrap_or(0)`
- When `format` is `"markdown"`: `evidence_limit` is not applied (formatter handles its own example selection)
- When `format` is `"json"`: `evidence_limit` is applied via existing clone-and-truncate; default 0 means no truncation
- Existing JSON consumers who want truncation must now pass `evidence_limit: 3` explicitly

This is a minor behavioral change for JSON consumers but aligns with the principle that JSON format returns the complete report.

### Consequences

- JSON responses are now larger by default (all evidence included). Consumers who want compact JSON must pass `evidence_limit` explicitly.
- Markdown responses are unaffected -- the formatter's k=3 selection is independent.
- The `evidence_limit` parameter semantics become cleaner: it controls JSON evidence truncation, period. Markdown has its own logic.
- Existing test `test_evidence_limit_default` must be updated: `unwrap_or(3)` -> `unwrap_or(0)`.
