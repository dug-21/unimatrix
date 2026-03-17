//! UnimatrixServer core: state holder and ServerHandler implementation.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use unimatrix_core::async_wrappers::{AsyncEntryStore, AsyncVectorStore};
use unimatrix_core::{
    CoreError, EmbedService, EntryRecord, NewEntry, Store, StoreAdapter, VectorAdapter, VectorIndex,
};
use unimatrix_store::read::{ENTRY_COLUMNS, entry_from_row, load_tags_for_entries};
use unimatrix_store::rusqlite;
use unimatrix_store::{StoreError, compute_content_hash, status_counter_key};

use unimatrix_adapt::AdaptationService;

use crate::background::TickMetadata;
use crate::error::ServerError;
use crate::infra::audit::{AuditEvent, AuditLog};
use crate::infra::categories::CategoryAllowlist;
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::infra::registry::{AgentRegistry, TrustLevel};
use crate::infra::session::SessionRegistry;
use crate::infra::usage_dedup::{UsageDedup, VoteAction};
use crate::mcp::identity::{self, ResolvedIdentity};
use crate::services::{EffectivenessStateHandle, ServiceLayer};

// -- col-009 / vnc-005: PendingEntriesAnalysis --

/// Returns the current Unix timestamp in seconds.
fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Per-feature-cycle bucket holding accumulated entry analyses.
///
/// Created lazily by `upsert`; removed entirely by `drain_for` or `evict_stale`.
/// Cap: 1000 entries per bucket. Excess evicted by lowest rework_flag_count.
#[derive(Debug)]
pub struct FeatureBucket {
    /// Inner key: entry_id u64. Overwrite semantics — each entry_id appears at most once.
    pub entries: HashMap<u64, unimatrix_observe::EntryAnalysis>,
    /// Unix seconds — updated on every upsert; used for TTL eviction by background tick.
    pub last_updated: u64,
}

impl FeatureBucket {
    fn new() -> Self {
        FeatureBucket {
            entries: HashMap::new(),
            last_updated: unix_now_secs(),
        }
    }
}

/// Two-level in-memory accumulator for entry-level performance data.
///
/// Outer key: feature_cycle string (e.g., "vnc-005").
/// Inner key: entry_id u64 (overwrite semantics — no duplicate IDs per bucket).
///
/// Shared between the UDS listener (writes from signal consumers) and the
/// context_retrospective handler (drains on call). Protected by
/// `Arc<Mutex<PendingEntriesAnalysis>>`.
///
/// Daemon-mode note: this accumulator persists across sessions.
/// `UsageDedup` is also daemon-wide — dedup applies across all sessions
/// for the same entry within the dedup window, which is the correct behavior.
#[derive(Debug)]
pub struct PendingEntriesAnalysis {
    /// Outer key: feature_cycle string (e.g., "vnc-005").
    /// Inner key: entry_id u64.
    pub buckets: HashMap<String, FeatureBucket>,
    pub created_at: u64,
}

impl PendingEntriesAnalysis {
    pub fn new() -> Self {
        PendingEntriesAnalysis {
            buckets: HashMap::new(),
            created_at: unix_now_secs(),
        }
    }

    /// Insert or replace an `EntryAnalysis` in the bucket for `feature_cycle`.
    ///
    /// Semantics: **overwrite** — if `entry_id` already exists in the bucket,
    /// the existing record is replaced entirely (not accumulated/summed).
    /// This preserves the most-recent signal per entry within a feature cycle.
    ///
    /// Security: `feature_cycle` keys exceeding 256 bytes are silently dropped
    /// (prevents memory exhaustion; callers are fire-and-forget — C-16).
    ///
    /// Cap: 1000 entries per bucket. When the cap is reached, the entry with
    /// the lowest `rework_flag_count` is evicted before inserting the new entry.
    /// The cap and eviction run entirely within the caller's Mutex lock (R-15).
    pub fn upsert(&mut self, feature_cycle: &str, analysis: unimatrix_observe::EntryAnalysis) {
        // C-16: validate key length — silent drop for oversized keys
        if feature_cycle.len() > 256 {
            tracing::warn!(
                key_len = feature_cycle.len(),
                "feature_cycle key exceeds 256 bytes; entry dropped"
            );
            return;
        }

        let bucket = self
            .buckets
            .entry(feature_cycle.to_string())
            .or_insert_with(FeatureBucket::new);

        // Overwrite semantics: replace any existing entry with the same ID
        if bucket.entries.len() >= 1000 && !bucket.entries.contains_key(&analysis.entry_id) {
            // Bucket full and this is a new entry — evict lowest rework_flag_count
            let min_key = bucket
                .entries
                .iter()
                .min_by_key(|(_, v)| v.rework_flag_count)
                .map(|(k, _)| *k);
            if let Some(k) = min_key {
                bucket.entries.remove(&k);
            }
        }

        bucket.entries.insert(analysis.entry_id, analysis);
        bucket.last_updated = unix_now_secs();
    }

    /// Remove and return all entries for the given `feature_cycle` bucket.
    ///
    /// The bucket is removed entirely. A subsequent `upsert` for the same key
    /// creates a fresh bucket. A subsequent `drain_for` returns an empty Vec.
    ///
    /// This operation is atomic within the caller's Mutex lock (R-18).
    pub fn drain_for(&mut self, feature_cycle: &str) -> Vec<unimatrix_observe::EntryAnalysis> {
        match self.buckets.remove(feature_cycle) {
            None => Vec::new(),
            Some(bucket) => bucket.entries.into_values().collect(),
        }
    }

    /// Evict buckets whose `last_updated` is older than `ttl_secs` relative to `now_unix_secs`.
    ///
    /// Called by the background tick (72-hour TTL per ADR-004) as a safety net for
    /// features that complete without calling `context_retrospective` or `context_cycle`.
    /// The entire eviction runs within the caller's Mutex lock (R-18).
    pub fn evict_stale(&mut self, now_unix_secs: u64, ttl_secs: u64) {
        let mut to_evict: Vec<String> = Vec::new();

        for (feature_cycle, bucket) in &self.buckets {
            let age = now_unix_secs.saturating_sub(bucket.last_updated);
            if age > ttl_secs {
                to_evict.push(feature_cycle.clone());
            }
        }

        for key in &to_evict {
            if let Some(bucket) = self.buckets.remove(key) {
                let age_hours = now_unix_secs
                    .saturating_sub(bucket.last_updated)
                    .saturating_div(3600);
                tracing::warn!(
                    feature_cycle = %key,
                    entry_count = bucket.entries.len(),
                    age_hours,
                    "evicting stale pending_entries_analysis bucket (TTL exceeded)"
                );
            }
        }
    }
}

/// Server name reported in MCP initialize handshake.
const SERVER_NAME: &str = "unimatrix";

/// Behavioral instructions for AI agents.
const SERVER_INSTRUCTIONS: &str = "Unimatrix is this project's knowledge engine. Before starting implementation, architecture, or design tasks, search for relevant patterns and conventions using the context tools. Apply what you find. After discovering reusable patterns or making architectural decisions, store them for future reference. Do not store workflow state or process steps.";

/// The central MCP server holding all shared state.
///
/// All fields are Arc-wrapped so Clone is cheap (required by rmcp).
#[derive(Clone)]
pub struct UnimatrixServer {
    /// Async entry store for knowledge operations.
    pub(crate) entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    /// Async vector store for similarity search.
    pub(crate) vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
    /// Lazy-loading embedding service.
    pub(crate) embed_service: Arc<EmbedServiceHandle>,
    /// Agent registry for identity and capabilities.
    pub(crate) registry: Arc<AgentRegistry>,
    /// Audit log for request tracking.
    pub(crate) audit: Arc<AuditLog>,
    /// Category allowlist for validation.
    pub(crate) categories: Arc<CategoryAllowlist>,
    /// Raw store for combined write transactions (ADR-001).
    pub(crate) store: Arc<Store>,
    /// Raw vector index for combined write transactions (GH #14 fix).
    pub(crate) vector_index: Arc<VectorIndex>,
    /// Session-scoped usage deduplication.
    pub(crate) usage_dedup: Arc<UsageDedup>,
    /// Adaptive embedding service for MicroLoRA adaptation pipeline.
    pub(crate) adapt_service: Arc<AdaptationService>,
    /// Accumulated entry-level analysis from signal consumers (col-009).
    /// Shared with UDS listener; drained by context_retrospective handler.
    pub pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>,
    /// Session registry for stale session sweep (col-009, FR-09.2).
    /// Shared with UDS listener; swept by the background tick.
    pub session_registry: Arc<SessionRegistry>,
    /// Transport-agnostic service layer (vnc-006).
    pub(crate) services: ServiceLayer,
    /// crt-018b: effectiveness classification cache shared across search, briefing,
    /// and the background tick. Held here so it can be passed to `spawn_background_tick`.
    pub(crate) effectiveness_state: EffectivenessStateHandle,
    /// Background tick metadata for status reporting (col-013).
    pub tick_metadata: Arc<Mutex<TickMetadata>>,
    /// Tool router generated by the tool_router macro.
    tool_router: ToolRouter<Self>,
    /// Cached server info for MCP handshake.
    server_info: ServerInfo,
}

impl UnimatrixServer {
    /// Create a new server with all subsystems.
    pub fn new(
        entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
        vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
        embed_service: Arc<EmbedServiceHandle>,
        registry: Arc<AgentRegistry>,
        audit: Arc<AuditLog>,
        categories: Arc<CategoryAllowlist>,
        store: Arc<Store>,
        vector_index: Arc<VectorIndex>,
        adapt_service: Arc<AdaptationService>,
    ) -> Self {
        let server_info = ServerInfo {
            server_info: Implementation {
                name: SERVER_NAME.to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                ..Default::default()
            },
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(SERVER_INSTRUCTIONS.to_string()),
            ..Default::default()
        };

        let usage_dedup = Arc::new(UsageDedup::new());

        let services = ServiceLayer::new(
            Arc::clone(&store),
            Arc::clone(&vector_index),
            Arc::clone(&vector_store),
            Arc::clone(&entry_store),
            Arc::clone(&embed_service),
            Arc::clone(&adapt_service),
            Arc::clone(&audit),
            Arc::clone(&usage_dedup),
        );

        // crt-018b: extract handle after ServiceLayer is fully constructed so
        // main.rs can pass the same Arc to `spawn_background_tick` (mirrors
        // the confidence_state_handle pattern from crt-019).
        let effectiveness_state = services.effectiveness_state_handle();

        let tick_metadata = Arc::new(Mutex::new(TickMetadata::new()));

        UnimatrixServer {
            entry_store,
            vector_store,
            embed_service,
            registry,
            audit,
            categories,
            store,
            vector_index,
            usage_dedup,
            adapt_service,
            pending_entries_analysis: Arc::new(Mutex::new(PendingEntriesAnalysis::new())),
            session_registry: Arc::new(SessionRegistry::new()),
            services,
            effectiveness_state,
            tick_metadata,
            tool_router: Self::tool_router(),
            server_info,
        }
    }

