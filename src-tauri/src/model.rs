use serde::{Deserialize, Serialize};
use url::Url;

pub type AppResult<T> = Result<T, String>;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Protocol {
    #[serde(rename = "openai-chat")]
    OpenaiChat,
    #[serde(rename = "openai-responses")]
    OpenaiResponses,
    #[serde(rename = "anthropic-messages")]
    AnthropicMessages,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Workbuddy,
    Claude,
    Codex,
}

impl AgentKind {
    /// Return the stable Chinese label used by confirmations and audit records.
    pub fn label(self) -> &'static str {
        match self {
            Self::Workbuddy => "WorkBuddy",
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
        }
    }

    /// Only offer a native protocol supported by the target application.
    pub fn supports(self, protocol: Protocol) -> bool {
        matches!(
            (self, protocol),
            (Self::Workbuddy, Protocol::OpenaiChat)
                | (Self::Claude, Protocol::AnthropicMessages)
                | (Self::Codex, Protocol::OpenaiResponses)
        )
    }
}

/// Default tool capability for models intended for agent use.
fn tools_enabled() -> bool {
    true
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelConfig {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub protocol: Protocol,
    pub base_url: String,
    pub model_id: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "tools_enabled")]
    pub supports_tool_call: bool,
    #[serde(default = "tools_enabled")]
    pub supports_images: bool,
    #[serde(default)]
    pub context_window: Option<u64>,
    #[serde(default)]
    pub reasoning_levels: Vec<String>,
}

impl ModelConfig {
    /// Validate an editable model without requiring credentials for local endpoints.
    pub fn validate(&mut self) -> AppResult<()> {
        self.name = self.name.trim().to_string();
        self.model_id = self.model_id.trim().to_string();
        if self.name.is_empty() || self.model_id.is_empty() {
            return Err("请填写名称和模型 ID".into());
        }
        if self.name.len() > 256 || self.model_id.len() > 512 || self.api_key.len() > 16384 {
            return Err("模型字段超出长度限制".into());
        }
        if !self.id.is_empty() && uuid::Uuid::parse_str(&self.id).is_err() {
            return Err("无效的模型内部 ID".into());
        }
        self.base_url = normalize_url(&self.base_url, self.protocol)?;
        if let Some(window) = self.context_window {
            if !(1024..=100_000_000).contains(&window) {
                return Err("上下文窗口应为 1024 至 100000000 的整数".into());
            }
        }
        for level in &self.reasoning_levels {
            if !["none", "minimal", "low", "medium", "high", "xhigh", "max"]
                .contains(&level.as_str())
            {
                return Err("不支持的推理档位".into());
            }
        }
        self.reasoning_levels.sort_by_key(|s| {
            ["none", "minimal", "low", "medium", "high", "xhigh", "max"]
                .iter()
                .position(|v| v == s)
                .unwrap_or(0)
        });
        self.reasoning_levels.dedup();
        Ok(())
    }

    /// Build an identity that ignores display names and secrets for import deduplication.
    pub fn same_endpoint(&self, other: &Self) -> bool {
        self.protocol == other.protocol
            && self.base_url == other.base_url
            && self.model_id == other.model_id
    }
}

/// Normalize an API base URL, accepting a pasted standard endpoint as a convenience.
pub fn normalize_url(raw: &str, protocol: Protocol) -> AppResult<String> {
    let mut url = Url::parse(raw.trim()).map_err(|_| "请输入有效的 API 地址")?;
    if !["http", "https"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("API 地址必须为 HTTP/HTTPS，且不能包含账号、查询参数或片段".into());
    }
    let mut path = url.path().trim_end_matches('/').to_owned();
    let suffix = match protocol {
        Protocol::OpenaiChat => "/chat/completions",
        Protocol::OpenaiResponses => "/responses",
        Protocol::AnthropicMessages => "/messages",
    };
    if path.ends_with(suffix) {
        path.truncate(path.len() - suffix.len());
    }
    // Anthropic SDKs add /v1/messages themselves, unlike OpenAI's base URL convention.
    if protocol == Protocol::AnthropicMessages && path.ends_with("/v1") {
        path.truncate(path.len() - 3);
    }
    url.set_path(&path);
    Ok(url.as_str().trim_end_matches('/').to_string())
}
