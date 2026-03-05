//! UnimatrixServer core: state holder and ServerHandler implementation.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use unimatrix_core::{
    CoreError, EmbedService, EntryRecord, NewEntry, StoreAdapter, Store, VectorAdapter, VectorIndex,
};
use unimatrix_core::async_wrappers::{AsyncEntryStore, AsyncVectorStore};
use unimatrix_store::rusqlite;
use unimatrix_store::{
    compute_content_hash, status_counter_key, StoreError,
};
use unimatrix_store::read::{entry_from_row, load_tags_for_entries, ENTRY_COLUMNS};

use unimatrix_adapt::AdaptationService;

use crate::infra::audit::{AuditEvent, AuditLog};
use crate::infra::categories::CategoryAllowlist;
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::error::ServerError;
use crate::mcp::identity::{self, ResolvedIdentity};
use crate::infra::registry::{AgentRegistry, TrustLevel};
use crate::services::ServiceLayer;
use crate::infra::session::SessionRegistry;
use crate::infra::usage_dedup::{UsageDedup, VoteAction};

// -- col-009: PendingEntriesAnalysis --

/// In-memory accumulator for entry-level performance data from signal consumers.
///
/// Shared between the UDS listener (writes from signal consumers) and the
/// context_retrospective handler (drains on call). Protected by Mutex.
/// Cap: 1000 entries. When cap reached, drops entry with lowest rework_flag_count.
pub struct PendingEntriesAnalysis {
    pub entries: HashMap<u64, unimatrix_observe::EntryAnalysis>,
    pub created_at: u64,
}

