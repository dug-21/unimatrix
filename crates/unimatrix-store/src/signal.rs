// LAYOUT FROZEN: bincode v2 positional encoding. Fields may only be APPENDED.
// See ADR-001 (col-009). Do not reorder or remove fields.

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// A single confidence signal record in the SIGNAL_QUEUE work queue.
///
/// Created at session end (Stop hook), consumed by dual consumers
/// (confidence pipeline and retrospective pipeline), then deleted.
///
/// Field order is frozen for bincode v2 positional compatibility (ADR-001).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SignalRecord {
    pub signal_id: u64,              // field 0 — monotonic key, also stored as value
    pub session_id: String,          // field 1 — which session generated this signal
    pub created_at: u64,             // field 2 — Unix seconds
    pub entry_ids: Vec<u64>,         // field 3 — deduplicated entries receiving this signal
    pub signal_type: SignalType,     // field 4 — Helpful | Flagged
    pub signal_source: SignalSource, // field 5 — ImplicitOutcome | ImplicitRework
}

/// Type of confidence signal.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum SignalType {
    Helpful = 0,
    Flagged = 1,
}

/// Source of the implicit confidence signal.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum SignalSource {
    ImplicitOutcome = 0,
    ImplicitRework = 1,
}

/// Serialize a SignalRecord to bincode bytes using the serde-compatible path.
///
/// Uses `bincode::serde::encode_to_vec` with `standard()` config,
/// matching the workspace convention for EntryRecord.
pub fn serialize_signal(record: &SignalRecord) -> Result<Vec<u8>> {
    let bytes = bincode::serde::encode_to_vec(record, bincode::config::standard())?;
    Ok(bytes)
}

/// Deserialize a SignalRecord from bincode bytes using the serde-compatible path.
pub fn deserialize_signal(bytes: &[u8]) -> Result<SignalRecord> {
    let (record, _) =
        bincode::serde::decode_from_slice::<SignalRecord, _>(bytes, bincode::config::standard())?;
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_signal(signal_type: SignalType) -> SignalRecord {
        SignalRecord {
            signal_id: 1,
            session_id: "test-session".to_string(),
            created_at: 1_700_000_000,
            entry_ids: vec![10, 20, 30],
            signal_type,
            signal_source: SignalSource::ImplicitOutcome,
        }
    }

    #[test]
    fn test_signal_record_roundtrip_helpful() {
        let record = make_signal(SignalType::Helpful);
        let bytes = serialize_signal(&record).expect("serialize");
        let deserialized = deserialize_signal(&bytes).expect("deserialize");
        assert_eq!(deserialized.signal_id, 1);
        assert_eq!(deserialized.session_id, "test-session");
        assert_eq!(deserialized.created_at, 1_700_000_000);
        assert_eq!(deserialized.entry_ids, vec![10, 20, 30]);
        assert_eq!(deserialized.signal_type, SignalType::Helpful);
        assert_eq!(deserialized.signal_source, SignalSource::ImplicitOutcome);
    }

    #[test]
    fn test_signal_record_roundtrip_flagged() {
        let record = SignalRecord {
            signal_id: 42,
            session_id: "session-2".to_string(),
            created_at: 2_000_000_000,
            entry_ids: vec![1, 2],
            signal_type: SignalType::Flagged,
            signal_source: SignalSource::ImplicitRework,
        };
        let bytes = serialize_signal(&record).expect("serialize");
        let deserialized = deserialize_signal(&bytes).expect("deserialize");
        assert_eq!(deserialized.signal_type, SignalType::Flagged);
        assert_eq!(deserialized.signal_source, SignalSource::ImplicitRework);
        assert_eq!(deserialized.signal_id, 42);
    }

    #[test]
    fn test_signal_type_discriminants() {
        assert_eq!(SignalType::Helpful as u8, 0);
        assert_eq!(SignalType::Flagged as u8, 1);
    }

    #[test]
    fn test_signal_source_discriminants() {
        assert_eq!(SignalSource::ImplicitOutcome as u8, 0);
        assert_eq!(SignalSource::ImplicitRework as u8, 1);
    }

    #[test]
    fn test_signal_record_empty_entry_ids() {
        let record = SignalRecord {
            signal_id: 0,
            session_id: String::new(),
            created_at: 0,
            entry_ids: vec![],
            signal_type: SignalType::Helpful,
            signal_source: SignalSource::ImplicitOutcome,
        };
        let bytes = serialize_signal(&record).expect("serialize");
        let deserialized = deserialize_signal(&bytes).expect("deserialize");
        assert!(deserialized.entry_ids.is_empty());
    }

    #[test]
    fn test_signal_record_roundtrip_max_values() {
        let record = SignalRecord {
            signal_id: u64::MAX,
            session_id: "x".repeat(256),
            created_at: u64::MAX,
            entry_ids: vec![u64::MAX],
            signal_type: SignalType::Flagged,
            signal_source: SignalSource::ImplicitRework,
        };
        let bytes = serialize_signal(&record).expect("serialize");
        let deserialized = deserialize_signal(&bytes).expect("deserialize");
        assert_eq!(deserialized.signal_id, u64::MAX);
    }
}
