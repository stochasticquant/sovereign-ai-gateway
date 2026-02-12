//! Chat completions handler.
//!
//! POST /v1/chat/completions — proxies requests to LLM providers.
//! Pipeline: auth → rate_limit → quota → handler → provider → audit → metrics

use audit_log::AuditEntry;
use axum::{
    Extension,
    extract::State,
    response::{IntoResponse, Json, Response},
};
use gateway_core::types::{DataClassification, ProviderId, RequestId, TenantId};
use provider_adapters::traits::LlmRequest;
use serde_json::json;
use token_governor::{CostCalculator, UsageIncrement};
use uuid::Uuid;

use crate::metrics as gw_metrics;
use crate::state::AppState;

/// POST /v1/chat/completions
///
/// Handles chat completion requests using OpenAI-compatible API format.
/// Routes to an LLM provider, tracks usage, writes audit entries, and records metrics.
pub async fn chat_completions(
    State(state): State<AppState>,
    Extension(tenant_id): Extension<Uuid>,
    Json(request): Json<LlmRequest>,
) -> Response {
    let request_id = RequestId::new();
    let start_time = std::time::Instant::now();
    let provider_id = state.config.providers.default_provider.clone();
    let model = request.model.clone();

    tracing::info!(
        tenant_id = %tenant_id,
        model = %model,
        provider = %provider_id,
        request_id = %request_id,
        "Processing chat completion request"
    );

    // Get provider from registry
    let provider = match state.provider_registry.get(&provider_id).await {
        Some(p) => p,
        None => {
            tracing::warn!(provider_id = %provider_id, "Provider not found in registry, returning placeholder");
            return placeholder_response(
                &state,
                tenant_id,
                &request_id,
                &provider_id,
                &model,
                start_time,
            )
            .await;
        }
    };

    // Call the provider
    match provider.send(&request).await {
        Ok(response) => {
            let latency_ms = start_time.elapsed().as_millis() as u64;
            let cost = CostCalculator::calculate(&provider_id, &response.model, &response.usage);

            // Track token usage (fire and forget)
            let counter = state.token_counter.clone();
            let increment = UsageIncrement {
                tenant_id,
                provider_id: provider_id.clone(),
                model: response.model.clone(),
                tokens: response.usage.total_tokens as i64,
                estimated_cost_usd: cost,
            };
            tokio::spawn(async move {
                if let Err(e) = counter.increment(increment).await {
                    tracing::error!("Failed to track token usage: {}", e);
                }
            });

            // Write audit entry (non-blocking)
            let audit_entry = AuditEntry {
                id: Uuid::now_v7(),
                request_id: request_id.clone(),
                timestamp: chrono::Utc::now(),
                tenant_id: TenantId(tenant_id),
                data_classification: DataClassification::Public,
                provider_id: ProviderId(provider_id.clone()),
                model: response.model.clone(),
                prompt_tokens: response.usage.prompt_tokens,
                completion_tokens: response.usage.completion_tokens,
                total_tokens: response.usage.total_tokens,
                region: provider.metadata().region.clone(),
                risk_score: 0,
                decision: "allow".to_string(),
                latency_ms,
                status_code: 200,
                error: None,
            };
            if let Err(e) = state.audit_writer.write(audit_entry) {
                tracing::error!("Failed to write audit entry: {}", e);
            }
            gw_metrics::record_audit_entry();

            // Record metrics
            gw_metrics::record_request(&tenant_id.to_string(), &provider_id, &response.model, 200);
            gw_metrics::record_latency(&provider_id, &response.model, latency_ms as f64);
            gw_metrics::record_tokens(
                &provider_id,
                &response.model,
                "prompt",
                response.usage.prompt_tokens,
            );
            gw_metrics::record_tokens(
                &provider_id,
                &response.model,
                "completion",
                response.usage.completion_tokens,
            );
            gw_metrics::record_cost(&provider_id, &response.model, cost);

            // Return OpenAI-compatible response
            let response_json = json!({
                "id": format!("chatcmpl-{}", Uuid::now_v7()),
                "object": "chat.completion",
                "created": chrono::Utc::now().timestamp(),
                "model": response.model,
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": response.content,
                    },
                    "finish_reason": "stop",
                }],
                "usage": {
                    "prompt_tokens": response.usage.prompt_tokens,
                    "completion_tokens": response.usage.completion_tokens,
                    "total_tokens": response.usage.total_tokens,
                },
            });

            Json(response_json).into_response()
        }
        Err(e) => {
            let latency_ms = start_time.elapsed().as_millis() as u64;
            let status_code = match &e {
                provider_adapters::traits::ProviderError::RateLimited => 429,
                provider_adapters::traits::ProviderError::Timeout => 504,
                provider_adapters::traits::ProviderError::AuthError(_) => 401,
                _ => 502,
            };

            tracing::error!(
                provider_id = %provider_id,
                error = %e,
                status_code,
                "Provider request failed"
            );

            // Write audit entry for the error
            let audit_entry = AuditEntry {
                id: Uuid::now_v7(),
                request_id: request_id.clone(),
                timestamp: chrono::Utc::now(),
                tenant_id: TenantId(tenant_id),
                data_classification: DataClassification::Public,
                provider_id: ProviderId(provider_id.clone()),
                model: model.clone(),
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                region: String::new(),
                risk_score: 0,
                decision: "allow".to_string(),
                latency_ms,
                status_code: status_code as u16,
                error: Some(e.to_string()),
            };
            if let Err(e) = state.audit_writer.write(audit_entry) {
                tracing::error!("Failed to write audit entry: {}", e);
            }

            // Record metrics
            gw_metrics::record_request(
                &tenant_id.to_string(),
                &provider_id,
                &model,
                status_code as u16,
            );
            gw_metrics::record_latency(&provider_id, &model, latency_ms as f64);

            let response_json = json!({
                "error": {
                    "message": format!("Provider error: {}", e),
                    "type": "provider_error",
                    "code": status_code,
                }
            });

            (
                axum::http::StatusCode::from_u16(status_code as u16)
                    .unwrap_or(axum::http::StatusCode::BAD_GATEWAY),
                Json(response_json),
            )
                .into_response()
        }
    }
}

