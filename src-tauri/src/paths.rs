use crate::model::{AgentKind, AppResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    #[serde(default = "system_theme")]
    pub theme: String,
    #[serde(default)]
    pub workbuddy_path: Option<PathBuf>,
    #[serde(default)]
    pub claude_path: Option<PathBuf>,
    #[serde(default)]
    pub codex_dir: Option<PathBuf>,
}

/// Follow the OS appearance until the user chooses a theme.
fn system_theme() -> String {
    "system".into()
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPath {
    pub agent: AgentKind,
    pub path: PathBuf,
    pub exists: bool,
}

#[derive(Clone)]
pub struct Paths {
    pub home: PathBuf,
    pub data: PathBuf,
    pub workbuddy_env: Option<PathBuf>,
    pub codex_env: Option<PathBuf>,
}

impl Paths {
    /// Resolve native user directories without shell expansion or cwd fallbacks.
    pub fn discover() -> AppResult<Self> {
        Ok(Self {
            home: dirs::home_dir().ok_or("无法获取系统用户目录")?,
            data: dirs::data_local_dir()
                .ok_or("无法获取系统应用数据目录")?
                .join("power-switch"),
            workbuddy_env: nonempty_env("WORKBUDDY_DATA_DIR"),
            codex_env: nonempty_env("CODEX_HOME"),
        })
    }

    /// Resolve one target with explicit user overrides taking precedence over environment values.
    pub fn target(&self, settings: &Settings, agent: AgentKind) -> AppResult<PathBuf> {
        let path = match agent {
            AgentKind::Workbuddy => settings.workbuddy_path.clone().unwrap_or_else(|| {
                self.workbuddy_env
                    .clone()
                    .unwrap_or_else(|| self.home.join(".workbuddy"))
                    .join("models.json")
            }),
            AgentKind::Claude => settings
                .claude_path
                .clone()
                .unwrap_or_else(|| self.home.join(".claude/settings.json")),
            AgentKind::Codex => settings
                .codex_dir
                .clone()
                .or_else(|| self.codex_env.clone())
                .unwrap_or_else(|| self.home.join(".codex"))
                .join("config.toml"),
        };
        validate_absolute(&path)?;
        if path.starts_with(&self.data) {
            return Err("Agent 配置不能指向 power-switch 数据目录".into());
        }
        Ok(path)
    }

    /// Describe resolved files for the settings page without reading their contents.
    pub fn describe(&self, settings: &Settings) -> AppResult<Vec<AgentPath>> {
        [AgentKind::Workbuddy, AgentKind::Claude, AgentKind::Codex]
            .iter()
            .map(|&agent| {
                let path = self.target(settings, agent)?;
                Ok(AgentPath {
                    agent,
                    exists: path.exists(),
                    path,
                })
            })
            .collect()
    }
}

/// Ignore empty environment overrides while preserving OS-native path characters.
fn nonempty_env(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

/// Reject ambiguous relative overrides before any files can be created.
pub fn validate_absolute(path: &Path) -> AppResult<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|p| matches!(p, std::path::Component::ParentDir))
    {
        return Err("配置路径必须为不含 .. 的绝对路径".into());
    }
    Ok(())
}
