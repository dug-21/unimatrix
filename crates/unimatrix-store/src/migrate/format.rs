//! Intermediate format types for JSON-lines migration files (ADR-001).
//!
//! Shared between export and import paths. Provides serde types for
//! table headers and data rows, plus base64 encoding/decoding helpers.

use std::io::{BufRead, Write};

use serde::{Deserialize, Serialize};

use super::MigrateError;

/// Key type classification for table schemas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyType {
    U64,
    Str,
    StrU64,
    U64U64,
    U8U64,
}

/// Value type classification for table schemas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    Blob,
    U64,
    Unit,
}

/// Header line marking the start of a table section in the intermediate file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TableHeader {
    pub table: String,
    pub key_type: KeyType,
    pub value_type: ValueType,
    #[serde(default, skip_serializing_if = "is_false")]
    pub multimap: bool,
    pub row_count: u64,
}

fn is_false(v: &bool) -> bool {
    !v
}

/// Data line representing a single row in the intermediate file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DataRow {
    pub key: serde_json::Value,
    pub value: serde_json::Value,
}

/// Write a table header as a JSON line.
pub fn write_header(writer: &mut impl Write, header: &TableHeader) -> Result<(), MigrateError> {
    serde_json::to_writer(&mut *writer, header)?;
    writer.write_all(b"\n")?;
    Ok(())
}

/// Write a data row as a JSON line.
pub fn write_row(writer: &mut impl Write, row: &DataRow) -> Result<(), MigrateError> {
    serde_json::to_writer(&mut *writer, row)?;
    writer.write_all(b"\n")?;
    Ok(())
}

/// Read and parse one JSON line from the reader.
///
/// Returns `None` on EOF or empty line.
pub fn read_line(reader: &mut impl BufRead) -> Result<Option<serde_json::Value>, MigrateError> {
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line).map_err(MigrateError::Io)?;
    if bytes_read == 0 {
        return Ok(None);
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let value = serde_json::from_str(trimmed)?;
    Ok(Some(value))
}

/// Encode bytes as standard base64 (RFC 4648, with padding).
pub fn encode_blob(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Decode a standard base64 string to bytes.
pub fn decode_blob(s: &str) -> Result<Vec<u8>, MigrateError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| MigrateError::Base64Decode(e.to_string()))
}

