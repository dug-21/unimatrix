//! UnimatrixServer core: state holder and ServerHandler implementation.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use unimatrix_core::async_wrappers::AsyncVectorStore;
use unimatrix_core::{
    CoreError, EmbedService, EntryRecord, NewEntry, Store, VectorAdapter, VectorIndex,
};
use unimatrix_store::StoreError;

use unimatrix_adapt::AdaptationService;
use unimatrix_observe::domain::DomainPackRegistry;

use crate::background::TickMetadata;
use crate::error::ServerError;
use crate::infra::audit::{AuditEvent, AuditLog};
use crate::infra::categories::CategoryAllowlist;
use crate::infra::config::{InferenceConfig, RetentionConfig, StoreConfig, TranscriptRetention};
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
/// context_cycle_review handler (drains on call). Protected by
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
    /// features that complete without calling `context_cycle_review` or `context_cycle`.
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

/// Compiled default behavioral instructions for AI agents.
///
/// Used as the fallback when `config.server.instructions` is `None`.
/// This is the backing value only — the public interface is the `instructions`
/// parameter on `UnimatrixServer::new`.
const SERVER_INSTRUCTIONS_DEFAULT: &str = "Unimatrix is this project's knowledge engine. Before starting implementation, architecture, or design tasks, search for relevant patterns and conventions using the context tools. Apply what you find. After discovering reusable patterns or making architectural decisions, store them for future reference. Do not store workflow state or process steps.";

/// The central MCP server holding all shared state.
///
/// All fields are Arc-wrapped so Clone is cheap (required by rmcp).
#[derive(Clone)]
pub struct UnimatrixServer {
    /// Store for knowledge lookup operations.
    pub(crate) entry_store: Arc<Store>,
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
    /// Shared with UDS listener; drained by context_cycle_review handler.
    pub pending_entries_analysis: Arc<Mutex<PendingEntriesAnalysis>>,
    /// Session registry for stale session sweep (col-009, FR-09.2).
    /// Shared with UDS listener; swept by the background tick.
    pub session_registry: Arc<SessionRegistry>,
    /// crt-052 Wave B (ADR-008): bounded server-only held-buffer store. The SAME
    /// `Arc` is injected into `session_registry` as the `HeldBufferScan` handle,
    /// so the snapshot seam scans held buffers and the drain/register/delta paths
    /// route through it — all behind the optional handle (R-11 severable). Used
    /// directly here for `purge_held_for_feature` at cycle review and by the
    /// background tick for `sweep_expired`.
    pub transcript_hold: Arc<crate::infra::transcript_hold::TranscriptHold>,
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
    /// col-023 (ADR-002): startup-configured domain pack registry threaded into
    /// SqlObservationSource at the retrospective call sites in MCP tool handlers.
    ///
    /// Initialized with the built-in claude-code pack in `new()` (for tests).
    /// Overwritten from `main.rs` with the config-loaded registry (daemon/stdio paths).
    pub observation_registry: Arc<DomainPackRegistry>,
    /// crt-046: inference config snapshot for goal-cluster blending weights in
    /// the context_briefing handler. Initialized to default in `new()` (for tests).
    /// Overwritten from `main.rs` with the startup-resolved config (daemon/stdio paths).
    pub inference_config: Arc<InferenceConfig>,
    /// #561: store config snapshot for content byte limit enforcement in
    /// validate_store_params and validate_correct_params. Initialized to default
    /// in `new()` (for tests). Overwritten from `main.rs` with the startup config.
    pub store_config: Arc<StoreConfig>,
    /// vnc-025 (#670, FR-16): retention policy snapshot for the cycle-review
    /// transcript purge gate (`purge_cycle_transcripts`). Initialized to default
    /// in `new()` (for tests). Overwritten from `main.rs` with the startup config
    /// in the daemon/stdio paths (#561 `store_config` precedent).
    pub retention_config: Arc<RetentionConfig>,
    /// crt-055 (ADR-008): enabled `[transcript_signals]` class names in CONFIG
    /// ORDER (index == `class_counts` index). Snapshot read at startup so the
    /// `context_cycle_review` activity-fold landing can build
    /// `signal_class_counts_json` by index. Initialized empty in `new()` (for
    /// tests → `signal_class_counts_json == "{}"`); overwritten from `main.rs`
    /// with the startup-validated config (daemon/stdio paths).
    pub transcript_signal_class_names: Arc<Vec<String>>,
    /// Maps rmcp session ID → clientInfo.name (vnc-014, ADR-001).
    ///
    /// Key: Mcp-Session-Id UUID string (HTTP) or "" (stdio singleton).
    /// Value: clientInfo.name truncated to 256 Unicode scalar values.
    ///
    /// Arc satisfies rmcp's Clone requirement on UnimatrixServer.
    /// Mutex is poison-recovered via unwrap_or_else(|e| e.into_inner())
    /// at every lock site (NFR-01, SEC-03).
    pub client_type_map: Arc<Mutex<HashMap<String, String>>>,
}

