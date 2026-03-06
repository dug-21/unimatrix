# Pseudocode: integration (Server + Confidence + Schema)

## Pattern: Minimal pipeline wiring with graceful degradation

Wire NeuralEnhancer into extraction_tick, add "neural" trust_source,
add shadow_evaluations table.

## Files

### crates/unimatrix-engine/src/confidence.rs

Change in `trust_score()`:

```pseudo
pub fn trust_score(trust_source: &str) -> f64 {
    match trust_source {
        "human" => 1.0,
        "system" => 0.7,
        "agent" => 0.5,
        "neural" => 0.40,  // NEW: crt-007 neural extraction (AC-15)
        "auto" => 0.35,
        _ => 0.3,
    }
}
```

### crates/unimatrix-server/src/background.rs

Modifications to `extraction_tick()`:

```pseudo
async fn extraction_tick(
    store: &Arc<Store>,
    vector_index: &Arc<VectorIndex>,
    embed_service: &Arc<EmbedServiceHandle>,
    ctx: &mut ExtractionContext,
    neural_enhancer: Option<&NeuralEnhancer>,      // NEW param
    shadow_evaluator: Option<&mut ShadowEvaluator>, // NEW param
) -> Result<ExtractionStats, ServiceError> {
    // ... existing observation query + rule execution ...

    // 3. Quality gate checks 1-4 (unchanged)
    let mut accepted: Vec<ProposedEntry> = Vec::new();
    for proposal in proposals { ... }

    // 3.5 [NEW] Neural enhancement (between rules and embedding checks)
    if let (Some(enhancer), Some(evaluator)) = (neural_enhancer, shadow_evaluator.as_deref_mut()) {
        let mut neural_accepted = Vec::new();
        for entry in accepted {
            let prediction = enhancer.enhance(&entry);

            match enhancer.mode() {
                EnhancerMode::Shadow => {
                    // Log prediction, pass entry unchanged
                    evaluator.log_prediction(&entry, &prediction, true);
                    neural_accepted.push(entry);
                }
                EnhancerMode::Active => {
                    // Suppress if Noise with high confidence
                    if prediction.classification.category == SignalCategory::Noise
                        && prediction.classification.confidence > 0.8
                    {
                        evaluator.log_prediction(&entry, &prediction, false);
                        ctx.stats.entries_rejected_total += 1;
                        continue;
                    }
                    evaluator.log_prediction(&entry, &prediction, true);
                    neural_accepted.push(entry);
                }
            }
        }
        accepted = neural_accepted;

        // Persist shadow evaluations to SQLite (batch)
        let logs = evaluator.drain_evaluations();
        if !logs.is_empty() {
            let store_for_shadow = Arc::clone(store);
            let _ = tokio::task::spawn_blocking(move || {
                persist_shadow_evaluations(&store_for_shadow, &logs)
            }).await;
        }
    }

    // 4. Quality gate checks 5-6 (unchanged)
    // ... existing near-duplicate + contradiction checks ...

    // 5. Store accepted entries
    // Changed: trust_source = "neural" if neural enhancer is in Active mode
    for entry in final_accepted {
        let trust_source = match neural_enhancer {
            Some(e) if e.mode() == EnhancerMode::Active => "neural",
            _ => "auto",
        };
        let new_entry = NewEntry {
            // ... existing fields ...
            trust_source: trust_source.to_string(),
        };
        // ... existing store logic ...
    }

    // ... watermark update unchanged ...
}
```

### Shadow evaluation persistence

```pseudo
fn persist_shadow_evaluations(store: &Store, logs: &[ShadowLogEntry]) {
    let conn = store.lock_conn();
    let mut stmt = conn.prepare_cached(
        "INSERT INTO shadow_evaluations
         (timestamp, rule_name, rule_category, neural_category,
          neural_confidence, convention_score, rule_accepted, digest)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
    ).ok();
    if let Some(ref mut stmt) = stmt {
        for log in logs {
            let _ = stmt.execute(rusqlite::params![
                log.timestamp as i64,
                log.rule_name,
                log.rule_category,
                log.neural_category,
                log.neural_confidence as f64,
                log.convention_score as f64,
                log.rule_accepted as i32,
                log.digest_bytes,
            ]);
        }
    }
}
```

### Schema migration: shadow_evaluations table

In `unimatrix-store/src/schema.rs` (or the SQLite migration path):

```pseudo
// Add to schema migration for schema version N+1:
CREATE TABLE IF NOT EXISTS shadow_evaluations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    rule_name TEXT NOT NULL,
    rule_category TEXT NOT NULL,
    neural_category TEXT NOT NULL,
    neural_confidence REAL NOT NULL,
    convention_score REAL NOT NULL,
    rule_accepted INTEGER NOT NULL,
    digest BLOB
);
```

### Server startup: NeuralEnhancer initialization

```pseudo
// In server.rs or background.rs initialization:
fn init_neural_enhancer(models_dir: &Path) -> Option<(NeuralEnhancer, ShadowEvaluator)> {
    let registry = ModelRegistry::new(models_dir.to_path_buf());

    // Try to load production models, fall back to baseline
    let classifier = match registry.load_model("signal-classifier", ModelSlot::Production) {
        Ok(Some(data)) => match SignalClassifier::deserialize(&data) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("classifier deserialize failed: {e}, using baseline");
                SignalClassifier::new_with_baseline()
            }
        },
        _ => {
            tracing::info!("no saved classifier, using baseline");
            SignalClassifier::new_with_baseline()
        }
    };

    let scorer = match registry.load_model("convention-scorer", ModelSlot::Production) {
        Ok(Some(data)) => match ConventionScorer::deserialize(&data) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("scorer deserialize failed: {e}, using baseline");
                ConventionScorer::new_with_baseline()
            }
        },
        _ => {
            tracing::info!("no saved scorer, using baseline");
            ConventionScorer::new_with_baseline()
        }
    };

    // Start in Shadow mode always
    let enhancer = NeuralEnhancer::new(classifier, scorer, EnhancerMode::Shadow);
    let evaluator = ShadowEvaluator::new(20, 0.05, 50);

    Some((enhancer, evaluator))
}
```

### Modifications to spawn_background_tick / background_tick_loop

```pseudo
// spawn_background_tick gains optional NeuralEnhancer + ShadowEvaluator params.
// If neural enhancement fails to init, None is passed and extraction_tick
// operates without neural enhancement (rule-only).

pub fn spawn_background_tick(
    store: ...,
    // ... existing params ...
    neural_enhancer: Option<NeuralEnhancer>,  // NEW
) -> JoinHandle<()> {
    // neural_enhancer and evaluator wrapped in Arc<Mutex<>> or owned by the loop
}
```

## Key Design Decisions

- Neural enhancement is optional (None = rule-only, zero behavior change)
- NeuralEnhancer inserted BETWEEN quality gate checks 1-4 and checks 5-6
- Shadow evaluations persisted via batch INSERT (R-08 mitigation)
- Server starts in Shadow mode always -- promotion is future (crt-008)
- trust_source "neural" only in Active mode -- Shadow mode uses "auto"
- Schema migration adds shadow_evaluations table without touching existing tables
- Graceful fallback: deserialize failure -> baseline weights, not crash