impl PendingEntriesAnalysis {
    pub fn new() -> Self {
        PendingEntriesAnalysis {
            entries: HashMap::new(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Insert or update an EntryAnalysis, enforcing the 1000-entry cap.
    ///
    /// Update: merge rework_flag_count, rework_session_count, success_session_count.
    /// Insert: enforce cap by dropping entry with lowest rework_flag_count before inserting.
    pub fn upsert(&mut self, analysis: unimatrix_observe::EntryAnalysis) {
        if let Some(existing) = self.entries.get_mut(&analysis.entry_id) {
            existing.rework_flag_count += analysis.rework_flag_count;
            existing.rework_session_count += analysis.rework_session_count;
            existing.success_session_count += analysis.success_session_count;
        } else {
            if self.entries.len() >= 1000 {
                // Drop entry with lowest rework_flag_count
                let min_key = self
                    .entries
                    .iter()
                    .min_by_key(|(_, v)| v.rework_flag_count)
                    .map(|(k, _)| *k);
                if let Some(k) = min_key {
                    self.entries.remove(&k);
                }
            }
            self.entries.insert(analysis.entry_id, analysis);
        }
    }

    /// Drain all entries and clear the map. Returns the drained entries.
    pub fn drain_all(&mut self) -> Vec<unimatrix_observe::EntryAnalysis> {
        let entries: Vec<_> = self.entries.values().cloned().collect();
        self.entries.clear();
        entries
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
    /// Session registry for stale session sweep in maintain=true path (col-009, FR-09.2).
    /// Shared with UDS listener; sweep called by context_status maintain=true.
    pub session_registry: Arc<SessionRegistry>,
    /// Transport-agnostic service layer (vnc-006).
    pub(crate) services: ServiceLayer,
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
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
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
            tool_router: Self::tool_router(),
            server_info,
        }
    }

    /// Resolve an agent identity from tool parameters.
    ///
    /// Convenience method combining extraction and resolution.
    pub fn resolve_agent(
        &self,
        agent_id: &Option<String>,
    ) -> Result<ResolvedIdentity, ServerError> {
        let extracted = identity::extract_agent_id(agent_id);
        identity::resolve_identity(&self.registry, &extracted)
    }

    /// Resolve identity, parse format, build audit context with optional session ID.
    ///
    /// Replaces the 15-25 line ceremony in each MCP handler with a single call.
    /// Capability checking is separate via `require_cap()` (ADR-002).
    /// Session ID is validated (S3) and prefixed with "mcp::" when present.
    pub(crate) fn build_context(
        &self,
        agent_id: &Option<String>,
        format: &Option<String>,
        session_id: &Option<String>,
    ) -> Result<crate::mcp::context::ToolContext, rmcp::ErrorData> {
        use crate::mcp::context::ToolContext;
        use crate::services::{AuditContext, AuditSource, CallerId, prefix_session_id};

        let identity = self.resolve_agent(agent_id)
            .map_err(rmcp::ErrorData::from)?;
        let format = crate::mcp::response::parse_format(format)
            .map_err(rmcp::ErrorData::from)?;

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
    pub(crate) fn require_cap(
        &self,
        agent_id: &str,
        cap: crate::infra::registry::Capability,
    ) -> Result<(), rmcp::ErrorData> {
        self.registry.require_capability(agent_id, cap)
            .map_err(rmcp::ErrorData::from)
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

        let (entry_id, record) = tokio::task::spawn_blocking(move || -> Result<(u64, EntryRecord), ServerError> {
            let txn = store.begin_write()
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
            ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;

            // Insert tags
            for tag in &record.tags {
                conn.execute(
                    "INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
                    rusqlite::params![id as i64, tag],
                ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;
            }

            // Insert vector mapping
            conn.execute(
                "INSERT OR REPLACE INTO vector_map (entry_id, hnsw_data_id) VALUES (?1, ?2)",
                rusqlite::params![id as i64, data_id as i64],
            ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;

            // Outcome index (if applicable)
            if record.category == "outcome" && !record.feature_cycle.is_empty() {
                conn.execute(
                    "INSERT OR IGNORE INTO outcome_index (feature_cycle, entry_id) VALUES (?1, ?2)",
                    rusqlite::params![&record.feature_cycle, id as i64],
                ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;
            }

            // Status counter
            unimatrix_store::counters::increment_counter(conn, status_counter_key(record.status), 1)
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
        }).await.map_err(|e| ServerError::Core(CoreError::JoinError(e.to_string())))??;

        if !embedding.is_empty() {
            self.vector_index.insert_hnsw_only(entry_id, data_id, &embedding)
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

        let (deprecated_original, new_correction) = tokio::task::spawn_blocking(move || -> Result<(EntryRecord, EntryRecord), ServerError> {
            let txn = store.begin_write()
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
            ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;

            // Update status counters
            unimatrix_store::counters::decrement_counter(conn, status_counter_key(old_status), 1)
                .map_err(|e| ServerError::Core(CoreError::Store(e)))?;
            unimatrix_store::counters::increment_counter(conn, status_counter_key(unimatrix_store::Status::Deprecated), 1)
                .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

            // Update original record for return value
            original.status = unimatrix_store::Status::Deprecated;
            original.superseded_by = Some(new_id);
            original.correction_count += 1;
            original.updated_at = now;

            // 5. Build correction EntryRecord
            let content_hash = compute_content_hash(&correction_entry.title, &correction_entry.content);
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
            ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;

            // 7. Insert tags for correction
            for tag in &correction.tags {
                conn.execute(
                    "INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
                    rusqlite::params![new_id as i64, tag],
                ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;
            }

            // 8. Insert vector mapping
            conn.execute(
                "INSERT OR REPLACE INTO vector_map (entry_id, hnsw_data_id) VALUES (?1, ?2)",
                rusqlite::params![new_id as i64, data_id as i64],
            ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;

            // 9. Status counter for correction
            unimatrix_store::counters::increment_counter(conn, status_counter_key(correction.status), 1)
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
        }).await.map_err(|e| ServerError::Core(CoreError::JoinError(e.to_string())))??;

        // HNSW insert for the correction (after commit)
        if !embedding.is_empty() {
            self.vector_index.insert_hnsw_only(new_correction.id, data_id, &embedding)
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
            let vote_actions = self.usage_dedup.check_votes(agent_id, entry_ids, helpful_value);
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

        // Step 3: Record usage WITH confidence computation (spawn_blocking)
        let store = Arc::clone(&self.store);
        let all_ids = entry_ids.to_vec();
        let access_ids_owned = access_ids;
        let helpful_owned = helpful_ids;
        let unhelpful_owned = unhelpful_ids;
        let dec_helpful_owned = decrement_helpful_ids;
        let dec_unhelpful_owned = decrement_unhelpful_ids;

        let usage_result = tokio::task::spawn_blocking(move || {
            store.record_usage_with_confidence(
                &all_ids,
                &access_ids_owned,
                &helpful_owned,
                &unhelpful_owned,
                &dec_helpful_owned,
                &dec_unhelpful_owned,
                Some(&crate::confidence::compute_confidence),
            )
        }).await;

        match usage_result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                tracing::warn!("usage recording failed: {e}");
            }
            Err(e) => {
                tracing::warn!("usage recording task failed: {e}");
            }
        }

        // Step 4: Record feature entries if applicable (trust gating)
        if let Some(feature_str) = feature {
            if matches!(trust_level, TrustLevel::System | TrustLevel::Privileged | TrustLevel::Internal) {
                let store = Arc::clone(&self.store);
                let feature_owned = feature_str.to_string();
                let ids = entry_ids.to_vec();

                let feature_result = tokio::task::spawn_blocking(move || {
                    store.record_feature_entries(&feature_owned, &ids)
                }).await;

                match feature_result {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        tracing::warn!("feature entry recording failed: {e}");
                    }
                    Err(e) => {
                        tracing::warn!("feature entry recording task failed: {e}");
                    }
                }
            }
            // Restricted agents' feature params silently ignored (AC-17)
        }

        // Step 5: Co-access recording (fire-and-forget, crt-004)
        if entry_ids.len() >= 2 {
            let pairs =
                crate::coaccess::generate_pairs(entry_ids, crate::coaccess::MAX_CO_ACCESS_ENTRIES);
            let new_pairs = self.usage_dedup.filter_co_access_pairs(&pairs);

            if !new_pairs.is_empty() {
                let store = Arc::clone(&self.store);
                let pairs_for_adapt: Vec<(u64, u64, u32)> = new_pairs
                    .iter()
                    .map(|p| (p.0, p.1, 1u32))
                    .collect();
                let co_access_result = tokio::task::spawn_blocking(move || {
                    store.record_co_access_pairs(&new_pairs)
                })
                .await;

                match co_access_result {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        tracing::warn!("co-access recording failed: {e}");
                    }
                    Err(e) => {
                        tracing::warn!("co-access recording task failed: {e}");
                    }
                }

                // Step 5b: Feed co-access pairs to adaptation training reservoir (crt-006)
                self.adapt_service.record_training_pairs(&pairs_for_adapt);

                // Step 5c: Attempt training step if reservoir has enough pairs (fire-and-forget)
                let adapt_svc = Arc::clone(&self.adapt_service);
                let embed_svc = Arc::clone(&self.embed_service);
                let store_for_train = Arc::clone(&self.store);
                let _ = tokio::task::spawn_blocking(move || {
                    // Only attempt training if embed model is ready
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
        ).await
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
        ).await
    }

    /// Restore a quarantined entry: set status to Active using direct SQL (nxs-008).
    pub(crate) async fn restore_with_audit(
        &self,
        entry_id: u64,
        reason: Option<String>,
        audit_event: AuditEvent,
    ) -> Result<EntryRecord, ServerError> {
        self.change_status_with_audit(
            entry_id,
            unimatrix_store::Status::Active,
            reason,
            audit_event,
            true, // set modified_by from audit agent_id
        ).await
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

            if set_modified_by {
                conn.execute(
                    "UPDATE entries SET status = ?1, modified_by = ?2, updated_at = ?3 WHERE id = ?4",
                    rusqlite::params![
                        new_status as u8 as i64,
                        &audit_event.agent_id,
                        now as i64,
                        entry_id as i64
                    ],
                ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;
                record.modified_by = audit_event.agent_id.clone();
            } else {
                conn.execute(
                    "UPDATE entries SET status = ?1, updated_at = ?2 WHERE id = ?3",
                    rusqlite::params![
                        new_status as u8 as i64,
                        now as i64,
                        entry_id as i64
                    ],
                ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;
            }

            record.status = new_status;
            record.updated_at = now;

            // 4. Update status counters
            unimatrix_store::counters::decrement_counter(conn, status_counter_key(old_status), 1)
                .map_err(|e| ServerError::Core(CoreError::Store(e)))?;
            unimatrix_store::counters::increment_counter(conn, status_counter_key(new_status), 1)
                .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

            // 5. Write audit event
            let detail = match &reason {
                Some(r) => format!("{action_name} entry #{entry_id}: {r}"),
                None => format!("{action_name} entry #{entry_id}"),
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
        let vector_index = Arc::new(
            unimatrix_core::VectorIndex::new(Arc::clone(&store), vector_config).unwrap(),
        );
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
        assert!(info.capabilities.tools.is_some(), "tools capability must be advertised");
    }

    #[test]
    fn test_server_is_clone() {
        let server = make_server();
        let _clone = server.clone();
    }

    #[test]
    fn test_resolve_agent_with_id() {
        let server = make_server();
        let identity = server
            .resolve_agent(&Some("human".to_string()))
            .unwrap();
        assert_eq!(identity.agent_id, "human");
        assert_eq!(identity.trust_level, crate::infra::registry::TrustLevel::Privileged);
    }

    #[test]
    fn test_resolve_agent_without_id() {
        let server = make_server();
        let identity = server.resolve_agent(&None).unwrap();
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
            .record_usage_for_entries(
                "test-agent",
                TrustLevel::Internal,
                &[id],
                None,
                None,
            )
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
            .record_usage_for_entries(
                "test-agent",
                TrustLevel::Internal,
                &[],
                None,
                None,
            )
            .await;
    }

    #[tokio::test]
    async fn test_record_usage_for_entries_access_dedup() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        // First call: access_count increments
        server
            .record_usage_for_entries(
                "test-agent",
                TrustLevel::Internal,
                &[id],
                None,
                None,
            )
            .await;
        assert_eq!(server.store.get(id).unwrap().access_count, 1);

        // Second call: same agent, same entry -> deduped (access_count stays 1)
        server
            .record_usage_for_entries(
                "test-agent",
                TrustLevel::Internal,
                &[id],
                None,
                None,
            )
            .await;
        assert_eq!(server.store.get(id).unwrap().access_count, 1);

        // Different agent: access_count increments again
        server
            .record_usage_for_entries(
                "other-agent",
                TrustLevel::Internal,
                &[id],
                None,
                None,
            )
            .await;
        assert_eq!(server.store.get(id).unwrap().access_count, 2);
    }

    #[tokio::test]
    async fn test_record_usage_for_entries_helpful_vote() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        server
            .record_usage_for_entries(
                "test-agent",
                TrustLevel::Internal,
                &[id],
                Some(true),
                None,
            )
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
            .record_usage_for_entries(
                "test-agent",
                TrustLevel::Internal,
                &[id],
                Some(false),
                None,
            )
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
            .record_usage_for_entries(
                "test-agent",
                TrustLevel::Internal,
                &[id],
                None,
                None,
            )
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
            .record_usage_for_entries(
                "test-agent",
                TrustLevel::Internal,
                &[id],
                Some(false),
                None,
            )
            .await;
        assert_eq!(server.store.get(id).unwrap().unhelpful_count, 1);

        // Correction: vote helpful (should flip)
        server
            .record_usage_for_entries(
                "test-agent",
                TrustLevel::Internal,
                &[id],
                Some(true),
                None,
            )
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
                .prepare("SELECT entry_id FROM feature_entries WHERE feature_id = ?1 ORDER BY entry_id")
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
            .record_usage_for_entries(
                "test-agent",
                TrustLevel::Internal,
                &[id],
                None,
                None,
            )
            .await;

        // Second: vote helpful (separate from access dedup)
        server
            .record_usage_for_entries(
                "test-agent",
                TrustLevel::Internal,
                &[id],
                Some(true),
                None,
            )
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
            .record_usage_for_entries(
                "test-agent",
                TrustLevel::Internal,
                &[id],
                None,
                None,
            )
            .await;

        // After retrieval: confidence > 0.0
        let r = server.store.get(id).unwrap();
        assert!(r.confidence > 0.0, "confidence should be updated after retrieval");
    }

    #[tokio::test]
    async fn test_confidence_matches_formula() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        server
            .record_usage_for_entries(
                "test-agent",
                TrustLevel::Internal,
                &[id],
                None,
                None,
            )
            .await;

        let entry = server.store.get(id).unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let expected = crate::confidence::compute_confidence(&entry, now);
        // Allow small tolerance for timestamp difference
        assert!((entry.confidence - expected).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_confidence_evolves_with_multiple_retrievals() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        // First retrieval
        server
            .record_usage_for_entries(
                "agent-a",
                TrustLevel::Internal,
                &[id],
                None,
                None,
            )
            .await;
        let after_first = server.store.get(id).unwrap().confidence;

        // Second retrieval (different agent to avoid access dedup)
        server
            .record_usage_for_entries(
                "agent-b",
                TrustLevel::Internal,
                &[id],
                None,
                None,
            )
            .await;
        let after_second = server.store.get(id).unwrap().confidence;

        // Confidence should change (access_count went from 1 to 2)
        assert_ne!(after_first, after_second, "confidence should evolve with retrievals");
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
            let conf = crate::confidence::compute_confidence(&entry, now);
            server.store.update_confidence(entry_id, conf).unwrap();
        }

        let r = server.store.get(entry_id).unwrap();
        assert!(r.confidence > 0.0, "confidence should be seeded on insert");
        // Agent-authored entry: expected ~0.525
        assert!((r.confidence - 0.525).abs() < 0.05);
    }

    #[tokio::test]
    async fn test_confidence_recomputed_on_deprecation() {
        let server = make_server();
        let id = insert_test_entry(&server.store);

        // First retrieval to give it some confidence
        server
            .record_usage_for_entries(
                "test-agent",
                TrustLevel::Internal,
                &[id],
                None,
                None,
            )
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
            let conf = crate::confidence::compute_confidence(&entry, now);
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

        assert_eq!(after_active, before_active - 1, "active counter should decrement");
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
        assert_eq!(server.store.get(id).unwrap().status, unimatrix_store::Status::Active);
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

        assert_eq!(final_active, initial_active, "active counter should return to initial");
        assert_eq!(final_quarantined, 0, "quarantined counter should return to 0");
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
            .prepare(
                "SELECT operation, target_ids, detail FROM audit_log WHERE operation = ?1",
            )
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
        let before = crate::confidence::compute_confidence(&entry, now);
        server.store.update_confidence(id, before).unwrap();

        // Quarantine
        server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        // Recompute confidence for quarantined entry
        let entry = server.store.get(id).unwrap();
        let after = crate::confidence::compute_confidence(&entry, now);
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
        let restored = crate::confidence::compute_confidence(&entry, now);

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

        assert!(result.is_err(), "quarantining nonexistent entry should fail");
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

    #[test]
    fn pending_entries_upsert_and_drain() {
        let mut pending = PendingEntriesAnalysis::new();
        pending.upsert(make_analysis(1, 3));
        pending.upsert(make_analysis(2, 1));

        let drained = pending.drain_all();
        assert_eq!(drained.len(), 2);
        assert!(pending.entries.is_empty());
    }

    #[test]
    fn pending_entries_upsert_merges_counts() {
        let mut pending = PendingEntriesAnalysis::new();
        pending.upsert(make_analysis(1, 2));
        let a = unimatrix_observe::EntryAnalysis {
            entry_id: 1,
            title: "entry-1".to_string(),
            category: "decision".to_string(),
            rework_flag_count: 3,
            injection_count: 0,
            success_session_count: 1,
            rework_session_count: 0,
        };
        pending.upsert(a);
        let entry = pending.entries.get(&1).unwrap();
        assert_eq!(entry.rework_flag_count, 5); // 2 + 3
        assert_eq!(entry.success_session_count, 1);
    }

    #[test]
    fn pending_entries_cap_at_1001_drops_lowest_rework() {
        let mut pending = PendingEntriesAnalysis::new();

        // Insert 1000 entries with rework_flag_count = entry_id (1..=1000)
        for i in 1u64..=1000 {
            pending.upsert(make_analysis(i, i as u32));
        }
        assert_eq!(pending.entries.len(), 1000);

        // Insert 1001st entry with rework_flag_count = 999 (above the minimum)
        pending.upsert(make_analysis(1001, 999));
        assert_eq!(pending.entries.len(), 1000, "cap should be enforced");

        // Entry 1 (rework_flag_count=1) should have been dropped (it was the minimum)
        assert!(!pending.entries.contains_key(&1), "lowest rework entry should be dropped");
        // Entry 1001 should be present
        assert!(pending.entries.contains_key(&1001), "new entry should be inserted");
    }

    #[test]
    fn pending_entries_cap_insert_below_minimum_not_inserted() {
        let mut pending = PendingEntriesAnalysis::new();

        // Fill to exactly 1000 with rework_flag_count = 5 each
        for i in 1u64..=1000 {
            pending.upsert(make_analysis(i, 5));
        }
        assert_eq!(pending.entries.len(), 1000);

        // Insert new entry with rework_flag_count = 5 (tied with minimum)
        // The cap logic drops the minimum (one of the 5s) and inserts the new one
        pending.upsert(make_analysis(1001, 5));
        assert_eq!(pending.entries.len(), 1000, "cap should be enforced");
        // Total entries still 1000 (one was dropped, new one added)
        assert!(pending.entries.contains_key(&1001) || pending.entries.len() == 1000);
    }

    #[test]
    fn pending_entries_drain_all_clears_map() {
        let mut pending = PendingEntriesAnalysis::new();
        for i in 0..5u64 {
            pending.upsert(make_analysis(i, i as u32 + 1));
        }
        let drained = pending.drain_all();
        assert_eq!(drained.len(), 5);
        assert!(pending.entries.is_empty(), "drain clears the map");
        // Second drain is idempotent
        let second = pending.drain_all();
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

        let (_id, record) = server.insert_with_audit(entry, embedding, audit).await.unwrap();
        assert_eq!(record.embedding_dim, 384, "embedding_dim must match embedding vector length");
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

        let (id, record) = server.insert_with_audit(entry, embedding, audit).await.unwrap();
        assert!(id > 0, "entry should be written to store");
        assert_eq!(record.embedding_dim, 0, "empty embedding means embedding_dim = 0");
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
        let (original_id, _) = server.insert_with_audit(entry, embedding, audit).await.unwrap();

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
        assert_eq!(new_correction.embedding_dim, 384, "correction embedding_dim must match embedding vector length");
    }
}