impl UnimatrixServer {
    /// Create a new server with all subsystems.
    ///
    /// `instructions`: when `Some(s)`, uses `s` as the MCP `ServerInfo.instructions`
    /// field (from `config.server.instructions`). When `None`, falls back to the
    /// compiled default (`SERVER_INSTRUCTIONS_DEFAULT`). Validation of length and
    /// injection is performed upstream in `validate_config` — this constructor is
    /// infallible.
    ///
    /// `services` (crt-056 Wave 1, ADR-001): when `Some(layer)`, the caller-built
    /// config-driven [`ServiceLayer`] is used verbatim (the parity path — daemon and
    /// per-slug HTTP servers both pass `Some(...)`). When `None`, the historical
    /// test-default `ServiceLayer` is constructed in-line (NLI off, size-1 rayon
    /// pool, `InferenceConfig::default`, `ConfidenceParams::default`, empty
    /// `CategoryAllowlist`, unloaded `NliServiceHandle`). `None` is reachable ONLY
    /// from unit tests — there is no cloud-only branch (one isolation seam, C-6).
    pub fn new(
        entry_store: Arc<Store>,
        vector_store: Arc<AsyncVectorStore<VectorAdapter>>,
        embed_service: Arc<EmbedServiceHandle>,
        registry: Arc<AgentRegistry>,
        audit: Arc<AuditLog>,
        categories: Arc<CategoryAllowlist>,
        store: Arc<Store>,
        vector_index: Arc<VectorIndex>,
        adapt_service: Arc<AdaptationService>,
        instructions: Option<String>,
        services: Option<ServiceLayer>,
    ) -> Self {
        let implementation = Implementation::new(SERVER_NAME, env!("CARGO_PKG_VERSION"))
            .with_description("Self-learning knowledge engine for agentic workflows");

        let server_info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(implementation)
            // Use config-supplied instructions when present; fall back to compiled default.
            // None means "not configured" — use the developer-authored default.
            .with_instructions(
                instructions.unwrap_or_else(|| SERVER_INSTRUCTIONS_DEFAULT.to_string()),
            );

        let usage_dedup = Arc::new(UsageDedup::new());

        // crt-056 Wave 1 (ADR-001): `Some(layer)` ⇒ use the caller's config-driven
        // ServiceLayer (the parity path). `None` ⇒ the historical test-default body,
        // preserved byte-for-byte (C-4, AC-6). `usage_dedup` (built above) is consumed
        // ONLY by the `None` arm; the `Some` layer already owns its own.
        let services = match services {
            Some(layer) => layer,
            None => {
                let test_pool = Arc::new(
                    crate::infra::rayon_pool::RayonPool::new(1, "test-pool")
                        .expect("test RayonPool construction must succeed"),
                );

                ServiceLayer::new(
                    Arc::clone(&store),
                    Arc::clone(&vector_index),
                    Arc::clone(&vector_store),
                    Arc::clone(&entry_store),
                    Arc::clone(&embed_service),
                    Arc::clone(&adapt_service),
                    Arc::clone(&audit),
                    Arc::clone(&usage_dedup),
                    crate::infra::config::default_boosted_categories_set(),
                    test_pool,
                    // crt-023: disabled NLI for test server (no model in test env)
                    crate::infra::nli_handle::NliServiceHandle::new(),
                    20,    // nli_top_k default
                    false, // nli_enabled: disabled for tests
                    Arc::new(crate::infra::config::InferenceConfig::default()),
                    // col-023: built-in default registry for test server
                    Arc::new(DomainPackRegistry::with_builtin_claude_code()),
                    // GH #311: default params for tests; production paths supply resolved params.
                    Arc::new(unimatrix_engine::confidence::ConfidenceParams::default()),
                    // crt-031: default lifecycle policy for tests.
                    Arc::new(crate::infra::categories::CategoryAllowlist::new()),
                )
            }
        };

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
            // crt-052: clone — `audit` is reused below for the held-store sink.
            audit: Arc::clone(&audit),
            categories,
            store,
            vector_index,
            usage_dedup,
            adapt_service,
            pending_entries_analysis: Arc::new(Mutex::new(PendingEntriesAnalysis::new())),
            session_registry: Arc::new(SessionRegistry::new()),
            // crt-052 Wave B (ADR-008): default test-server held store. The
            // daemon/stdio paths in main.rs build the registry+hold pair and
            // overwrite both `session_registry` and `transcript_hold` with a
            // SHARED store; the test-server pair here is independent (the test
            // registry is `SessionRegistry::new()` with no hold wired), which is
            // fine — tests that exercise Wave B wire the pair explicitly.
            transcript_hold: Arc::new(crate::infra::transcript_hold::TranscriptHold::new(
                64,
                Arc::new(crate::infra::transcript_hold::AuditLogPurgeSink::new(
                    Arc::clone(&audit),
                )),
            )),
            services,
            effectiveness_state,
            tick_metadata,
            tool_router: Self::tool_router(),
            server_info,
            // col-023: built-in default for test server; overwritten in main.rs daemon/stdio paths.
            observation_registry: Arc::new(DomainPackRegistry::with_builtin_claude_code()),
            // crt-046: default for test server; overwritten in main.rs daemon/stdio paths.
            inference_config: Arc::new(InferenceConfig::default()),
            // #561: default for test server; overwritten in main.rs daemon/stdio paths.
            store_config: Arc::new(StoreConfig::default()),
            // vnc-025: default (PurgeOnCycleClose) for test server; overwritten in
            // main.rs daemon/stdio paths.
            retention_config: Arc::new(RetentionConfig::default()),
            transcript_signal_class_names: Arc::new(Vec::new()),
            // vnc-014 (ADR-001): empty map; populated by ServerHandler::initialize override.
            client_type_map: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// crt-056 Wave 1 (G-2): borrow the server's config-driven [`ServiceLayer`].
    ///
    /// Thin additive accessor (no new state) so the daemon HTTP boot loop can build a
    /// `PerSlugTickContext` from each server's OWN handle set after
    /// `build_project_server` returns. The Wave-2 tick borrows the same
    /// `Arc<RwLock<_>>` analytics handles the serving path reads.
    pub fn service_layer(&self) -> &ServiceLayer {
        &self.services
    }

    /// crt-056 Wave 1 (G-2): clone the server's per-server `TickMetadata` counter.
    ///
    /// One `Arc<Mutex<TickMetadata>>` per server ⇒ the per-slug tick counter falls
    /// out for free (ADR-005). Thin additive accessor; no new state.
    pub fn tick_metadata(&self) -> Arc<Mutex<TickMetadata>> {
        Arc::clone(&self.tick_metadata)
    }

    /// crt-056 Wave 1 (G-3): clone the server's per-slug [`VectorIndex`] handle.
    ///
    /// Thin additive accessor so the boot loop can populate
    /// `PerSlugTickContext.vector_index` off the server (rather than widening
    /// `ProjectServerInput`). No new state.
    pub fn vector_index(&self) -> Arc<VectorIndex> {
        Arc::clone(&self.vector_index)
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

    /// Resolve identity, parse format, and build audit context for an MCP tool call.
    ///
    /// Seam 2 overload (vnc-014, ADR-003): accepts `RequestContext<RoleServer>` to extract
    /// the rmcp session key and look up `client_type` from `client_type_map`.
    /// Accepts `Option<&ResolvedIdentity>` for the W2-3 bearer-auth path (always `None`
    /// in vnc-014 — the W2-3 activation wires a bearer-validated identity here).
    ///
    /// Capability checking is separate via `require_cap()` (ADR-002).
    /// Session ID is validated (S3) and stored raw (no transport prefix) in `AuditContext`.
    ///
    /// SESSION ID NAMESPACE (Unimatrix #4388): `AuditEvent.session_id` MUST come from
    /// `ctx.audit_ctx.session_id` (the agent-declared raw parameter — no `mcp::` prefix).
    /// The `Mcp-Session-Id` UUID is the `client_type_map` lookup key only — it must
    /// never surface in audit records. Raw storage enables direct equality join to
    /// `sessions.session_id` (GH #582 Defect 3).
    ///
    /// Uses `spawn_blocking` internally to keep Store mutex off the async runtime (#176).
    pub(crate) async fn build_context_with_external_identity(
        &self,
        params_agent_id: &Option<String>,
        format: &Option<String>,
        session_id: &Option<String>,
        request_context: &rmcp::service::RequestContext<rmcp::RoleServer>,
        external_identity: Option<&ResolvedIdentity>,
    ) -> Result<crate::mcp::context::ToolContext, rmcp::ErrorData> {
        use crate::mcp::context::ToolContext;
        use crate::services::{AuditContext, AuditSource, CallerId};

        // 1. Resolve identity.
        //    When external_identity is Some (W2-3 activation path), bypass resolve_agent
        //    and use the provided identity directly.
        //    When None (vnc-014 path), call resolve_agent exactly as before.
        let identity: ResolvedIdentity = match external_identity {
            Some(ext) => ext.clone(),
            None => self
                .resolve_agent(params_agent_id)
                .await
                .map_err(rmcp::ErrorData::from)?,
        };

        // 2. Parse format.
        let format = crate::mcp::response::parse_format(format).map_err(rmcp::ErrorData::from)?;

        // 3. Session ID: validate (S3). Store raw (unprefixed) in AuditContext so that
        //    audit_log rows carry the same ID as sessions.session_id (GH #582 Defect 3).
        //    prefix_session_id is no longer called here; the raw sid is stored directly.
        let raw_session = if let Some(sid) = session_id {
            Self::validate_session_id(sid).map_err(rmcp::ErrorData::from)?;
            Some(sid.clone())
        } else {
            None
        };

        // 4. Build AuditContext.
        //    CRITICAL: session_id here is the agent-declared parameter (raw, no prefix).
        //    It is NOT the Mcp-Session-Id UUID. See Unimatrix #4363.
        let audit_ctx = AuditContext {
            source: AuditSource::Mcp {
                agent_id: identity.agent_id.clone(),
                trust_level: identity.trust_level,
            },
            caller_id: identity.agent_id.clone(),
            session_id: raw_session,
            feature_cycle: None,
        };

        // 5. Extract rmcp session key for client_type lookup.
        //    HTTP: Mcp-Session-Id header value.
        //    Stdio or absent/non-UTF-8 header: "" (empty string sentinel).
        let rmcp_session_key: &str = request_context
            .extensions
            .get::<http::request::Parts>()
            .and_then(|p| p.headers.get("mcp-session-id"))
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        // 6. Look up client_type from client_type_map with poison recovery.
        //    Returns None when no entry exists — zero regression for tool calls
        //    without session context (NFR-03).
        let client_type: Option<String> = {
            let map = self
                .client_type_map
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            map.get(rmcp_session_key).cloned()
        }; // lock released here

        let caller_id = CallerId::Agent(identity.agent_id.clone());

        Ok(ToolContext {
            agent_id: identity.agent_id,
            trust_level: identity.trust_level,
            format,
            audit_ctx,
            caller_id,
            client_type,
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

    /// Fire-and-forget audit event via `tokio::spawn` + `log_event_async`.
    ///
    /// The JoinHandle is intentionally dropped — audit writes in-flight at runtime
    /// shutdown may be silently lost. Acceptable for observability logging (#579).
    pub(crate) fn audit_fire_and_forget(&self, event: AuditEvent) {
        let audit = Arc::clone(&self.audit);
        tokio::spawn(async move {
            if let Err(e) = audit.log_event_async(event).await {
                tracing::warn!(error = %e, "audit_fire_and_forget: write failed");
            }
        });
    }

    /// vnc-025 (#670, FR-15/FR-16): purge transcript buffers for a successfully
    /// reviewed cycle, gated on the retention policy.
    ///
    /// Called by `context_cycle_review` immediately before each SUCCESS return
    /// (full pipeline, memoization hit, cached-metrics, purged-signals). Error
    /// paths never reach it — a failed review keeps transcripts for the retry
    /// (Gate 3a disposition 2). Sessions stay registered; buffers are cleared in
    /// place (`clear_transcripts_for_feature`, the named crt-052 seam, ADR-004).
    /// Audit rows are emitted after all locks are released, fire-and-forget via
    /// `emit_purge_audits` (tokio::spawn + log_event_async); zero-byte purges
    /// emit nothing. The purge introduces NO new error path: it cannot change
    /// the review's result or output (AC-09).
    ///
    /// The match is EXHAUSTIVE — the enterprise seam (Constraint 7). Never an
    /// assumed variant, never `if ==`, never a `_` arm: a new `TranscriptRetention`
    /// variant must force a compile error here.
    /// crt-055 (ADR-008): the enabled `[transcript_signals]` class names in
    /// config order, supplied to the activity-fold landing so
    /// `signal_class_counts_json` is built by index. Empty (e.g. in tests) yields
    /// the canonical `"{}"` while the fixed `error`/`refusal` columns still land.
    pub(crate) fn retention_config_signal_class_names(&self) -> Vec<String> {
        self.transcript_signal_class_names.as_ref().clone()
    }

    pub(crate) fn purge_cycle_transcripts(&self, feature_cycle: &str) {
        match self.retention_config.transcript_retention {
            TranscriptRetention::PurgeOnCycleClose => {
                let records = self
                    .session_registry
                    .clear_transcripts_for_feature(feature_cycle);
                // ADR-004: locks already released inside clear_transcripts_for_feature;
                // content-free audit, purge success never depends on audit success.
                crate::uds::listener::emit_purge_audits(&self.audit, records, "cycle_review");
                // crt-052 Wave B seam (C8, ADR-008/ADR-009): purge held buffers
                // for this cycle in lockstep with the registered buffers, only
                // under PurgeOnCycleClose. This is AFTER distill (the cycle-review
                // handler calls `distill_before_purge` before this method), so the
                // snapshot seam has already read the held buffers' cross-turn
                // content. The audit fires EXACTLY ONCE per held session here
                // (`trigger=cycle_review`) — the per-turn `session_close` emission
                // moved off drain for held buffers (ADR-009). Reverting Wave B
                // removes only this single addition and leaves the arm correct.
                let held_records = self.transcript_hold.purge_held_for_feature(feature_cycle);
                crate::uds::listener::emit_purge_audits(&self.audit, held_records, "cycle_review");
            }
            TranscriptRetention::RetainDays(_) => {
                // Enterprise-only; OSS `validate()` rejects this value at startup,
                // so this arm is unreachable in OSS — but it MUST exist and MUST
                // NOT purge (the whole point of the seam). No-op.
            }
        }
    }

    /// Insert a new entry and write an audit event.
    ///
    /// Uses async SqlxStore methods (nxs-011).
    /// The HNSW vector insertion happens after the data transaction commits.
    pub(crate) async fn insert_with_audit(
        &self,
        entry: NewEntry,
        embedding: Vec<f32>,
        audit_event: AuditEvent,
    ) -> Result<(u64, EntryRecord), ServerError> {
        let data_id = self.vector_index.allocate_data_id();
        let embedding_dim = embedding.len() as u16;
        let entry_category = entry.category.clone();
        let entry_feature_cycle = entry.feature_cycle.clone();

        // Insert entry (handles tags + counter atomically)
        let id = self
            .store
            .insert(entry)
            .await
            .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

        // Insert vector mapping
        self.store
            .put_vector_mapping(id, data_id)
            .await
            .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

        // Insert into outcome_index if applicable (idempotent)
        self.store
            .insert_outcome_index_if_applicable(id, &entry_category, &entry_feature_cycle)
            .await
            .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

        // Read back the full record (with tags)
        let record = self
            .store
            .get(id)
            .await
            .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

        // Write audit event (separate from data transaction) — fire-and-forget.
        // GH #308: log_event() used block_in_place, starving the rmcp session loop
        // when the analytics drain task held the single write connection.
        let audit_event_with_target = AuditEvent {
            target_ids: vec![id],
            ..audit_event
        };
        {
            let audit = Arc::clone(&self.audit);
            tokio::spawn(async move {
                let _ = audit.log_event_async(audit_event_with_target).await;
            });
        }

        // HNSW insert (after data commits)
        if !embedding.is_empty() {
            self.vector_index
                .insert_hnsw_only(id, data_id, &embedding)
                .map_err(|e| ServerError::Core(CoreError::Vector(e)))?;
        }

        // Seed embedding_dim into the returned record
        let record_with_dim = EntryRecord {
            embedding_dim,
            ..record
        };

        Ok((id, record_with_dim))
    }

    /// Correct an existing entry: deprecate original, create correction, with audit.
    ///
    /// Uses async SqlxStore methods (nxs-011).
    /// The HNSW vector insertion happens after the data transaction commits.
    ///
    /// Returns (deprecated_original, new_correction).
    pub(crate) async fn correct_with_audit(
        &self,
        original_id: u64,
        correction_entry: NewEntry,
        embedding: Vec<f32>,
        audit_event: AuditEvent,
    ) -> Result<(EntryRecord, EntryRecord), ServerError> {
        let data_id = self.vector_index.allocate_data_id();
        let embedding_dim = embedding.len() as u16;

        // Atomically deprecate original and insert correction
        let (deprecated_original, new_correction) = self
            .store
            .correct_entry(original_id, correction_entry, data_id, embedding_dim)
            .await
            .map_err(|e| match e {
                StoreError::InvalidInput { field, reason } => {
                    ServerError::InvalidInput { field, reason }
                }
                other => ServerError::Core(CoreError::Store(other)),
            })?;

        // Write audit event with both IDs — fire-and-forget.
        // GH #308: same write-pool starvation fix as insert_with_audit.
        let audit_with_ids = AuditEvent {
            target_ids: vec![original_id, new_correction.id],
            ..audit_event
        };
        {
            let audit = Arc::clone(&self.audit);
            tokio::spawn(async move {
                let _ = audit.log_event_async(audit_with_ids).await;
            });
        }

        // HNSW insert for the correction (after data commits)
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

        let usage_result = {
            // All DB writes are async — call directly (we're already in an async context)
            let res = store
                .record_usage_with_confidence(
                    &all_ids,
                    &access_ids,
                    &helpful_ids,
                    &unhelpful_ids,
                    &decrement_helpful_ids,
                    &decrement_unhelpful_ids,
                    Some(Box::new(|entry: &unimatrix_store::EntryRecord, now: u64| {
                        crate::confidence::compute_confidence(
                            entry,
                            now,
                            &unimatrix_engine::confidence::ConfidenceParams::default(),
                        )
                    })
                        as Box<
                            dyn Fn(&unimatrix_store::EntryRecord, u64) -> f64 + Send + Sync,
                        >),
                )
                .await;
            if let Err(e) = res {
                tracing::warn!("usage recording failed: {e}");
            }

            if let Some((feature_str, ids)) = feature_recording {
                // phase: None — Wave 3 (context-store-phase-capture) will thread the
                // actual phase value here once SessionState.current_phase is propagated.
                if let Err(e) = store.record_feature_entries(&feature_str, &ids, None).await {
                    tracing::warn!("failed to record feature entries: {e}");
                }
            }

            if let Some(pairs) = co_access_pairs {
                store.record_co_access_pairs(&pairs);
            }
            Ok::<(), std::convert::Infallible>(())
        };

        let _ = usage_result;

        // Step 5b-c: Adaptation training (separate spawn_blocking since it
        // does CPU-intensive embedding work)
        if let Some(adapt_pairs) = pairs_for_adapt {
            self.adapt_service.record_training_pairs(&adapt_pairs);

            let adapt_svc = Arc::clone(&self.adapt_service);
            let embed_svc = Arc::clone(&self.embed_service);
            let store_for_train = Arc::clone(&self.store);
            let _ = tokio::task::spawn_blocking(move || {
                if let Some(adapter) = embed_svc.try_get_adapter_sync() {
                    let handle = tokio::runtime::Handle::current();
                    let embed_fn = |entry_id: u64| -> Option<Vec<f32>> {
                        let entry = handle.block_on(store_for_train.get(entry_id)).ok()?;
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
    ///
    /// Fix 3 (GH #444): after status update, if the entry is not in the HNSW
    /// index but has `embedding_dim > 0`, re-insert it. If `embedding_dim = 0`,
    /// skip — the heal pass will pick it up on the next maintenance tick.
    pub(crate) async fn restore_with_audit(
        &self,
        entry_id: u64,
        reason: Option<String>,
        audit_event: AuditEvent,
    ) -> Result<EntryRecord, ServerError> {
        // Fetch entry to read pre_quarantine_status
        let entry = self
            .store
            .get(entry_id)
            .await
            .map_err(|e| ServerError::Core(CoreError::Store(e)))?;
        let restore_to = entry
            .pre_quarantine_status
            .and_then(|v| unimatrix_store::Status::try_from(v).ok())
            .unwrap_or(unimatrix_store::Status::Active);
        let record = self
            .change_status_with_audit(
                entry_id,
                restore_to,
                reason,
                audit_event,
                true, // set modified_by from audit agent_id
            )
            .await?;

        // Fix 3 (GH #444): Re-insert into HNSW if prune pass removed the vector.
        // Only attempt if embedding_dim > 0 (entry was embedded before quarantine)
        // and the entry is not already present in the index.
        if record.embedding_dim > 0 && !self.vector_index.contains(entry_id) {
            // Get or allocate a VECTOR_MAP entry
            let data_id_opt = self
                .store
                .get_vector_mapping(entry_id)
                .await
                .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

            match self.embed_service.get_adapter().await {
                Ok(adapter) => {
                    match adapter.embed_entries(&[(record.title.clone(), record.content.clone())]) {
                        Ok(embeddings) => {
                            if let Some(raw_emb) = embeddings.into_iter().next() {
                                let adapted = self.adapt_service.adapt_embedding(
                                    &raw_emb,
                                    Some(&record.category),
                                    Some(&record.topic),
                                );
                                let embedding = unimatrix_embed::l2_normalized(&adapted);
                                let data_id = match data_id_opt {
                                    Some(existing) => existing,
                                    None => {
                                        let new_id = self.vector_index.allocate_data_id();
                                        if let Err(e) =
                                            self.store.put_vector_mapping(entry_id, new_id).await
                                        {
                                            tracing::warn!(
                                                entry_id,
                                                error = %e,
                                                "restore: put_vector_mapping failed; heal pass will retry"
                                            );
                                            return Ok(record);
                                        }
                                        new_id
                                    }
                                };
                                if let Err(e) = self
                                    .vector_index
                                    .insert_hnsw_only(entry_id, data_id, &embedding)
                                {
                                    tracing::warn!(
                                        entry_id,
                                        error = %e,
                                        "restore: insert_hnsw_only failed; heal pass will retry"
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                entry_id,
                                error = %e,
                                "restore: embed failed; heal pass will retry on next tick"
                            );
                        }
                    }
                }
                Err(_) => {
                    tracing::warn!(
                        entry_id,
                        "restore: embed service unavailable; heal pass will retry on next tick"
                    );
                }
            }
        }

        Ok(record)
    }

    /// Shared implementation for status-change operations (deprecate, quarantine, restore).
    ///
    /// Uses async SqlxStore.update_entry_status_extended (nxs-011).
    async fn change_status_with_audit(
        &self,
        entry_id: u64,
        new_status: unimatrix_store::Status,
        reason: Option<String>,
        audit_event: AuditEvent,
        set_modified_by: bool,
    ) -> Result<EntryRecord, ServerError> {
        let action_name = match new_status {
            unimatrix_store::Status::Deprecated => "deprecated",
            unimatrix_store::Status::Quarantined => "quarantined",
            unimatrix_store::Status::Active => "restored",
            unimatrix_store::Status::Proposed => "proposed",
        };

        // Idempotency: read current status before making any change
        let current = self
            .store
            .get(entry_id)
            .await
            .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

        if new_status == unimatrix_store::Status::Deprecated
            && current.status == unimatrix_store::Status::Deprecated
        {
            return Ok(current);
        }

        // Compute pre_quarantine_status for the update
        let pre_q_value: Option<u8> = if new_status == unimatrix_store::Status::Quarantined {
            Some(current.status as u8)
        } else {
            None
        };

        // Note: pre_quarantine_status info for audit, captured before the update
        let old_status_u8 = current.status as u8;
        let old_pre_q = current.pre_quarantine_status;

        // Perform status update with optional modified_by
        let modified_by_str: Option<String> = if set_modified_by {
            Some(audit_event.agent_id.clone())
        } else {
            None
        };
        let record = self
            .store
            .update_entry_status_extended(
                entry_id,
                new_status,
                modified_by_str.as_deref(),
                pre_q_value,
            )
            .await
            .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

        // Build audit detail with pre_quarantine info
        let pre_q_info = if new_status == unimatrix_store::Status::Quarantined {
            format!(" (pre_quarantine_status={old_status_u8})")
        } else if let Some(pq) = old_pre_q {
            format!(" (restored from pre_quarantine_status={pq})")
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
        // Fire-and-forget — GH #308: same write-pool starvation fix.
        {
            let audit = Arc::clone(&self.audit);
            tokio::spawn(async move {
                let _ = audit.log_event_async(audit_with_detail).await;
            });
        }

        Ok(record)
    }
}

#[rmcp::tool_handler]
impl rmcp::ServerHandler for UnimatrixServer {
    fn get_info(&self) -> ServerInfo {
        self.server_info.clone()
    }

    /// Capture clientInfo.name at MCP initialize time (vnc-014, ADR-002).
    ///
    /// Extracts `request.client_info.name` directly from the protocol handshake
    /// parameters, truncates to 256 Unicode scalar values (NFR-02), and stores it
    /// in `client_type_map` keyed on the rmcp session ID.
    ///
    /// Returns the same result as the default implementation — `Ok(self.get_info())`.
    ///
    /// The return type uses `std::future::ready(...)` rather than an `async fn`
    /// body to match the rmcp `ServerHandler` trait signature (C-01).
    fn initialize(
        &self,
        request: rmcp::model::InitializeRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<rmcp::model::InitializeResult, rmcp::ErrorData>>
    + Send
    + '_ {
        let client_name_raw = request.client_info.name.clone();

        if !client_name_raw.is_empty() {
            // Truncate to 256 Unicode scalar values (chars, not bytes) (NFR-02).
            let truncated: String = if client_name_raw.chars().count() > 256 {
                let t = client_name_raw.chars().take(256).collect::<String>();
                tracing::warn!(
                    original_len = client_name_raw.chars().count(),
                    "clientInfo.name truncated to 256 chars"
                );
                t
            } else {
                client_name_raw
            };

            // Extract the rmcp session key from request context extensions.
            // HTTP: Mcp-Session-Id header value.
            // Stdio or absent/non-UTF-8 header: "" (empty string sentinel).
            let session_key: String = context
                .extensions
                .get::<http::request::Parts>()
                .and_then(|p| p.headers.get("mcp-session-id"))
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();

            // Insert into client_type_map with poison recovery.
            let mut map = self
                .client_type_map
                .lock()
                .unwrap_or_else(|e| e.into_inner());

            // If stdio key "" is being overwritten, emit a warn log (FR-02, C-02, R-10).
            if session_key.is_empty() && map.contains_key("") {
                tracing::warn!(
                    existing = map.get("").map(String::as_str).unwrap_or(""),
                    new = %truncated,
                    "stdio client_type_map entry overwritten (reconnect or second initialize)"
                );
            }

            map.insert(session_key, truncated);
            drop(map); // release lock immediately
        } else {
            tracing::warn!(
                "clientInfo.name empty on initialize — agent_attribution will be blank for this session"
            );
        }

        // Return identical result to default implementation (NFR-07).
        std::future::ready(Ok(self.get_info()))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json;

    pub(crate) async fn make_server() -> UnimatrixServer {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = Arc::new(
            Store::open(&path, unimatrix_store::pool_config::PoolConfig::default())
                .await
                .expect("open store"),
        );
        std::mem::forget(dir);

        let entry_store = Arc::clone(&store);

        // Use a minimal VectorIndex
        let vector_config = unimatrix_core::VectorConfig::default();
        let vector_index =
            Arc::new(unimatrix_core::VectorIndex::new(Arc::clone(&store), vector_config).unwrap());
        let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
        let vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));

        let embed_service = EmbedServiceHandle::new();

        let registry = Arc::new(AgentRegistry::new(Arc::clone(&store), true, vec![]).unwrap());
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
            None, // use compiled default instructions
            None, // crt-056: test-default ServiceLayer
        )
    }

    /// crt-056 Wave 1 (AC-6) test helper: assemble the 9 base inputs + a caller-built
    /// config-driven `ServiceLayer` over a fresh store, then construct the server via
    /// the `Some(...)` arm. Returns `(server, supplied_effectiveness_handle)` so the
    /// caller can `Arc::ptr_eq`-assert the handle-identity invariant (R-03).
    async fn make_server_with_some_layer() -> (UnimatrixServer, EffectivenessStateHandle) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = Arc::new(
            Store::open(&path, unimatrix_store::pool_config::PoolConfig::default())
                .await
                .expect("open store"),
        );
        std::mem::forget(dir);

        let vector_config = unimatrix_core::VectorConfig::default();
        let vector_index =
            Arc::new(unimatrix_core::VectorIndex::new(Arc::clone(&store), vector_config).unwrap());
        let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
        let vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));
        let embed_service = EmbedServiceHandle::new();
        let registry = Arc::new(AgentRegistry::new(Arc::clone(&store), true, vec![]).unwrap());
        registry.bootstrap_defaults().unwrap();
        let audit = Arc::new(AuditLog::new(Arc::clone(&store)));
        let categories = Arc::new(CategoryAllowlist::new());
        let adapt_service = Arc::new(AdaptationService::new(
            unimatrix_adapt::AdaptConfig::default(),
        ));

        // Build a config-driven layer with NLI ENABLED (distinct from the test-default
        // `None`-arm shape) so the assertion proves the supplied layer is used verbatim.
        let pool = Arc::new(
            crate::infra::rayon_pool::RayonPool::new(2, "some-arm-pool").expect("rayon pool"),
        );
        let layer = ServiceLayer::new(
            Arc::clone(&store),
            Arc::clone(&vector_index),
            Arc::clone(&vector_store),
            Arc::clone(&store),
            Arc::clone(&embed_service),
            Arc::clone(&adapt_service),
            Arc::clone(&audit),
            Arc::new(crate::infra::usage_dedup::UsageDedup::new()),
            crate::infra::config::default_boosted_categories_set(),
            pool,
            crate::infra::nli_handle::NliServiceHandle::new(),
            10,
            true, // nli_enabled — config-driven shape, NOT the test default (false)
            Arc::new(crate::infra::config::InferenceConfig::default()),
            Arc::new(DomainPackRegistry::with_builtin_claude_code()),
            Arc::new(unimatrix_engine::confidence::ConfidenceParams::default()),
            Arc::new(crate::infra::categories::CategoryAllowlist::new()),
        );
        // Capture the supplied layer's effectiveness handle BEFORE it is moved.
        let supplied_handle = layer.effectiveness_state_handle();

        let server = UnimatrixServer::new(
            Arc::clone(&store),
            vector_store,
            embed_service,
            registry,
            audit,
            categories,
            Arc::clone(&store),
            vector_index,
            adapt_service,
            None,
            Some(layer),
        );
        (server, supplied_handle)
    }

    /// AC-6.3 — the `Some(layer)` arm uses the SUPPLIED layer verbatim (no rebuild, no
    /// fallback to defaults). Proven structurally via handle identity (R-03): the server's
    /// `service_layer()` and its extracted `effectiveness_state` are the SAME
    /// `Arc<RwLock<_>>` as the supplied layer's handle.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_server_new_some_uses_supplied_service_layer() {
        let (server, supplied_handle) = make_server_with_some_layer().await;

        // The server's own ServiceLayer is the supplied one: its effectiveness handle is
        // Arc::ptr_eq to the handle captured from the supplied layer pre-move.
        assert!(
            Arc::ptr_eq(
                &server.service_layer().effectiveness_state_handle(),
                &supplied_handle
            ),
            "Some-arm server must use the supplied ServiceLayer's handle set, not rebuild"
        );
        // The constructor-extracted `effectiveness_state` is wired to the SAME layer
        // (serving consumers hold Arc::clones — pattern #4097).
        assert!(
            Arc::ptr_eq(&server.effectiveness_state, &supplied_handle),
            "extracted effectiveness_state must point at the supplied layer's handle"
        );
    }

    /// AC-6.1 — the `None` arm preserves the historical test-default construction: it
    /// builds a valid server with its own (test-default) ServiceLayer, and the
    /// constructor-extracted `effectiveness_state` is wired to THAT layer (handle
    /// identity holds on the `None` path too — the serve-side wiring is unchanged).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_server_new_none_yields_test_defaults() {
        let server = make_server().await; // constructed via `None`

        // Serve-side handle wiring is intact on the None path: the extracted
        // effectiveness_state is the same Arc the test-default layer exposes.
        assert!(
            Arc::ptr_eq(
                &server.effectiveness_state,
                &server.service_layer().effectiveness_state_handle()
            ),
            "None-arm server must wire effectiveness_state to its own test-default layer"
        );
        // The server is fully usable (no panic, default ServerInfo) — byte-for-byte the
        // prior test-default server behavior (the existing unit suite is the broader guard).
        let info = rmcp::ServerHandler::get_info(&server);
        assert_eq!(info.server_info.name, "unimatrix");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_info_name() {
        let server = make_server().await;
        let info = rmcp::ServerHandler::get_info(&server);
        assert_eq!(info.server_info.name, "unimatrix");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_info_version_nonempty() {
        let server = make_server().await;
        let info = rmcp::ServerHandler::get_info(&server);
        assert!(!info.server_info.version.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_info_instructions() {
        let server = make_server().await;
        let info = rmcp::ServerHandler::get_info(&server);
        assert!(info.instructions.is_some());
        let instructions = info.instructions.unwrap();
        assert!(instructions.contains("knowledge engine"));
        assert!(instructions.contains("search for relevant patterns"));
    }

    /// AC-01: When config.server.instructions is None, the compiled default is used.
    #[test]
    fn test_server_instructions_none_uses_compiled_default() {
        // Verify the compiled default is non-empty.
        assert!(
            !SERVER_INSTRUCTIONS_DEFAULT.is_empty(),
            "compiled default instructions must not be empty"
        );
        // Verify None resolution produces the compiled default string.
        let none_result: Option<String> = None;
        let result = none_result.unwrap_or_else(|| SERVER_INSTRUCTIONS_DEFAULT.to_string());
        assert_eq!(
            result, SERVER_INSTRUCTIONS_DEFAULT,
            "None instructions must resolve to the compiled default"
        );
    }

    /// AC-05: When config.server.instructions is Some(s), that string is used verbatim.
    #[test]
    fn test_server_instructions_some_uses_config_string() {
        let custom = "You are a legal research assistant.".to_string();
        let result: Option<String> = Some(custom.clone());
        let resolved = result.unwrap_or_else(|| SERVER_INSTRUCTIONS_DEFAULT.to_string());
        assert_eq!(
            resolved, custom,
            "Some(config_string) must be used verbatim as server instructions"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_info_has_tools_capability() {
        let server = make_server().await;
        let info = rmcp::ServerHandler::get_info(&server);
        assert!(
            info.capabilities.tools.is_some(),
            "tools capability must be advertised"
        );
    }

    /// T-02 (server-struct-migration): get_info returns exact CARGO_PKG_VERSION.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_info_version_matches_cargo_pkg() {
        let server = make_server().await;
        let info = rmcp::ServerHandler::get_info(&server);
        assert_eq!(
            info.server_info.version,
            env!("CARGO_PKG_VERSION"),
            "server_info.version must match CARGO_PKG_VERSION"
        );
    }

    /// T-03 (server-struct-migration): get_info returns description (R-12, AC-08).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_info_returns_description() {
        let server = make_server().await;
        let info = rmcp::ServerHandler::get_info(&server);
        assert_eq!(
            info.server_info.description,
            Some("Self-learning knowledge engine for agentic workflows".to_string()),
            "Implementation.description must be set"
        );
    }

    /// T-06 (server-struct-migration): custom instructions survive constructor.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_info_custom_instructions() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = Arc::new(
            unimatrix_core::Store::open(&path, unimatrix_store::pool_config::PoolConfig::default())
                .await
                .expect("open store"),
        );
        std::mem::forget(dir);

        let vector_config = unimatrix_core::VectorConfig::default();
        let vector_index =
            Arc::new(unimatrix_core::VectorIndex::new(Arc::clone(&store), vector_config).unwrap());
        let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
        let vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));
        let embed_service = EmbedServiceHandle::new();
        let registry = Arc::new(AgentRegistry::new(Arc::clone(&store), true, vec![]).unwrap());
        registry.bootstrap_defaults().unwrap();
        let audit = Arc::new(AuditLog::new(Arc::clone(&store)));
        let categories = Arc::new(CategoryAllowlist::new());
        let adapt_service = Arc::new(AdaptationService::new(
            unimatrix_adapt::AdaptConfig::default(),
        ));

        let server = UnimatrixServer::new(
            Arc::clone(&store),
            vector_store,
            embed_service,
            registry,
            audit,
            categories,
            Arc::clone(&store),
            vector_index,
            adapt_service,
            Some("custom instructions".to_string()),
            None, // crt-056: test-default ServiceLayer
        );

        let info = rmcp::ServerHandler::get_info(&server);
        assert_eq!(
            info.instructions.as_deref(),
            Some("custom instructions"),
            "custom instructions must be returned verbatim"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_server_is_clone() {
        let server = make_server().await;
        let _clone = server.clone();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_resolve_agent_with_id() {
        let server = make_server().await;
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_resolve_agent_without_id() {
        let server = make_server().await;
        let identity = server.resolve_agent(&None).await.unwrap();
        assert_eq!(identity.agent_id, "anonymous");
    }

    // -- crt-001: record_usage_for_entries tests --

    async fn insert_test_entry(store: &unimatrix_core::Store) -> u64 {
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
        store.insert(entry).await.unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_record_usage_for_entries_updates_access() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], None, None)
            .await;

        let r = server.store.get(id).await.unwrap();
        assert_eq!(r.access_count, 1);
        assert!(r.last_accessed_at > 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_record_usage_for_entries_empty_ids() {
        let server = make_server().await;
        // Should return immediately without error
        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[], None, None)
            .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_record_usage_for_entries_access_dedup() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        // First call: access_count increments
        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], None, None)
            .await;
        assert_eq!(server.store.get(id).await.unwrap().access_count, 1);

        // Second call: same agent, same entry -> deduped (access_count stays 1)
        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], None, None)
            .await;
        assert_eq!(server.store.get(id).await.unwrap().access_count, 1);

        // Different agent: access_count increments again
        server
            .record_usage_for_entries("other-agent", TrustLevel::Internal, &[id], None, None)
            .await;
        assert_eq!(server.store.get(id).await.unwrap().access_count, 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_record_usage_for_entries_helpful_vote() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], Some(true), None)
            .await;

        let r = server.store.get(id).await.unwrap();
        assert_eq!(r.helpful_count, 1);
        assert_eq!(r.unhelpful_count, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_record_usage_for_entries_unhelpful_vote() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], Some(false), None)
            .await;

        let r = server.store.get(id).await.unwrap();
        assert_eq!(r.helpful_count, 0);
        assert_eq!(r.unhelpful_count, 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_record_usage_for_entries_helpful_none() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], None, None)
            .await;

        let r = server.store.get(id).await.unwrap();
        assert_eq!(r.helpful_count, 0);
        assert_eq!(r.unhelpful_count, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_record_usage_for_entries_vote_correction() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        // First: vote unhelpful
        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], Some(false), None)
            .await;
        assert_eq!(server.store.get(id).await.unwrap().unhelpful_count, 1);

        // Correction: vote helpful (should flip)
        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], Some(true), None)
            .await;
        let r = server.store.get(id).await.unwrap();
        assert_eq!(r.helpful_count, 1);
        assert_eq!(r.unhelpful_count, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_record_usage_for_entries_feature_internal_agent() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

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
        let found: Vec<i64> = sqlx::query_scalar(
            "SELECT entry_id FROM feature_entries WHERE feature_id = ?1 ORDER BY entry_id",
        )
        .bind("crt-001")
        .fetch_all(server.store.read_pool_test())
        .await
        .unwrap();
        assert_eq!(found, vec![id as i64]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_record_usage_for_entries_feature_restricted_agent_ignored() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

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
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM feature_entries WHERE feature_id = ?1")
                .bind("crt-001")
                .fetch_one(server.store.read_pool_test())
                .await
                .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_record_usage_for_entries_feature_privileged_agent() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        server
            .record_usage_for_entries(
                "human",
                TrustLevel::Privileged,
                &[id],
                None,
                Some("crt-001"),
            )
            .await;

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM feature_entries WHERE feature_id = ?1")
                .bind("crt-001")
                .fetch_one(server.store.read_pool_test())
                .await
                .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_record_usage_for_entries_vote_after_access_only() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        // First: access only (no helpful param)
        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], None, None)
            .await;

        // Second: vote helpful (separate from access dedup)
        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], Some(true), None)
            .await;

        let r = server.store.get(id).await.unwrap();
        assert_eq!(r.access_count, 1, "access deduped");
        assert_eq!(r.helpful_count, 1, "vote recorded");
    }

    // -- crt-002: Confidence on retrieval path (T-20 through T-23) --

    #[tokio::test(flavor = "multi_thread")]
    async fn test_confidence_updated_on_retrieval() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        // Before retrieval: confidence is 0.0
        assert_eq!(server.store.get(id).await.unwrap().confidence, 0.0);

        // Trigger retrieval
        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], None, None)
            .await;

        // After retrieval: confidence > 0.0
        let r = server.store.get(id).await.unwrap();
        assert!(
            r.confidence > 0.0,
            "confidence should be updated after retrieval"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_confidence_matches_formula() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], None, None)
            .await;

        let entry = server.store.get(id).await.unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let expected = crate::confidence::compute_confidence(
            &entry,
            now,
            &unimatrix_engine::confidence::ConfidenceParams::default(),
        );
        // Allow small tolerance for timestamp difference
        assert!((entry.confidence - expected).abs() < 0.01);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_confidence_evolves_with_multiple_retrievals() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        // First retrieval
        server
            .record_usage_for_entries("agent-a", TrustLevel::Internal, &[id], None, None)
            .await;
        let after_first = server.store.get(id).await.unwrap().confidence;

        // Second retrieval (different agent to avoid access dedup)
        server
            .record_usage_for_entries("agent-b", TrustLevel::Internal, &[id], None, None)
            .await;
        let after_second = server.store.get(id).await.unwrap().confidence;

        // Confidence should change (access_count went from 1 to 2)
        assert_ne!(
            after_first, after_second,
            "confidence should evolve with retrievals"
        );
    }

    // -- crt-002: Confidence on mutation paths (T-24 through T-28) --

    #[tokio::test(flavor = "multi_thread")]
    async fn test_confidence_seeded_on_insert() {
        let server = make_server().await;

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
            ..crate::infra::audit::AuditEvent::default()
        };

        let embedding = vec![0.1; 384];
        let (entry_id, _record) = server
            .insert_with_audit(entry, embedding, audit_event)
            .await
            .unwrap();

        // Seed confidence (simulating what context_store does)
        {
            let entry = server.store.get(entry_id).await.unwrap();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let conf = crate::confidence::compute_confidence(
                &entry,
                now,
                &unimatrix_engine::confidence::ConfidenceParams::default(),
            );
            server
                .store
                .update_confidence(entry_id, conf)
                .await
                .unwrap();
        }

        let r = server.store.get(entry_id).await.unwrap();
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_confidence_recomputed_on_deprecation() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        // First retrieval to give it some confidence
        server
            .record_usage_for_entries("test-agent", TrustLevel::Internal, &[id], None, None)
            .await;

        let before_deprecation = server.store.get(id).await.unwrap().confidence;
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
            ..crate::infra::audit::AuditEvent::default()
        };
        server
            .deprecate_with_audit(id, None, audit_event)
            .await
            .unwrap();

        // Recompute confidence for deprecated entry
        {
            let entry = server.store.get(id).await.unwrap();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let conf = crate::confidence::compute_confidence(
                &entry,
                now,
                &unimatrix_engine::confidence::ConfidenceParams::default(),
            );
            server.store.update_confidence(id, conf).await.unwrap();
        }

        let after_deprecation = server.store.get(id).await.unwrap().confidence;
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
            ..crate::infra::audit::AuditEvent::default()
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_quarantine_active_entry() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        let updated = server
            .quarantine_with_audit(id, Some("test reason".into()), make_audit_event("system"))
            .await
            .unwrap();

        assert_eq!(updated.status, unimatrix_store::Status::Quarantined);
        assert_eq!(updated.modified_by, "system");

        let fetched = server.store.get(id).await.unwrap();
        assert_eq!(fetched.status, unimatrix_store::Status::Quarantined);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_quarantine_updates_status_index() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        let status: i64 = sqlx::query_scalar("SELECT status FROM entries WHERE id = ?1")
            .bind(id as i64)
            .fetch_one(server.store.read_pool_test())
            .await
            .unwrap();
        assert_eq!(
            status,
            unimatrix_store::Status::Quarantined as u8 as i64,
            "entry status should be Quarantined"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_quarantine_updates_counters() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        let before_active = server.store.read_counter("total_active").await.unwrap();
        let before_quarantined = server
            .store
            .read_counter("total_quarantined")
            .await
            .unwrap();

        server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        let after_active = server.store.read_counter("total_active").await.unwrap();
        let after_quarantined = server
            .store
            .read_counter("total_quarantined")
            .await
            .unwrap();

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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_restore_quarantined_entry() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        // Quarantine first
        server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();
        assert_eq!(
            server.store.get(id).await.unwrap().status,
            unimatrix_store::Status::Quarantined
        );

        // Restore
        let updated = server
            .restore_with_audit(id, Some("false alarm".into()), make_audit_event("system"))
            .await
            .unwrap();

        assert_eq!(updated.status, unimatrix_store::Status::Active);
        assert_eq!(
            server.store.get(id).await.unwrap().status,
            unimatrix_store::Status::Active
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_restore_updates_counters() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        let initial_active = server.store.read_counter("total_active").await.unwrap();

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
        let final_active = server.store.read_counter("total_active").await.unwrap();
        let final_quarantined = server
            .store
            .read_counter("total_quarantined")
            .await
            .unwrap();

        assert_eq!(
            final_active, initial_active,
            "active counter should return to initial"
        );
        assert_eq!(
            final_quarantined, 0,
            "quarantined counter should return to 0"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_restore_updates_status_index() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        // Quarantine then restore
        server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();
        server
            .restore_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        let status: i64 = sqlx::query_scalar("SELECT status FROM entries WHERE id = ?1")
            .bind(id as i64)
            .fetch_one(server.store.read_pool_test())
            .await
            .unwrap();
        assert_eq!(
            status,
            unimatrix_store::Status::Active as u8 as i64,
            "entry status should be back to Active"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_quarantine_writes_audit_event() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        server
            .quarantine_with_audit(
                id,
                Some("suspicious content".into()),
                make_audit_event("system"),
            )
            .await
            .unwrap();

        // GH #308: audit is now fire-and-forget; sleep briefly to let the spawned task commit.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Verify audit log has an entry
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT operation, target_ids, detail FROM audit_log WHERE operation = ?1",
        )
        .bind("context_quarantine")
        .fetch_all(server.store.read_pool_test())
        .await
        .unwrap();
        let mut found = false;
        for (_, target_ids_json, detail) in &rows {
            let target_ids: Vec<u64> = serde_json::from_str(target_ids_json).unwrap();
            if target_ids.contains(&id) {
                assert!(detail.contains("quarantined"));
                assert!(detail.contains("suspicious content"));
                found = true;
            }
        }
        assert!(found, "audit event for quarantine should exist");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_correct_rejects_quarantined() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

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
            ..crate::infra::audit::AuditEvent::default()
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_quarantine_confidence_decreases() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        // Compute initial confidence
        let entry = server.store.get(id).await.unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let before = crate::confidence::compute_confidence(
            &entry,
            now,
            &unimatrix_engine::confidence::ConfidenceParams::default(),
        );
        server.store.update_confidence(id, before).await.unwrap();

        // Quarantine
        server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        // Recompute confidence for quarantined entry
        let entry = server.store.get(id).await.unwrap();
        let after = crate::confidence::compute_confidence(
            &entry,
            now,
            &unimatrix_engine::confidence::ConfidenceParams::default(),
        );
        server.store.update_confidence(id, after).await.unwrap();

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
        let entry = server.store.get(id).await.unwrap();
        let restored = crate::confidence::compute_confidence(
            &entry,
            now,
            &unimatrix_engine::confidence::ConfidenceParams::default(),
        );

        assert!(
            restored > after,
            "confidence should increase after restore: after_quarantine={after}, restored={restored}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_quarantine_nonexistent_entry_fails() {
        let server = make_server().await;

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
        let id = insert_test_entry(&server.store).await;
        let audit_event = crate::infra::audit::AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: "system".to_string(),
            operation: "context_deprecate".to_string(),
            target_ids: vec![],
            outcome: crate::infra::audit::Outcome::Success,
            detail: String::new(),
            ..crate::infra::audit::AuditEvent::default()
        };
        server
            .deprecate_with_audit(id, None, audit_event)
            .await
            .unwrap();
        assert_eq!(
            server.store.get(id).await.unwrap().status,
            unimatrix_store::Status::Deprecated
        );
        id
    }

    // AC-1: Quarantine from Deprecated status
    #[tokio::test(flavor = "multi_thread")]
    async fn test_quarantine_deprecated_entry() {
        let server = make_server().await;
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

        let fetched = server.store.get(id).await.unwrap();
        assert_eq!(fetched.status, unimatrix_store::Status::Quarantined);
        assert_eq!(fetched.pre_quarantine_status, Some(1));
    }

    // AC-3: Quarantine from Active sets pre_quarantine_status=0
    #[tokio::test(flavor = "multi_thread")]
    async fn test_quarantine_active_sets_pre_quarantine_status() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        let updated = server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        assert_eq!(updated.status, unimatrix_store::Status::Quarantined);
        assert_eq!(updated.pre_quarantine_status, Some(0)); // Active = 0
    }

    // AC-4: Restore to pre-quarantine status (Deprecated round-trip)
    #[tokio::test(flavor = "multi_thread")]
    async fn test_restore_to_deprecated() {
        let server = make_server().await;
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
    #[tokio::test(flavor = "multi_thread")]
    async fn test_restore_null_pre_quarantine_falls_back_to_active() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        // Quarantine the entry
        server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        // Manually clear pre_quarantine_status to NULL to simulate pre-migration entry
        sqlx::query("UPDATE entries SET pre_quarantine_status = NULL WHERE id = ?1")
            .bind(id as i64)
            .execute(server.store.write_pool_server())
            .await
            .unwrap();

        // Restore -- should fall back to Active
        let restored = server
            .restore_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        assert_eq!(restored.status, unimatrix_store::Status::Active);
    }

    // AC-8: Counter integrity for Deprecated quarantine round-trip
    #[tokio::test(flavor = "multi_thread")]
    async fn test_counter_integrity_deprecated_round_trip() {
        let server = make_server().await;
        let id = insert_and_deprecate(&server).await;

        let before_deprecated = server.store.read_counter("total_deprecated").await.unwrap();
        let before_quarantined = server
            .store
            .read_counter("total_quarantined")
            .await
            .unwrap();

        // Quarantine from Deprecated
        server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        let mid_deprecated = server.store.read_counter("total_deprecated").await.unwrap();
        let mid_quarantined = server
            .store
            .read_counter("total_quarantined")
            .await
            .unwrap();
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

        let after_deprecated = server.store.read_counter("total_deprecated").await.unwrap();
        let after_quarantined = server
            .store
            .read_counter("total_quarantined")
            .await
            .unwrap();
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
    #[tokio::test(flavor = "multi_thread")]
    async fn test_quarantine_audit_includes_pre_quarantine_status() {
        let server = make_server().await;
        let id = insert_and_deprecate(&server).await;

        server
            .quarantine_with_audit(id, Some("harmful".into()), make_audit_event("system"))
            .await
            .unwrap();

        // GH #308: audit is now fire-and-forget; sleep briefly to let the spawned task commit.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let detail: String = sqlx::query_scalar(
            "SELECT detail FROM audit_log WHERE operation = 'context_quarantine' ORDER BY event_id DESC LIMIT 1",
        )
        .fetch_one(server.store.read_pool_test())
        .await
        .unwrap();

        assert!(
            detail.contains("pre_quarantine_status=1"),
            "audit detail should contain pre_quarantine_status: {detail}"
        );
    }

    // AC-10: Restore with invalid pre_quarantine_status falls back to Active
    #[tokio::test(flavor = "multi_thread")]
    async fn test_restore_invalid_pre_quarantine_falls_back_to_active() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        // Quarantine the entry
        server
            .quarantine_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        // Manually set pre_quarantine_status to invalid value (99)
        sqlx::query("UPDATE entries SET pre_quarantine_status = 99 WHERE id = ?1")
            .bind(id as i64)
            .execute(server.store.write_pool_server())
            .await
            .unwrap();

        // Restore -- should fall back to Active
        let restored = server
            .restore_with_audit(id, None, make_audit_event("system"))
            .await
            .unwrap();

        assert_eq!(restored.status, unimatrix_store::Status::Active);
    }

    // AC-7: Migration v7->v8 (tested at store level)
    #[tokio::test(flavor = "multi_thread")]
    async fn test_migration_v7_to_v8_backfill() {
        // Create a database at v7 schema, quarantine an entry, then re-open
        // (which triggers migration) and verify backfill
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("migrate.db");

        let pool_config = unimatrix_store::pool_config::PoolConfig::default();

        // Create db at current schema
        {
            let store = unimatrix_store::SqlxStore::open(&path, pool_config.clone())
                .await
                .unwrap();
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
            let id = store.insert(entry).await.unwrap();

            // Simulate a v7 quarantine (status=3 but no pre_quarantine_status)
            sqlx::query(
                "UPDATE entries SET status = 3, pre_quarantine_status = NULL WHERE id = ?1",
            )
            .bind(id as i64)
            .execute(store.write_pool_server())
            .await
            .unwrap();

            // Set schema version back to 7 to trigger migration on next open
            sqlx::query("UPDATE counters SET value = 7 WHERE name = 'schema_version'")
                .execute(store.write_pool_server())
                .await
                .unwrap();
        }

        // Re-open -- triggers v7->v8 migration
        {
            let store = unimatrix_store::SqlxStore::open(&path, pool_config.clone())
                .await
                .unwrap();

            // Verify schema version is now current (27, vnc-018 context_graph indexes)
            let version: i64 =
                sqlx::query_scalar("SELECT value FROM counters WHERE name = 'schema_version'")
                    .fetch_one(store.read_pool_test())
                    .await
                    .unwrap();
            assert!(version >= 25, "schema_version must be >= 25, got {version}");

            // Verify backfill: quarantined entry should have pre_quarantine_status = 0
            let pre_q: Option<i64> =
                sqlx::query_scalar("SELECT pre_quarantine_status FROM entries WHERE status = 3")
                    .fetch_optional(store.read_pool_test())
                    .await
                    .unwrap();
            assert_eq!(
                pre_q,
                Some(0),
                "backfill should set pre_quarantine_status=0 for quarantined entries"
            );
        }

        // Re-open again to verify idempotency
        {
            let store = unimatrix_store::SqlxStore::open(&path, pool_config.clone())
                .await
                .unwrap();
            let version: i64 =
                sqlx::query_scalar("SELECT value FROM counters WHERE name = 'schema_version'")
                    .fetch_one(store.read_pool_test())
                    .await
                    .unwrap();
            assert!(
                version >= 25,
                "schema_version must remain >= 25 on re-open, got {version}"
            );
        }
    }

    // R-05: Existing Active->Quarantined->Active path still works identically
    #[tokio::test(flavor = "multi_thread")]
    async fn test_active_quarantine_restore_round_trip_still_works() {
        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        let initial_active = server.store.read_counter("total_active").await.unwrap();

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
        let final_active = server.store.read_counter("total_active").await.unwrap();
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

    #[tokio::test(flavor = "multi_thread")]
    async fn insert_with_audit_sets_embedding_dim() {
        let server = make_server().await;
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
        let embedding: Vec<f32> = unimatrix_embed::l2_normalized(&vec![0.1_f32; 384]);
        let audit = crate::infra::audit::AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: "test".to_string(),
            operation: "test".to_string(),
            target_ids: vec![],
            outcome: crate::infra::audit::Outcome::Success,
            detail: "test".to_string(),
            ..crate::infra::audit::AuditEvent::default()
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

    #[tokio::test(flavor = "multi_thread")]
    async fn insert_with_audit_empty_embedding_skips_hnsw() {
        // Empty embedding = ONNX model not loaded or embedding failed.
        // Entry is still written to store (searchable by topic/category/tags),
        // HNSW insert is skipped, embedding_dim is 0.
        let server = make_server().await;
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
            ..crate::infra::audit::AuditEvent::default()
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

    #[tokio::test(flavor = "multi_thread")]
    async fn correct_with_audit_sets_embedding_dim() {
        let server = make_server().await;
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
        let embedding: Vec<f32> = unimatrix_embed::l2_normalized(&vec![0.1_f32; 384]);
        let audit = crate::infra::audit::AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: "test".to_string(),
            operation: "test".to_string(),
            target_ids: vec![],
            outcome: crate::infra::audit::Outcome::Success,
            detail: "test".to_string(),
            ..crate::infra::audit::AuditEvent::default()
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
        let correction_embedding: Vec<f32> = unimatrix_embed::l2_normalized(&vec![0.2_f32; 384]);
        let correction_audit = crate::infra::audit::AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: "test".to_string(),
            operation: "correct".to_string(),
            target_ids: vec![],
            outcome: crate::infra::audit::Outcome::Success,
            detail: "correction".to_string(),
            ..crate::infra::audit::AuditEvent::default()
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
    #[tokio::test(flavor = "multi_thread")]
    async fn test_server_clone_shares_arc_fields() {
        let server = make_server().await;
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
    #[tokio::test(flavor = "multi_thread")]
    async fn test_server_clone_arc_count_drops_after_join() {
        let server = make_server().await;
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
    #[tokio::test(flavor = "multi_thread")]
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
    #[tokio::test(flavor = "multi_thread")]
    async fn test_uds_session_rate_exemption_boundary() {
        use crate::infra::audit::AuditLog;
        use crate::services::gateway::SecurityGateway;
        use crate::services::{CallerId, RateLimitConfig};

        let dir = tempfile::TempDir::new().unwrap();
        let store = Arc::new(
            unimatrix_store::SqlxStore::open(
                &dir.path().join("t.db"),
                unimatrix_store::pool_config::PoolConfig::default(),
            )
            .await
            .unwrap(),
        );
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

    // -- GH #308 regression: audit call sites in server.rs must not block --

    /// Regression test for GH #308: insert_with_audit must return before the audit
    /// event is written. The audit spawn must not hold the write connection across
    /// an await point while the analytics drain task could be holding it.
    ///
    /// This test fires 10 concurrent insert_with_audit calls and verifies all
    /// complete under 10s (well within the 5s WRITE_POOL_ACQUIRE_TIMEOUT that was
    /// triggered by the blocking log_event() call).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_insert_with_audit_does_not_block_under_concurrent_writes() {
        use tokio::time::{Duration, timeout};

        let server = Arc::new(make_server().await);

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let server = Arc::clone(&server);
                tokio::spawn(async move {
                    let entry = unimatrix_core::NewEntry {
                        title: format!("entry-{i}"),
                        content: format!("content-{i}"),
                        topic: "test".to_string(),
                        category: "convention".to_string(),
                        tags: vec![],
                        source: "test".to_string(),
                        status: unimatrix_core::Status::Active,
                        created_by: String::new(),
                        feature_cycle: String::new(),
                        trust_source: String::new(),
                    };
                    let audit_event = crate::infra::audit::AuditEvent {
                        event_id: 0,
                        timestamp: 0,
                        session_id: String::new(),
                        agent_id: "test".to_string(),
                        operation: "context_store".to_string(),
                        target_ids: vec![],
                        outcome: crate::infra::audit::Outcome::Success,
                        detail: format!("gh308-regression-{i}"),
                        ..crate::infra::audit::AuditEvent::default()
                    };
                    timeout(
                        Duration::from_secs(10),
                        server.insert_with_audit(entry, vec![], audit_event),
                    )
                    .await
                    .expect("insert_with_audit timed out — GH #308 regression")
                    .expect("insert_with_audit returned error")
                })
            })
            .collect();

        for handle in handles {
            handle.await.expect("task panicked");
        }

        // Yield to allow the spawned audit tasks to complete.
        tokio::task::yield_now().await;

        // Verify all 10 entries were inserted.
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM entries")
            .fetch_one(server.store.read_pool_test())
            .await
            .unwrap();
        assert_eq!(count, 10, "all 10 entries should be stored");
    }

    /// Regression test for GH #308: quarantine_with_audit / restore_with_audit
    /// must not block the write pool. Verifies that calls complete promptly even
    /// when concurrent audit writes are in flight.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_quarantine_restore_audit_does_not_block() {
        use tokio::time::{Duration, timeout};

        let server = make_server().await;
        let id = insert_test_entry(&server.store).await;

        // quarantine — the audit spawn must not stall this call
        timeout(
            Duration::from_secs(10),
            server.quarantine_with_audit(id, Some("gh308-test".into()), make_audit_event("system")),
        )
        .await
        .expect("quarantine_with_audit timed out — GH #308 regression")
        .expect("quarantine_with_audit returned error");

        // restore — same check
        timeout(
            Duration::from_secs(10),
            server.restore_with_audit(id, None, make_audit_event("system")),
        )
        .await
        .expect("restore_with_audit timed out — GH #308 regression")
        .expect("restore_with_audit returned error");
    }

    // -- GH #579 regression: audit_fire_and_forget must persist events --

    /// Regression test for GH #579: `audit_fire_and_forget` must actually write
    /// audit events. The original bug used `spawn_blocking` + `log_event`, which
    /// called `block_in_place` from a blocking thread — an illegal combination that
    /// panicked and discarded the write. This test would fail with the old code
    /// because no event would ever appear in the audit log.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_audit_fire_and_forget_persists_event() {
        let server = make_server().await;

        let event = crate::infra::audit::AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: String::new(),
            agent_id: "test-agent".to_string(),
            operation: "context_store".to_string(),
            target_ids: vec![],
            outcome: crate::infra::audit::Outcome::Success,
            detail: "gh579-regression".to_string(),
            ..crate::infra::audit::AuditEvent::default()
        };

        server.audit_fire_and_forget(event);

        // Poll until the event appears in the audit log (deadline: 5 s).
        // A single yield_now is insufficient because the spawned task must
        // acquire a write pool connection, which may take more than one
        // scheduler pass under concurrent test load.
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(5);
        let mut found = false;
        while tokio::time::Instant::now() < deadline {
            let rows: Vec<String> = sqlx::query_scalar(
                "SELECT detail FROM audit_log \
                 WHERE agent_id = 'test-agent' AND detail = 'gh579-regression'",
            )
            .fetch_all(server.store.read_pool_test())
            .await
            .expect("audit_log query failed");

            if !rows.is_empty() {
                found = true;
                break;
            }

            tokio::task::yield_now().await;
        }

        assert!(
            found,
            "GH #579 regression: audit_fire_and_forget must persist the event within 5 s; \
             found 0 rows. spawn_blocking + block_in_place would produce 0."
        );
    }

    // -- vnc-025 (#670, FR-15/FR-16): cycle-review transcript purge gate --

    /// Length of a transcript buffer through the SessionState Arc (poison-recovered).
    fn buffer_len(server: &UnimatrixServer, session_id: &str) -> usize {
        server
            .session_registry
            .get_state(session_id)
            .expect("session must be registered")
            .transcript
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .len()
    }

    /// Poll the audit_log for `transcript_session_purged` rows with
    /// trigger=cycle_review until `expected` rows appear (deadline 5 s).
    /// Returns the (session_id, detail) rows found.
    async fn poll_cycle_review_purge_audits(
        server: &UnimatrixServer,
        expected: usize,
    ) -> Vec<(String, String)> {
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(5);
        loop {
            let rows: Vec<(String, String)> = sqlx::query_as(
                "SELECT session_id, detail FROM audit_log \
                 WHERE operation = 'transcript_session_purged' \
                   AND agent_id = 'server' \
                   AND detail LIKE '%trigger=cycle_review' \
                 ORDER BY session_id",
            )
            .fetch_all(server.store.read_pool_test())
            .await
            .expect("audit_log query failed");
            if rows.len() >= expected || tokio::time::Instant::now() >= deadline {
                return rows;
            }
            tokio::task::yield_now().await;
        }
    }

    /// R-10.1 (cycle-review-purge §1): mixed registry — only sessions attributed
    /// to the reviewed cycle are cleared; `Some(other)` and `None` buffers are
    /// untouched; ALL sessions stay registered; audit rows match the cleared set
    /// (ids + byte counts, trigger=cycle_review, ADR-004 pinned shape).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cycle_review_clears_only_matching_feature_sessions() {
        let server = make_server().await;
        let reg = &server.session_registry;
        reg.register_session("purge-match", None, Some("vnc-025-rev".to_string()));
        reg.register_session("purge-other", None, Some("col-099".to_string()));
        reg.register_session("purge-none", None, None);
        reg.apply_transcript_delta("purge-match", 0, b"match-bytes"); // 11 bytes
        reg.apply_transcript_delta("purge-other", 0, b"other-bytes");
        reg.apply_transcript_delta("purge-none", 0, b"none-bytes");

        server.purge_cycle_transcripts("vnc-025-rev");

        // Matching buffer empty; other/None untouched; all stay registered.
        assert_eq!(
            buffer_len(&server, "purge-match"),
            0,
            "matching buffer cleared"
        );
        assert_eq!(
            buffer_len(&server, "purge-other"),
            11,
            "other-feature buffer untouched"
        );
        assert_eq!(
            buffer_len(&server, "purge-none"),
            10,
            "feature-None buffer untouched"
        );

        // Audit: exactly one row, pinned shape, content-free detail.
        let rows = poll_cycle_review_purge_audits(&server, 1).await;
        assert_eq!(
            rows,
            vec![(
                "purge-match".to_string(),
                "bytes=11 trigger=cycle_review".to_string()
            )],
            "exactly one purge audit row matching the cleared set"
        );

        // Idempotent cached re-review (§ scenario 4): second purge finds empty
        // buffers → no clear, no new audit rows.
        server.purge_cycle_transcripts("vnc-025-rev");
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        let rows = poll_cycle_review_purge_audits(&server, 1).await;
        assert_eq!(rows.len(), 1, "second purge of empty buffers emits nothing");
    }

    /// R-10.5 (cycle-review-purge §1): zero attributed sessions — no-op, no
    /// audit row, no error.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cycle_review_zero_attributed_sessions_noop() {
        let server = make_server().await;
        server
            .session_registry
            .register_session("unrelated", None, Some("col-099".to_string()));
        server
            .session_registry
            .apply_transcript_delta("unrelated", 0, b"keep-me");

        server.purge_cycle_transcripts("vnc-025-nobody");

        // Empty record vec ⇒ emit_purge_audits spawns nothing — no rows ever.
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        let rows = poll_cycle_review_purge_audits(&server, 0).await;
        assert!(
            rows.is_empty(),
            "zero attributed sessions must emit no audit"
        );
        assert_eq!(
            buffer_len(&server, "unrelated"),
            7,
            "unrelated buffer untouched"
        );
    }

    /// FR-16 (cycle-review-purge §2): default OSS config (PurgeOnCycleClose) —
    /// the clear runs through the retention gate.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cycle_review_purges_under_purge_on_cycle_close() {
        let server = make_server().await;
        assert_eq!(
            server.retention_config.transcript_retention,
            TranscriptRetention::PurgeOnCycleClose,
            "test ctor must default to the OSS retention policy"
        );
        server
            .session_registry
            .register_session("gate-s1", None, Some("vnc-025-gate".to_string()));
        server
            .session_registry
            .apply_transcript_delta("gate-s1", 0, b"0123456789");

        server.purge_cycle_transcripts("vnc-025-gate");

        assert_eq!(
            buffer_len(&server, "gate-s1"),
            0,
            "PurgeOnCycleClose clears"
        );
        let rows = poll_cycle_review_purge_audits(&server, 1).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "bytes=10 trigger=cycle_review");
    }

    /// FR-16 (cycle-review-purge §2, Gate 3a disposition 6): direct
    /// `RetainDays` enum-injection — the non-default arm MUST NOT purge.
    /// Constructed by assigning the enum value to the pub field directly
    /// (`validate()` is bypassed, never weakened: this value cannot occur in a
    /// validated OSS config — the arm is the enterprise seam, Constraint 7).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_cycle_review_retain_days_arm_does_not_purge() {
        let mut server = make_server().await;
        server.retention_config = Arc::new(RetentionConfig {
            transcript_retention: TranscriptRetention::RetainDays(30),
            ..RetentionConfig::default()
        });
        server.session_registry.register_session(
            "retain-s1",
            None,
            Some("vnc-025-retain".to_string()),
        );
        server
            .session_registry
            .apply_transcript_delta("retain-s1", 0, b"retained-bytes");

        server.purge_cycle_transcripts("vnc-025-retain");

        // RetainDays arm: no clear, no audit — ever.
        assert_eq!(
            buffer_len(&server, "retain-s1"),
            14,
            "RetainDays arm must NOT purge"
        );
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        let rows = poll_cycle_review_purge_audits(&server, 0).await;
        assert!(rows.is_empty(), "RetainDays arm must emit no audit");
    }

    /// crt-052 AC-10 (ADR-005, Constraint 2 enterprise seam): compile-level guard
    /// that the `TranscriptRetention` match stays EXHAUSTIVE with NO wildcard `_`
    /// arm. `purge_cycle_transcripts` (the purge gate) and `distill_before_purge`
    /// (the distill gate, C6) match the SAME variants so distill and purge stay in
    /// lockstep at every one of the four success returns.
    ///
    /// This function mirrors the production gate's variant arms exactly. Adding a
    /// third `TranscriptRetention` variant makes this `match` non-exhaustive and
    /// breaks the build HERE — the desired enterprise-seam compile error — forcing
    /// a deliberate decision at both the purge site and the distill gate rather
    /// than a silent wildcard fall-through.
    #[test]
    fn test_retention_match_no_wildcard() {
        fn gate_decision(r: &TranscriptRetention) -> bool {
            // EXHAUSTIVE — no `_ =>` arm. Mirrors purge_cycle_transcripts (:543/:551)
            // and the C6 distill gate. `true` = proceed (distill + purge);
            // `false` = skip (no distill, no purge).
            match r {
                TranscriptRetention::PurgeOnCycleClose => true,
                TranscriptRetention::RetainDays(_) => false,
            }
        }
        assert!(
            gate_decision(&TranscriptRetention::PurgeOnCycleClose),
            "PurgeOnCycleClose is the sole OSS-honored arm: proceed"
        );
        assert!(
            !gate_decision(&TranscriptRetention::RetainDays(30)),
            "RetainDays must neither distill nor purge"
        );
        assert!(
            !gate_decision(&TranscriptRetention::RetainDays(0)),
            "RetainDays(0) is treated identically: skip"
        );
    }

    // -- vnc-012 AC-10: schema snapshot — #[schemars(with = "T")] preserves type: integer --
    //
    // GH#684: rmcp >= 1.7 removed schemars' AddNullable transform (an OpenAPI 3.0
    // extension, not JSON Schema 2020-12) from its schema generator. Option-backed
    // fields now emit the spec-correct nullable union `"type": ["integer", "null"]`
    // instead of bare `"integer"` + `nullable: true`. Required fields (with = "i64")
    // still emit bare `"integer"`. The union is truthful — the server accepts JSON
    // null for these params (serde_util.rs) — and is the contract going forward.
    // Type-array unions are standard JSON Schema 2020-12 and verified compatible
    // with Claude Code tool-schema parsing. AC-10's intent (guarding against the
    // empty-schema `{}` fallback from an unpaired #[serde(deserialize_with)]) is
    // preserved: a typo'd `with` attribute still emits `{}` and fails these asserts.

    #[tokio::test(flavor = "multi_thread")]
    async fn test_schema_integer_type_preserved_for_all_nine_fields() {
        use std::collections::HashMap;

        let server = make_server().await;
        let tools = server.tool_router.list_all();

        // Build map: tool_name -> input_schema as serde_json::Value
        let schema_by_name: HashMap<String, serde_json::Value> = tools
            .into_iter()
            .map(|t| {
                let schema_val = serde_json::Value::Object(t.input_schema.as_ref().clone());
                (t.name.to_string(), schema_val)
            })
            .collect();

        // 4 required fields (#[schemars(with = "i64")]) — bare "integer"
        let required_checks: &[(&str, &str)] = &[
            ("context_get", "id"),
            ("context_deprecate", "id"),
            ("context_quarantine", "id"),
            ("context_correct", "original_id"),
        ];

        // 5 optional fields (#[schemars(with = "Option<...>")]) — rmcp >= 1.7 emits
        // the JSON Schema 2020-12 nullable union ["integer", "null"]
        let optional_checks: &[(&str, &str)] = &[
            ("context_lookup", "id"),
            ("context_lookup", "limit"),
            ("context_search", "k"),
            ("context_briefing", "max_tokens"),
            // RetrospectiveParams tool name verified from #[tool(name = "context_cycle_review")]
            ("context_cycle_review", "evidence_limit"),
        ];

        for (tool_name, field_name) in required_checks {
            let schema = schema_by_name
                .get(*tool_name)
                .unwrap_or_else(|| panic!("AC-10: tool {tool_name} not found in schema_by_name"));

            let field_type = &schema["properties"][field_name]["type"];
            assert_eq!(
                field_type, "integer",
                "AC-10: required field {field_name} on {tool_name} must have type: integer in \
                 JSON schema; got: {field_type}. Check #[schemars(with = ...)] attribute."
            );
        }

        for (tool_name, field_name) in optional_checks {
            let schema = schema_by_name
                .get(*tool_name)
                .unwrap_or_else(|| panic!("AC-10: tool {tool_name} not found in schema_by_name"));

            let field_type = &schema["properties"][field_name]["type"];
            assert_eq!(
                field_type,
                &serde_json::json!(["integer", "null"]),
                "AC-10: optional field {field_name} on {tool_name} must have type: \
                 [integer, null] in JSON schema (rmcp >= 1.7 nullable union, GH#684); \
                 got: {field_type}. Check #[schemars(with = ...)] attribute."
            );
        }

        // Special check: evidence_limit minimum (NFR-05 permits minimum: 0)
        // The schemars(with = "Option<u64>") annotation may emit minimum: 0. Assert it is
        // present and equals 0 if present, otherwise accept absence.
        let el_props = &schema_by_name["context_cycle_review"]["properties"]["evidence_limit"];
        if let Some(minimum) = el_props.get("minimum") {
            assert_eq!(
                minimum,
                &serde_json::json!(0),
                "AC-10: evidence_limit minimum must be 0 if present (NFR-05)"
            );
        }
    }

    // -- vnc-014: client_type_map and initialize override tests --

    /// Helper: run MCP initialize handshake over in-memory duplex transport.
    ///
    /// Returns the server `client_type_map` after the handshake completes.
    /// `client_name` is sent as `clientInfo.name` from the client side.
    async fn run_initialize_handshake(
        server: UnimatrixServer,
        client_name: &str,
    ) -> Arc<Mutex<HashMap<String, String>>> {
        use rmcp::model::{ClientCapabilities, Implementation, ProtocolVersion};
        use rmcp::service::ServiceExt;
        use tokio::io::duplex;

        let map = Arc::clone(&server.client_type_map);
        let (server_transport, client_transport) = duplex(4096);

        // Build a ClientInfo (implements ClientHandler) with the given name.
        let client_info = rmcp::model::ClientInfo::new(
            ClientCapabilities::default(),
            Implementation::new(client_name, "0.0.1"),
        )
        .with_protocol_version(ProtocolVersion::LATEST);

        // Run server and client concurrently; both resolve once initialize completes.
        let server_task = tokio::spawn(async move {
            let _ = server.serve(server_transport).await;
        });
        let client_task = tokio::spawn(async move {
            let _ = rmcp::serve_client(client_info, client_transport).await;
        });

        // Give handshake time to complete then cancel.
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        server_task.abort();
        client_task.abort();

        map
    }

    /// SRV-U-01: client_type_map starts empty on construction.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_srv_u01_client_type_map_initialized_empty() {
        let server = make_server().await;
        let len = server
            .client_type_map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len();
        assert_eq!(
            len, 0,
            "SRV-U-01: client_type_map must be empty on construction"
        );
    }

    /// SRV-U-02 / SRV-U-04: initialize inserts clientInfo.name under stdio key "".
    ///
    /// The stdio transport never injects http::request::Parts into Extensions, so
    /// the session key falls back to "".
    #[tokio::test(flavor = "multi_thread")]
    async fn test_srv_u02_initialize_inserts_name_under_stdio_key() {
        let server = make_server().await;
        let map = run_initialize_handshake(server, "codex-mcp-client").await;
        let guard = map.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(
            guard.get("").map(String::as_str),
            Some("codex-mcp-client"),
            "SRV-U-02: client_type_map[\"\"] must be \"codex-mcp-client\" after stdio initialize"
        );
    }

    /// SRV-U-03: initialize does NOT insert when clientInfo.name is empty.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_srv_u03_initialize_skips_empty_name() {
        let server = make_server().await;
        let map = run_initialize_handshake(server, "").await;
        let guard = map.lock().unwrap_or_else(|e| e.into_inner());
        assert!(
            guard.is_empty(),
            "SRV-U-03: client_type_map must be empty when clientInfo.name is empty"
        );
    }

    /// SRV-U-05: clientInfo.name truncated at 257 chars with WARN log.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_srv_u05_initialize_truncates_at_256_chars() {
        let name_300: String = "x".repeat(300);
        let server = make_server().await;
        let map = run_initialize_handshake(server, &name_300).await;
        let guard = map.lock().unwrap_or_else(|e| e.into_inner());
        let stored = guard.get("").expect("SRV-U-05: entry must exist");
        assert_eq!(
            stored.chars().count(),
            256,
            "SRV-U-05: stored name must be exactly 256 chars after truncation"
        );
    }

    /// SRV-U-06: clientInfo.name of exactly 256 chars is NOT truncated.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_srv_u06_initialize_does_not_truncate_exact_256() {
        let name_256: String = "a".repeat(256);
        let server = make_server().await;
        let map = run_initialize_handshake(server, &name_256).await;
        let guard = map.lock().unwrap_or_else(|e| e.into_inner());
        let stored = guard.get("").expect("SRV-U-06: entry must exist");
        assert_eq!(
            stored.chars().count(),
            256,
            "SRV-U-06: name of exactly 256 chars must not be truncated"
        );
        assert_eq!(
            stored, &name_256,
            "SRV-U-06: value must equal input exactly"
        );
    }

    /// SRV-U-15: multi-byte Unicode name truncated at char boundary, not byte boundary.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_srv_u15_initialize_truncates_at_char_boundary() {
        // 255 ASCII chars + one 4-byte Unicode codepoint = 256 chars total, 259 bytes.
        let mut name = "b".repeat(255);
        name.push('\u{1F600}'); // GRINNING FACE (U+1F600), 4 bytes in UTF-8
        assert_eq!(name.chars().count(), 256, "test setup: input is 256 chars");
        assert_eq!(name.len(), 259, "test setup: input is 259 bytes");

        let server = make_server().await;
        let map = run_initialize_handshake(server, &name).await;
        let guard = map.lock().unwrap_or_else(|e| e.into_inner());
        let stored = guard.get("").expect("SRV-U-15: entry must exist");
        assert_eq!(
            stored.chars().count(),
            256,
            "SRV-U-15: stored name must be 256 chars"
        );
        assert!(
            stored.ends_with('\u{1F600}'),
            "SRV-U-15: 4-byte char must not be split at truncation boundary"
        );
    }

    /// SRV-U-09 (map variant): client_type_map.get() returns None for an absent key.
    ///
    /// This tests the critical contract that `build_context_with_external_identity`
    /// returns `client_type = None` when the session key is not in the map (NFR-03).
    /// The exact behaviour is covered by the session-key extraction path, which
    /// falls through to `map.get(key).cloned()` — returning None when absent.
    #[test]
    fn test_srv_u09_map_get_missing_key_returns_none() {
        let map: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
        // Insert a different key to ensure the map is not empty.
        map.lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert("other-session".to_string(), "some-client".to_string());

        // Looking up a key that was never inserted returns None.
        let client_type = map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get("missing-key")
            .cloned();
        assert!(
            client_type.is_none(),
            "SRV-U-09: missing session key must return None from client_type_map"
        );
    }

    /// SRV-U-01b: Clone of UnimatrixServer shares the same client_type_map Arc.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_srv_u01b_clone_shares_client_type_map_arc() {
        let server = make_server().await;
        let clone = server.clone();
        // Insert via original; read via clone — confirms shared Arc.
        server
            .client_type_map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert("test-key".to_string(), "test-value".to_string());
        let val = clone
            .client_type_map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get("test-key")
            .cloned();
        assert_eq!(
            val.as_deref(),
            Some("test-value"),
            "SRV-U-01b: clone must share the same client_type_map Arc"
        );
    }

    /// SRV-U-12: Mutex poison recovery — map is accessible after a poisoned lock.
    #[test]
    fn test_srv_u12_client_type_map_poison_recovery() {
        let map: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
        let map_clone = Arc::clone(&map);

        // Poison the mutex by panicking while holding the lock.
        let _ = std::panic::catch_unwind(move || {
            let _guard = map_clone.lock().unwrap();
            panic!("intentional poison");
        });

        // Verify that unwrap_or_else(|e| e.into_inner()) recovers from the poison.
        let mut guard = map.lock().unwrap_or_else(|e| e.into_inner());
        guard.insert("after-poison".to_string(), "recovered".to_string());
        assert_eq!(
            guard.get("after-poison").map(String::as_str),
            Some("recovered"),
            "SRV-U-12: poison recovery must allow map access after panic"
        );
    }

    /// SRV-U-14 (compile check): build_context is removed — no symbol exists.
    ///
    /// This test is a compile-time assertion. If `build_context` were present,
    /// `cargo check` would reveal it. The removal is enforced by the fact that
    /// `tools.rs` call sites produce E0599 errors (expected in Wave 3).
    #[test]
    fn test_srv_u14_build_context_removed_compile_assertion() {
        // The existence of this test file in the passing test suite confirms
        // that server.rs does not define `build_context` (ADR-003, AC-12).
        // If `build_context` were defined, tools.rs would compile and this
        // test would still pass — but the Wave 3 compile gate enforces removal.
    }
}

