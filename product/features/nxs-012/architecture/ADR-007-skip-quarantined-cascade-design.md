## ADR-007: Skip-Quarantined Cascade Design Using HashSet Built During Entry Ingest

**STATUS: SUPERSEDED by ADR-008**

This ADR described an import-side design where `--skip-quarantined` was a flag on the `import` subcommand. The HashSet was built from `ExportRow::Entry` rows with `status == 3` during `ingest_rows` processing.

The design has been superseded because filtering at import time means the export file still contains quarantined data. By moving the filter to export time, the export file itself becomes a clean snapshot -- import remains a simple full-restore with hash integrity preserved. The HashSet concept transfers, but the integration point changes from `ingest_rows` to `do_export`, and the query mechanism changes from inline detection during JSONL processing to a pre-query of the entries table inside the `BEGIN DEFERRED` snapshot transaction.

See ADR-008 for the replacement design.