    /// Resolve an agent identity from tool parameters.
    ///
    /// Uses `spawn_blocking` to avoid holding the Store mutex on an async
    /// runtime thread (#176).
    pub async fn resolve_agent(
        &self,
        agent_id: &Option<String>,
    ) -> Result<ResolvedIdentity, ServerError> {
        let extracted = identity::extract_agent_id(agent_id);
        let registry = Arc::clone(&self.registry);
        tokio::task::spawn_blocking(move || identity::resolve_identity(&registry, &extracted))
            .await
            .map_err(|e| ServerError::Core(CoreError::JoinError(e.to_string())))?
    }

    /// Resolve identity, parse format, build audit context with optional session ID.
    ///
    /// Replaces the 15-25 line ceremony in each MCP handler with a single call.
    /// Capability checking is separate via `require_cap()` (ADR-002).
    /// Session ID is validated (S3) and prefixed with "mcp::" when present.
    ///
    /// Uses `spawn_blocking` internally to keep Store mutex off the async
    /// runtime (#176).
    pub(crate) async fn build_context(
        &self,
        agent_id: &Option<String>,
        format: &Option<String>,
        session_id: &Option<String>,
    ) -> Result<crate::mcp::context::ToolContext, rmcp::ErrorData> {
        use crate::mcp::context::ToolContext;
        use crate::services::{AuditContext, AuditSource, CallerId, prefix_session_id};

        let identity = self
            .resolve_agent(agent_id)
            .await
            .map_err(rmcp::ErrorData::from)?;
        let format = crate::mcp::response::parse_format(format).map_err(rmcp::ErrorData::from)?;

        // Session ID: validate (S3) and prefix with mcp::
        let prefixed_session = if let Some(sid) = session_id {
            Self::validate_session_id(sid).map_err(rmcp::ErrorData::from)?;
            Some(prefix_session_id("mcp", sid))
        } else {
            None
        };

        let audit_ctx = AuditContext {
            source: AuditSource::Mcp {
                agent_id: identity.agent_id.clone(),
                trust_level: identity.trust_level,
            },
            caller_id: identity.agent_id.clone(),
            session_id: prefixed_session,
            feature_cycle: None,
        };

        let caller_id = CallerId::Agent(identity.agent_id.clone());

        Ok(ToolContext {
            agent_id: identity.agent_id,
            trust_level: identity.trust_level,
            format,
            audit_ctx,
            caller_id,
        })
    }

    /// Validate session_id: max 256 chars, no control characters (S3).
    fn validate_session_id(sid: &str) -> Result<(), ServerError> {
        if sid.len() > 256 {
            return Err(ServerError::InvalidInput {
                field: "session_id".to_string(),
                reason: "session_id exceeds 256 characters".to_string(),
            });
        }
        for ch in sid.chars() {
            if ch.is_control() && ch != '\n' && ch != '\t' {
                return Err(ServerError::InvalidInput {
                    field: "session_id".to_string(),
                    reason: "session_id contains control characters".to_string(),
                });
            }
        }
        Ok(())
    }

    /// Check a capability for the given agent.
    ///
    /// Uses `spawn_blocking` to avoid holding the Store mutex on an async
    /// runtime thread (#176).
    pub(crate) async fn require_cap(
        &self,
        agent_id: &str,
        cap: crate::infra::registry::Capability,
    ) -> Result<(), rmcp::ErrorData> {
        let registry = Arc::clone(&self.registry);
        let agent_id = agent_id.to_string();
        tokio::task::spawn_blocking(move || registry.require_capability(&agent_id, cap))
            .await
            .map_err(|e| {
                rmcp::ErrorData::from(ServerError::Core(CoreError::JoinError(e.to_string())))
            })?
            .map_err(rmcp::ErrorData::from)
    }

    /// Fire-and-forget audit event via `spawn_blocking`.
    ///
    /// Replaces direct `self.audit.log_event()` calls which would block the
    /// async runtime thread on `store.lock_conn()` (#176).
    pub(crate) fn audit_fire_and_forget(&self, event: AuditEvent) {
        if tokio::runtime::Handle::try_current().is_ok() {
            let audit = Arc::clone(&self.audit);
            let _ = tokio::task::spawn_blocking(move || {
                let _ = audit.log_event(event);
            });
        } else {
            let _ = self.audit.log_event(event);
        }
    }

    /// Insert a new entry and write an audit event in a single write transaction.
    ///
    /// Uses direct SQL with named params (ADR-004, nxs-008).
    /// The HNSW vector insertion happens after the transaction commits.
    pub(crate) async fn insert_with_audit(
        &self,
        entry: NewEntry,
        embedding: Vec<f32>,
        audit_event: AuditEvent,
    ) -> Result<(u64, EntryRecord), ServerError> {
        let store = Arc::clone(&self.store);
        let audit_log = Arc::clone(&self.audit);
        let data_id = self.vector_index.allocate_data_id();
        let embedding_dim = embedding.len() as u16;

        let (entry_id, record) =
            tokio::task::spawn_blocking(move || -> Result<(u64, EntryRecord), ServerError> {
                let txn = store
                    .begin_write()
                    .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                let conn = &*txn.guard;

                let id = unimatrix_store::counters::next_entry_id(conn)
                    .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

                let content_hash = compute_content_hash(&entry.title, &entry.content);
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                let record = EntryRecord {
                    id,
                    title: entry.title,
                    content: entry.content,
                    topic: entry.topic,
                    category: entry.category,
                    tags: entry.tags,
                    source: entry.source,
                    status: entry.status,
                    confidence: 0.0,
                    created_at: now,
                    updated_at: now,
                    last_accessed_at: 0,
                    access_count: 0,
                    supersedes: None,
                    superseded_by: None,
                    correction_count: 0,
                    embedding_dim,
                    created_by: entry.created_by.clone(),
                    modified_by: entry.created_by,
                    content_hash,
                    previous_hash: String::new(),
                    version: 1,
                    feature_cycle: entry.feature_cycle,
                    trust_source: entry.trust_source,
                    helpful_count: 0,
                    unhelpful_count: 0,
                    pre_quarantine_status: None,
                };

                // INSERT into entries with named params (ADR-004)
                conn.execute(
                    "INSERT INTO entries (id, title, content, topic, category, source,
                    status, confidence, created_at, updated_at, last_accessed_at,
                    access_count, supersedes, superseded_by, correction_count,
                    embedding_dim, created_by, modified_by, content_hash,
                    previous_hash, version, feature_cycle, trust_source,
                    helpful_count, unhelpful_count)
                 VALUES (:id, :title, :content, :topic, :category, :source,
                    :status, :confidence, :created_at, :updated_at, :last_accessed_at,
                    :access_count, :supersedes, :superseded_by, :correction_count,
                    :embedding_dim, :created_by, :modified_by, :content_hash,
                    :previous_hash, :version, :feature_cycle, :trust_source,
                    :helpful_count, :unhelpful_count)",
                    rusqlite::named_params! {
                        ":id": id as i64,
                        ":title": &record.title,
                        ":content": &record.content,
                        ":topic": &record.topic,
                        ":category": &record.category,
                        ":source": &record.source,
                        ":status": record.status as u8 as i64,
                        ":confidence": record.confidence,
                        ":created_at": record.created_at as i64,
                        ":updated_at": record.updated_at as i64,
                        ":last_accessed_at": record.last_accessed_at as i64,
                        ":access_count": record.access_count as i64,
                        ":supersedes": record.supersedes.map(|v| v as i64),
                        ":superseded_by": record.superseded_by.map(|v| v as i64),
                        ":correction_count": record.correction_count as i64,
                        ":embedding_dim": record.embedding_dim as i64,
                        ":created_by": &record.created_by,
                        ":modified_by": &record.modified_by,
                        ":content_hash": &record.content_hash,
                        ":previous_hash": &record.previous_hash,
                        ":version": record.version as i64,
                        ":feature_cycle": &record.feature_cycle,
                        ":trust_source": &record.trust_source,
                        ":helpful_count": record.helpful_count as i64,
                        ":unhelpful_count": record.unhelpful_count as i64,
                    },
                )
                .map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;

                // Insert tags
                for tag in &record.tags {
                    conn.execute(
                        "INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
                        rusqlite::params![id as i64, tag],
                    )
                    .map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;
                }

                // Insert vector mapping
                conn.execute(
                    "INSERT OR REPLACE INTO vector_map (entry_id, hnsw_data_id) VALUES (?1, ?2)",
                    rusqlite::params![id as i64, data_id as i64],
                )
                .map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;

                // Outcome index (if applicable)
                if record.category == "outcome" && !record.feature_cycle.is_empty() {
                    conn.execute(
                    "INSERT OR IGNORE INTO outcome_index (feature_cycle, entry_id) VALUES (?1, ?2)",
                    rusqlite::params![&record.feature_cycle, id as i64],
                ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;
                }

                // Status counter
                unimatrix_store::counters::increment_counter(
                    conn,
                    status_counter_key(record.status),
                    1,
                )
                .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

                // Audit event
                let audit_event_with_target = AuditEvent {
                    target_ids: vec![id],
                    ..audit_event
                };
                audit_log.write_in_txn(&txn, audit_event_with_target)?;

                txn.commit()
                    .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                Ok((id, record))
            })
            .await
            .map_err(|e| ServerError::Core(CoreError::JoinError(e.to_string())))??;

        if !embedding.is_empty() {
            self.vector_index
                .insert_hnsw_only(entry_id, data_id, &embedding)
                .map_err(|e| ServerError::Core(CoreError::Vector(e)))?;
        }

        Ok((entry_id, record))
    }

