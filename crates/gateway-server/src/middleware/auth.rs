// TODO(phase-1): API key extraction and validation middleware.
// Extracts API key from `Authorization: Bearer <key>` or `X-Api-Key` header.
// Resolves tenant from key lookup. Rejects with 401 on invalid/missing key.
