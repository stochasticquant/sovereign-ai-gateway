// TODO(phase-1): Chat completions handler.
// POST /v1/chat/completions — proxies to LLM providers.
// Supports both synchronous JSON response and SSE streaming (stream: true).
// Pipeline: auth → firewall → policy → route → provider → audit log
