//! Estimated API prices in USD per million tokens. Real billing may differ
//! (subscriptions, discounts); Argus labels costs as estimates.

use crate::model::Usage;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Price {
    pub input: f64,
    pub output: f64,
}

pub fn price_for(model: &str) -> Price {
    let m = model.to_lowercase();
    if m.contains("opus") {
        let legacy = ["opus-4-0", "opus-4-1", "opus-4-2025", "3-opus", "opus-3"];
        if legacy.iter().any(|p| m.contains(p)) {
            Price {
                input: 15.0,
                output: 75.0,
            }
        } else {
            Price {
                input: 5.0,
                output: 25.0,
            }
        }
    } else if m.contains("haiku") {
        Price {
            input: 1.0,
            output: 5.0,
        }
    } else {
        // Sonnet and unknown models.
        Price {
            input: 3.0,
            output: 15.0,
        }
    }
}

/// Estimated cost of `usage` on `model`. Cache writes cost 1.25× input,
/// cache reads 0.1× input.
pub fn cost(model: &str, usage: &Usage) -> f64 {
    let p = price_for(model);
    let per = 1_000_000.0;
    (usage.input as f64 * p.input
        + usage.cache_write as f64 * p.input * 1.25
        + usage.cache_read as f64 * p.input * 0.1
        + usage.output as f64 * p.output)
        / per
}

/// Context window size for a model id.
pub fn context_window(model: &str, observed: u64) -> u64 {
    if model.contains("[1m]") || observed > 200_000 {
        1_000_000
    } else {
        200_000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_prices_by_family() {
        assert_eq!(price_for("claude-opus-4-1-20250805").input, 15.0);
        assert_eq!(price_for("claude-opus-5-5").input, 5.0);
        assert_eq!(price_for("claude-sonnet-5-5").output, 15.0);
        assert_eq!(price_for("claude-haiku-5-5").input, 1.0);
    }

    #[test]
    fn computes_cost() {
        let u = Usage {
            input: 1_000_000,
            output: 0,
            cache_read: 0,
            cache_write: 0,
        };
        assert!((cost("claude-sonnet-5-5", &u) - 3.0).abs() < 1e-9);
    }
}
