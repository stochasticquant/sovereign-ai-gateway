// TODO(phase-4): Retry logic with exponential backoff + jitter.
// Only retries transient errors (5xx, timeout). Never retries 4xx.
// Configurable max retries and base delay.
