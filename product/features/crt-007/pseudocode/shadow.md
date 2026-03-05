# Pseudocode: shadow (Wave 5)

## shadow.rs — ShadowEvaluator

```rust
use std::sync::Arc;
use unimatrix_store::Store;
use crate::digest::SignalDigest;

/// Shadow mode evaluation logger (ADR-004).
///
/// Persists side-by-side rule vs neural predictions to the
/// shadow_evaluations SQLite table for accuracy analysis.
pub struct ShadowEvaluator {
    store: Arc<Store>,
}

impl ShadowEvaluator {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    /// Log a shadow evaluation record.
    pub fn evaluate(
        &self,
        model_name: &str,
        model_version: u32,
        digest: &SignalDigest,
        rule_prediction: &str,
        neural_prediction: &str,
        neural_confidence: f32,
        feature_cycle: Option<&str>,
    ) -> Result<(), String> {
        // 1. Serialize digest.features to bytes (128 bytes = 32 * f32)
        let digest_bytes = digest.to_bytes();

        // 2. Get current timestamp millis
        let ts_millis = now_millis();

        // 3. INSERT INTO shadow_evaluations
        let conn = self.store.lock_conn();
        conn.execute(
            "INSERT INTO shadow_evaluations
             (model_name, model_version, ts_millis, signal_digest,
              rule_prediction, neural_prediction, neural_confidence,
              feature_cycle)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![model_name, model_version as i64, ts_millis,
                    &digest_bytes, rule_prediction, neural_prediction,
                    neural_confidence as f64, feature_cycle],
        ).map_err(|e| format!("shadow eval insert failed: {e}"))?;

        Ok(())
    }

    /// Compute overall accuracy for a model version.
    pub fn accuracy(
        &self,
        model_name: &str,
        model_version: u32,
    ) -> Result<f64, String> {
        // SELECT COUNT(*) as total,
        //        SUM(CASE WHEN rule_prediction = neural_prediction THEN 1 ELSE 0 END) as correct
        // FROM shadow_evaluations
        // WHERE model_name = ? AND model_version = ?
        //
        // Return correct / total, or 0.0 if total == 0
    }

    /// Count evaluations for a model version.
    pub fn evaluation_count(
        &self,
        model_name: &str,
        model_version: u32,
    ) -> Result<u64, String> {
        // SELECT COUNT(*) FROM shadow_evaluations
        // WHERE model_name = ? AND model_version = ?
    }

    /// Per-class accuracy breakdown.
    pub fn per_class_accuracy(
        &self,
        model_name: &str,
        model_version: u32,
    ) -> Result<HashMap<String, f64>, String> {
        // SELECT rule_prediction,
        //        COUNT(*) as total,
        //        SUM(CASE WHEN rule_prediction = neural_prediction THEN 1 ELSE 0 END) as correct
        // FROM shadow_evaluations
        // WHERE model_name = ? AND model_version = ?
        // GROUP BY rule_prediction
        //
        // Return HashMap: class_name -> correct/total
    }

    /// Divergence rate: fraction where rule != neural.
    pub fn divergence_rate(
        &self,
        model_name: &str,
        model_version: u32,
    ) -> Result<f64, String> {
        // 1.0 - accuracy
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
```

## Schema Migration: v7 -> v8

In `crates/unimatrix-store/src/migration.rs`:

```rust
// Update CURRENT_SCHEMA_VERSION from 7 to 8

// Add migration block after v6->v7:
if current_version < 8 {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS shadow_evaluations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            model_name TEXT NOT NULL,
            model_version INTEGER NOT NULL,
            ts_millis INTEGER NOT NULL,
            signal_digest BLOB NOT NULL,
            rule_prediction TEXT NOT NULL,
            neural_prediction TEXT NOT NULL,
            neural_confidence REAL NOT NULL,
            ground_truth TEXT,
            feature_cycle TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_shadow_model
            ON shadow_evaluations(model_name, model_version);
        CREATE INDEX IF NOT EXISTS idx_shadow_ts
            ON shadow_evaluations(ts_millis);"
    ).map_err(StoreError::Sqlite)?;
}
```

In `crates/unimatrix-store/src/db.rs` `create_tables()`:

```rust
// Add CREATE TABLE IF NOT EXISTS shadow_evaluations (same DDL)
// Add CREATE INDEX IF NOT EXISTS (same indexes)
```

Following the pattern from Unimatrix procedure #390.
