use hnsw_rs::prelude::FilterT;

/// A filter that restricts hnsw_rs search to a pre-computed allow-list of data IDs.
///
/// Used internally by `VectorIndex::search_filtered`. Not part of the public API.
pub(crate) struct EntryIdFilter {
    allowed_data_ids: Vec<usize>,
}

impl FilterT for EntryIdFilter {
    fn hnsw_filter(&self, id: &usize) -> bool {
        self.allowed_data_ids.binary_search(id).is_ok()
    }
}

impl EntryIdFilter {
    /// Construct from a list of allowed hnsw data IDs.
    /// The input is sorted and deduplicated internally.
    pub(crate) fn new(mut allowed_data_ids: Vec<usize>) -> Self {
        allowed_data_ids.sort_unstable();
        allowed_data_ids.dedup();
        EntryIdFilter { allowed_data_ids }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_accepts_allowed_id() {
        let filter = EntryIdFilter::new(vec![1, 5, 10]);
        assert!(filter.hnsw_filter(&5));
        assert!(filter.hnsw_filter(&1));
        assert!(filter.hnsw_filter(&10));
    }

    #[test]
    fn test_filter_rejects_disallowed_id() {
        let filter = EntryIdFilter::new(vec![1, 5, 10]);
        assert!(!filter.hnsw_filter(&2));
        assert!(!filter.hnsw_filter(&0));
        assert!(!filter.hnsw_filter(&100));
    }

    #[test]
    fn test_filter_empty_rejects_all() {
        let filter = EntryIdFilter::new(vec![]);
        assert!(!filter.hnsw_filter(&0));
        assert!(!filter.hnsw_filter(&1));
    }

    #[test]
    fn test_filter_single_element() {
        let filter = EntryIdFilter::new(vec![42]);
        assert!(filter.hnsw_filter(&42));
        assert!(!filter.hnsw_filter(&41));
        assert!(!filter.hnsw_filter(&43));
    }

    #[test]
    fn test_filter_unsorted_input() {
        let filter = EntryIdFilter::new(vec![10, 1, 5]);
        assert!(filter.hnsw_filter(&1));
        assert!(filter.hnsw_filter(&5));
        assert!(filter.hnsw_filter(&10));
    }

    #[test]
    fn test_filter_duplicates() {
        let filter = EntryIdFilter::new(vec![5, 5, 5, 1, 1]);
        assert!(filter.hnsw_filter(&5));
        assert!(filter.hnsw_filter(&1));
        assert!(!filter.hnsw_filter(&2));
    }
}
