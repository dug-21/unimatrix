# Pseudocode: rollback-enhancements (Wave 3)

## Purpose

Add NaN/Inf check after training step and per-class regression check to promotion criteria. Extends `service.rs`.

## NaN/Inf Check

Already part of `try_train_step` in training-service pseudocode (step 4):

```pseudo
// After training loop, before saving:
let final_params = model.flat_parameters();
if final_params.iter().any(|p| p.is_nan() || p.is_infinite()) {
    // Log warning, discard model, release lock, return
    return;
}
```

This is implemented inline in the training closure, not as a separate function.

## Per-Class Regression Check

Add method to TrainingService for promotion evaluation:

```pseudo
impl TrainingService {
    /// Check if shadow model meets promotion criteria.
    /// Returns true if safe to promote, false if per-class regression detected.
    fn check_promotion_safe(
        &self,
        model_name: &str,
        shadow_per_class: &[f64],    // per-class accuracy of shadow model
        production_per_class: &[f64], // per-class accuracy of production model
    ) -> bool {
        if shadow_per_class.len() != production_per_class.len() {
            return false;
        }
        let threshold = self.config.per_class_regression_threshold;
        for (shadow_acc, prod_acc) in shadow_per_class.iter().zip(production_per_class.iter()) {
            // Check if any class drops more than threshold
            if prod_acc - shadow_acc > threshold {
                return false; // Regression detected for this class
            }
        }
        true
    }
}
```

## Config Extension

Add to LearnConfig:
```pseudo
per_class_regression_threshold: f64,  // default 0.10 (10%)
```

This was already included in the training-service pseudocode config section.
