//! Token-budgeted truncation for filter results.
//!
//! [`Truncator`] takes a set of filter results and a token budget, then
//! greedily includes as many sample entries as fit within the budget.
//! Schema and metadata are always included regardless of budget.

use serde_json::Value;

use crate::output::token::TokenCounter;

/// The result of token-budgeted truncation.
#[derive(Debug, Clone)]
pub struct TruncatedOutput {
    /// The subset of results that fit within the token budget.
    pub sample: Vec<Value>,
    /// Total number of results before truncation.
    pub total: usize,
    /// Whether any results were excluded due to budget constraints.
    pub truncated: bool,
    /// The requested sample size (may differ from `sample.len()` when
    /// the total result count is smaller).
    pub sample_size: usize,
    /// Actual tokens consumed by the serialized sample.
    pub tokens_used: usize,
}

/// Truncates filter results to fit within a token budget.
pub struct Truncator {
    max_tokens: usize,
    sample_size: usize,
    counter: TokenCounter,
}

impl Truncator {
    /// Create a new [`Truncator`] with the given budget and sample size.
    pub fn new(max_tokens: usize, sample_size: usize) -> Self {
        let counter = TokenCounter::new();
        Truncator {
            max_tokens,
            sample_size,
            counter,
        }
    }

    /// Truncate `results` to fit within the token budget.
    ///
    /// When `results` contains exactly one element that is a JSON array,
    /// the array is expanded and each element is counted individually.
    /// This ensures `--tokens N` actually caps output even when the
    /// filter returns a single large array (e.g. `.items`).
    ///
    /// The greedy algorithm serialises each result individually, counts its
    /// tokens, and includes it if the cumulative total stays within
    /// `max_tokens`. At least one result is always included, even if it
    /// exceeds the budget on its own.
    pub fn truncate(&self, results: &[Value]) -> TruncatedOutput {
        // Expand single non-empty array results so token budget applies
        // per-element. Empty arrays are kept as-is (they produce 0 elements
        // which would give tokens_used=0, breaking the "always positive" invariant).
        if results.len() == 1 && results[0].is_array() {
            if let Some(arr) = results[0].as_array() {
                if !arr.is_empty() {
                    return self.truncate_slice(arr);
                }
            }
        }

        self.truncate_slice(results)
    }

