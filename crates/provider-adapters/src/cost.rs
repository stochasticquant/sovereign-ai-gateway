//! Cost estimation for LLM provider requests.
//!
//! Provides pricing data and cost calculation for various LLM providers and models.
//! Pricing is based on published rates as of early 2025 and may change over time.

use crate::traits::Usage;

/// Pricing information for a model.
#[derive(Debug, Clone)]
pub struct ModelPricing {
    /// Cost per 1,000 input tokens in USD.
    pub input_cost_per_1k: f64,
    /// Cost per 1,000 output tokens in USD.
    pub output_cost_per_1k: f64,
}

impl ModelPricing {
    /// Calculate the estimated cost for a given usage.
    pub fn calculate_cost(&self, usage: &Usage) -> f64 {
        let input_cost = (usage.prompt_tokens as f64 / 1000.0) * self.input_cost_per_1k;
        let output_cost = (usage.completion_tokens as f64 / 1000.0) * self.output_cost_per_1k;
        input_cost + output_cost
    }
}

/// Get pricing for an OpenAI model.
pub fn get_openai_pricing(model: &str) -> Option<ModelPricing> {
    match model {
        // GPT-4 models
        "gpt-4" | "gpt-4-0613" => Some(ModelPricing {
            input_cost_per_1k: 0.03,
            output_cost_per_1k: 0.06,
        }),
        "gpt-4-32k" | "gpt-4-32k-0613" => Some(ModelPricing {
            input_cost_per_1k: 0.06,
            output_cost_per_1k: 0.12,
        }),
        "gpt-4-turbo" | "gpt-4-turbo-preview" | "gpt-4-1106-preview" | "gpt-4-0125-preview" => {
            Some(ModelPricing {
                input_cost_per_1k: 0.01,
                output_cost_per_1k: 0.03,
            })
        }
        "gpt-4o" | "gpt-4o-2024-05-13" | "gpt-4o-2024-08-06" => Some(ModelPricing {
            input_cost_per_1k: 0.0025,
            output_cost_per_1k: 0.01,
        }),
        "gpt-4o-mini" | "gpt-4o-mini-2024-07-18" => Some(ModelPricing {
            input_cost_per_1k: 0.00015,
            output_cost_per_1k: 0.0006,
        }),
        // GPT-3.5 models
        "gpt-3.5-turbo" | "gpt-3.5-turbo-0125" | "gpt-3.5-turbo-1106" => Some(ModelPricing {
            input_cost_per_1k: 0.0005,
            output_cost_per_1k: 0.0015,
        }),
        "gpt-3.5-turbo-16k" => Some(ModelPricing {
            input_cost_per_1k: 0.001,
            output_cost_per_1k: 0.002,
        }),
        _ => None,
    }
}

/// Get pricing for an Anthropic Claude model.
pub fn get_anthropic_pricing(model: &str) -> Option<ModelPricing> {
    match model {
        // Claude 3.5 models
        "claude-3-5-sonnet-20241022" | "claude-3-5-sonnet-20240620" => Some(ModelPricing {
            input_cost_per_1k: 0.003,
            output_cost_per_1k: 0.015,
        }),
        // Claude 3 models
        "claude-3-opus-20240229" => Some(ModelPricing {
            input_cost_per_1k: 0.015,
            output_cost_per_1k: 0.075,
        }),
        "claude-3-sonnet-20240229" => Some(ModelPricing {
            input_cost_per_1k: 0.003,
            output_cost_per_1k: 0.015,
        }),
        "claude-3-haiku-20240307" => Some(ModelPricing {
            input_cost_per_1k: 0.00025,
            output_cost_per_1k: 0.00125,
        }),
        _ => None,
    }
}

/// Get pricing for an Azure OpenAI model.
/// Azure pricing is typically the same as OpenAI but may vary by region.
pub fn get_azure_pricing(model: &str) -> Option<ModelPricing> {
    // Azure uses deployment names which may differ from OpenAI model names
    // For now, we'll use the same pricing as OpenAI
    get_openai_pricing(model)
}

/// Get pricing for a local model.
/// Local models have no API costs, so this returns zero cost.
pub fn get_local_pricing(_model: &str) -> Option<ModelPricing> {
    Some(ModelPricing {
        input_cost_per_1k: 0.0,
        output_cost_per_1k: 0.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_pricing_calculation() {
        let pricing = get_openai_pricing("gpt-4o-mini").unwrap();
        let usage = Usage {
            prompt_tokens: 1000,
            completion_tokens: 500,
            total_tokens: 1500,
        };

        let cost = pricing.calculate_cost(&usage);
        // (1000/1000 * 0.00015) + (500/1000 * 0.0006) = 0.00015 + 0.0003 = 0.00045
        assert!((cost - 0.00045).abs() < 0.000001);
    }

    #[test]
    fn test_anthropic_pricing_calculation() {
        let pricing = get_anthropic_pricing("claude-3-haiku-20240307").unwrap();
        let usage = Usage {
            prompt_tokens: 2000,
            completion_tokens: 1000,
            total_tokens: 3000,
        };

        let cost = pricing.calculate_cost(&usage);
        // (2000/1000 * 0.00025) + (1000/1000 * 0.00125) = 0.0005 + 0.00125 = 0.00175
        assert!((cost - 0.00175).abs() < 0.000001);
    }

    #[test]
    fn test_local_pricing_is_zero() {
        let pricing = get_local_pricing("llama-3-70b").unwrap();
        let usage = Usage {
            prompt_tokens: 1000,
            completion_tokens: 1000,
            total_tokens: 2000,
        };

        let cost = pricing.calculate_cost(&usage);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_unknown_model_returns_none() {
        assert!(get_openai_pricing("unknown-model").is_none());
        assert!(get_anthropic_pricing("unknown-model").is_none());
    }
}
