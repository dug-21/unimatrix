//! Export path: redb database -> JSON-lines intermediate file (ADR-003).
//!
//! Only compiled under the redb backend (`#[cfg(not(feature = "backend-sqlite"))]`).
//! Opens the database directly via `redb::Builder` to avoid running migrations.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use redb::{ReadableDatabase, ReadableMultimapTable, ReadableTable, ReadableTableMetadata};

use crate::schema::{
    AGENT_REGISTRY, AUDIT_LOG, CATEGORY_INDEX, CO_ACCESS, COUNTERS, ENTRIES, FEATURE_ENTRIES,
    INJECTION_LOG, OBSERVATION_METRICS, OUTCOME_INDEX, SESSIONS, SIGNAL_QUEUE, STATUS_INDEX,
    TAG_INDEX, TIME_INDEX, TOPIC_INDEX, VECTOR_MAP,
};

use super::format::{DataRow, TableHeader, encode_blob, write_header, write_row};
use super::{ALL_TABLES, MigrateError, MigrationSummary, TableDescriptor};

/// Export all 17 tables from a redb database to a JSON-lines file.
///
/// Opens the database with `redb::Builder::new().create()` (ADR-003) to avoid
/// running `migrate_if_needed()`. The export is read-only after the initial open.
pub fn export(db_path: &Path, output_path: &Path) -> Result<MigrationSummary, MigrateError> {
    let db = redb::Builder::new().create(db_path)?;
    let txn = db.begin_read()?;

    let file = File::create(output_path)?;
    let mut writer = BufWriter::new(file);
    let mut summary = MigrationSummary::new();

    for descriptor in ALL_TABLES {
        match descriptor {
            TableDescriptor::U64Blob { name } => {
                let def = match *name {
                    "entries" => ENTRIES,
                    "audit_log" => AUDIT_LOG,
                    "signal_queue" => SIGNAL_QUEUE,
                    "injection_log" => INJECTION_LOG,
                    _ => {
                        return Err(MigrateError::Validation(format!(
                            "unknown U64Blob table: {name}"
                        )))
                    }
                };
                let table = txn.open_table(def)?;
                let count = table.len()?;
                write_header(
                    &mut writer,
                    &make_header(descriptor, count),
                )?;
                for result in table.iter()? {
                    let (key, value) = result?;
                    write_row(
                        &mut writer,
                        &DataRow {
                            key: serde_json::json!(key.value()),
                            value: serde_json::json!(encode_blob(value.value())),
                        },
                    )?;
                }
                summary.add(name, count);
            }

            TableDescriptor::StrBlob { name } => {
                let def = match *name {
                    "agent_registry" => AGENT_REGISTRY,
                    "observation_metrics" => OBSERVATION_METRICS,
                    "sessions" => SESSIONS,
                    _ => {
                        return Err(MigrateError::Validation(format!(
                            "unknown StrBlob table: {name}"
                        )))
                    }
                };
                let table = txn.open_table(def)?;
                let count = table.len()?;
                write_header(
                    &mut writer,
                    &make_header(descriptor, count),
                )?;
                for result in table.iter()? {
                    let (key, value) = result?;
                    write_row(
                        &mut writer,
                        &DataRow {
                            key: serde_json::json!(key.value()),
                            value: serde_json::json!(encode_blob(value.value())),
                        },
                    )?;
                }
                summary.add(name, count);
            }

            TableDescriptor::StrU64 { name } => {
                // counters: &str -> u64
                let table = txn.open_table(COUNTERS)?;
                let count = table.len()?;
                write_header(
                    &mut writer,
                    &make_header(descriptor, count),
                )?;
                for result in table.iter()? {
                    let (key, value) = result?;
                    write_row(
                        &mut writer,
                        &DataRow {
                            key: serde_json::json!(key.value()),
                            value: serde_json::json!(value.value()),
                        },
                    )?;
                }
                summary.add(name, count);
            }

            TableDescriptor::StrU64Unit { name } => {
                let def = match *name {
                    "topic_index" => TOPIC_INDEX,
                    "category_index" => CATEGORY_INDEX,
                    "outcome_index" => OUTCOME_INDEX,
                    _ => {
                        return Err(MigrateError::Validation(format!(
                            "unknown StrU64Unit table: {name}"
                        )))
                    }
                };
                let table = txn.open_table(def)?;
                let count = table.len()?;
                write_header(
                    &mut writer,
                    &make_header(descriptor, count),
                )?;
                for result in table.iter()? {
                    let (key, _value) = result?;
                    let (s, id) = key.value();
                    write_row(
                        &mut writer,
                        &DataRow {
                            key: serde_json::json!([s, id]),
                            value: serde_json::json!(null),
                        },
                    )?;
                }
                summary.add(name, count);
            }

            TableDescriptor::U64U64Unit { name } => {
                // time_index: (u64, u64) -> ()
                let table = txn.open_table(TIME_INDEX)?;
                let count = table.len()?;
                write_header(
                    &mut writer,
                    &make_header(descriptor, count),
                )?;
                for result in table.iter()? {
                    let (key, _value) = result?;
                    let (k0, k1) = key.value();
                    write_row(
                        &mut writer,
                        &DataRow {
                            key: serde_json::json!([k0, k1]),
                            value: serde_json::json!(null),
                        },
                    )?;
                }
                summary.add(name, count);
            }

            TableDescriptor::U8U64Unit { name } => {
                // status_index: (u8, u64) -> ()
                let table = txn.open_table(STATUS_INDEX)?;
                let count = table.len()?;
                write_header(
                    &mut writer,
                    &make_header(descriptor, count),
                )?;
                for result in table.iter()? {
                    let (key, _value) = result?;
                    let (k0, k1) = key.value();
                    write_row(
                        &mut writer,
                        &DataRow {
                            key: serde_json::json!([k0, k1]),
                            value: serde_json::json!(null),
                        },
                    )?;
                }
                summary.add(name, count);
            }

            TableDescriptor::U64U64 { name } => {
                // vector_map: u64 -> u64
                let table = txn.open_table(VECTOR_MAP)?;
                let count = table.len()?;
                write_header(
                    &mut writer,
                    &make_header(descriptor, count),
                )?;
                for result in table.iter()? {
                    let (key, value) = result?;
                    write_row(
                        &mut writer,
                        &DataRow {
                            key: serde_json::json!(key.value()),
                            value: serde_json::json!(value.value()),
                        },
                    )?;
                }
                summary.add(name, count);
            }

            TableDescriptor::U64U64Blob { name } => {
                // co_access: (u64, u64) -> blob
                let table = txn.open_table(CO_ACCESS)?;
                let count = table.len()?;
                write_header(
                    &mut writer,
                    &make_header(descriptor, count),
                )?;
                for result in table.iter()? {
                    let (key, value) = result?;
                    let (k0, k1) = key.value();
                    write_row(
                        &mut writer,
                        &DataRow {
                            key: serde_json::json!([k0, k1]),
                            value: serde_json::json!(encode_blob(value.value())),
                        },
                    )?;
                }
                summary.add(name, count);
            }

            TableDescriptor::MultimapStrU64 { name } => {
                let def = match *name {
                    "tag_index" => TAG_INDEX,
                    "feature_entries" => FEATURE_ENTRIES,
                    _ => {
                        return Err(MigrateError::Validation(format!(
                            "unknown MultimapStrU64 table: {name}"
                        )))
                    }
                };
                let table = txn.open_multimap_table(def)?;
                // Count ALL (key, value) pairs for multimap tables.
                // We must buffer because we need row_count in the header before rows.
                let mut rows = Vec::new();
                for result in table.iter()? {
                    let (key, values) = result?;
                    let k = key.value().to_string();
                    for val_result in values {
                        let val = val_result?;
                        rows.push(DataRow {
                            key: serde_json::json!(k),
                            value: serde_json::json!(val.value()),
                        });
                    }
                }
                let total_pairs = rows.len() as u64;
                write_header(
                    &mut writer,
                    &TableHeader {
                        table: name.to_string(),
                        key_type: descriptor.key_type(),
                        value_type: descriptor.value_type(),
                        multimap: true,
                        row_count: total_pairs,
                    },
                )?;
                for row in &rows {
                    write_row(&mut writer, row)?;
                }
                summary.add(name, total_pairs);
            }
        }
    }

    writer.flush().map_err(MigrateError::Io)?;
    Ok(summary)
}

fn make_header(descriptor: &TableDescriptor, row_count: u64) -> TableHeader {
    TableHeader {
        table: descriptor.name().to_string(),
        key_type: descriptor.key_type(),
        value_type: descriptor.value_type(),
        multimap: descriptor.is_multimap(),
        row_count,
    }
}
