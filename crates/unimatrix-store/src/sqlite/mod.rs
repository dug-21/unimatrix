pub(crate) mod db;
pub(crate) mod txn;
pub(crate) mod write;
pub(crate) mod write_ext;
pub(crate) mod read;
pub(crate) mod signal;
pub(crate) mod sessions;
pub(crate) mod injection_log;
pub(crate) mod migration;
pub(crate) mod compat;
pub(crate) mod compat_handles;
pub(crate) mod compat_txn;

pub use db::Store;
pub use txn::{SqliteReadTransaction, SqliteWriteTransaction};
pub use compat::{
    SqliteTableDef, SqliteMultimapDef,
    ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX,
    STATUS_INDEX, VECTOR_MAP, COUNTERS, OUTCOME_INDEX, AUDIT_LOG,
    AGENT_REGISTRY, FEATURE_ENTRIES, CO_ACCESS, SIGNAL_QUEUE,
    SESSIONS, INJECTION_LOG,
    next_entry_id, increment_counter, decrement_counter,
    BlobGuard, U64Guard, UnitGuard, CompositeKeyGuard, U64KeyGuard,
    RangeResult,
};
pub use compat_handles::{
    TableU64Blob, TableStrU64, TableStrBlob,
    TableStrU64Comp, TableU64U64Comp, TableU8U64Comp,
    TableU64U64, MultimapStrU64,
};
pub use compat_txn::{TableSpec, MultimapSpec};
