//! Bounded, server-only held-buffer store (crt-052 Wave B, Option B — ADR-008).
//!
//! The per-turn drain (#4799) frees a session's transcript buffer at every
//! `Stop`→`SessionClose`, so a multi-turn session's buffer is empty at cycle
//! review and the primary distillation path is starved to the 0.81 reconstruction
//! fallback. This store is the continuity remedy: drained buffers are HELD here
//! instead of dropped, keep merging deltas across drains, re-adopt on
//! re-registration (loud on `feature_cycle` match, FAIL LOUD on mismatch), and
//! purge only at cycle review (post-distill), stale sweep, or cap-eviction.
//!
//! # Bounded memory (R-02 / SR-01)
//!
//! Two INDEPENDENT reclamation mechanisms, both mandatory — either alone bounds
//! memory:
//! - **Held-count cap** (`max_sessions`): on every insert, evict oldest
//!   `last_activity_at` first until the count is within the cap. Every eviction
//!   emits the purge audit (R-16) — cap-hit loss is NEVER silent.
//! - **Independent stale-sweep TTL**: `sweep_expired(ttl)` reclaims buffers whose
//!   `last_activity_at` is older than the TTL, regardless of whether cycle review
//!   ever fires. Memory is bounded by `buffer_cap × max_sessions` always.
//!
//! # Loud re-adoption (R-01 / SR-02, cite #981)
//!
//! `readopt` rebinds a held buffer ONLY when the re-registering `feature_cycle`
//! matches the held cycle (vnc-030 ADR-007 §2 makes a declared cycle
//! un-flippable). On mismatch or empty cycle it FAILS LOUD: the held buffer is
//! dropped (treated as a fresh session), a metadata-only diagnostic is emitted,
//! and its terminal purge audit fires. A mis-set `feature_cycle` silently
//! mis-scoping candidates (the #981 failure mode) is impossible by construction.
//!
//! # Wave A/B boundary (R-11)
//!
//! This module DEPENDS ON `infra::session` (it impls that module's
//! [`HeldBufferScan`] trait); the reverse never holds. `session.rs` never
//! `use`s this module — it holds only an `Option<Arc<dyn HeldBufferScan>>`.
//! Reverting Wave B (this file + the thin drain/listener/server diffs) leaves
//! Wave A compiling and shipping degraded to the reconstruction fallback.
//!
//! # Content opacity (R-19 / AC-06)
//!
//! [`HeldBuffer`]'s `Debug` is hand-written and metadata-only — it carries the
//! `feature_cycle`, `last_activity_at`, and a byte COUNT, never a transcript
//! byte. The audit `detail` is content-free (`bytes=<n> trigger=<token>`).

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::infra::audit::AuditLog;
use crate::infra::session::HeldBufferScan;
use crate::infra::session_transcript::{TranscriptBuffer, TranscriptPurgeRecord, session_key};

/// Audit trigger tokens for `transcript_session_purged` (ADR-009). The event
/// SHAPE is unchanged from vnc-025; the per-turn `session_close` token simply
/// goes away for held buffers — they purge at one of these terminal points.
pub const TRIGGER_CYCLE_REVIEW: &str = "cycle_review";
pub const TRIGGER_STALE_SWEEP: &str = "stale_sweep";
pub const TRIGGER_CAP_EVICT: &str = "cap_evict";
pub const TRIGGER_READOPT_MISMATCH: &str = "readopt_mismatch";

/// Sink for the terminal purge audit a held buffer emits when it is evicted on
/// cap-hit or dropped on a loud re-adopt mismatch — the two paths that purge
/// OUTSIDE the `Vec<TranscriptPurgeRecord>`-returning methods (sweep/review),
/// where the caller emits. Production backs this with the same fire-and-forget
/// `emit_purge_audits` path the registered-buffer purge uses; tests back it with
/// a synchronous capture so eviction is observable without a tokio runtime
/// (R-16: cap-hit / mismatch loss is never silent).
pub trait PurgeAuditSink: Send + Sync {
    /// Emit one `transcript_session_purged` row per record, content-free, under
    /// the given trigger token. Fire-and-forget — purge never depends on it.
    fn emit(&self, records: Vec<TranscriptPurgeRecord>, trigger: &'static str);
}

/// Production [`PurgeAuditSink`] backed by the server's [`AuditLog`]. Routes
/// through the SAME fire-and-forget `emit_purge_audits` path the
/// registered-buffer purge uses, so the audit SHAPE is identical (ADR-009): the
/// only thing that moves is the trigger token and the cadence.
pub struct AuditLogPurgeSink {
    audit: Arc<AuditLog>,
}

impl AuditLogPurgeSink {
    pub fn new(audit: Arc<AuditLog>) -> Self {
        AuditLogPurgeSink { audit }
    }
}

impl fmt::Debug for AuditLogPurgeSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuditLogPurgeSink").finish_non_exhaustive()
    }
}

