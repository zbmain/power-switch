//! Opt-in acceptance harness. Never logs credentials or changes targets without an explicit mode.
use power_switch::{
    engine::Engine,
    model::{AgentKind, ModelConfig, Protocol},
    paths::Paths,
};
use std::{env, fs, path::PathBuf};

/// Exercise the same guarded core used by the desktop application with isolated app data.
fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let mode = args
        .get(1)
        .ok_or("usage: acceptance codex|workbuddy|restore <absolute-data-dir> [backup-id]")?;
    let root = PathBuf::from(args.get(2).ok_or("missing acceptance data directory")?);
    power_switch::paths::validate_absolute(&root)?;
    let mut paths = Paths::discover()?;
    paths.data = root.join("power-switch");
    if mode == "codex" {
        paths.home = root.clone();
        paths.codex_env = None;
        paths.workbuddy_env = None;
    }
    let mut engine = Engine::open(paths)?;
    if mode == "restore" {
        let preview = engine.preview_restore(args.get(3).ok_or("missing backup id")?)?;
        let result = engine.apply(&preview.token)?;
        println!("restored; safety backup {}", result.backup_id);
        return Ok(());
    }
    let (model, agent) = if mode == "codex" {
        (
            ModelConfig {
                id: String::new(),
                name: "Power Switch Acceptance".into(),
                protocol: Protocol::OpenaiResponses,
                base_url: "http://127.0.0.1:9/v1".into(),
                model_id: "power-switch-acceptance".into(),
                api_key: "test-only-not-a-real-key".into(),
                supports_tool_call: true,
                supports_images: false,
                context_window: Some(128000),
                reasoning_levels: vec![],
            },
            AgentKind::Codex,
        )
    } else if mode == "workbuddy" {
        let data = engine.data()?;
        let path = &data
            .agents
            .iter()
            .find(|a| a.agent == AgentKind::Workbuddy)
            .ok_or("missing WorkBuddy path")?
            .path;
        let value: serde_json::Value =
            serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|_| "invalid WorkBuddy JSON")?;
        let rows = value
            .as_array()
            .or_else(|| value.get("models").and_then(|v| v.as_array()))
            .ok_or("invalid model list")?;
        let existing = rows
            .iter()
            .find(|m| {
                m["apiKey"].as_str().is_some_and(|s| !s.is_empty())
                    && m["url"].as_str().is_some_and(|s| !s.is_empty())
                    && (m["useCustomProtocol"].as_bool() != Some(true)
                        || m["url"]
                            .as_str()
                            .is_some_and(|s| s.ends_with("/chat/completions")))
            })
            .ok_or("no existing complete Chat model")?;
        let get = |key: &str| {
            existing[key]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("missing {key}"))
        };
        (
            ModelConfig {
                id: String::new(),
                name: "Power Switch 验收".into(),
                protocol: Protocol::OpenaiChat,
                base_url: get("url")?,
                model_id: get("id")?,
                api_key: get("apiKey")?,
                supports_tool_call: existing["supportsToolCall"].as_bool().unwrap_or(true),
                supports_images: existing["supportsImages"].as_bool().unwrap_or(false),
                context_window: None,
                reasoning_levels: vec![],
            },
            AgentKind::Workbuddy,
        )
    } else {
        return Err("unknown acceptance mode".into());
    };
    let model = engine.upsert(model)?;
    let preview = engine.preview_apply(&model.id, &[agent])?;
    let result = engine.apply(&preview.token)?;
    println!("applied; backup {}", result.backup_id);
    println!("target {}", result.paths[0].display());
    Ok(())
}
