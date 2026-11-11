use reqwest::{header::HeaderValue, Client, Method, RequestBuilder, Response};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::time::sleep;
use tracing::{trace, warn};

use crate::config::{RetryPolicy, WorkerBridgeConfig};
use crate::secret_store;
use std::time::Duration;

const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(90);
const PAYLOAD_PREVIEW_LIMIT: usize = 400;
const AUTH_ERROR_PREFIX: &str = "worker auth failed";

pub fn build_http_client() -> Result<Client, String> {
    Client::builder()
        .timeout(DEFAULT_HTTP_TIMEOUT)
        .pool_max_idle_per_host(0)
        .build()
        .map_err(|err| format!("failed to build worker HTTP client: {err}"))
}

fn base_url(config: &WorkerBridgeConfig) -> String {
    config.base_url().trim_end_matches('/').to_string()
}

pub fn chat_endpoint(config: &WorkerBridgeConfig) -> String {
    format!("{}/chat", base_url(config))
}

pub fn planner_endpoint(config: &WorkerBridgeConfig) -> String {
    format!("{}/planner", base_url(config))
}

pub fn memory_endpoint(config: &WorkerBridgeConfig) -> String {
    format!("{}/memory", base_url(config))
}

pub fn sync_endpoint(config: &WorkerBridgeConfig) -> String {
    format!("{}/sync_machine_state", base_url(config))
}

pub fn config_endpoint(config: &WorkerBridgeConfig) -> String {
    format!("{}/config", base_url(config))
}

/// Send a JSON POST with exponential backoff and decode the response.
/// Centralizes retry/timeouts/error logging for worker/planner HTTP calls.
pub(crate) fn backoff_delay(policy: RetryPolicy, attempt: u32) -> Duration {
    let base = policy.base_delay_ms();
    let exponent = attempt.saturating_sub(1).min(10);
    let factor = 1u64 << exponent;
    Duration::from_millis(base.saturating_mul(factor))
}

fn log_payload_preview(label: &str, payload: &str) {
    let preview = truncate_log_payload(payload, PAYLOAD_PREVIEW_LIMIT);
    warn!("{label}: {preview}", label = label, preview = preview);
    trace!(
        "{label} (full): {payload}",
        label = label,
        payload = payload
    );
}

fn truncate_log_payload(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }

    let truncated = &text[..limit];
    format!("{truncated}... ({} bytes truncated)", text.len() - limit)
}

pub fn is_auth_error_message(message: &str) -> bool {
    message.starts_with(AUTH_ERROR_PREFIX)
}

pub fn auth_error_status(message: &str) -> Option<u16> {
    let marker = "status ";
    let start = message.find(marker)? + marker.len();
    let digits = message[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();
    digits.parse::<u16>().ok()
}
pub async fn post_json_with_retry<T>(
    client: &Client,
    endpoint: &str,
    body: &Value,
    config: &WorkerBridgeConfig,
    payload_label: &str,
) -> Result<T, String>
where
    T: DeserializeOwned + Send + 'static,
{
    let payload_string = body.to_string();
    let response = send_with_retry(
        client,
        Method::POST,
        endpoint,
        Some(body),
        Some((payload_label, payload_string.as_str())),
        config,
    )
    .await?;

    decode_json(response, payload_label, Some(&payload_string)).await
}

pub(crate) async fn delete_with_retry(
    client: &Client,
    endpoint: &str,
    config: &WorkerBridgeConfig,
) -> Result<(), String> {
    send_with_retry(client, Method::DELETE, endpoint, None, None, config).await?;
    Ok(())
}

pub(crate) async fn get_json_with_retry<T>(
    client: &Client,
    endpoint: &str,
    config: &WorkerBridgeConfig,
    payload_label: &str,
) -> Result<T, String>
where
    T: DeserializeOwned + Send + 'static,
{
    let response = send_with_retry(client, Method::GET, endpoint, None, None, config).await?;
    decode_json(response, payload_label, None).await
}

async fn send_with_retry(
    client: &Client,
    method: Method,
    endpoint: &str,
    body: Option<&Value>,
    payload_preview: Option<(&str, &str)>,
    config: &WorkerBridgeConfig,
) -> Result<Response, String> {
    let retry_policy = config.retry_policy();
    let mut attempt: u32 = 0;

    loop {
        attempt += 1;
        let request = build_request(client, method.clone(), endpoint, body);
        let resp = request.send().await;
        let response = match resp {
            Ok(response) => response,
            Err(err) => {
                warn!(attempt, error = ?err, "Request failed");
                if let Some((label, preview)) = payload_preview {
                    log_payload_preview(label, preview);
                }
                if attempt >= retry_policy.max_attempts() {
                    return Err(format!(
                        "failed to contact worker after {attempt} attempts: {err}"
                    ));
                }
                sleep(backoff_delay(retry_policy, attempt)).await;
                continue;
            }
        };

        if response.status().is_success() {
            return Ok(response);
        }

        let status = response.status();
        let body_text = response
            .text()
            .await
            .unwrap_or_else(|_| String::from("<unavailable>"));
        if matches!(status.as_u16(), 401 | 403) {
            return Err(format!(
                "{AUTH_ERROR_PREFIX} (status {status}): {body_text}"
            ));
        }
        warn!(attempt, %status, "Request returned non-success status");
        if let Some((label, preview)) = payload_preview {
            log_payload_preview(label, preview);
        }
        if attempt >= retry_policy.max_attempts() {
            return Err(format!("worker returned status {status}: {body_text}"));
        }
        sleep(backoff_delay(retry_policy, attempt)).await;
    }
}

async fn decode_json<T>(
    response: Response,
    payload_label: &str,
    payload_preview: Option<&str>,
) -> Result<T, String>
where
    T: DeserializeOwned + Send + 'static,
{
    response.json::<T>().await.map_err(|err| {
        if let Some(preview) = payload_preview {
            log_payload_preview(payload_label, preview);
        }
        format!("failed to decode worker reply: {err}")
    })
}

fn build_request(
    client: &Client,
    method: Method,
    endpoint: &str,
    body: Option<&Value>,
) -> RequestBuilder {
    let mut builder = match method {
        Method::POST => client.post(endpoint),
        Method::GET => client.get(endpoint),
        Method::DELETE => client.delete(endpoint),
        other => client.request(other, endpoint),
    };

    if let Some(header_value) = worker_auth_header() {
        builder = builder.header("Authorization", header_value);
    }

    if let Some(body) = body {
        builder.json(body)
    } else {
        builder
    }
}

fn worker_auth_header() -> Option<HeaderValue> {
    let token = secret_store::load_worker_auth_token()?;
    HeaderValue::from_str(format!("Bearer {}", token).as_str()).ok()
}