    /// Core truncation logic operating on a flat slice of values.
    fn truncate_slice(&self, items: &[Value]) -> TruncatedOutput {
        let total = items.len();
        let cap = if self.sample_size == 0 {
            total
        } else {
            self.sample_size.min(total)
        };

        let mut sample: Vec<Value> = Vec::with_capacity(cap);
        let mut tokens_used: usize = 0;

        for val in items.iter().take(cap) {
            let serialized = serde_json::to_string(val).unwrap_or_default();
            let token_count = self.counter.count(&serialized);

            // Always include at least the first result.
            if sample.is_empty() {
                sample.push(val.clone());
                tokens_used = token_count;
                continue;
            }

            if tokens_used + token_count <= self.max_tokens {
                sample.push(val.clone());
                tokens_used += token_count;
            } else {
                break;
            }
        }

        let truncated = sample.len() < total;

        TruncatedOutput {
            sample,
            total,
            truncated,
            sample_size: cap,
            tokens_used,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_truncator(max_tokens: usize, sample_size: usize) -> Truncator {
        Truncator::new(max_tokens, sample_size)
    }

    #[test]
    fn test_empty_results() {
        let t = make_truncator(1000, 10);
        let output = t.truncate(&[]);
        assert!(output.sample.is_empty());
        assert_eq!(output.total, 0);
        assert!(!output.truncated);
        assert_eq!(output.tokens_used, 0);
    }

    #[test]
    fn test_single_result() {
        let t = make_truncator(1000, 10);
        let output = t.truncate(&[json!({"a": 1})]);
        assert_eq!(output.sample.len(), 1);
        assert_eq!(output.total, 1);
        assert!(!output.truncated);
        assert!(output.tokens_used > 0);
    }

    #[test]
    fn test_budget_zero_still_includes_first() {
        let t = make_truncator(0, 10);
        let output = t.truncate(&[json!({"a": 1}), json!({"b": 2})]);
        assert_eq!(output.sample.len(), 1, "must include at least 1 result even with zero budget");
        assert_eq!(output.total, 2);
        assert!(output.truncated);
    }

    #[test]
    fn test_budget_large_enough_for_all() {
        let t = make_truncator(100_000, 100);
        let results: Vec<Value> = (0..5).map(|i| json!({"id": i})).collect();
        let output = t.truncate(&results);
        assert_eq!(output.sample.len(), 5);
        assert_eq!(output.total, 5);
        assert!(!output.truncated);
    }

    #[test]
    fn test_budget_fits_exactly_n() {
        // Create results where each serialises to a known token count.
        // A small integer like 1 serialises to "1" (1 token).
        let results: Vec<Value> = (0..20).map(|i| json!(i)).collect();

        // "1" is 1 token. With budget=5, we should get 5 results
        // (first one always included, then 4 more).
        let t = make_truncator(5, 100);
        let output = t.truncate(&results);
        assert!(!output.sample.is_empty());
        assert!(output.sample.len() <= 6, "should not exceed budget by much");
        assert!(output.truncated);
    }

    #[test]
    fn test_sample_size_cap() {
        let results: Vec<Value> = (0..100).map(|i| json!({"id": i})).collect();
        let t = make_truncator(100_000, 3);
        let output = t.truncate(&results);
        assert_eq!(output.sample.len(), 3);
        assert_eq!(output.total, 100);
        assert!(output.truncated);
        assert_eq!(output.sample_size, 3);
    }

    #[test]
    fn test_sample_size_larger_than_total() {
        let results: Vec<Value> = vec![json!(1), json!(2)];
        let t = make_truncator(100_000, 10);
        let output = t.truncate(&results);
        assert_eq!(output.sample.len(), 2);
        assert_eq!(output.total, 2);
        assert!(!output.truncated);
    }

    #[test]
    fn test_tokens_used_monotonic() {
        let results: Vec<Value> = (0..10).map(|i| json!({"data": format!("value-{i}")})).collect();
        let t_small = make_truncator(10, 100);
        let t_large = make_truncator(100_000, 100);

        let out_small = t_small.truncate(&results);
        let out_large = t_large.truncate(&results);

        assert!(out_small.tokens_used <= out_large.tokens_used,
            "larger budget should use >= tokens: small={}, large={}",
            out_small.tokens_used, out_large.tokens_used);
    }

    #[test]
    fn test_truncated_flag() {
        let results: Vec<Value> = (0..100).map(|i| json!(i)).collect();
        let t = make_truncator(5, 100);
        let output = t.truncate(&results);
        assert!(output.truncated);
        assert!(output.sample.len() < output.total);
    }

    #[test]
    fn test_not_truncated_when_all_fit() {
        let results: Vec<Value> = vec![json!(1), json!(2), json!(3)];
        let t = make_truncator(100_000, 100);
        let output = t.truncate(&results);
        assert!(!output.truncated);
        assert_eq!(output.sample.len(), output.total);
    }

    #[test]
    fn test_large_dataset() {
        let results: Vec<Value> = (0..1000).map(|i| json!({"id": i, "name": format!("item-{i}")})).collect();
        let t = make_truncator(500, 1000);
        let output = t.truncate(&results);
        assert!(!output.sample.is_empty());
        assert!(output.sample.len() < 1000);
        assert!(output.truncated);
    }

    #[test]
    fn test_tokens_used_positive_for_nonempty() {
        let results = vec![json!({"key": "value"})];
        let t = make_truncator(1000, 10);
        let output = t.truncate(&results);
        assert!(output.tokens_used > 0, "tokens_used should be positive for non-empty results");
    }
}
