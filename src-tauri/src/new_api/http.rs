use super::{Error, Result};
use reqwest::{
    cookie::{CookieStore, Jar},
    redirect::Policy,
    Client, Method, RequestBuilder,
};
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use url::Url;

const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone)]
pub struct ApiClient {
    pub base: String,
    pub client: Client,
    jar: Arc<Jar>,
}

impl ApiClient {
    /// Create one isolated, bounded HTTP session; never forward credentials through redirects.
    pub fn new(base: &str) -> Result<Self> {
        let jar = Arc::new(Jar::default());
        let client = Client::builder()
            .cookie_provider(jar.clone())
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .user_agent("power-switch/0.1.1")
            .build()
            .map_err(|_| Error::new("network", "无法初始化 HTTPS 客户端"))?;
        Ok(Self {
            base: base.into(),
            client,
            jar,
        })
    }

    /// Restore only the New API session cookie, scoped to this exact HTTPS instance.
    pub fn restore_cookie(&self, cookie: &str) -> Result<()> {
        if !cookie.starts_with("session=") || cookie.contains(['\r', '\n', ';']) {
            return Err(Error::new(
                "keychain",
                "已保存的登录会话格式无效，请重新登录",
            ));
        }
        let url = Url::parse(&self.base).map_err(|_| Error::new("url", "实例地址无效"))?;
        self.jar
            .add_cookie_str(&format!("{cookie}; Path=/; Secure; HttpOnly"), &url);
        Ok(())
    }

    /// Export only the instance's session cookie to the system credential store.
    pub fn session_cookie(&self) -> Result<String> {
        let url = Url::parse(&self.base).map_err(|_| Error::new("url", "实例地址无效"))?;
        self.jar
            .cookies(&url)
            .and_then(|h| h.to_str().ok().map(str::to_owned))
            .and_then(|s| {
                s.split(';')
                    .map(str::trim)
                    .find(|s| s.starts_with("session="))
                    .map(str::to_owned)
            })
            .ok_or_else(|| Error::new("login_required", "服务端未返回登录会话，请重新登录"))
    }

    /// Attach the legacy user header only to this instance's management endpoints.
    pub fn management(&self, method: Method, path: &str, user_id: Option<i64>) -> RequestBuilder {
        let request = self.client.request(method, format!("{}{path}", self.base));
        match user_id {
            Some(id) => request.header("New-Api-User", id),
            None => request,
        }
    }

    /// Validate both HTTP status and New API's success envelope without exposing raw responses.
    pub async fn api(&self, request: RequestBuilder) -> Result<Value> {
        let value = read_json(request).await?;
        if value.get("success").and_then(Value::as_bool) != Some(true) {
            return Err(Error::new(
                "api_rejected",
                "New API 未接受请求，请检查登录状态、账号权限和令牌设置",
            ));
        }
        Ok(value)
    }
}

/// Parse a size-bounded response; raw URLs, bodies and transport errors may contain credentials.
pub async fn read_json(request: RequestBuilder) -> Result<Value> {
    let mut response = request
        .send()
        .await
        .map_err(|_| Error::new("network", "连接失败或请求超时，请检查网络后重试"))?;
    let status = response.status();
    if !status.is_success() {
        let (code, message) = match status.as_u16() {
            401 => ("login_required", "登录已失效，请重新登录"),
            403 => (
                "forbidden",
                "请求被拒绝，请检查账号权限、模型权限或 IP 限制",
            ),
            429 => ("rate_limit", "请求过于频繁，请稍后重试"),
            300..=399 => ("redirect", "接口发生重定向，请填写最终的 HTTPS 实例地址"),
            _ => ("http", "服务端请求失败，请稍后重试或检查服务状态"),
        };
        return Err(Error::new(code, message));
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_RESPONSE_BYTES as u64)
    {
        return Err(Error::new("response", "服务端响应超过大小限制"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| Error::new("network", "响应读取中断，请重试"))?
    {
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(Error::new("response", "服务端响应超过大小限制"));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| Error::new("response", "服务端返回了非预期格式，请检查实例版本和地址"))
}
