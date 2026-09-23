//! Read model identifiers from the configured provider without persisting credentials.
use crate::model::{normalize_url, AppResult, Protocol};
use reqwest::{redirect::Policy, Client};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeSet, time::Duration};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConnection {
    pub protocol: Protocol,
    pub base_url: String,
    pub api_key: String,
}

/// Bound the entire paginated catalog operation and never expose transport URLs or response bodies.
pub async fn list_models(connection: ModelConnection) -> AppResult<Vec<String>> {
    tokio::time::timeout(Duration::from_secs(30), fetch_catalog(connection))
        .await
        .map_err(|_| "获取模型清单超时，请稍后重试".to_string())?
}

/// Fetch standard models endpoints with protocol-specific authentication and bounded pagination.
async fn fetch_catalog(connection: ModelConnection) -> AppResult<Vec<String>> {
    if connection.api_key.trim().is_empty() {
        return Err("请先填写 API Key".into());
    }
    let base = normalize_url(&connection.base_url, connection.protocol)?;
    let anthropic = connection.protocol == Protocol::AnthropicMessages;
    let suffix = if anthropic { "/v1/models" } else { "/models" };
    let client = Client::builder()
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| "无法初始化模型清单连接")?;
    let mut ids = BTreeSet::new();
    let mut cursors = BTreeSet::new();
    let mut cursor = None;
    let mut total = 0;
    for _ in 0..100 {
        let mut request = client.get(format!("{base}{suffix}"));
        if anthropic {
            request = request.header("anthropic-version", "2023-06-01");
            if !connection.api_key.is_empty() {
                request = request.header("x-api-key", &connection.api_key);
            }
            if let Some(ref id) = cursor {
                request = request.query(&[("after_id", id)]);
            }
        } else if !connection.api_key.is_empty() {
            request = request.bearer_auth(&connection.api_key);
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| "无法获取模型清单，请检查地址、网络和 API Key")?;
        if !response.status().is_success() {
            return Err(format!(
                "获取模型清单失败（HTTP {}），请检查地址、协议和 API Key",
                response.status().as_u16()
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| "读取模型清单失败")? {
            total += chunk.len();
            if total > 1024 * 1024 {
                return Err("模型清单超过 1 MiB，无法加载".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| "模型清单不是有效 JSON")?;
        let data = value
            .get("data")
            .and_then(Value::as_array)
            .filter(|_| value.get("error").is_none())
            .ok_or("平台未返回有效模型清单")?;
        for model in data {
            let id = model
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.trim().is_empty())
                .ok_or("模型清单包含无效模型 ID")?;
            ids.insert(id.to_string());
        }
        if !anthropic || value.get("has_more") != Some(&Value::Bool(true)) {
            return Ok(ids.into_iter().collect());
        }
        let next = value
            .get("last_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or("模型清单分页信息无效")?
            .to_string();
        if !cursors.insert(next.clone()) {
            return Err("模型清单分页重复，无法加载".into());
        }
        cursor = Some(next);
    }
    Err("模型清单页数过多，无法加载".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::{
        matchers::{header, method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    /// Reject an empty key before sending any catalog request.
    #[tokio::test]
    async fn requires_api_key() {
        let server = MockServer::start().await;
        let result = list_models(ModelConnection {
            protocol: Protocol::OpenaiChat,
            base_url: server.uri(),
            api_key: String::new(),
        })
        .await;
        assert_eq!(result.unwrap_err(), "请先填写 API Key");
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    /// Verify both authentication schemes, normalized endpoints and Anthropic pagination.
    #[tokio::test]
    async fn retrieves_provider_catalogs() {
        for protocol in [
            Protocol::OpenaiChat,
            Protocol::OpenaiResponses,
            Protocol::AnthropicMessages,
        ] {
            let server = MockServer::start().await;
            let anthropic = protocol == Protocol::AnthropicMessages;
            let auth = if anthropic {
                header("x-api-key", "secret")
            } else {
                header("authorization", "Bearer secret")
            };
            Mock::given(method("GET")).and(path("/v1/models")).and(auth)
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[{"id":"b"},{"id":"a"},{"id":"a"}],"has_more":anthropic,"last_id":"b"})))
                .mount(&server).await;
            if anthropic {
                Mock::given(query_param("after_id", "b"))
                    .and(header("anthropic-version", "2023-06-01"))
                    .respond_with(
                        ResponseTemplate::new(200)
                            .set_body_json(json!({"data":[{"id":"c"}],"has_more":false})),
                    )
                    .with_priority(1)
                    .mount(&server)
                    .await;
            }
            let ids = list_models(ModelConnection {
                protocol,
                base_url: format!("{}/v1", server.uri()),
                api_key: "secret".into(),
            })
            .await
            .unwrap();
            assert_eq!(
                ids,
                if anthropic {
                    vec!["a", "b", "c"]
                } else {
                    vec!["a", "b"]
                }
            );
        }
    }

    /// Reject malformed responses and authentication failures without reflecting server secrets.
    #[tokio::test]
    async fn rejects_invalid_catalogs() {
        for (status, body) in [
            (401, json!({"error":"secret"})),
            (200, json!({"data":[{}]})),
            (200, json!({"unexpected":"secret"})),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(status).set_body_json(body))
                .mount(&server)
                .await;
            let error = list_models(ModelConnection {
                protocol: Protocol::OpenaiChat,
                base_url: server.uri(),
                api_key: "secret".into(),
            })
            .await
            .unwrap_err();
            assert!(!error.contains("secret"));
        }
    }
}