/// Placeholder response when no provider is registered.
/// This allows the gateway to function for testing without real API keys.
async fn placeholder_response(
    state: &AppState,
    tenant_id: Uuid,
    request_id: &RequestId,
    provider_id: &str,
    model: &str,
    start_time: std::time::Instant,
) -> Response {
    let latency_ms = start_time.elapsed().as_millis() as u64;
    let response_content =
        "No provider registered. Configure provider API keys to enable real LLM routing.";
    let prompt_tokens = 10u32;
    let completion_tokens = 20u32;
    let total_tokens = prompt_tokens + completion_tokens;

    // Track token usage
    let counter = state.token_counter.clone();
    let tenant_id_clone = tenant_id;
    let provider_id_owned = provider_id.to_string();
    let model_owned = model.to_string();
    tokio::spawn(async move {
        let increment = UsageIncrement {
            tenant_id: tenant_id_clone,
            provider_id: provider_id_owned,
            model: model_owned,
            tokens: total_tokens as i64,
            estimated_cost_usd: 0.0,
        };
        if let Err(e) = counter.increment(increment).await {
            tracing::error!("Failed to track token usage: {}", e);
        }
    });

    // Write audit entry
    let audit_entry = AuditEntry {
        id: Uuid::now_v7(),
        request_id: request_id.clone(),
        timestamp: chrono::Utc::now(),
        tenant_id: TenantId(tenant_id),
        data_classification: DataClassification::Public,
        provider_id: ProviderId(provider_id.to_string()),
        model: model.to_string(),
        prompt_tokens,
        completion_tokens,
        total_tokens,
        region: "local".to_string(),
        risk_score: 0,
        decision: "allow".to_string(),
        latency_ms,
        status_code: 200,
        error: None,
    };
    if let Err(e) = state.audit_writer.write(audit_entry) {
        tracing::error!("Failed to write audit entry: {}", e);
    }
    gw_metrics::record_audit_entry();
    gw_metrics::record_request(&tenant_id.to_string(), provider_id, model, 200);
    gw_metrics::record_latency(provider_id, model, latency_ms as f64);

    let response_json = json!({
        "id": format!("chatcmpl-{}", Uuid::now_v7()),
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": model,
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