impl PurgeAuditSink for AuditLogPurgeSink {
    fn emit(&self, records: Vec<TranscriptPurgeRecord>, trigger: &'static str) {
        crate::uds::listener::emit_purge_audits(&self.audit, records, trigger);
    }
}

/// A single held buffer plus the metadata the state machine keys on.
///
/// `Debug` is hand-written and metadata-only (R-19 / AC-06): it MUST NOT carry
/// a transcript byte. `derive(Debug)` would leak the buffer behind the `Arc` and
/// is forbidden by the content-leak gate.
struct HeldBuffer {
    /// The live buffer handle, handed off from the registry at drain. Deltas
    /// keep merging into it while held (listener routes to it via the registry).
    arc: Arc<Mutex<TranscriptBuffer>>,
    /// The contract-attributed cycle this buffer is bound to (SR-02). Re-adoption
    /// rebinds ONLY on an exact match; the seam/purge scan keys on it.
    feature_cycle: String,
    /// Basis for TTL expiry and oldest-first cap-eviction ordering; bumped on
    /// every delta merge while held.
    last_activity_at: Instant,
}

impl fmt::Debug for HeldBuffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Metadata-only (R-19): a byte COUNT under the buffer lock, never bytes.
        // Poison-recover the count read so Debug never panics.
        let bytes = match self.arc.lock() {
            Ok(buf) => buf.len(),
            Err(p) => p.into_inner().len(),
        };
        f.debug_struct("HeldBuffer")
            .field("feature_cycle", &self.feature_cycle)
            .field("last_activity_at", &self.last_activity_at)
            .field("bytes", &bytes)
            .finish()
    }
}

/// Injectable clock so TTL/eviction tests are deterministic (no wall-clock, no
/// sleeps). Production uses [`SystemClock`] (`Instant::now()`).
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

/// Real monotonic clock.
#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Bounded server-only held-buffer store (Option B).
///
/// Owned by the server alongside the `SessionRegistry`; the SAME `Arc` is
/// injected into the registry as a `dyn HeldBufferScan` so the snapshot seam
/// (C1) scans held buffers without importing this module (R-11).
pub struct TranscriptHold {
    /// `session_id -> HeldBuffer`. O(1) keyed lookup on the hot delta path
    /// (R-17 — no linear scan).
    held: Mutex<HashMap<String, HeldBuffer>>,
    /// Held-count ceiling (`cfg.transcript_hold_max_sessions`, C9).
    max_sessions: usize,
    /// Audit sink for cap-eviction and readopt-mismatch terminal purges (R-16).
    audit: Arc<dyn PurgeAuditSink>,
    /// Injectable clock (deterministic in tests).
    clock: Arc<dyn Clock>,
}

impl fmt::Debug for TranscriptHold {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let held = self.held.lock().unwrap_or_else(|p| p.into_inner());
        f.debug_struct("TranscriptHold")
            .field("held_count", &held.len())
            .field("max_sessions", &self.max_sessions)
            .finish()
    }
}

impl TranscriptHold {
    /// Construct with the held-count cap and a production audit sink + system
    /// clock.
    pub fn new(max_sessions: usize, audit: Arc<dyn PurgeAuditSink>) -> Self {
        Self::with_clock(max_sessions, audit, Arc::new(SystemClock))
    }

    /// Construct with an explicit clock (tests inject a controllable clock).
    pub fn with_clock(
        max_sessions: usize,
        audit: Arc<dyn PurgeAuditSink>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        TranscriptHold {
            held: Mutex::new(HashMap::new()),
            // A zero cap would evict everything immediately and defeat the
            // remedy; clamp to at least 1 (config validate() enforces >= 1, this
            // is defense in depth).
            max_sessions: max_sessions.max(1),
            audit,
            clock,
        }
    }