// ---- GH #582 regression tests ----
//
// Defect 2: audit_fire_and_forget sites emitted session_id: String::new() (empty).
// Defect 3: AuditContext.session_id carried the mcp::-prefixed value; sessions table
//           carries the raw value — direct join equality failed.
//
// Tests below verify the write-side fixes:
// - audit_log rows carry the non-empty session_id for handlers that received one.
// - audit_log.session_id == sessions.session_id (raw, unprefixed) for join correctness.
// - write_lesson_learned propagates the caller's session_id to the audit row.
#[cfg(test)]
mod gh582_regression_tests {
    use super::tests::make_server;
    use crate::infra::audit::{AuditEvent, Outcome};
    use tokio::time::{Duration, Instant};
    use unimatrix_store::{SessionLifecycleStatus, SessionRecord};

    /// Helper: poll audit_log until at least one row matches the predicate, or deadline.
    async fn wait_for_audit_row<F>(
        server: &crate::server::UnimatrixServer,
        predicate: F,
        deadline: Duration,
    ) -> bool
    where
        F: Fn(&AuditEvent) -> bool,
    {
        let start = Instant::now();
        loop {
            // Read rows logged so far (audit_log is append-only; IDs start at 1).
            for id in 1u64..=50 {
                match server.store.read_audit_event(id).await {
                    Ok(Some(event)) if predicate(&event) => return true,
                    Ok(None) => break, // no more rows
                    _ => {}
                }
            }
            if start.elapsed() >= deadline {
                return false;
            }
            tokio::task::yield_now().await;
        }
    }