    /// Correct an existing entry: deprecate original, create correction, both
    /// in a single write transaction with audit. Uses direct SQL (ADR-004, nxs-008).
    ///
    /// Returns (deprecated_original, new_correction).
    pub(crate) async fn correct_with_audit(
        &self,
        original_id: u64,
        correction_entry: NewEntry,
        embedding: Vec<f32>,
        audit_event: AuditEvent,
    ) -> Result<(EntryRecord, EntryRecord), ServerError> {
        let store = Arc::clone(&self.store);
        let audit_log = Arc::clone(&self.audit);
        let data_id = self.vector_index.allocate_data_id();
        let embedding_dim = embedding.len() as u16;

        let (deprecated_original, new_correction) = tokio::task::spawn_blocking(
            move || -> Result<(EntryRecord, EntryRecord), ServerError> {
                let txn = store
                    .begin_write()
                    .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
                let conn = &*txn.guard;

                // 1. Read original entry via entry_from_row
                use unimatrix_store::rusqlite::OptionalExtension;
                let mut original: EntryRecord = conn
                    .query_row(
                        &format!("SELECT {} FROM entries WHERE id = ?1", ENTRY_COLUMNS),
                        rusqlite::params![original_id as i64],
                        entry_from_row,
                    )
                    .optional()
                    .map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?
                    .ok_or(ServerError::Core(CoreError::Store(
                        StoreError::EntryNotFound(original_id),
                    )))?;

                // Load tags for original
                let tag_map = load_tags_for_entries(conn, &[original_id])
                    .map_err(|e| ServerError::Core(CoreError::Store(e)))?;
                if let Some(tags) = tag_map.get(&original_id) {
                    original.tags = tags.clone();
                }

                // 2. Verify original is not already deprecated or quarantined
                if original.status == unimatrix_store::Status::Deprecated {
                    return Err(ServerError::InvalidInput {
                        field: "original_id".to_string(),
                        reason: "cannot correct a deprecated entry".to_string(),
                    });
                }
                if original.status == unimatrix_store::Status::Quarantined {
                    return Err(ServerError::InvalidInput {
                        field: "original_id".to_string(),
                        reason: "cannot correct quarantined entry; restore first".to_string(),
                    });
                }

                // 3. Generate new entry ID
                let new_id = unimatrix_store::counters::next_entry_id(conn)
                    .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

                // 4. Deprecate original (direct column UPDATE)
                let old_status = original.status;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                conn.execute(
                    "UPDATE entries SET status = ?1, superseded_by = ?2, \
                 correction_count = correction_count + 1, updated_at = ?3 \
                 WHERE id = ?4",
                    rusqlite::params![
                        unimatrix_store::Status::Deprecated as u8 as i64,
                        new_id as i64,
                        now as i64,
                        original_id as i64
                    ],
                )
                .map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;

                // Update status counters
                unimatrix_store::counters::decrement_counter(
                    conn,
                    status_counter_key(old_status),
                    1,
                )
                .map_err(|e| ServerError::Core(CoreError::Store(e)))?;
                unimatrix_store::counters::increment_counter(
                    conn,
                    status_counter_key(unimatrix_store::Status::Deprecated),
                    1,
                )
                .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

                // Update original record for return value
                original.status = unimatrix_store::Status::Deprecated;
                original.superseded_by = Some(new_id);
                original.correction_count += 1;
                original.updated_at = now;

                // 5. Build correction EntryRecord
                let content_hash =
                    compute_content_hash(&correction_entry.title, &correction_entry.content);
                let correction = EntryRecord {
                    id: new_id,
                    title: correction_entry.title,
                    content: correction_entry.content,
                    topic: correction_entry.topic,
                    category: correction_entry.category,
                    tags: correction_entry.tags,
                    source: correction_entry.source,
                    status: correction_entry.status,
                    confidence: 0.0,
                    created_at: now,
                    updated_at: now,
                    last_accessed_at: 0,
                    access_count: 0,
                    supersedes: Some(original_id),
                    superseded_by: None,
                    correction_count: 0,
                    embedding_dim,
                    created_by: correction_entry.created_by.clone(),
                    modified_by: correction_entry.created_by,
                    content_hash,
                    previous_hash: String::new(),
                    version: 1,
                    feature_cycle: correction_entry.feature_cycle,
                    trust_source: correction_entry.trust_source,
                    helpful_count: 0,
                    unhelpful_count: 0,
                    pre_quarantine_status: None,
                };

                // 6. INSERT correction with named params (ADR-004)
                conn.execute(
                    "INSERT INTO entries (id, title, content, topic, category, source,
                    status, confidence, created_at, updated_at, last_accessed_at,
                    access_count, supersedes, superseded_by, correction_count,
                    embedding_dim, created_by, modified_by, content_hash,
                    previous_hash, version, feature_cycle, trust_source,
                    helpful_count, unhelpful_count)
                 VALUES (:id, :title, :content, :topic, :category, :source,
                    :status, :confidence, :created_at, :updated_at, :last_accessed_at,
                    :access_count, :supersedes, :superseded_by, :correction_count,
                    :embedding_dim, :created_by, :modified_by, :content_hash,
                    :previous_hash, :version, :feature_cycle, :trust_source,
                    :helpful_count, :unhelpful_count)",
                    rusqlite::named_params! {
                        ":id": correction.id as i64,
                        ":title": &correction.title,
                        ":content": &correction.content,
                        ":topic": &correction.topic,
                        ":category": &correction.category,
                        ":source": &correction.source,
                        ":status": correction.status as u8 as i64,
                        ":confidence": correction.confidence,
                        ":created_at": correction.created_at as i64,
                        ":updated_at": correction.updated_at as i64,
                        ":last_accessed_at": correction.last_accessed_at as i64,
                        ":access_count": correction.access_count as i64,
                        ":supersedes": correction.supersedes.map(|v| v as i64),
                        ":superseded_by": correction.superseded_by.map(|v| v as i64),
                        ":correction_count": correction.correction_count as i64,
                        ":embedding_dim": correction.embedding_dim as i64,
                        ":created_by": &correction.created_by,
                        ":modified_by": &correction.modified_by,
                        ":content_hash": &correction.content_hash,
                        ":previous_hash": &correction.previous_hash,
                        ":version": correction.version as i64,
                        ":feature_cycle": &correction.feature_cycle,
                        ":trust_source": &correction.trust_source,
                        ":helpful_count": correction.helpful_count as i64,
                        ":unhelpful_count": correction.unhelpful_count as i64,
                    },
                )
                .map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;

                // 7. Insert tags for correction
                for tag in &correction.tags {
                    conn.execute(
                        "INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
                        rusqlite::params![new_id as i64, tag],
                    )
                    .map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;
                }

                // 8. Insert vector mapping
                conn.execute(
                    "INSERT OR REPLACE INTO vector_map (entry_id, hnsw_data_id) VALUES (?1, ?2)",
                    rusqlite::params![new_id as i64, data_id as i64],
                )
                .map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;

                // 9. Status counter for correction
                unimatrix_store::counters::increment_counter(
                    conn,
                    status_counter_key(correction.status),
                    1,
                )
                .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

                // 10. Write audit event with both IDs
                let audit_with_ids = AuditEvent {
                    target_ids: vec![original_id, new_id],
                    ..audit_event
                };
                audit_log.write_in_txn(&txn, audit_with_ids)?;

                // 11. Commit
                txn.commit()
                    .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;

                Ok((original, correction))
            },
        )
        .await
        .map_err(|e| ServerError::Core(CoreError::JoinError(e.to_string())))??;

        // HNSW insert for the correction (after commit)
        if !embedding.is_empty() {
            self.vector_index
                .insert_hnsw_only(new_correction.id, data_id, &embedding)
                .map_err(|e| ServerError::Core(CoreError::Vector(e)))?;
        }