    /// Poison-recover the hold lock (#4764): a panic mid-mutation leaves the map
    /// the only recoverable state — `into_inner` + `clear_poison` so recovery
    /// happens once, never re-panics.
    fn lock_held(&self) -> std::sync::MutexGuard<'_, HashMap<String, HeldBuffer>> {
        match self.held.lock() {
            Ok(g) => g,
            Err(poisoned) => {
                let g = poisoned.into_inner();
                self.held.clear_poison();
                g
            }
        }
    }

    /// Hold a drained buffer (called from `drain_and_signal_session` via the
    /// trait — 3-arg form, ADR-008 / ARCH §4). `feature_cycle` is REQUIRED for
    /// SR-02 loud re-adoption.
    ///
    /// A buffer with NO attributed cycle can never be loud-re-adopted safely
    /// (#981), so it is NOT held — the existing drain frees it and the session
    /// degrades to the Wave A fallback. On every insert the cap is enforced
    /// (oldest-first eviction, each evicted buffer audited — R-16).
    fn hold_on_drain_inner(
        &self,
        session_id: &str,
        arc: Arc<Mutex<TranscriptBuffer>>,
        feature_cycle: &str,
    ) {
        if feature_cycle.is_empty() {
            // No attributed cycle — do not hold (let drain free it). Wave A
            // fallback covers this session.
            return;
        }
        let now = self.clock.now();
        let evicted = {
            let mut guard = self.lock_held();
            guard.insert(
                session_id.to_string(),
                HeldBuffer {
                    arc,
                    feature_cycle: feature_cycle.to_string(),
                    last_activity_at: now,
                },
            );
            self.enforce_cap(&mut guard)
        }; // hold lock RELEASED before audit emission
        if !evicted.is_empty() {
            // R-16: cap-hit eviction is never silent.
            self.audit.emit(evicted, TRIGGER_CAP_EVICT);
        }
    }

    /// Re-adopt a held buffer on `SessionRegister` (2-arg form, ADR-008 / ARCH
    /// §4). Rebinds ONLY on `feature_cycle` MATCH (R-01 / SR-02). On mismatch or
    /// empty cycle it FAILS LOUD: the held buffer is DROPPED (treated as fresh),
    /// a metadata-only diagnostic is logged, and its terminal purge audit fires
    /// (`trigger=readopt_mismatch`). No content is ever re-adopted under a wrong
    /// cycle (the #981 failure mode is impossible).
    fn readopt_inner(
        &self,
        session_id: &str,
        registering_feature_cycle: &str,
    ) -> Option<Arc<Mutex<TranscriptBuffer>>> {
        let (arc, mismatch_record) = {
            let mut guard = self.lock_held();
            // Determine match/mismatch under a scoped read, then remove — avoids
            // holding an immutable borrow across the mutating `remove`.
            let cycle_matches = match guard.get(session_id) {
                None => return None,
                Some(held) => {
                    !registering_feature_cycle.is_empty()
                        && held.feature_cycle == registering_feature_cycle
                }
            };
            if cycle_matches {
                // MATCH — hand the live buffer back to the registry.
                let held = guard.remove(session_id).expect("present: just matched");
                (Some(held.arc), None)
            } else {
                // MISMATCH or empty cycle — FAIL LOUD (R-01, cite #981).
                let held = guard.remove(session_id).expect("present: just fetched");
                let held_cycle = held.feature_cycle.clone();
                let record = purge_record(session_id, &held);
                // Metadata-only diagnostic (R-04): session_id, both cycles, byte
                // count — NEVER transcript content.
                tracing::warn!(
                    session_id,
                    held_feature_cycle = %held_cycle,
                    registering_feature_cycle = %registering_feature_cycle,
                    bytes = record.as_ref().map(|r| r.bytes_purged).unwrap_or(0),
                    "crt-052: held buffer re-adopt mismatch — dropped (fail loud, #981); \
                     no content re-adopted under the wrong cycle"
                );
                (None, record)
            }
        }; // hold lock RELEASED before audit emission
        if let Some(rec) = mismatch_record {
            // Exactly-once terminal purge audit for the dropped held buffer.
            self.audit.emit(vec![rec], TRIGGER_READOPT_MISMATCH);
        }
        arc
    }

    /// Reclaim held buffers whose `last_activity_at` is older than `ttl`
    /// (independent stale-sweep — R-02). Reclamation does NOT depend on cycle
    /// review ever firing (SR-01). Returns the purge records; the caller emits
    /// them with `trigger=stale_sweep` (exactly-once).
    pub fn sweep_expired(&self, ttl: Duration) -> Vec<TranscriptPurgeRecord> {
        let now = self.clock.now();
        let mut guard = self.lock_held();
        let expired: Vec<String> = guard
            .iter()
            .filter(|(_, h)| now.saturating_duration_since(h.last_activity_at) >= ttl)
            .map(|(sid, _)| sid.clone())
            .collect();
        let records: Vec<TranscriptPurgeRecord> = expired
            .into_iter()
            .filter_map(|sid| {
                guard
                    .remove(&sid)
                    .and_then(|held| purge_record(&sid, &held))
            })
            .collect();
        records
    }

    /// Purge held buffers for a reviewed cycle, post-distill (R-03 / R-13).
    /// Returns the purge records; the caller emits with `trigger=cycle_review`
    /// (exactly-once). Called from `purge_cycle_transcripts` (C7) alongside the
    /// registered-buffer `clear_transcripts_for_feature`.
    pub fn purge_held_for_feature(&self, feature_cycle: &str) -> Vec<TranscriptPurgeRecord> {
        let mut guard = self.lock_held();
        let matching: Vec<String> = guard
            .iter()
            .filter(|(_, h)| h.feature_cycle == feature_cycle)
            .map(|(sid, _)| sid.clone())
            .collect();
        let records: Vec<TranscriptPurgeRecord> = matching
            .into_iter()
            .filter_map(|sid| {
                guard
                    .remove(&sid)
                    .and_then(|held| purge_record(&sid, &held))
            })
            .collect();
        records
    }

    /// Cap-hit eviction (R-02 / R-16): evict oldest `last_activity_at` first
    /// until the count is within `max_sessions`. Returns evicted records so the
    /// caller emits the audit AFTER releasing the hold lock (eviction never
    /// silent). Buffer is purged (`clear()`) so its bytes are reclaimed.
    fn enforce_cap(&self, guard: &mut HashMap<String, HeldBuffer>) -> Vec<TranscriptPurgeRecord> {
        let mut records = Vec::new();
        while guard.len() > self.max_sessions {
            // Oldest-last-activity-first victim.
            let victim = guard
                .iter()
                .min_by_key(|(_, h)| h.last_activity_at)
                .map(|(sid, _)| sid.clone());
            let Some(sid) = victim else { break };
            if let Some(rec) = guard
                .remove(&sid)
                .and_then(|held| purge_record(&sid, &held))
            {
                records.push(rec);
            }
        }
        records
    }

    /// Test-only: current held count.
    #[cfg(any(test, feature = "test-support"))]
    pub fn held_count(&self) -> usize {
        self.lock_held().len()
    }

    /// Test-only: is a session currently held?
    #[cfg(any(test, feature = "test-support"))]
    pub fn is_held(&self, session_id: &str) -> bool {
        self.lock_held().contains_key(session_id)
    }
}

