//! Memory-bounded training buffer using reservoir sampling.
//!
//! Maintains a uniform random sample of fixed size from a potentially unbounded
//! stream of items. Generic over any `Clone` type.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

/// Memory-bounded training buffer using reservoir sampling.
///
/// Maintains a uniform random sample of fixed size from a potentially unbounded
/// stream of items.
pub struct TrainingReservoir<T: Clone> {
    items: Vec<T>,
    capacity: usize,
    total_seen: u64,
    rng: StdRng,
}

impl<T: Clone> TrainingReservoir<T> {
    /// Create a new reservoir with the given capacity and RNG seed.
    pub fn new(capacity: usize, seed: u64) -> Self {
        Self {
            items: Vec::with_capacity(capacity),
            capacity,
            total_seen: 0,
            rng: StdRng::seed_from_u64(seed),
        }
    }

    /// Add items to the reservoir via reservoir sampling.
    pub fn add(&mut self, items: &[T]) {
        for item in items {
            self.total_seen += 1;

            if self.items.len() < self.capacity {
                self.items.push(item.clone());
            } else {
                // Reservoir sampling: replace with probability capacity / total_seen
                let j = self.rng.random_range(0..self.total_seen);
                if j < self.capacity as u64 {
                    self.items[j as usize] = item.clone();
                }
            }
        }
    }

    /// Sample a batch of items (with replacement for simplicity).
    pub fn sample_batch(&mut self, batch_size: usize) -> Vec<&T> {
        let actual_size = batch_size.min(self.items.len());
        if actual_size == 0 {
            return Vec::new();
        }
        let mut batch = Vec::with_capacity(actual_size);
        for _ in 0..actual_size {
            let idx = self.rng.random_range(0..self.items.len());
            batch.push(&self.items[idx]);
        }
        batch
    }

    /// Number of items currently in the reservoir.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the reservoir is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Total items seen (including replaced ones).
    pub fn total_seen(&self) -> u64 {
        self.total_seen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-LC-01: Generic reservoir basic add
    #[test]
    fn generic_reservoir_basic_add() {
        let mut r = TrainingReservoir::new(10, 42);
        r.add(&[(1u64, 2u64), (3, 4), (5, 6), (7, 8), (9, 10)]);
        assert_eq!(r.len(), 5);
        assert_eq!(r.total_seen(), 5);
    }

    // T-LC-02: Generic reservoir capacity bound
    #[test]
    fn generic_reservoir_capacity_bound() {
        let mut r: TrainingReservoir<u64> = TrainingReservoir::new(10, 42);
        let items: Vec<u64> = (0..100).collect();
        r.add(&items);
        assert_eq!(r.len(), 10);
        assert_eq!(r.total_seen(), 100);
    }

    // T-LC-03: Generic reservoir sample_batch
    #[test]
    fn generic_reservoir_sample_batch() {
        let mut r: TrainingReservoir<u64> = TrainingReservoir::new(100, 42);
        let items: Vec<u64> = (0..50).collect();
        r.add(&items);

        assert_eq!(r.sample_batch(32).len(), 32);
        assert_eq!(r.sample_batch(50).len(), 50);
        assert_eq!(r.sample_batch(100).len(), 50); // capped at len
    }

    #[test]
    fn empty_reservoir_sample() {
        let mut r: TrainingReservoir<u64> = TrainingReservoir::new(10, 42);
        assert!(r.sample_batch(5).is_empty());
        assert!(r.is_empty());
    }

    #[test]
    fn reservoir_overflow_no_growth() {
        let mut r: TrainingReservoir<u64> = TrainingReservoir::new(100, 42);
        for i in 0..10_000u64 {
            r.add(&[i]);
            assert!(r.len() <= 100);
        }
        assert_eq!(r.total_seen(), 10_000);
    }
}
