# Pseudocode: ground-truth-backfill (Wave 5)

## Purpose

Backfill ground_truth column in shadow_evaluations on category corrections and consistent multi-votes. Extends `service.rs`.

## Category Correction Backfill

```pseudo
impl TrainingService {
    fn backfill_ground_truth_correction(
        &self,
        entry_id: u64,
        new_category: &str,
        store: &Store,
    ) {
        // UPDATE shadow_evaluations
        // SET ground_truth = new_category
        // WHERE entry_id = entry_id
        //   AND ground_truth IS NULL
        //   AND model_name = 'signal_classifier'
        store.update_shadow_evaluation_ground_truth(entry_id, new_category)
            .unwrap_or_else(|e| {
                // Log error but don't block feedback processing (NFR-05)
                eprintln!("ground truth backfill failed: {e}");
            });
    }
}
```

## Consistent Multi-Vote Backfill

```pseudo
impl TrainingService {
    fn check_multi_vote_backfill(
        &self,
        entry_id: u64,
        store: &Store,
    ) {
        // Check vote counts for this entry
        if let Ok(Some(entry)) = store.get_entry(entry_id) {
            let unhelpful = entry.unhelpful_count;
            let helpful = entry.helpful_count;

            // 3+ unhelpful with 0 helpful -> backfill as noise
            if unhelpful >= 3 && helpful == 0 {
                store.update_shadow_evaluation_ground_truth(entry_id, "noise")
                    .unwrap_or_else(|e| {
                        eprintln!("multi-vote backfill failed: {e}");
                    });
            }
        }
    }
}
```

## Integration with record_feedback

```pseudo
fn record_feedback(&self, signal: FeedbackSignal) {
    // ... existing label generation and reservoir routing ...

    // Ground truth backfill checks (ADR-006)
    match &signal {
        FeedbackSignal::CategoryCorrection { entry_id, new_category, .. } => {
            self.backfill_ground_truth_correction(*entry_id, new_category, &self.store);
        }
        FeedbackSignal::UnhelpfulVote { entry_id, .. } |
        FeedbackSignal::HelpfulVote { entry_id, .. } => {
            self.check_multi_vote_backfill(*entry_id, &self.store);
        }
        _ => {}
    }
}
```

## Notes

- Backfill failure is logged but does not block feedback processing (NFR-05)
- Single votes do NOT trigger backfill -- only the multi-vote check
- The `shadow_evaluations` table already exists from crt-007
- Need store access from TrainingService -- pass as parameter or store Arc<Store>
