//! Explicit, bounded text inference for arbitrary user-configured model endpoints.
use crate::model::{AppResult, ModelConfig, Protocol};
use reqwest::{redirect::Policy, Client, StatusCode};
use serde::Serialize;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelTestResult {
    pub message: String,
    pub elapsed_ms: u64,
}

/// Send exactly one "test" request without redirects, retries, login cookies or filesystem writes.
pub async fn test_model(model: &ModelConfig) -> AppResult<ModelTestResult> {
    test_with_timeout(model, Duration::from_secs(30)).await
}

/// Validate a draft before connecting, then bound the complete request and response read.
async fn test_with_timeout(model: &ModelConfig, timeout: Duration) -> AppResult<ModelTestResult> {
    let mut model = model.clone();
    model.validate()?;
    let client = Client::builder()
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(10).min(timeout))
        .timeout(timeout)
        .user_agent("power-switch/0.1.1")
        .build()
        .map_err(|_| "无法初始化模型测试连接")?;
    let (suffix, body) = match model.protocol {
        Protocol::OpenaiChat => (
            "/chat/completions",
            json!({"model":model.model_id,"messages":[{"role":"user","content":"test"}],"max_tokens":256,"stream":false}),
        ),
        Protocol::OpenaiResponses => (
            "/responses",
            json!({"model":model.model_id,"input":"test","max_output_tokens":256,"stream":false}),
        ),
        Protocol::AnthropicMessages => (
            "/v1/messages",
            json!({"model":model.model_id,"messages":[{"role":"user","content":"test"}],"max_tokens":256,"stream":false}),
        ),
    };
    let mut request = client
        .post(format!("{}{suffix}", model.base_url))
        .json(&body);
    if model.protocol == Protocol::AnthropicMessages {
        request = request.header("anthropic-version", "2023-06-01");
        if !model.api_key.is_empty() {
            request = request.header("x-api-key", &model.api_key);
        }
    } else if !model.api_key.is_empty() {
        request = request.bearer_auth(&model.api_key);
    }
    let started = Instant::now();
    let mut response = request.send().await.map_err(transport_error)?;
    if !response.status().is_success() {
        return Err(status_error(response.status()));
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_RESPONSE_BYTES as u64)
    {
        return Err("测试未通过：接口响应超过 1 MiB，请检查 API 地址".into());
    }
    let mut bytes = vec![];
    while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err("测试未通过：接口响应超过 1 MiB，请检查 API 地址".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "测试未通过：接口未返回有效 JSON，请检查 API 地址和协议")?;
    validate_response(model.protocol, &value)?;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(ModelTestResult {
        message: format!("测试通过，耗时{:.2} 秒", elapsed_ms as f64 / 1000.0),
        elapsed_ms,
    })
}

/// Report transport failures without reflecting URLs, API keys or raw server messages.
fn transport_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "测试未通过：请求超时，请检查网络或稍后重试".into()
    } else {
        "测试未通过：连接失败，请检查地址、网络、证书和 API Key 格式".into()
    }
}

/// Translate HTTP failures into actionable local text without exposing response bodies.
fn status_error(status: StatusCode) -> String {
    let hint = match status.as_u16() {
        401 => "鉴权失败，请检查 API Key",
        403 => "请求被拒绝，请检查模型权限或 IP 限制",
        404 => "接口或模型不存在，请检查地址、协议和模型 ID",
        429 => "请求受限，请检查额度或稍后重试",
        300..=399 => "接口发生重定向，请直接填写最终 API 地址",
        400 | 422 => "接口拒绝测试参数，请检查协议、模型 ID 及服务商兼容性",
        500..=599 => "模型服务暂时不可用，请稍后重试",
        _ => "请求失败，请检查模型服务状态",
    };
    format!("测试未通过：HTTP {}，{hint}", status.as_u16())
}

/// Require a nonempty assistant text reply instead of accepting an arbitrary HTTP 200 JSON envelope.
fn validate_response(protocol: Protocol, value: &Value) -> AppResult<()> {
    if value.get("error").is_some_and(|e| !e.is_null()) {
        return Err("测试未通过：模型接口返回错误，请检查模型 ID、密钥权限和服务商状态".into());
    }
    let valid = match protocol {
        Protocol::OpenaiChat => value["choices"].as_array().is_some_and(|choices| {
            choices
                .iter()
                .any(|c| nonempty_text(&c["message"]["content"]))
        }),
        Protocol::OpenaiResponses => {
            value["object"] == "response"
                && matches!(value["status"].as_str(), Some("completed" | "incomplete"))
                && value["output"].as_array().is_some_and(|output| {
                    output.iter().any(|item| {
                        item["type"] == "message"
                            && item["role"] == "assistant"
                            && item["content"].as_array().is_some_and(|blocks| {
                                blocks.iter().any(|b| {
                                    b["type"] == "output_text" && nonempty_text(&b["text"])
                                })
                            })
                    })
                })
        }
        Protocol::AnthropicMessages => {
            value["type"] == "message"
                && value["role"] == "assistant"
                && value["content"].as_array().is_some_and(|blocks| {
                    blocks
                        .iter()
                        .any(|b| b["type"] == "text" && nonempty_text(&b["text"]))
                })
        }
    };
    if valid {
        Ok(())
    } else {
        Err("测试未通过：未收到所选协议的有效文本回复，请检查协议和模型；推理模型也可能耗尽本次测试的输出额度".into())
    }
}

