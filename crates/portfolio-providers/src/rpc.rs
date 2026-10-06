//! Read-only JSON-RPC transport. Never echo server messages: they may contain keys.
use crate::{ProviderError, http::HttpClient};
use serde_json::{Value, json};
use url::Url;

pub fn invalid(provider: &'static str, endpoint: &'static str, detail: &str) -> ProviderError {
    ProviderError::InvalidResponse {
        provider,
        endpoint,
        detail: detail.into(),
    }
}

pub async fn call(
    http: &HttpClient,
    url: Url,
    method: &'static str,
    params: Value,
    cost: u32,
) -> Result<Value, ProviderError> {
    let provider = http.provider();
    let body = http
        .post_json_cost(
            method,
            url,
            &json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}),
            cost,
        )
        .await?;
    // Deserialize into Value to avoid including untrusted string values in serde errors.
    let v: Value = body.json(provider, method)?;
    if let Some(e) = v.get("error").filter(|e| !e.is_null()) {
        let code = e.get("code").and_then(Value::as_i64).unwrap_or(0);
        let msg = e
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        let error = if code == 429
            || code == -32005
            || msg.contains("rate limit")
            || msg.contains("quota")
            || msg.contains("credits exceeded")
        {
            ProviderError::RateLimited {
                provider,
                endpoint: method,
                retry_after_secs: None,
            }
        } else if [401, 403, -32600].contains(&code)
            && (msg.contains("key") || msg.contains("auth"))
        {
            ProviderError::Auth {
                provider,
                endpoint: method,
                status: 401,
            }
        } else {
            invalid(
                provider,
                method,
                &format!("RPC code {code}; method or account entitlement unavailable"),
            )
        };
        http.record_rpc_failure(&error);
        return Err(error);
    }
    if v.get("jsonrpc").and_then(Value::as_str) != Some("2.0") || v.get("id") != Some(&json!(1)) {
        return Err(invalid(provider, method, "mismatched JSON-RPC envelope"));
    }
    v.get("result")
        .cloned()
        .ok_or_else(|| invalid(provider, method, "missing result"))
}

pub fn text<'a>(
    v: &'a Value,
    key: &str,
    provider: &'static str,
    method: &'static str,
) -> Result<&'a str, ProviderError> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(provider, method, "missing required string"))
}

pub fn raw_hex(
    s: &str,
    provider: &'static str,
    method: &'static str,
) -> Result<num_bigint::BigInt, ProviderError> {
    s.strip_prefix("0x")
        .filter(|s| !s.is_empty() && s.len() <= 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
        .and_then(|s| num_bigint::BigInt::parse_bytes(s.as_bytes(), 16))
        .ok_or_else(|| invalid(provider, method, "invalid hexadecimal quantity"))
}