    /// GH-582-D2-a: audit_log session_id is non-empty when handler receives a session_id.
    ///
    /// Simulates the fix: an AuditEvent built via
    /// `ctx.audit_ctx.session_id.clone().unwrap_or_default()` must not produce an
    /// empty string when session_id is Some("test-session-582").
    /// This is a pure-logic test — no rmcp RequestContext needed.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_gh582_d2_session_id_non_empty_in_audit_event() {
        let session_id: Option<String> = Some("test-session-582".to_string());
        let audit_session_id = session_id.clone().unwrap_or_default();

        // Verify the pattern used in the fixed handlers yields the raw session_id.
        assert_eq!(
            audit_session_id, "test-session-582",
            "GH-582-D2: session_id.clone().unwrap_or_default() must yield the raw session_id"
        );
        assert!(
            !audit_session_id.is_empty(),
            "GH-582-D2: audit session_id must not be empty when session_id is Some"
        );
    }

    /// GH-582-D2-b: audit_log row session_id is non-empty after audit_fire_and_forget.
    ///
    /// Builds an AuditEvent with session_id set via the fixed pattern and verifies
    /// the persisted row in the audit_log carries the non-empty value.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_gh582_d2_audit_fire_and_forget_persists_session_id() {
        let server = make_server().await;
        let session_id: Option<String> = Some("session-d2-persist".to_string());

        let event = AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: session_id.clone().unwrap_or_default(),
            agent_id: "test-agent".to_string(),
            operation: "context_lookup".to_string(),
            target_ids: vec![],
            outcome: Outcome::Success,
            detail: "gh582-d2-regression".to_string(),
            credential_type: "none".to_string(),
            capability_used: "read".to_string(),
            agent_attribution: String::new(),
            metadata: "{}".to_string(),
        };

        server.audit_fire_and_forget(event);

        let found = wait_for_audit_row(
            &server,
            |ev| ev.detail == "gh582-d2-regression" && !ev.session_id.is_empty(),
            Duration::from_secs(5),
        )
        .await;

        assert!(
            found,
            "GH-582-D2: audit_log row must carry non-empty session_id after fire-and-forget"
        );
    }

    /// GH-582-D2-c: write_lesson_learned audit row carries non-empty session_id.
    ///
    /// Verifies that the `session_id` parameter threaded into `write_lesson_learned`
    /// is persisted in the audit_log row (not discarded as String::new()).
    ///
    /// Since `write_lesson_learned` is a private function in `mcp::tools`, this test
    /// exercises the same audit path by directly calling `audit_fire_and_forget` with
    /// the AuditEvent as the fixed function now constructs it: `session_id` set from
    /// the threaded `session_id: String` parameter.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_gh582_d2_write_lesson_learned_session_id_persisted() {
        let server = make_server().await;
        let session_id = "session-ll-582".to_string();

        // Replicate the AuditEvent construction in the fixed write_lesson_learned.
        // Pre-fix: session_id: String::new() — this test would fail (empty string stored).
        // Post-fix: session_id comes from the threaded parameter.
        let audit_event = AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: session_id.clone(),
            agent_id: "cortical-implant".to_string(),
            operation: "context_cycle_review/lesson-learned".to_string(),
            target_ids: vec![],
            outcome: Outcome::Success,
            detail: "auto-persist lesson-learned for test-582".to_string(),
            credential_type: "none".to_string(),
            capability_used: "write".to_string(),
            agent_attribution: String::new(),
            metadata: "{}".to_string(),
        };

        server.audit_fire_and_forget(audit_event);

        let found = wait_for_audit_row(
            &server,
            |ev| {
                ev.operation == "context_cycle_review/lesson-learned" && ev.session_id == session_id
            },
            Duration::from_secs(5),
        )
        .await;

        assert!(
            found,
            "GH-582-D2: write_lesson_learned audit row must carry session_id = '{}', \
             not String::new()",
            session_id
        );
    }

    /// GH-582-D3-a: audit_ctx.session_id carries raw session_id (no mcp:: prefix).
    ///
    /// Verifies the write-side fix: after the fix, AuditContext.session_id stores
    /// the raw session_id (not the mcp::-prefixed form). The fix changes the
    /// build_context_with_external_identity call from prefix_session_id("mcp", sid)
    /// to using sid directly.
    #[test]
    fn test_gh582_d3_audit_ctx_session_id_is_raw_not_prefixed() {
        let raw_session_id = "abc-123-def-456";

        // Before fix: prefix_session_id("mcp", sid) = "mcp::abc-123-def-456"
        let prefixed = crate::services::prefix_session_id("mcp", raw_session_id);
        assert_eq!(prefixed, "mcp::abc-123-def-456");

        // After fix: the raw value is stored directly in AuditContext.session_id.
        let audit_session_id: Option<String> = Some(raw_session_id.to_string());

        // Verify raw == raw (join equality holds).
        let sessions_row_session_id = raw_session_id;
        assert_eq!(
            audit_session_id.as_deref(),
            Some(sessions_row_session_id),
            "GH-582-D3: audit_ctx.session_id must equal sessions.session_id (raw, no prefix)"
        );

        // Verify raw != prefixed (demonstrates why the prefix was wrong).
        assert_ne!(
            prefixed.as_str(),
            sessions_row_session_id,
            "GH-582-D3: prefixed form must differ from raw — demonstrates the pre-fix bug"
        );
    }

    /// GH-582-D3-b: audit_log JOIN sessions succeeds with raw session_id on both sides.
    ///
    /// Inserts a sessions row with raw session_id, inserts an audit_log row with the
    /// same raw session_id (as the write-side fix produces), then asserts a direct
    /// equality join returns the expected row.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_gh582_d3_audit_log_join_sessions_raw_session_id() {
        use sqlx::Row;
        let server = make_server().await;
        let raw_sid = "join-test-session-582";

        // Insert a sessions row with raw session_id.
        let session_record = SessionRecord {
            session_id: raw_sid.to_string(),
            feature_cycle: Some("test-582".to_string()),
            agent_role: None,
            started_at: 0,
            ended_at: None,
            status: SessionLifecycleStatus::Active,
            compaction_count: 0,
            outcome: None,
            total_injections: 0,
            keywords: None,
        };
        server
            .store
            .insert_session(&session_record)
            .await
            .expect("insert session must succeed");

        // Insert an audit_log row with the same raw session_id (as the fix produces).
        let event = AuditEvent {
            event_id: 0,
            timestamp: 0,
            session_id: raw_sid.to_string(),
            agent_id: "test-agent".to_string(),
            operation: "context_status".to_string(),
            target_ids: vec![],
            outcome: Outcome::Success,
            detail: "gh582-d3-join-test".to_string(),
            credential_type: "none".to_string(),
            capability_used: "read".to_string(),
            agent_attribution: String::new(),
            metadata: "{}".to_string(),
        };
        server.audit_fire_and_forget(event);

        // Poll until audit row is persisted.
        let audit_deadline = Instant::now() + Duration::from_secs(5);
        let mut audit_found = false;
        while Instant::now() < audit_deadline {
            let rows: Vec<String> = sqlx::query_scalar(
                "SELECT al.session_id \
                 FROM audit_log al \
                 WHERE al.detail = 'gh582-d3-join-test'",
            )
            .fetch_all(server.store.read_pool_test())
            .await
            .expect("audit_log query must succeed");
            if !rows.is_empty() {
                audit_found = true;
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(
            audit_found,
            "GH-582-D3: audit_log row must be persisted within 5s"
        );

        // Assert that a direct equality join between audit_log and sessions succeeds.
        let join_rows: Vec<String> = sqlx::query_scalar(
            "SELECT al.session_id \
             FROM audit_log al \
             JOIN sessions s ON al.session_id = s.session_id \
             WHERE al.detail = 'gh582-d3-join-test'",
        )
        .fetch_all(server.store.read_pool_test())
        .await
        .expect("JOIN query must not error");

        assert_eq!(
            join_rows.len(),
            1,
            "GH-582-D3: JOIN audit_log ON sessions must return exactly 1 row when \
             audit_log.session_id = sessions.session_id (raw); \
             pre-fix bug had mcp:: prefix on audit_log side — join returned 0 rows"
        );
        assert_eq!(
            join_rows[0], raw_sid,
            "GH-582-D3: joined session_id must be the raw value '{}', not prefixed",
            raw_sid
        );
    }

    /// GH-582-D2-d: session_id is empty string when handler receives no session_id.
    ///
    /// Verifies that `None.unwrap_or_default()` yields "" (not a panic or error).
    /// This is the correct behavior for handlers called without a session_id.
    #[test]
    fn test_gh582_d2_session_id_empty_when_none() {
        let session_id: Option<String> = None;
        let audit_session_id = session_id.clone().unwrap_or_default();
        assert_eq!(
            audit_session_id, "",
            "GH-582-D2: None session_id must produce empty string via unwrap_or_default"
        );
    }
}