/// Snapshot-and-clear a held buffer into a counts-only purge record (ADR-004
/// shape). `clear()` (not `len()`) so a racing second purge point sees 0 —
/// exactly-once non-zero audit per buffer content. `None` for an empty buffer
/// (zero-byte purges emit nothing). Poison-recovers (#4764).
fn purge_record(session_id: &str, held: &HeldBuffer) -> Option<TranscriptPurgeRecord> {
    let purged = match held.arc.lock() {
        Ok(mut buf) => buf.clear(),
        Err(poisoned) => {
            let purged = poisoned.into_inner().clear();
            held.arc.clear_poison();
            purged
        }
    };
    if purged == 0 {
        None
    } else {
        Some(TranscriptPurgeRecord {
            session_id: session_key("default", "", session_id),
            bytes_purged: purged,
        })
    }
}

/// Wave B wiring: the held store IS the registry's `HeldBufferScan` handle. The
/// trait is owned by Wave A (`infra::session`); this impl is the one-way
/// dependency (C8 → C1), so the seam scans held buffers, routes held deltas, and
/// drives drain-hold/re-adopt without `session.rs` importing this module (R-11).
impl HeldBufferScan for TranscriptHold {
    fn held_arcs_for_feature(
        &self,
        feature_cycle: &str,
    ) -> Vec<(String, Arc<Mutex<TranscriptBuffer>>)> {
        // Arc-clone only, under the hold lock; no buffer lock, no parse (lock
        // discipline — vnc-025 ADR-001).
        let guard = self.lock_held();
        guard
            .iter()
            .filter(|(_, h)| h.feature_cycle == feature_cycle)
            .map(|(sid, h)| (sid.clone(), Arc::clone(&h.arc)))
            .collect()
    }

    fn hold_on_drain(
        &self,
        session_id: &str,
        arc: Arc<Mutex<TranscriptBuffer>>,
        feature_cycle: &str,
    ) {
        self.hold_on_drain_inner(session_id, arc, feature_cycle);
    }

    fn readopt(
        &self,
        session_id: &str,
        registering_feature_cycle: &str,
    ) -> Option<Arc<Mutex<TranscriptBuffer>>> {
        self.readopt_inner(session_id, registering_feature_cycle)
    }

    fn held_arc_for_session(&self, session_id: &str) -> Option<Arc<Mutex<TranscriptBuffer>>> {
        // O(1) keyed lookup (R-17) for the listener delta route + activity bump.
        let mut guard = self.lock_held();
        let now = self.clock.now();
        guard.get_mut(session_id).map(|h| {
            h.last_activity_at = now;
            Arc::clone(&h.arc)
        })
    }

    fn sweep_expired(&self, ttl: Duration) -> Vec<TranscriptPurgeRecord> {
        // Delegate to the inherent method (same logic; the trait route is how
        // the maintenance tick reaches it without `session.rs` importing this
        // module — R-11).
        TranscriptHold::sweep_expired(self, ttl)
    }
}

#[cfg(test)]
#[path = "transcript_hold_tests.rs"]
mod tests;
