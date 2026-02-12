//! Pre-flight token estimation for quota pre-checks.
//!
//! Uses a character-based heuristic for fast, dependency-free estimation.
//! Accurate enough for quota enforcement; for exact counts, tiktoken-rs
//! can be added as an optional feature later.

/// Heuristic token estimator for pre-flight quota checks.
///
/// Uses the rule-of-thumb that ~4 characters ≈ 1 token for English text.
/// Each message also has a small overhead for role and formatting tokens.
pub struct TokenEstimator;

/// Per-message overhead in tokens (role, separators, formatting).
const MESSAGE_OVERHEAD_TOKENS: u32 = 4;

/// Base overhead per request (system-level framing tokens).
const REQUEST_OVERHEAD_TOKENS: u32 = 3;

/// Average characters per token (English text).
const CHARS_PER_TOKEN: f64 = 4.0;

impl TokenEstimator {
    /// Estimate the number of prompt tokens for a set of messages.
    ///
    /// Heuristic: each message contributes (content_chars / 4) + 4 overhead tokens,
    /// plus a base request overhead of 3 tokens.
    pub fn estimate_prompt_tokens(messages: &[(impl AsRef<str>, impl AsRef<str>)]) -> u32 {
        let mut total = REQUEST_OVERHEAD_TOKENS;

        for (_role, content) in messages {
            let content_tokens = (content.as_ref().len() as f64 / CHARS_PER_TOKEN).ceil() as u32;
            total += content_tokens + MESSAGE_OVERHEAD_TOKENS;
        }

        total
    }

    /// Estimate total tokens (prompt + expected completion).
    ///
    /// Uses `max_tokens` if provided, otherwise falls back to a model-specific default.
    pub fn estimate_total_tokens(
        model: &str,
        messages: &[(impl AsRef<str>, impl AsRef<str>)],
        max_tokens: Option<u32>,
    ) -> u32 {
        let prompt_tokens = Self::estimate_prompt_tokens(messages);
        let completion_tokens = max_tokens.unwrap_or_else(|| Self::default_max_tokens(model));
        prompt_tokens + completion_tokens
    }

    /// Default max_tokens for a model family when not specified by the caller.
    fn default_max_tokens(model: &str) -> u32 {
        let model_lower = model.to_lowercase();
        if model_lower.starts_with("gpt-4") || model_lower.starts_with("claude-3") {
            4096
        } else {
            2048
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_prompt_tokens_single_message() {
        // "Hello, world!" = 13 chars → ceil(13/4) = 4 tokens + 4 overhead = 8
        // Plus 3 request overhead = 11
        let messages = vec![("user", "Hello, world!")];
        let estimate = TokenEstimator::estimate_prompt_tokens(&messages);
        assert_eq!(estimate, 11);
    }

    #[test]
    fn test_estimate_prompt_tokens_multiple_messages() {
        let messages = vec![
            ("system", "You are a helpful assistant."), // 28 chars → ceil(28/4)=7 + 4 = 11
            ("user", "What is Rust?"),                  // 13 chars → ceil(13/4)=4 + 4 = 8
        ];
        // 3 (base) + 11 + 8 = 22
        let estimate = TokenEstimator::estimate_prompt_tokens(&messages);
        assert_eq!(estimate, 22);
    }

    #[test]
    fn test_estimate_prompt_tokens_empty_messages() {
        let messages: Vec<(&str, &str)> = vec![];
        let estimate = TokenEstimator::estimate_prompt_tokens(&messages);
        assert_eq!(estimate, REQUEST_OVERHEAD_TOKENS);
    }

    #[test]
    fn test_estimate_prompt_tokens_long_content() {
        // 400 chars → 100 tokens + 4 overhead = 104 + 3 base = 107
        let long_content = "a".repeat(400);
        let messages = vec![("user", long_content.as_str())];
        let estimate = TokenEstimator::estimate_prompt_tokens(&messages);
        assert_eq!(estimate, 107);
    }

    #[test]
    fn test_estimate_total_tokens_with_max_tokens() {
        let messages = vec![("user", "Hello")]; // 5 chars → 2 + 4 = 6, + 3 base = 9
        let estimate = TokenEstimator::estimate_total_tokens("gpt-4", &messages, Some(1000));
        assert_eq!(estimate, 9 + 1000);
    }

    #[test]
    fn test_estimate_total_tokens_gpt4_default() {
        let messages = vec![("user", "Hello")]; // 9 prompt tokens
        let estimate = TokenEstimator::estimate_total_tokens("gpt-4o", &messages, None);
        assert_eq!(estimate, 9 + 4096);
    }

    #[test]
    fn test_estimate_total_tokens_claude_default() {
        let messages = vec![("user", "Hello")];
        let estimate =
            TokenEstimator::estimate_total_tokens("claude-3-sonnet-20240229", &messages, None);
        assert_eq!(estimate, 9 + 4096);
    }

    #[test]
    fn test_estimate_total_tokens_gpt35_default() {
        let messages = vec![("user", "Hello")];
        let estimate = TokenEstimator::estimate_total_tokens("gpt-3.5-turbo", &messages, None);
        assert_eq!(estimate, 9 + 2048);
    }

    #[test]
    fn test_estimate_total_tokens_unknown_model_default() {
        let messages = vec![("user", "Hello")];
        let estimate = TokenEstimator::estimate_total_tokens("some-local-model", &messages, None);
        assert_eq!(estimate, 9 + 2048);
    }
}
