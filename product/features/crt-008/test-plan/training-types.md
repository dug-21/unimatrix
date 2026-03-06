# Test Plan: training-types (Wave 1)

## Tests

### T-FR01-01: TrainingSample type construction
- **Location**: `training.rs::tests`
- **Setup**: Construct TrainingSample with all field types
- **Assert**: Clone works, fields accessible
- **Risk**: AC-01

### T-FR02-01: HelpfulVote generates positive classifier label
- **Setup**: `FeedbackSignal::HelpfulVote { category: "convention", ... }`
- **Action**: `LabelGenerator::generate(&signal)`
- **Assert**: Returns 1 label, model = "signal_classifier", target = [1,0,0,0,0], weight = 1.0

### T-FR02-02: UnhelpfulVote generates noise classifier label
- **Setup**: `FeedbackSignal::UnhelpfulVote { ... }`
- **Action**: `LabelGenerator::generate(&signal)`
- **Assert**: Returns 1 label, target = [0,0,0,0,1], weight = 1.0

### T-FR02-03: CategoryCorrection generates ground truth re-label
- **Setup**: `FeedbackSignal::CategoryCorrection { old: "noise", new: "convention", ... }`
- **Action**: `LabelGenerator::generate(&signal)`
- **Assert**: Returns 1 label, target = [1,0,0,0,0], weight = 1.0

### T-FR02-04: Deprecation generates dual model labels
- **Setup**: `FeedbackSignal::Deprecation { category: "convention", ... }`
- **Action**: `LabelGenerator::generate(&signal)`
- **Assert**: Returns 2 labels: ("signal_classifier", noise), ("convention_scorer", 0.0)

### T-FR02-05: FeatureOutcome success generates weak labels
- **Setup**: `FeedbackSignal::FeatureOutcome { result: Success, entry_ids: [1,2], categories: ["convention","pattern"], ... }`
- **Action**: `LabelGenerator::generate(&signal)`
- **Assert**: Returns 2 labels, weight = 0.3, targets match entry categories

### T-FR02-06: ConventionFollowed generates positive scorer label
- **Setup**: `FeedbackSignal::ConventionFollowed { ... }`
- **Assert**: model = "convention_scorer", target = 1.0, weight = 1.0

### T-FR02-07: ConventionDeviated generates negative scorer label
- **Setup**: `FeedbackSignal::ConventionDeviated { ... }`
- **Assert**: model = "convention_scorer", target = 0.0, weight = 1.0

### T-FR02-08: StaleEntry generates weak dead label
- **Setup**: `FeedbackSignal::StaleEntry { ... }`
- **Assert**: model = "signal_classifier", target = [0,0,0,1,0], weight = 0.3

### T-FR02-09: ContentCorrection generates noise label with 0.7 weight
- **Setup**: `FeedbackSignal::ContentCorrection { ... }`
- **Assert**: model = "signal_classifier", target = [0,0,0,0,1], weight = 0.7
