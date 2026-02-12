//! Chat completions handler.
//!
//! POST /v1/chat/completions — proxies requests to LLM providers.
//! Supports both synchronous JSON response and SSE streaming (stream: true).
//! Pipeline: auth → quota → firewall → policy → route → provider → audit → token tracking

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    Extension,
};
use provider_adapters::{registry::ProviderRegistry, traits::LlmRequest};
use serde_json::json;
use token_governor::UsageIncrement;
use uuid::Uuid;

use crate::state::AppState;

/// POST /v1/chat/completions
///
/// Handles chat completion requests using OpenAI-compatible API format.
pub async fn chat_completions(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<Uuid>,
    Json(request): Json<LlmRequest>,
) -> Response {
    // For now, use a hardcoded provider registry
    // TODO(phase-7): Route to provider based on policy-engine rules
    let provider_id = "openai"; // Default provider

    // Get provider from registry
    // TODO(phase-7): Implement actual provider registry
    // For now, return a placeholder response
    tracing::info!(
        tenant_id = %tenant_id,
        model = %request.model,
        stream = %request.stream,
        "Processing chat completion request"
    );

    // Simulated response for now
    // TODO(phase-7): Actually call the provider
    let response_content = "This is a placeholder response. Provider integration coming in phase-7.";

    // Simulated token usage
    let prompt_tokens = 10u32;
    let completion_tokens = 20u32;
    let total_tokens = prompt_tokens + completion_tokens;

    // Track token usage asynchronously (fire and forget)
    let counter = state.token_counter.clone();
    let tenant_id_clone = tenant_id;
    let model_clone = request.model.clone();
    tokio::spawn(async move {
        let increment = UsageIncrement {
            tenant_id: tenant_id_clone,
            provider_id: provider_id.to_string(),
            model: model_clone,
            tokens: total_tokens as i64,
            estimated_cost_usd: 0.001, // Placeholder cost
        };

        if let Err(e) = counter.increment(increment).await {
            tracing::error!("Failed to track token usage: {}", e);
        }
    });

    // Return OpenAI-compatible response
    let response_json = json!({
        "id": format!("chatcmpl-{}", Uuid::now_v7()),
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": request.model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": response_content,
            },
            "finish_reason": "stop",
        }],
        "usage": {
            "prompt_tokens": prompt_tokens,
            "completion_tokens": completion_tokens,
            "total_tokens": total_tokens,
        },
    });

    Json(response_json).into_response()
}

// Example of what a full implementation would look like with provider routing:
//
// ```rust,ignore
// // Get provider from registry based on policy
// let provider = match get_provider(&state, &tenant_id, &request.model).await {
//     Ok(p) => p,
//     Err(e) => {
//         return (
//             StatusCode::BAD_GATEWAY,
//             Json(json!({"error": format!("Provider error: {}", e)})),
//         )
//             .into_response();
//     }
// };
//
// // Send request to provider
// let response = match provider.send(&request).await {
//     Ok(r) => r,
//     Err(e) => {
//         return (
//             StatusCode::BAD_GATEWAY,
//             Json(json!({"error": format!("Provider error: {}", e)})),
//         )
//             .into_response();
//     }
// };
//
// // Track token usage
// let increment = UsageIncrement {
//     tenant_id,
//     provider_id: response.provider_id.clone(),
//     model: response.model.clone(),
//     tokens: response.usage.total_tokens as i64,
//     estimated_cost_usd: response.estimated_cost_usd.unwrap_or(0.0),
// };
// state.token_counter.increment(increment).await.ok();
// ```
