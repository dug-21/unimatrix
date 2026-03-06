# Test Plan: ground-truth-backfill (Wave 5)

## Tests

### T-FR10-01: Category correction backfills ground truth
- **Setup**: Insert shadow_evaluation row with ground_truth = None
- **Action**: Process CategoryCorrection signal
- **Assert**: ground_truth column updated to new_category

### T-FR10-02: Single vote does NOT backfill ground truth
- **Setup**: Insert shadow_evaluation row with ground_truth = None, entry with 1 unhelpful / 0 helpful
- **Action**: Process UnhelpfulVote signal
- **Assert**: ground_truth remains None (only 1 vote, need 3+)

## Notes

These tests require access to the shadow_evaluations table. Since crt-008 operates within unimatrix-learn which doesn't directly own the DB, these tests may need to be structured as:
1. Unit tests with mock store interface, or
2. Tests that exercise TrainingService with a real store (higher integration level)

For Wave 5, use approach 1: test the backfill logic functions in isolation with mock/stub store calls.