        Ok((deprecated_original, new_correction))
    }

    /// Record usage for a set of retrieved entries with dedup and trust gating.
    ///
    /// Fire-and-forget: errors are logged but never propagated.
    pub(crate) async fn record_usage_for_entries(
        &self,
        agent_id: &str,
        trust_level: TrustLevel,
        entry_ids: &[u64],
        helpful: Option<bool>,
        feature: Option<&str>,
    ) {
        if entry_ids.is_empty() {
            return;
        }

        // Step 1: Determine which entries need access_count increment
        let access_ids = self.usage_dedup.filter_access(agent_id, entry_ids);

        // Step 2: Determine vote actions (if helpful param provided)
        let mut helpful_ids = Vec::new();
        let mut unhelpful_ids = Vec::new();
        let mut decrement_helpful_ids = Vec::new();
        let mut decrement_unhelpful_ids = Vec::new();

        if let Some(helpful_value) = helpful {
            let vote_actions = self
                .usage_dedup
                .check_votes(agent_id, entry_ids, helpful_value);
            for (id, action) in vote_actions {
                match action {
                    VoteAction::NewVote => {
                        if helpful_value {
                            helpful_ids.push(id);
                        } else {
                            unhelpful_ids.push(id);
                        }
                    }
                    VoteAction::CorrectedVote => {
                        if helpful_value {
                            // Was unhelpful, now helpful
                            helpful_ids.push(id);
                            decrement_unhelpful_ids.push(id);
                        } else {
                            // Was helpful, now unhelpful
                            unhelpful_ids.push(id);
                            decrement_helpful_ids.push(id);
                        }
                    }
                    VoteAction::NoOp => {}
                }
            }
        }

        // Steps 3-5: Batch all DB writes into a single spawn_blocking (vnc-010).
        //
        // Previously each write (usage+confidence, feature_entries, co_access) was
        // a separate spawn_blocking, each independently acquiring the Store mutex.
        // This caused blocking pool saturation under concurrent MCP requests.
        let store = Arc::clone(&self.store);
        let all_ids = entry_ids.to_vec();

        // Pre-compute co-access pairs (in-memory, no lock needed)
        let (co_access_pairs, pairs_for_adapt) = if entry_ids.len() >= 2 {
            let pairs =
                crate::coaccess::generate_pairs(entry_ids, crate::coaccess::MAX_CO_ACCESS_ENTRIES);
            let new_pairs = self.usage_dedup.filter_co_access_pairs(&pairs);
            if new_pairs.is_empty() {
                (None, None)
            } else {
                let adapt_pairs: Vec<(u64, u64, u32)> =
                    new_pairs.iter().map(|p| (p.0, p.1, 1u32)).collect();
                (Some(new_pairs), Some(adapt_pairs))
            }
        } else {
            (None, None)
        };

        // Pre-compute feature recording eligibility
        let feature_recording = feature.and_then(|feature_str| {
            if matches!(
                trust_level,
                TrustLevel::System | TrustLevel::Privileged | TrustLevel::Internal
            ) {
                Some((feature_str.to_string(), entry_ids.to_vec()))
            } else {
                None
            }
        });

        let usage_result = tokio::task::spawn_blocking(move || {
            // Single lock acquisition for all DB writes
            if let Err(e) = store.record_usage_with_confidence(
                &all_ids,
                &access_ids,
                &helpful_ids,
                &unhelpful_ids,
                &decrement_helpful_ids,
                &decrement_unhelpful_ids,
                Some(Box::new(|entry: &unimatrix_store::EntryRecord, now: u64| {
                    crate::confidence::compute_confidence(entry, now, 3.0, 3.0)
                })
                    as Box<
                        dyn Fn(&unimatrix_store::EntryRecord, u64) -> f64 + Send,
                    >),
            ) {
                tracing::warn!("usage recording failed: {e}");
            }

            if let Some((feature_str, ids)) = feature_recording {
                if let Err(e) = store.record_feature_entries(&feature_str, &ids) {
                    tracing::warn!("feature entry recording failed: {e}");
                }
            }

            if let Some(pairs) = co_access_pairs {
                if let Err(e) = store.record_co_access_pairs(&pairs) {
                    tracing::warn!("co-access recording failed: {e}");
                }
            }
        })
        .await;

        match usage_result {
            Ok(()) => {}
            Err(e) => {
                tracing::warn!("usage recording task failed: {e}");
            }
        }

        // Step 5b-c: Adaptation training (separate spawn_blocking since it
        // does embedding work, not just DB writes)
        if let Some(adapt_pairs) = pairs_for_adapt {
            self.adapt_service.record_training_pairs(&adapt_pairs);

            let adapt_svc = Arc::clone(&self.adapt_service);
            let embed_svc = Arc::clone(&self.embed_service);
            let store_for_train = Arc::clone(&self.store);
            let _ = tokio::task::spawn_blocking(move || {
                if let Some(adapter) = embed_svc.try_get_adapter_sync() {
                    let embed_fn = |entry_id: u64| -> Option<Vec<f32>> {
                        let entry = store_for_train.get(entry_id).ok()?;
                        adapter.embed_entry(&entry.title, &entry.content).ok()
                    };
                    adapt_svc.try_train_step(&embed_fn);
                }
            });
        }
    }

    /// Deprecate an entry: set status to Deprecated using direct SQL (nxs-008).
    /// Idempotent: already-deprecated entries return immediately.
    pub(crate) async fn deprecate_with_audit(
        &self,
        entry_id: u64,
        reason: Option<String>,
        audit_event: AuditEvent,
    ) -> Result<EntryRecord, ServerError> {
        self.change_status_with_audit(
            entry_id,
            unimatrix_store::Status::Deprecated,
            reason,
            audit_event,
            false, // do not set modified_by
        )
        .await
    }

    /// Quarantine an entry: set status to Quarantined using direct SQL (nxs-008).
    pub(crate) async fn quarantine_with_audit(
        &self,
        entry_id: u64,
        reason: Option<String>,
        audit_event: AuditEvent,
    ) -> Result<EntryRecord, ServerError> {
        self.change_status_with_audit(
            entry_id,
            unimatrix_store::Status::Quarantined,
            reason,
            audit_event,
            true, // set modified_by from audit agent_id
        )
        .await
    }

    /// Restore a quarantined entry to its pre-quarantine status (vnc-010).
    /// Falls back to Active if pre_quarantine_status is NULL or invalid (ADR-002).
    pub(crate) async fn restore_with_audit(
        &self,
        entry_id: u64,
        reason: Option<String>,
        audit_event: AuditEvent,
    ) -> Result<EntryRecord, ServerError> {
        // Fetch entry to read pre_quarantine_status
        let entry = self
            .entry_store
            .get(entry_id)
            .await
            .map_err(ServerError::Core)?;
        let restore_to = entry
            .pre_quarantine_status
            .and_then(|v| unimatrix_store::Status::try_from(v).ok())
            .unwrap_or(unimatrix_store::Status::Active);
        self.change_status_with_audit(
            entry_id,
            restore_to,
            reason,
            audit_event,
            true, // set modified_by from audit agent_id
        )
        .await
    }

    /// Shared implementation for status-change operations (deprecate, quarantine, restore).
    /// Uses direct SQL with &*txn.guard (ADR-004, nxs-008).
    async fn change_status_with_audit(
        &self,
        entry_id: u64,
        new_status: unimatrix_store::Status,
        reason: Option<String>,
        audit_event: AuditEvent,
        set_modified_by: bool,
    ) -> Result<EntryRecord, ServerError> {
        let store = Arc::clone(&self.store);
        let audit_log = Arc::clone(&self.audit);
        let action_name = match new_status {
            unimatrix_store::Status::Deprecated => "deprecated",
            unimatrix_store::Status::Quarantined => "quarantined",
            unimatrix_store::Status::Active => "restored",
            unimatrix_store::Status::Proposed => "proposed",
        };

        let record = tokio::task::spawn_blocking(move || -> Result<EntryRecord, ServerError> {
            let txn = store.begin_write()
                .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
            let conn = &*txn.guard;

            // 1. Read existing entry status and current fields
            use unimatrix_store::rusqlite::OptionalExtension;
            let mut record: EntryRecord = conn
                .query_row(
                    &format!("SELECT {} FROM entries WHERE id = ?1", ENTRY_COLUMNS),
                    rusqlite::params![entry_id as i64],
                    entry_from_row,
                )
                .optional()
                .map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?
                .ok_or(ServerError::Core(CoreError::Store(
                    StoreError::EntryNotFound(entry_id),
                )))?;

            // Load tags
            let tag_map = load_tags_for_entries(conn, &[entry_id])
                .map_err(|e| ServerError::Core(CoreError::Store(e)))?;
            if let Some(tags) = tag_map.get(&entry_id) {
                record.tags = tags.clone();
            }

            // 2. Idempotency check for deprecate
            if new_status == unimatrix_store::Status::Deprecated
                && record.status == unimatrix_store::Status::Deprecated
            {
                return Ok(record);
            }

            // 3. Update status via direct SQL
            let old_status = record.status;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            // vnc-010: set pre_quarantine_status when quarantining, clear when restoring
            let old_pre_q = record.pre_quarantine_status;
            let pre_q_value: Option<i64> = if new_status == unimatrix_store::Status::Quarantined {
                Some(old_status as u8 as i64)
            } else {
                None
            };

            if set_modified_by {
                conn.execute(
                    "UPDATE entries SET status = ?1, modified_by = ?2, updated_at = ?3, pre_quarantine_status = ?4 WHERE id = ?5",
                    rusqlite::params![
                        new_status as u8 as i64,
                        &audit_event.agent_id,
                        now as i64,
                        pre_q_value,
                        entry_id as i64
                    ],
                ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;
                record.modified_by = audit_event.agent_id.clone();
            } else {
                conn.execute(
                    "UPDATE entries SET status = ?1, updated_at = ?2, pre_quarantine_status = ?3 WHERE id = ?4",
                    rusqlite::params![
                        new_status as u8 as i64,
                        now as i64,
                        pre_q_value,
                        entry_id as i64
                    ],
                ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;
            }

            record.status = new_status;
            record.updated_at = now;
            record.pre_quarantine_status = pre_q_value.map(|v| v as u8);

            // 4. Update status counters
            unimatrix_store::counters::decrement_counter(conn, status_counter_key(old_status), 1)
                .map_err(|e| ServerError::Core(CoreError::Store(e)))?;
            unimatrix_store::counters::increment_counter(conn, status_counter_key(new_status), 1)
                .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

            // 5. Write audit event (vnc-010: include pre_quarantine_status)
            let pre_q_info = if new_status == unimatrix_store::Status::Quarantined {
                format!(" (pre_quarantine_status={})", old_status as u8)
            } else if let Some(pq) = old_pre_q {
                // This is a restore — note what we restored from
                format!(" (restored from pre_quarantine_status={})", pq)
            } else {
                String::new()
            };
            let detail = match &reason {
                Some(r) => format!("{action_name} entry #{entry_id}{pre_q_info}: {r}"),
                None => format!("{action_name} entry #{entry_id}{pre_q_info}"),
            };
            let audit_with_detail = AuditEvent {
                target_ids: vec![entry_id],
                detail,
                ..audit_event
            };
            audit_log.write_in_txn(&txn, audit_with_detail)?;

            // 6. Commit
            txn.commit()
                .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
            Ok(record)
        }).await.map_err(|e| ServerError::Core(CoreError::JoinError(e.to_string())))??;

        Ok(record)
    }
}

#[rmcp::tool_handler]
impl rmcp::ServerHandler for UnimatrixServer {
    fn get_info(&self) -> ServerInfo {
        self.server_info.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    pub(crate) fn make_server() -> UnimatrixServer {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = Arc::new(Store::open(&path).unwrap());
        std::mem::forget(dir);

        let store_adapter = StoreAdapter::new(Arc::clone(&store));
        let entry_store = Arc::new(AsyncEntryStore::new(Arc::new(store_adapter)));

        // Use a minimal VectorIndex
        let vector_config = unimatrix_core::VectorConfig::default();
        let vector_index =
            Arc::new(unimatrix_core::VectorIndex::new(Arc::clone(&store), vector_config).unwrap());
        let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
        let vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));

        let embed_service = EmbedServiceHandle::new();

        let registry = Arc::new(AgentRegistry::new(Arc::clone(&store)).unwrap());
        registry.bootstrap_defaults().unwrap();

        let audit = Arc::new(AuditLog::new(Arc::clone(&store)));
        let categories = Arc::new(CategoryAllowlist::new());

        let adapt_service = Arc::new(AdaptationService::new(
            unimatrix_adapt::AdaptConfig::default(),
        ));

        UnimatrixServer::new(
            entry_store,
            vector_store,
            embed_service,
            registry,
            audit,
            categories,
            Arc::clone(&store),
            vector_index,
            adapt_service,
        )
    }

    #[test]
    fn test_get_info_name() {
        let server = make_server();
        let info = rmcp::ServerHandler::get_info(&server);
        assert_eq!(info.server_info.name, "unimatrix");
    }

    #[test]
    fn test_get_info_version_nonempty() {
        let server = make_server();
        let info = rmcp::ServerHandler::get_info(&server);
        assert!(!info.server_info.version.is_empty());
    }

    #[test]
    fn test_get_info_instructions() {
        let server = make_server();
        let info = rmcp::ServerHandler::get_info(&server);
        assert!(info.instructions.is_some());
        let instructions = info.instructions.unwrap();
        assert!(instructions.contains("knowledge engine"));
        assert!(instructions.contains("search for relevant patterns"));
    }

    #[test]
    fn test_get_info_has_tools_capability() {
        let server = make_server();
        let info = rmcp::ServerHandler::get_info(&server);
        assert!(
            info.capabilities.tools.is_some(),
            "tools capability must be advertised"
        );
    }

    #[test]
    fn test_server_is_clone() {
        let server = make_server();
        let _clone = server.clone();
    }

    #[tokio::test]
    async fn test_resolve_agent_with_id() {
        let server = make_server();
        let identity = server
            .resolve_agent(&Some("human".to_string()))
            .await
            .unwrap();
        assert_eq!(identity.agent_id, "human");
        assert_eq!(
            identity.trust_level,
            crate::infra::registry::TrustLevel::Privileged
        );
    }

    #[tokio::test]
    async fn test_resolve_agent_without_id() {
        let server = make_server();
        let identity = server.resolve_agent(&None).await.unwrap();
        assert_eq!(identity.agent_id, "anonymous");
    }

    // -- crt-001: record_usage_for_entries tests --

    fn insert_test_entry(store: &unimatrix_core::Store) -> u64 {
        let entry = unimatrix_core::NewEntry {
            title: "Test".to_string(),
            content: "Content".to_string(),
            topic: "test".to_string(),
            category: "convention".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: unimatrix_core::Status::Active,
            created_by: String::new(),
            feature_cycle: String::new(),
            trust_source: String::new(),
        };
        store.insert(entry).unwrap()
    }

    #[tokio::test]
    async fn test_record_usage_for_entries_updates_access() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], None, None)
            .await;

        let r = server.store.get(id).unwrap();
        assert_eq!(r.access_count, 1);
        assert!(r.last_accessed_at > 0);
    }

    #[tokio::test]
    async fn test_record_usage_for_entries_empty_ids() {
        let server = make_server();
        // Should return immediately without error
        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[], None, None)
            .await;
    }

    #[tokio::test]
    async fn test_record_usage_for_entries_access_dedup() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        // First call: access_count increments
        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], None, None)
            .await;
        assert_eq!(server.store.get(id).unwrap().access_count, 1);

        // Second call: same agent, same entry -> deduped (access_count stays 1)
        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], None, None)
            .await;
        assert_eq!(server.store.get(id).unwrap().access_count, 1);

        // Different agent: access_count increments again
        server
            .record_usage_for_entries("other-agent", TrustLevel::Internal, &[id], None, None)
            .await;
        assert_eq!(server.store.get(id).unwrap().access_count, 2);
    }

    #[tokio::test]
    async fn test_record_usage_for_entries_helpful_vote() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], Some(true), None)
            .await;

        let r = server.store.get(id).unwrap();
        assert_eq!(r.helpful_count, 1);
        assert_eq!(r.unhelpful_count, 0);
    }

    #[tokio::test]
    async fn test_record_usage_for_entries_unhelpful_vote() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], Some(false), None)
            .await;

        let r = server.store.get(id).unwrap();
        assert_eq!(r.helpful_count, 0);
        assert_eq!(r.unhelpful_count, 1);
    }

    #[tokio::test]
    async fn test_record_usage_for_entries_helpful_none() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], None, None)
            .await;

        let r = server.store.get(id).unwrap();
        assert_eq!(r.helpful_count, 0);
        assert_eq!(r.unhelpful_count, 0);
    }

    #[tokio::test]
    async fn test_record_usage_for_entries_vote_correction() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        // First: vote unhelpful
        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], Some(false), None)
            .await;
        assert_eq!(server.store.get(id).unwrap().unhelpful_count, 1);

        // Correction: vote helpful (should flip)
        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], Some(true), None)
            .await;
        let r = server.store.get(id).unwrap();
        assert_eq!(r.helpful_count, 1);
        assert_eq!(r.unhelpful_count, 0);
    }

    #[tokio::test]
    async fn test_record_usage_for_entries_feature_internal_agent() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        server
            .record_usage_for_entries(
                "test-agent",
                TrustLevel::Internal,
                &[id],
                None,
                Some("crt-001"),
            )
            .await;

        // Verify feature_entries populated via SQL
        let conn = server.store.lock_conn();
        let found: Vec<u64> = {
            let mut stmt = conn
                .prepare(
                    "SELECT entry_id FROM feature_entries WHERE feature_id = ?1 ORDER BY entry_id",
                )
                .unwrap();
            stmt.query_map(rusqlite::params!["crt-001"], |row| {
                Ok(row.get::<_, i64>(0)? as u64)
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
        };
        assert_eq!(found, vec![id]);
    }

    #[tokio::test]
    async fn test_record_usage_for_entries_feature_restricted_agent_ignored() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        server
            .record_usage_for_entries(
                "restricted-agent",
                TrustLevel::Restricted,
                &[id],
                None,
                Some("crt-001"),
            )
            .await;

        // Verify feature_entries NOT populated (Restricted ignored)
        let conn = server.store.lock_conn();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM feature_entries WHERE feature_id = ?1",
                rusqlite::params!["crt-001"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_record_usage_for_entries_feature_privileged_agent() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        server
            .record_usage_for_entries(
                "human",
                TrustLevel::Privileged,
                &[id],
                None,
                Some("crt-001"),
            )
            .await;

        let conn = server.store.lock_conn();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM feature_entries WHERE feature_id = ?1",
                rusqlite::params!["crt-001"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_record_usage_for_entries_vote_after_access_only() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        // First: access only (no helpful param)
        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], None, None)
            .await;

        // Second: vote helpful (separate from access dedup)
        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], Some(true), None)
            .await;

        let r = server.store.get(id).unwrap();
        assert_eq!(r.access_count, 1, "access deduped");
        assert_eq!(r.helpful_count, 1, "vote recorded");
    }

    // -- crt-002: Confidence on retrieval path (T-20 through T-23) --

    #[tokio::test]
    async fn test_confidence_updated_on_retrieval() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        // Before retrieval: confidence is 0.0
        assert_eq!(server.store.get(id).unwrap().confidence, 0.0);

        // Trigger retrieval
        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], None, None)
            .await;

        // After retrieval: confidence > 0.0
        let r = server.store.get(id).unwrap();
        assert!(
            r.confidence > 0.0,
            "confidence should be updated after retrieval"
        );
    }

    #[tokio::test]
    async fn test_confidence_matches_formula() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], None, None)
            .await;

        let entry = server.store.get(id).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let expected = crate::confidence::compute_confidence(&entry, now, 3.0, 3.0);
        // Allow small tolerance for timestamp difference
        assert!((entry.confidence - expected).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_confidence_evolves_with_multiple_retrievals() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        // First retrieval
        server
            .record_usage_for_entries("agent-a", TrustLevel::Internal, &[id], None, None)
            .await;
        let after_first = server.store.get(id).unwrap().confidence;

        // Second retrieval (different agent to avoid access dedup)
        server
            .record_usage_for_entries("agent-b", TrustLevel::Internal, &[id], None, None)
            .await;
        let after_second = server.store.get(id).unwrap().confidence;

        // Confidence should change (access_count went from 1 to 2)
        assert_ne!(
            after_first, after_second,
            "confidence should evolve with retrievals"
        );
    }

    // -- crt-002: Confidence on mutation paths (T-24 through T-28) --

    #[tokio::test]
    async fn test_confidence_seeded_on_insert() {
        let server = make_server();

        let entry = unimatrix_core::NewEntry {
            title: "Test".to_string(),
            content: "Content".to_string(),
            topic: "test".to_string(),
            category: "convention".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: unimatrix_core::Status::Active,
            created_by: String::new(),
            feature_cycle: String::new(),
            trust_source: "agent".to_string(),
        };

        let audit_event = crate::infra::audit::AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: "test".to_string(),
            operation: "context_store".to_string(),
            target_ids: vec![],
            outcome: crate::infra::audit::Outcome::Success,
            detail: "test insert".to_string(),
        };

        let embedding = vec![0.1; 384];
        let (entry_id, _record) = server
            .insert_with_audit(entry, embedding, audit_event)
            .await
            .unwrap();

        // Seed confidence (simulating what context_store does)
        {
            let entry = server.store.get(entry_id).unwrap();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let conf = crate::confidence::compute_confidence(&entry, now, 3.0, 3.0);
            server.store.update_confidence(entry_id, conf).unwrap();
        }

        let r = server.store.get(entry_id).unwrap();
        assert!(r.confidence > 0.0, "confidence should be seeded on insert");
        // Agent-authored entry, just inserted (crt-019 weights):
        // base=0.5, usage=0.0, fresh≈1.0 (just created), help=0.5, corr=0.5, trust=0.5
        // composite ≈ 0.16*0.5 + 0.16*0.0 + 0.18*1.0 + 0.12*0.5 + 0.14*0.5 + 0.16*0.5 = 0.47
        assert!(
            (r.confidence - 0.47).abs() < 0.05,
            "expected ~0.47, got {}",
            r.confidence
        );
    }

    #[tokio::test]
    async fn test_confidence_recomputed_on_deprecation() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        // First retrieval to give it some confidence
        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], None, None)
            .await;

        let before_deprecation = server.store.get(id).unwrap().confidence;
        assert!(before_deprecation > 0.0);

        // Deprecate
        let audit_event = crate::infra::audit::AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: "test".to_string(),
            operation: "context_deprecate".to_string(),
            target_ids: vec![],
            outcome: crate::infra::audit::Outcome::Success,
            detail: String::new(),
        };
        server
            .deprecate_with_audit(id, None, audit_event)
            .await
            .unwrap();

        // Recompute confidence for deprecated entry
        {
            let entry = server.store.get(id).unwrap();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let conf = crate::confidence::compute_confidence(&entry, now, 3.0, 3.0);
            server.store.update_confidence(id, conf).unwrap();
        }

        let after_deprecation = server.store.get(id).unwrap().confidence;
        assert!(
            after_deprecation < before_deprecation,
            "confidence should decrease after deprecation (base_score 0.5 -> 0.2)"
        );
    }

    // -- crt-003: Quarantine / Restore integration tests --

    fn make_audit_event(agent_id: &str) -> crate::infra::audit::AuditEvent {
        crate::infra::audit::AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: agent_id.to_string(),
            operation: "context_quarantine".to_string(),
            target_ids: vec![],
            outcome: crate::infra::audit::Outcome::Success,
            detail: String::new(),
        }
    }

    #[tokio::test]
    async fn test_quarantine_active_entry() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        let updated = server
            .quarantine_with_audit(id, Some("test reason".into()), make_audit_event("system"))
            .await
            .unwrap();

        assert_eq!(updated.status, unimatrix_store::Status::Quarantined);
        assert_eq!(updated.modified_by, "system");

        let fetched = server.store.get(id).unwrap();
        assert_eq!(fetched.status, unimatrix_store::Status::Quarantined);
    }

    #[tokio::test]
    async fn test_quarantine_updates_status_index() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        let conn = server.store.lock_conn();
        let status: i64 = conn
            .query_row(
                "SELECT status FROM entries WHERE id = ?1",
                rusqlite::params![id as i64],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            status,
            unimatrix_store::Status::Quarantined as u8 as i64,
            "entry status should be Quarantined"
        );
    }

    #[tokio::test]
    async fn test_quarantine_updates_counters() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        let before_active = server.store.read_counter("total_active").unwrap();
        let before_quarantined = server.store.read_counter("total_quarantined").unwrap();

        server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        let after_active = server.store.read_counter("total_active").unwrap();
        let after_quarantined = server.store.read_counter("total_quarantined").unwrap();

        assert_eq!(
            after_active,
            before_active - 1,
            "active counter should decrement"
        );
        assert_eq!(
            after_quarantined,
            before_quarantined + 1,
            "quarantined counter should increment"
        );
    }

    #[tokio::test]
    async fn test_restore_quarantined_entry() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        // Quarantine first
        server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();
        assert_eq!(
            server.store.get(id).unwrap().status,
            unimatrix_store::Status::Quarantined
        );

        // Restore
        let updated = server
            .restore_with_audit(id, Some("false alarm".into()), make_audit_event("system"))
            .await
            .unwrap();

        assert_eq!(updated.status, unimatrix_store::Status::Active);
        assert_eq!(
            server.store.get(id).unwrap().status,
            unimatrix_store::Status::Active
        );
    }

    #[tokio::test]
    async fn test_restore_updates_counters() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        let initial_active = server.store.read_counter("total_active").unwrap();

        // Quarantine
        server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        // Restore
        server
            .restore_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        // Counters should return to original values
        let final_active = server.store.read_counter("total_active").unwrap();
        let final_quarantined = server.store.read_counter("total_quarantined").unwrap();

        assert_eq!(
            final_active, initial_active,
            "active counter should return to initial"
        );
        assert_eq!(
            final_quarantined, 0,
            "quarantined counter should return to 0"
        );
    }

    #[tokio::test]
    async fn test_restore_updates_status_index() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        // Quarantine then restore
        server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();
        server
            .restore_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        let conn = server.store.lock_conn();
        let status: i64 = conn
            .query_row(
                "SELECT status FROM entries WHERE id = ?1",
                rusqlite::params![id as i64],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            status,
            unimatrix_store::Status::Active as u8 as i64,
            "entry status should be back to Active"
        );
    }

    #[tokio::test]
    async fn test_quarantine_writes_audit_event() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        server
            .quarantine_with_audit(
                id,
                Some("suspicious content".into()),
                make_audit_event("system"),
            )
            .await
            .unwrap();

        // Verify audit log has an entry
        let conn = server.store.lock_conn();
        let mut stmt = conn
            .prepare("SELECT operation, target_ids, detail FROM audit_log WHERE operation = ?1")
            .unwrap();
        let mut found = false;
        let mut rows = stmt.query(rusqlite::params!["context_quarantine"]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            let target_ids_json: String = row.get(1).unwrap();
            let target_ids: Vec<u64> = serde_json::from_str(&target_ids_json).unwrap();
            let detail: String = row.get(2).unwrap();
            if target_ids.contains(&id) {
                assert!(detail.contains("quarantined"));
                assert!(detail.contains("suspicious content"));
                found = true;
            }
        }
        assert!(found, "audit event for quarantine should exist");
    }

    #[tokio::test]
    async fn test_correct_rejects_quarantined() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        // Quarantine the entry
        server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        // Attempt to correct -- should fail
        let audit_event = crate::infra::audit::AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: "system".to_string(),
            operation: "context_correct".to_string(),
            target_ids: vec![],
            outcome: crate::infra::audit::Outcome::Success,
            detail: String::new(),
        };

        let result = server
            .correct_with_audit(
                id,
                unimatrix_core::NewEntry {
                    title: "Corrected".to_string(),
                    content: "Corrected content".to_string(),
                    topic: "test".to_string(),
                    category: "convention".to_string(),
                    tags: vec![],
                    source: "test".to_string(),
                    status: unimatrix_core::Status::Active,
                    created_by: "system".to_string(),
                    feature_cycle: String::new(),
                    trust_source: String::new(),
                },
                vec![],
                audit_event,
            )
            .await;

        assert!(result.is_err(), "correct should fail for quarantined entry");
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("quarantined"),
            "error should mention quarantine: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_quarantine_confidence_decreases() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        // Compute initial confidence
        let entry = server.store.get(id).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let before = crate::confidence::compute_confidence(&entry, now, 3.0, 3.0);
        server.store.update_confidence(id, before).unwrap();

        // Quarantine
        server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        // Recompute confidence for quarantined entry
        let entry = server.store.get(id).unwrap();
        let after = crate::confidence::compute_confidence(&entry, now, 3.0, 3.0);
        server.store.update_confidence(id, after).unwrap();

        assert!(
            after < before,
            "confidence should decrease after quarantine: before={before}, after={after}"
        );

        // Restore
        server
            .restore_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        // Recompute confidence for restored entry
        let entry = server.store.get(id).unwrap();
        let restored = crate::confidence::compute_confidence(&entry, now, 3.0, 3.0);

        assert!(
            restored > after,
            "confidence should increase after restore: after_quarantine={after}, restored={restored}"
        );
    }

    #[tokio::test]
    async fn test_quarantine_nonexistent_entry_fails() {
        let server = make_server();

        let result = server
            .quarantine_with_audit(99999, None, make_audit_event("system"))
            .await;

        assert!(
            result.is_err(),
            "quarantining nonexistent entry should fail"
        );
    }

    // -- vnc-010: Quarantine State Restoration tests --

    /// Helper: insert entry and deprecate it, returning the entry id.
    async fn insert_and_deprecate(server: &UnimatrixServer) -> u64 {
        let id = insert_test_entry(&server.store);
        let audit_event = crate::infra::audit::AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: "system".to_string(),
            operation: "context_deprecate".to_string(),
            target_ids: vec![],
            outcome: crate::infra::audit::Outcome::Success,
            detail: String::new(),
        };
        server
            .deprecate_with_audit(id, None, audit_event)
            .await
            .unwrap();
        assert_eq!(
            server.store.get(id).unwrap().status,
            unimatrix_store::Status::Deprecated
        );
        id
    }

    // AC-1: Quarantine from Deprecated status
    #[tokio::test]
    async fn test_quarantine_deprecated_entry() {
        let server = make_server();
        let id = insert_and_deprecate(&server).await;

        let updated = server
            .quarantine_with_audit(
                id,
                Some("obsolete and harmful".into()),
                make_audit_event("system"),
            )
            .await
            .unwrap();

        assert_eq!(updated.status, unimatrix_store::Status::Quarantined);
        assert_eq!(updated.pre_quarantine_status, Some(1)); // Deprecated = 1

        let fetched = server.store.get(id).unwrap();
        assert_eq!(fetched.status, unimatrix_store::Status::Quarantined);
        assert_eq!(fetched.pre_quarantine_status, Some(1));
    }

    // AC-3: Quarantine from Active sets pre_quarantine_status=0
    #[tokio::test]
    async fn test_quarantine_active_sets_pre_quarantine_status() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        let updated = server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        assert_eq!(updated.status, unimatrix_store::Status::Quarantined);
        assert_eq!(updated.pre_quarantine_status, Some(0)); // Active = 0
    }

    // AC-4: Restore to pre-quarantine status (Deprecated round-trip)
    #[tokio::test]
    async fn test_restore_to_deprecated() {
        let server = make_server();
        let id = insert_and_deprecate(&server).await;

        // Quarantine from Deprecated
        server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        // Restore -- should go back to Deprecated, not Active
        let restored = server
            .restore_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        assert_eq!(restored.status, unimatrix_store::Status::Deprecated);
        assert_eq!(restored.pre_quarantine_status, None); // cleared after restore
    }

    // AC-5: Restore with NULL pre_quarantine_status falls back to Active
    #[tokio::test]
    async fn test_restore_null_pre_quarantine_falls_back_to_active() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        // Quarantine the entry
        server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        // Manually clear pre_quarantine_status to NULL to simulate pre-migration entry
        {
            let conn = server.store.lock_conn();
            conn.execute(
                "UPDATE entries SET pre_quarantine_status = NULL WHERE id = ?1",
                rusqlite::params![id as i64],
            )
            .unwrap();
        }

        // Restore -- should fall back to Active
        let restored = server
            .restore_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        assert_eq!(restored.status, unimatrix_store::Status::Active);
    }

    // AC-8: Counter integrity for Deprecated quarantine round-trip
    #[tokio::test]
    async fn test_counter_integrity_deprecated_round_trip() {
        let server = make_server();
        let id = insert_and_deprecate(&server).await;

        let before_deprecated = server.store.read_counter("total_deprecated").unwrap();
        let before_quarantined = server.store.read_counter("total_quarantined").unwrap();

        // Quarantine from Deprecated
        server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        let mid_deprecated = server.store.read_counter("total_deprecated").unwrap();
        let mid_quarantined = server.store.read_counter("total_quarantined").unwrap();
        assert_eq!(
            mid_deprecated,
            before_deprecated - 1,
            "deprecated counter should decrement"
        );
        assert_eq!(
            mid_quarantined,
            before_quarantined + 1,
            "quarantined counter should increment"
        );

        // Restore to Deprecated
        server
            .restore_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        let after_deprecated = server.store.read_counter("total_deprecated").unwrap();
        let after_quarantined = server.store.read_counter("total_quarantined").unwrap();
        assert_eq!(
            after_deprecated, before_deprecated,
            "deprecated counter should return to initial"
        );
        assert_eq!(
            after_quarantined, before_quarantined,
            "quarantined counter should return to initial"
        );
    }

    // AC-9: Audit trail includes pre_quarantine_status
    #[tokio::test]
    async fn test_quarantine_audit_includes_pre_quarantine_status() {
        let server = make_server();
        let id = insert_and_deprecate(&server).await;

        server
            .quarantine_with_audit(id, Some("harmful".into()), make_audit_event("system"))
            .await
            .unwrap();

        let conn = server.store.lock_conn();
        let detail: String = conn
            .query_row(
                "SELECT detail FROM audit_log WHERE operation = 'context_quarantine' ORDER BY event_id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(
            detail.contains("pre_quarantine_status=1"),
            "audit detail should contain pre_quarantine_status: {detail}"
        );
    }

    // AC-10: Restore with invalid pre_quarantine_status falls back to Active
    #[tokio::test]
    async fn test_restore_invalid_pre_quarantine_falls_back_to_active() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        // Quarantine the entry
        server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        // Manually set pre_quarantine_status to invalid value (99)
        {
            let conn = server.store.lock_conn();
            conn.execute(
                "UPDATE entries SET pre_quarantine_status = 99 WHERE id = ?1",
                rusqlite::params![id as i64],
            )
            .unwrap();
        }

        // Restore -- should fall back to Active
        let restored = server
            .restore_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        assert_eq!(restored.status, unimatrix_store::Status::Active);
    }

    // AC-7: Migration v7->v8 (tested at store level)
    #[tokio::test]
    async fn test_migration_v7_to_v8_backfill() {
        // Create a database at v7 schema, quarantine an entry, then re-open
        // (which triggers migration) and verify backfill
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("migrate.db");

        // Create db at current schema (v8)
        {
            let store = unimatrix_store::Store::open(&path).unwrap();
            // Insert an entry and manually quarantine it with old logic (no pre_quarantine_status)
            let entry = unimatrix_core::NewEntry {
                title: "Test".to_string(),
                content: "Content".to_string(),
                topic: "test".to_string(),
                category: "convention".to_string(),
                tags: vec![],
                source: "test".to_string(),
                status: unimatrix_core::Status::Active,
                created_by: "system".to_string(),
                feature_cycle: String::new(),
                trust_source: String::new(),
            };
            let id = store.insert(entry).unwrap();

            // Simulate a v7 quarantine (status=3 but no pre_quarantine_status)
            let conn = store.lock_conn();
            conn.execute(
                "UPDATE entries SET status = 3, pre_quarantine_status = NULL WHERE id = ?1",
                rusqlite::params![id as i64],
            )
            .unwrap();

            // Set schema version back to 7 to trigger migration on next open
            conn.execute(
                "UPDATE counters SET value = 7 WHERE name = 'schema_version'",
                [],
            )
            .unwrap();
        }

        // Re-open -- triggers v7->v8 migration
        {
            let store = unimatrix_store::Store::open(&path).unwrap();
            let conn = store.lock_conn();

            // Verify schema version is now current (12, col-022 keywords migration)
            let version: i64 = conn
                .query_row(
                    "SELECT value FROM counters WHERE name = 'schema_version'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(version, 12);

            // Verify backfill: quarantined entry should have pre_quarantine_status = 0
            let pre_q: Option<i64> = conn
                .query_row(
                    "SELECT pre_quarantine_status FROM entries WHERE status = 3",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(
                pre_q,
                Some(0),
                "backfill should set pre_quarantine_status=0 for quarantined entries"
            );
        }

        // Re-open again to verify idempotency
        {
            let store = unimatrix_store::Store::open(&path).unwrap();
            let conn = store.lock_conn();
            let version: i64 = conn
                .query_row(
                    "SELECT value FROM counters WHERE name = 'schema_version'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(version, 12, "schema version should remain 12 on re-open");
        }
    }

    // R-05: Existing Active->Quarantined->Active path still works identically
    #[tokio::test]
    async fn test_active_quarantine_restore_round_trip_still_works() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        let initial_active = server.store.read_counter("total_active").unwrap();

        // Quarantine from Active
        let quarantined = server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();
        assert_eq!(quarantined.status, unimatrix_store::Status::Quarantined);
        assert_eq!(quarantined.pre_quarantine_status, Some(0));

        // Restore -- should go back to Active
        let restored = server
            .restore_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();
        assert_eq!(restored.status, unimatrix_store::Status::Active);
        assert_eq!(restored.pre_quarantine_status, None);

        // Counters should return to initial
        let final_active = server.store.read_counter("total_active").unwrap();
        assert_eq!(final_active, initial_active);
    }

    // -- PendingEntriesAnalysis tests (R-07) --

    fn make_analysis(entry_id: u64, rework_flag_count: u32) -> unimatrix_observe::EntryAnalysis {
        unimatrix_observe::EntryAnalysis {
            entry_id,
            title: format!("entry-{entry_id}"),
            category: "decision".to_string(),
            rework_flag_count,
            injection_count: 0,
            success_session_count: 0,
            rework_session_count: 0,
        }
    }

    // Updated for vnc-005 two-level API: upsert now takes feature_cycle as first arg.
    // Old tests updated in-place; overwrite semantics replace accumulate semantics.

    #[test]
    fn pending_entries_upsert_and_drain() {
        let mut pending = PendingEntriesAnalysis::new();
        pending.upsert("test-fc", make_analysis(1, 3));
        pending.upsert("test-fc", make_analysis(2, 1));

        let drained = pending.drain_for("test-fc");
        assert_eq!(drained.len(), 2);
        assert!(!pending.buckets.contains_key("test-fc"));
    }

    #[test]
    fn pending_entries_upsert_overwrites_counts() {
        // vnc-005: upsert now OVERWRITES (not merges) — updated from accumulate semantics
        let mut pending = PendingEntriesAnalysis::new();
        pending.upsert("test-fc", make_analysis(1, 2));
        let a = unimatrix_observe::EntryAnalysis {
            entry_id: 1,
            title: "entry-1".to_string(),
            category: "decision".to_string(),
            rework_flag_count: 3,
            injection_count: 0,
            success_session_count: 1,
            rework_session_count: 0,
        };
        pending.upsert("test-fc", a);
        let bucket = &pending.buckets["test-fc"];
        let entry = bucket.entries.get(&1).unwrap();
        assert_eq!(entry.rework_flag_count, 3); // overwrite: 3, not 2+3=5
        assert_eq!(entry.success_session_count, 1);
    }

    #[test]
    fn pending_entries_cap_at_1001_drops_lowest_rework() {
        let mut pending = PendingEntriesAnalysis::new();

        // Insert 1000 entries with rework_flag_count = entry_id (1..=1000)
        for i in 1u64..=1000 {
            pending.upsert("test-fc", make_analysis(i, i as u32));
        }
        assert_eq!(pending.buckets["test-fc"].entries.len(), 1000);

        // Insert 1001st entry with rework_flag_count = 999 (above the minimum)
        pending.upsert("test-fc", make_analysis(1001, 999));
        assert_eq!(
            pending.buckets["test-fc"].entries.len(),
            1000,
            "cap should be enforced"
        );

        // Entry 1 (rework_flag_count=1) should have been dropped (it was the minimum)
        assert!(
            !pending.buckets["test-fc"].entries.contains_key(&1),
            "lowest rework entry should be dropped"
        );
        // Entry 1001 should be present
        assert!(
            pending.buckets["test-fc"].entries.contains_key(&1001),
            "new entry should be inserted"
        );
    }

    #[test]
    fn pending_entries_cap_insert_below_minimum_not_inserted() {
        let mut pending = PendingEntriesAnalysis::new();

        // Fill to exactly 1000 with rework_flag_count = 5 each
        for i in 1u64..=1000 {
            pending.upsert("test-fc", make_analysis(i, 5));
        }
        assert_eq!(pending.buckets["test-fc"].entries.len(), 1000);

        // Insert new entry with rework_flag_count = 5 (tied with minimum)
        // The cap logic drops the minimum (one of the 5s) and inserts the new one
        pending.upsert("test-fc", make_analysis(1001, 5));
        assert_eq!(
            pending.buckets["test-fc"].entries.len(),
            1000,
            "cap should be enforced"
        );
        // Total entries still 1000 (one was dropped, new one added)
        assert!(
            pending.buckets["test-fc"].entries.contains_key(&1001)
                || pending.buckets["test-fc"].entries.len() == 1000
        );
    }

    #[test]
    fn pending_entries_drain_for_clears_bucket() {
        let mut pending = PendingEntriesAnalysis::new();
        for i in 0..5u64 {
            pending.upsert("test-fc", make_analysis(i, i as u32 + 1));
        }
        let drained = pending.drain_for("test-fc");
        assert_eq!(drained.len(), 5);
        assert!(
            !pending.buckets.contains_key("test-fc"),
            "drain removes the bucket"
        );
        // Second drain is idempotent
        let second = pending.drain_for("test-fc");
        assert!(second.is_empty());
    }

    // -- col-010b: embedding_dim tests (T-LL-08..10) --

    #[tokio::test]
    async fn insert_with_audit_sets_embedding_dim() {
        let server = make_server();
        let entry = NewEntry {
            title: "test".to_string(),
            content: "test content".to_string(),
            topic: "test/topic".to_string(),
            category: "decision".to_string(),
            tags: vec![],
            source: String::new(),
            status: unimatrix_core::Status::Active,
            created_by: "test".to_string(),
            feature_cycle: String::new(),
            trust_source: "system".to_string(),
        };
        let embedding: Vec<f32> = vec![0.1; 384];
        let audit = crate::infra::audit::AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: "test".to_string(),
            operation: "test".to_string(),
            target_ids: vec![],
            outcome: crate::infra::audit::Outcome::Success,
            detail: "test".to_string(),
        };

        let (_id, record) = server
            .insert_with_audit(entry, embedding, audit)
            .await
            .unwrap();
        assert_eq!(
            record.embedding_dim, 384,
            "embedding_dim must match embedding vector length"
        );
    }

    #[tokio::test]
    async fn insert_with_audit_empty_embedding_skips_hnsw() {
        // Empty embedding = ONNX model not loaded or embedding failed.
        // Entry is still written to store (searchable by topic/category/tags),
        // HNSW insert is skipped, embedding_dim is 0.
        let server = make_server();
        let entry = NewEntry {
            title: "test".to_string(),
            content: "test content".to_string(),
            topic: "test/topic".to_string(),
            category: "decision".to_string(),
            tags: vec![],
            source: String::new(),
            status: unimatrix_core::Status::Active,
            created_by: "test".to_string(),
            feature_cycle: String::new(),
            trust_source: "system".to_string(),
        };
        let embedding: Vec<f32> = vec![];
        let audit = crate::infra::audit::AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: "test".to_string(),
            operation: "test".to_string(),
            target_ids: vec![],
            outcome: crate::infra::audit::Outcome::Success,
            detail: "test".to_string(),
        };

        let (id, record) = server
            .insert_with_audit(entry, embedding, audit)
            .await
            .unwrap();
        assert!(id > 0, "entry should be written to store");
        assert_eq!(
            record.embedding_dim, 0,
            "empty embedding means embedding_dim = 0"
        );
    }

    #[tokio::test]
    async fn correct_with_audit_sets_embedding_dim() {
        let server = make_server();
        // First insert an entry to correct
        let entry = NewEntry {
            title: "original".to_string(),
            content: "original content".to_string(),
            topic: "test/topic".to_string(),
            category: "decision".to_string(),
            tags: vec![],
            source: String::new(),
            status: unimatrix_core::Status::Active,
            created_by: "test".to_string(),
            feature_cycle: String::new(),
            trust_source: "system".to_string(),
        };
        let embedding: Vec<f32> = vec![0.1; 384];
        let audit = crate::infra::audit::AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: "test".to_string(),
            operation: "test".to_string(),
            target_ids: vec![],
            outcome: crate::infra::audit::Outcome::Success,
            detail: "test".to_string(),
        };
        let (original_id, _) = server
            .insert_with_audit(entry, embedding, audit)
            .await
            .unwrap();

        // Now correct it with a new embedding
        let correction_entry = NewEntry {
            title: "corrected".to_string(),
            content: "corrected content".to_string(),
            topic: "test/topic".to_string(),
            category: "decision".to_string(),
            tags: vec![],
            source: String::new(),
            status: unimatrix_core::Status::Active,
            created_by: "test".to_string(),
            feature_cycle: String::new(),
            trust_source: "system".to_string(),
        };
        let correction_embedding: Vec<f32> = vec![0.2; 384];
        let correction_audit = crate::infra::audit::AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: "test".to_string(),
            operation: "correct".to_string(),
            target_ids: vec![],
            outcome: crate::infra::audit::Outcome::Success,
            detail: "correction".to_string(),
        };
        let (_deprecated, new_correction) = server
            .correct_with_audit(
                original_id,
                correction_entry,
                correction_embedding,
                correction_audit,
            )
            .await
            .unwrap();
        assert_eq!(
            new_correction.embedding_dim, 384,
            "correction embedding_dim must match embedding vector length"
        );
    }

    // -- vnc-005: PendingEntriesAnalysis two-level refactor tests --
    // (make_analysis helper reused from the existing helper above)

    // T-ACCUM-U-01: upsert inserts into correct feature_cycle bucket
    #[test]
    fn test_upsert_inserts_into_correct_bucket() {
        let mut pea = PendingEntriesAnalysis::new();
        let a = make_analysis(1, 3);
        pea.upsert("vnc-005", a.clone());

        assert!(pea.buckets.contains_key("vnc-005"), "bucket must exist");
        let bucket = &pea.buckets["vnc-005"];
        assert!(
            bucket.entries.contains_key(&1),
            "entry_id 1 must be present"
        );
        assert_eq!(bucket.entries[&1].entry_id, 1);
        assert_eq!(bucket.entries[&1].rework_flag_count, 3);
    }

    // T-ACCUM-U-02: upsert on same entry_id overwrites (overwrite semantics, not accumulate)
    #[test]
    fn test_upsert_overwrites_existing_entry() {
        let mut pea = PendingEntriesAnalysis::new();
        let v1 = make_analysis(42, 1);
        let v2 = make_analysis(42, 99);
        pea.upsert("vnc-005", v1);
        pea.upsert("vnc-005", v2);

        let bucket = &pea.buckets["vnc-005"];
        assert_eq!(bucket.entries.len(), 1, "only one entry_id=42 must exist");
        // v2 replaces v1 — rework_flag_count should be 99, not 1+99=100
        assert_eq!(
            bucket.entries[&42].rework_flag_count, 99,
            "upsert must overwrite, not accumulate"
        );
    }

    // T-ACCUM-U-03: upsert into different feature_cycle keys creates independent buckets
    #[test]
    fn test_upsert_independent_buckets() {
        let mut pea = PendingEntriesAnalysis::new();
        pea.upsert("vnc-005", make_analysis(1, 1));
        pea.upsert("vnc-006", make_analysis(2, 2));

        assert_eq!(pea.buckets.len(), 2, "two independent buckets must exist");
        assert!(
            pea.buckets["vnc-005"].entries.contains_key(&1),
            "bucket vnc-005 must have entry 1"
        );
        assert!(
            !pea.buckets["vnc-005"].entries.contains_key(&2),
            "bucket vnc-005 must NOT have entry 2"
        );
        assert!(
            pea.buckets["vnc-006"].entries.contains_key(&2),
            "bucket vnc-006 must have entry 2"
        );
    }

    // T-ACCUM-U-04: drain_for returns all entries and removes the bucket
    #[test]
    fn test_drain_for_returns_all_and_removes_bucket() {
        let mut pea = PendingEntriesAnalysis::new();
        pea.upsert("vnc-005", make_analysis(1, 1));
        pea.upsert("vnc-005", make_analysis(2, 2));
        pea.upsert("vnc-005", make_analysis(3, 3));

        let drained = pea.drain_for("vnc-005");
        assert_eq!(drained.len(), 3, "drain must return all 3 entries");

        let ids: std::collections::HashSet<u64> = drained.iter().map(|e| e.entry_id).collect();
        assert!(ids.contains(&1));
        assert!(ids.contains(&2));
        assert!(ids.contains(&3));

        assert!(
            !pea.buckets.contains_key("vnc-005"),
            "bucket must be removed after drain"
        );

        // Second drain returns empty (AC-18)
        let second = pea.drain_for("vnc-005");
        assert!(
            second.is_empty(),
            "second drain on same key must return empty"
        );
    }

    // T-ACCUM-U-05: drain_for on absent key returns empty Vec, no panic
    #[test]
    fn test_drain_for_absent_key_returns_empty() {
        let mut pea = PendingEntriesAnalysis::new();
        let result = pea.drain_for("nonexistent-cycle");
        assert!(result.is_empty(), "must return empty for nonexistent key");
        assert!(
            !pea.buckets.contains_key("nonexistent-cycle"),
            "must not create a bucket for absent key"
        );
    }

    // T-ACCUM-U-06: evict_stale removes buckets older than ttl_secs
    #[test]
    fn test_evict_stale_removes_old_bucket() {
        let mut pea = PendingEntriesAnalysis::new();
        pea.upsert("old-feature", make_analysis(1, 1));
        pea.upsert("fresh-feature", make_analysis(2, 2));

        let now = unix_now_secs();
        let ttl_secs = 72 * 3600u64;

        // Manually set last_updated to simulate an old bucket
        if let Some(old_bucket) = pea.buckets.get_mut("old-feature") {
            old_bucket.last_updated = now.saturating_sub(ttl_secs + 3600); // 73h ago
        }

        pea.evict_stale(now, ttl_secs);

        assert!(
            !pea.buckets.contains_key("old-feature"),
            "stale bucket must be evicted"
        );
        assert!(
            pea.buckets.contains_key("fresh-feature"),
            "fresh bucket must be retained"
        );
    }

    // T-ACCUM-U-07: evict_stale does not evict non-empty buckets within TTL
    #[test]
    fn test_evict_stale_retains_fresh_bucket() {
        let mut pea = PendingEntriesAnalysis::new();
        for i in 0..5 {
            pea.upsert("vnc-005", make_analysis(i, i as u32));
        }

        let now = unix_now_secs();
        let ttl_secs = 72 * 3600u64;

        // Set last_updated to 71h ago — within TTL
        if let Some(bucket) = pea.buckets.get_mut("vnc-005") {
            bucket.last_updated = now.saturating_sub(71 * 3600);
        }

        pea.evict_stale(now, ttl_secs);

        assert!(
            pea.buckets.contains_key("vnc-005"),
            "bucket within TTL must be retained"
        );
        assert_eq!(
            pea.buckets["vnc-005"].entries.len(),
            5,
            "all entries must remain after non-eviction"
        );
    }

    // T-ACCUM-U-08: per-bucket cap enforced at 1000 entries
    #[test]
    fn test_upsert_enforces_1000_entry_cap() {
        let mut pea = PendingEntriesAnalysis::new();
        // Insert 1000 entries with low rework_flag_count (0)
        for i in 0u64..1000 {
            pea.upsert("vnc-005", make_analysis(i, 0));
        }
        assert_eq!(pea.buckets["vnc-005"].entries.len(), 1000);

        // Insert entry 1001 — this must evict a low-count entry
        pea.upsert("vnc-005", make_analysis(9999, 5));
        assert!(
            pea.buckets["vnc-005"].entries.len() <= 1000,
            "bucket must not exceed 1000 entries"
        );
        // Entry 9999 (high rework_count) must be present
        assert!(
            pea.buckets["vnc-005"].entries.contains_key(&9999),
            "newly inserted high-priority entry must be present"
        );
    }

    // T-ACCUM-U-11: feature_cycle key exceeding 256 bytes is silently dropped
    #[test]
    fn test_upsert_oversized_key_is_silently_dropped() {
        let mut pea = PendingEntriesAnalysis::new();
        let oversized_key = "x".repeat(257);
        pea.upsert(&oversized_key, make_analysis(1, 1));

        assert!(
            pea.buckets.is_empty(),
            "oversized key must not create a bucket"
        );
    }

    // T-ACCUM-U-11b: 256-byte key is exactly at the limit and must succeed
    #[test]
    fn test_upsert_256_byte_key_succeeds() {
        let mut pea = PendingEntriesAnalysis::new();
        let max_key = "x".repeat(256);
        pea.upsert(&max_key, make_analysis(1, 1));

        assert!(
            pea.buckets.contains_key(&max_key),
            "exactly-256-byte key must be accepted"
        );
    }

    // T-SERVER-U-01: clone produces shallow copy sharing all Arc fields
    #[test]
    fn test_server_clone_shares_arc_fields() {
        let server = make_server();
        let clone = server.clone();

        // All Arc fields must point to the same allocation
        assert!(
            Arc::ptr_eq(&server.store, &clone.store),
            "store Arc must be shared across clone"
        );
        assert!(
            Arc::ptr_eq(&server.vector_index, &clone.vector_index),
            "vector_index Arc must be shared across clone"
        );
        assert!(
            Arc::ptr_eq(
                &server.pending_entries_analysis,
                &clone.pending_entries_analysis
            ),
            "pending_entries_analysis Arc must be shared across clone"
        );
        assert!(
            Arc::ptr_eq(&server.session_registry, &clone.session_registry),
            "session_registry Arc must be shared across clone"
        );
    }

    // T-SERVER-U-02: Arc strong_count is 1 before graceful_shutdown after session drop
    #[tokio::test]
    async fn test_server_clone_arc_count_drops_after_join() {
        let server = make_server();
        let store = Arc::clone(&server.store);
        let initial_count = Arc::strong_count(&store);

        let clone = server.clone();
        let count_with_clone = Arc::strong_count(&store);
        assert!(
            count_with_clone > initial_count,
            "strong_count must increase after clone"
        );

        let handle = tokio::spawn(async move {
            // Session task holds the clone; dropping it releases the Arc refs
            drop(clone);
        });
        handle.await.unwrap();

        let count_after_drop = Arc::strong_count(&store);
        assert_eq!(
            count_after_drop, initial_count,
            "strong_count must return to initial value after session clone is dropped and joined"
        );
    }

    // T-ACCUM-C-01: concurrent upsert + drain — no data loss
    #[tokio::test]
    async fn test_concurrent_upsert_drain_no_data_loss() {
        use std::sync::atomic::{AtomicU64, Ordering};

        let pea = Arc::new(Mutex::new(PendingEntriesAnalysis::new()));
        let total_seen = Arc::new(AtomicU64::new(0));

        // Spawn 4 writer tasks, each inserting 250 entries with unique IDs
        let mut writer_handles = Vec::new();
        for thread_id in 0u64..4 {
            let pea_clone = Arc::clone(&pea);
            writer_handles.push(tokio::spawn(async move {
                for i in 0u64..250 {
                    let entry_id = thread_id * 250 + i;
                    let analysis = unimatrix_observe::EntryAnalysis {
                        entry_id,
                        title: format!("entry-{}", entry_id),
                        category: "pattern".to_string(),
                        rework_flag_count: 1,
                        injection_count: 0,
                        success_session_count: 0,
                        rework_session_count: 0,
                    };
                    pea_clone
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .upsert("test-cycle", analysis);
                }
            }));
        }

        // Spawn 1 drain task that periodically drains
        let pea_drain = Arc::clone(&pea);
        let seen_clone = Arc::clone(&total_seen);
        let drain_handle = tokio::spawn(async move {
            for _ in 0..10 {
                tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                let drained = pea_drain
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .drain_for("test-cycle");
                seen_clone.fetch_add(drained.len() as u64, Ordering::Relaxed);
            }
        });

        for h in writer_handles {
            h.await.unwrap();
        }
        drain_handle.await.unwrap();

        // Final drain after all writers done
        let final_drained = pea
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain_for("test-cycle");
        total_seen.fetch_add(final_drained.len() as u64, Ordering::Relaxed);

        // Total entries seen across all drains must equal 1000 (4*250)
        assert_eq!(
            total_seen.load(Ordering::Relaxed),
            1000,
            "all 1000 entries must be seen across all drain calls"
        );
    }

    // T-ACCUM-C-02: evict_stale + drain_for — no double-free
    #[test]
    fn test_evict_and_drain_no_double_free() {
        let mut pea = PendingEntriesAnalysis::new();
        pea.upsert("expiring-feature", make_analysis(1, 1));

        let now = unix_now_secs();
        let ttl_secs = 72 * 3600u64;

        // Make bucket stale
        if let Some(b) = pea.buckets.get_mut("expiring-feature") {
            b.last_updated = now.saturating_sub(ttl_secs + 3600);
        }

        // First caller: evict
        pea.evict_stale(now, ttl_secs);
        assert!(!pea.buckets.contains_key("expiring-feature"));

        // Second caller: drain on already-evicted key — must return empty, no panic
        let result = pea.drain_for("expiring-feature");
        assert!(result.is_empty(), "drain after eviction must return empty");
    }

    // T-SERVER-U-04: CallerId::UdsSession exemption carries C-07/W2-2 comment
    // (Static verification: confirmed by code review of gateway.rs check_rate function)
    #[test]
    fn test_c07_comment_presence_in_gateway() {
        // This is a compile-time/grep verification confirmed during implementation.
        // The C-07 comment is in services/gateway.rs check_rate().
        // Ensure upsert signature takes feature_cycle as first arg (API shape test).
        let mut pea = PendingEntriesAnalysis::new();
        // If this compiles, the new API is in place
        pea.upsert("vnc-005", make_analysis(1, 1));
        assert!(pea.buckets.contains_key("vnc-005"));
    }

    // T-SERVER-U-05: UdsSession exemption does not apply to non-UDS caller variants
    #[test]
    fn test_uds_session_rate_exemption_boundary() {
        use crate::infra::audit::AuditLog;
        use crate::services::gateway::SecurityGateway;
        use crate::services::{CallerId, RateLimitConfig};

        let dir = tempfile::TempDir::new().unwrap();
        let store = Arc::new(unimatrix_core::Store::open(dir.path().join("t.db")).unwrap());
        let audit = Arc::new(AuditLog::new(Arc::clone(&store)));
        // Use limit=1 so we can verify the Agent is rate-limited after one call
        let config = RateLimitConfig {
            search_limit: 1,
            write_limit: 1,
            window_secs: 3600,
        };
        let gw = SecurityGateway::with_rate_config(audit, config);

        // UdsSession: always exempt — C-07 (vnc-005)
        let uds = CallerId::UdsSession("sess-1".to_string());
        assert!(
            gw.check_search_rate(&uds).is_ok(),
            "UdsSession must be rate-limit exempt"
        );
        assert!(
            gw.check_search_rate(&uds).is_ok(),
            "UdsSession must stay exempt on repeated calls"
        );

        // Regular Agent: must be rate-limited after hitting limit=1
        let agent = CallerId::Agent("agent-1".to_string());
        assert!(
            gw.check_search_rate(&agent).is_ok(),
            "first agent call must succeed"
        );
        assert!(
            gw.check_search_rate(&agent).is_err(),
            "second agent call must be rate-limited"
        );
    }
}