/// Ignore empty or whitespace-only content when determining whether text inference succeeded.
fn nonempty_text(value: &Value) -> bool {
    value.as_str().is_some_and(|text| !text.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{body_json, header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    /// Keep test requests confined to a local mock server with a clearly synthetic API key.
    fn fixture(base: &str, protocol: Protocol) -> ModelConfig {
        ModelConfig {
            id: String::new(),
            name: "测试".into(),
            protocol,
            base_url: base.into(),
            model_id: "example-model".into(),
            api_key: "sk-TEST-ONLY".into(),
            supports_tool_call: true,
            supports_images: true,
            context_window: None,
            reasoning_levels: vec![],
        }
    }

    /// All supported protocols send the literal prompt, native auth headers and exactly one request.
    #[tokio::test]
    async fn probes_all_protocols_using_test_text_and_correct_auth() {
        for (protocol, endpoint, reply) in [
            (
                Protocol::OpenaiChat,
                "/proxy/v1/chat/completions",
                json!({"choices":[{"message":{"role":"assistant","content":"Hello"}}]}),
            ),
            (
                Protocol::OpenaiResponses,
                "/proxy/v1/responses",
                json!({"object":"response","status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Hello"}]}]}),
            ),
            (
                Protocol::AnthropicMessages,
                "/proxy/v1/messages",
                json!({"type":"message","role":"assistant","content":[{"type":"text","text":"Hello"}]}),
            ),
        ] {
            let server = MockServer::start().await;
            let model = fixture(&format!("{}{endpoint}", server.uri()), protocol);
            let expected = if protocol == Protocol::OpenaiResponses {
                json!({"model":"example-model","input":"test","max_output_tokens":256,"stream":false})
            } else {
                json!({"model":"example-model","messages":[{"role":"user","content":"test"}],"max_tokens":256,"stream":false})
            };
            let auth = if protocol == Protocol::AnthropicMessages {
                header("x-api-key", "sk-TEST-ONLY")
            } else {
                header("authorization", "Bearer sk-TEST-ONLY")
            };
            Mock::given(method("POST"))
                .and(path(endpoint))
                .and(auth)
                .and(body_json(expected))
                .respond_with(ResponseTemplate::new(200).set_body_json(reply))
                .expect(1)
                .mount(&server)
                .await;
            assert!(test_model(&model)
                .await
                .unwrap()
                .message
                .contains("测试通过"));
            let requests = server.received_requests().await.unwrap();
            assert_eq!(requests.len(), 1);
            assert!(!requests[0].headers.contains_key("cookie"));
            assert!(!requests[0].headers.contains_key("new-api-user"));
            if protocol == Protocol::AnthropicMessages {
                assert_eq!(requests[0].headers["anthropic-version"], "2023-06-01");
                assert!(!requests[0].headers.contains_key("authorization"));
            } else {
                assert!(!requests[0].headers.contains_key("x-api-key"));
            }
        }
    }

    /// Empty credentials are omitted for local servers, and invalid drafts never connect.
    #[tokio::test]
    async fn unauthenticated_local_endpoints_and_validation() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"choices":[{"message":{"content":"OK"}}]})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let mut model = fixture(&server.uri(), Protocol::OpenaiChat);
        model.api_key.clear();
        test_model(&model).await.unwrap();
        assert!(!server.received_requests().await.unwrap()[0]
            .headers
            .contains_key("authorization"));
        model.base_url.push_str("?api_key=secret");
        assert!(test_model(&model).await.is_err());
    }

    /// Error bodies, error envelopes, HTML, and empty protocol-shaped JSON never produce success.
    #[tokio::test]
    async fn rejects_failures_without_exposing_upstream_secrets() {
        for template in [
            ResponseTemplate::new(401).set_body_string("sk-TEST-ONLY"),
            ResponseTemplate::new(429).set_body_string("sk-TEST-ONLY"),
            ResponseTemplate::new(200).set_body_json(json!({"error":{"message":"sk-TEST-ONLY"}})),
            ResponseTemplate::new(200).set_body_string("<html>sk-TEST-ONLY</html>"),
            ResponseTemplate::new(200).set_body_json(json!({"choices":[{}]})),
            ResponseTemplate::new(200).set_body_string("x".repeat(MAX_RESPONSE_BYTES + 1)),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(template)
                .expect(1)
                .mount(&server)
                .await;
            let error = test_model(&fixture(&server.uri(), Protocol::OpenaiChat))
                .await
                .unwrap_err();
            assert!(!error.contains("sk-TEST-ONLY"));
            assert!(error.contains("测试未通过"));
        }
        assert!(validate_response(
            Protocol::OpenaiResponses,
            &json!({"object":"response","status":"completed","output":[]})
        )
        .is_err());
        assert!(validate_response(
            Protocol::AnthropicMessages,
            &json!({"type":"message","content":[]})
        )
        .is_err());
    }

    /// Redirects cannot forward a model key to a second server, and slow calls release the caller.
    #[tokio::test]
    async fn redirects_and_timeouts_are_bounded_without_retry() {
        let target = MockServer::start().await;
        let redirect = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(307)
                    .insert_header("location", format!("{}/target", target.uri())),
            )
            .expect(1)
            .mount(&redirect)
            .await;
        assert!(test_model(&fixture(&redirect.uri(), Protocol::OpenaiChat))
            .await
            .unwrap_err()
            .contains("重定向"));
        assert!(target.received_requests().await.unwrap().is_empty());
        let slow = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(1)))
            .mount(&slow)
            .await;
        assert!(test_with_timeout(
            &fixture(&slow.uri(), Protocol::OpenaiChat),
            Duration::from_millis(50)
        )
        .await
        .unwrap_err()
        .contains("超时"));
    }
}
