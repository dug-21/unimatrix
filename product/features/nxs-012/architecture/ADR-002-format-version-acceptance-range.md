## ADR-002: Format Version Acceptance Range (1 and 2)

### Context

The export format_version is currently 1. nxs-012 bumps it to 2 to signal the presence of 3 new table types (`graph_edges`, `observations`, `cycle_events`). The import pipeline must decide which format_versions to accept.

The current import code rejects anything other than format_version 1 with `"unsupported format_version: {v}. Only format_version 1 is supported."`. This is overly restrictive for forward compatibility and needs updating.

Options considered:
- Accept only 2: breaks backward compatibility with existing exports
- Accept 1 and 2: preserves backward compatibility, new tables simply absent in v1 files
- Accept 1, 2, and future versions permissively: risky, unknown table types could cause silent failures

### Decision

Import accepts format_version 1 (legacy, 8 tables) and format_version 2 (11 tables). All other values (0, 3+) are rejected with a clear error message.

For format_version 1 imports, the 3 new `_table` values simply never appear in the JSONL stream, so no special handling is needed -- the `ingest_rows` match arms for the new variants are never reached. The `ImportCounts` fields for the 3 new tables remain 0.

For format_version 2, all 11 table types may appear. The pipeline processes them identically to v1 tables.

The validation replaces the current `if header.format_version != 1` check with a match expression:

```rust
match header.format_version {
    1 | 2 => Ok(()),
    v => Err(format!(
        "unsupported format_version: {v}. This binary supports format_version 1 and 2."
    ))
}
```

Export always writes format_version 2. There is no option to produce a v1 export from the new binary.

### Consequences

- Old exports (v1) remain importable -- no data loss for users who exported before upgrading
- New exports (v2) are NOT importable by old binaries (they reject format_version != 1) -- this is acceptable and documented
- The error message for rejected versions tells the user exactly which versions are supported
- Future format_version 3 will require another code change to the acceptance range -- intentional, prevents silent misinterpretation of unknown formats