/// Validate that a u64 value fits in i64 range for SQLite INTEGER storage.
pub fn validate_i64_range(val: u64) -> Result<(), MigrateError> {
    if val > i64::MAX as u64 {
        return Err(MigrateError::Validation(format!(
            "value {val} exceeds i64::MAX ({})",
            i64::MAX
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- T-03: Base64 round-trip --

    #[test]
    fn test_base64_roundtrip_empty() {
        let bytes: &[u8] = &[];
        assert_eq!(decode_blob(&encode_blob(bytes)).unwrap(), bytes);
    }

    #[test]
    fn test_base64_roundtrip_one_byte() {
        let bytes: &[u8] = &[0xFF];
        assert_eq!(decode_blob(&encode_blob(bytes)).unwrap(), bytes);
    }

    #[test]
    fn test_base64_roundtrip_two_bytes() {
        let bytes: &[u8] = &[0xAB, 0xCD];
        assert_eq!(decode_blob(&encode_blob(bytes)).unwrap(), bytes);
    }

    #[test]
    fn test_base64_roundtrip_three_bytes() {
        let bytes: &[u8] = &[0x01, 0x02, 0x03];
        assert_eq!(decode_blob(&encode_blob(bytes)).unwrap(), bytes);
    }

    #[test]
    fn test_base64_roundtrip_100_bytes() {
        let bytes: Vec<u8> = (0..100).collect();
        assert_eq!(decode_blob(&encode_blob(&bytes)).unwrap(), bytes);
    }

    #[test]
    fn test_base64_roundtrip_large() {
        let bytes: Vec<u8> = (0..100_000).map(|i| (i % 256) as u8).collect();
        assert_eq!(decode_blob(&encode_blob(&bytes)).unwrap(), bytes);
    }

    #[test]
    fn test_base64_decode_invalid() {
        assert!(decode_blob("!!!not-base64!!!").is_err());
    }

    // -- TableHeader serde --

    #[test]
    fn test_table_header_roundtrip() {
        let header = TableHeader {
            table: "entries".to_string(),
            key_type: KeyType::U64,
            value_type: ValueType::Blob,
            multimap: false,
            row_count: 53,
        };
        let json = serde_json::to_string(&header).unwrap();
        let parsed: TableHeader = serde_json::from_str(&json).unwrap();
        assert_eq!(header, parsed);
        // multimap should be absent when false
        assert!(!json.contains("multimap"));
    }

    #[test]
    fn test_table_header_multimap() {
        let header = TableHeader {
            table: "tag_index".to_string(),
            key_type: KeyType::Str,
            value_type: ValueType::U64,
            multimap: true,
            row_count: 106,
        };
        let json = serde_json::to_string(&header).unwrap();
        assert!(json.contains("\"multimap\":true"));
        let parsed: TableHeader = serde_json::from_str(&json).unwrap();
        assert_eq!(header, parsed);
    }

    #[test]
    fn test_table_header_missing_multimap_defaults_false() {
        let json = r#"{"table":"entries","key_type":"u64","value_type":"blob","row_count":10}"#;
        let parsed: TableHeader = serde_json::from_str(json).unwrap();
        assert!(!parsed.multimap);
    }

    // -- DataRow serde --

    #[test]
    fn test_data_row_u64_blob() {
        let row = DataRow {
            key: serde_json::json!(42),
            value: serde_json::json!("dGVzdA=="),
        };
        let json = serde_json::to_string(&row).unwrap();
        let parsed: DataRow = serde_json::from_str(&json).unwrap();
        assert_eq!(row, parsed);
    }

    #[test]
    fn test_data_row_composite_key() {
        let row = DataRow {
            key: serde_json::json!(["auth", 42]),
            value: serde_json::json!(null),
        };
        let json = serde_json::to_string(&row).unwrap();
        let parsed: DataRow = serde_json::from_str(&json).unwrap();
        assert_eq!(row, parsed);
    }

    #[test]
    fn test_data_row_null_value() {
        let row = DataRow {
            key: serde_json::json!([100, 200]),
            value: serde_json::json!(null),
        };
        let json = serde_json::to_string(&row).unwrap();
        let parsed: DataRow = serde_json::from_str(&json).unwrap();
        assert_eq!(row, parsed);
    }

    // -- KeyType / ValueType snake_case serde --

    #[test]
    fn test_key_type_serde() {
        assert_eq!(serde_json::to_string(&KeyType::U64).unwrap(), "\"u64\"");
        assert_eq!(serde_json::to_string(&KeyType::Str).unwrap(), "\"str\"");
        assert_eq!(
            serde_json::to_string(&KeyType::StrU64).unwrap(),
            "\"str_u64\""
        );
        assert_eq!(
            serde_json::to_string(&KeyType::U64U64).unwrap(),
            "\"u64_u64\""
        );
        assert_eq!(
            serde_json::to_string(&KeyType::U8U64).unwrap(),
            "\"u8_u64\""
        );
    }

    #[test]
    fn test_value_type_serde() {
        assert_eq!(
            serde_json::to_string(&ValueType::Blob).unwrap(),
            "\"blob\""
        );
        assert_eq!(
            serde_json::to_string(&ValueType::U64).unwrap(),
            "\"u64\""
        );
        assert_eq!(
            serde_json::to_string(&ValueType::Unit).unwrap(),
            "\"unit\""
        );
    }

    // -- I/O helpers --

    #[test]
    fn test_write_header_produces_json_line() {
        let header = TableHeader {
            table: "test".to_string(),
            key_type: KeyType::U64,
            value_type: ValueType::Blob,
            multimap: false,
            row_count: 5,
        };
        let mut buf = Vec::new();
        write_header(&mut buf, &header).unwrap();
        let line = String::from_utf8(buf).unwrap();
        assert!(line.ends_with('\n'));
        assert_eq!(line.matches('\n').count(), 1);
        let parsed: TableHeader = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(parsed, header);
    }

    #[test]
    fn test_write_row_produces_json_line() {
        let row = DataRow {
            key: serde_json::json!(1),
            value: serde_json::json!("data"),
        };
        let mut buf = Vec::new();
        write_row(&mut buf, &row).unwrap();
        let line = String::from_utf8(buf).unwrap();
        assert!(line.ends_with('\n'));
        assert_eq!(line.matches('\n').count(), 1);
    }

    #[test]
    fn test_read_line_empty() {
        let data = b"";
        let mut cursor = std::io::Cursor::new(&data[..]);
        assert!(read_line(&mut cursor).unwrap().is_none());
    }

    #[test]
    fn test_read_line_valid_json() {
        let data = b"{\"key\":1}\n";
        let mut cursor = std::io::Cursor::new(&data[..]);
        let val = read_line(&mut cursor).unwrap().unwrap();
        assert_eq!(val["key"], 1);
    }

    // -- validate_i64_range --

    #[test]
    fn test_validate_i64_range_valid() {
        assert!(validate_i64_range(0).is_ok());
        assert!(validate_i64_range(1).is_ok());
        assert!(validate_i64_range(i64::MAX as u64).is_ok());
    }

    #[test]
    fn test_validate_i64_range_overflow() {
        assert!(validate_i64_range(i64::MAX as u64 + 1).is_err());
        assert!(validate_i64_range(u64::MAX).is_err());
    }
}
