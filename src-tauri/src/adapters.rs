use crate::{
    files::Snapshot,
    model::{AgentKind, AppResult, ModelConfig},
    paths::{Paths, Settings},
};
use serde_json::{json, Value};
use std::path::Path;
use toml_edit::{value, DocumentMut, Item, Table};

#[derive(Clone)]
pub struct Change {
    pub before: Snapshot,
    pub after: Option<Vec<u8>>,
}

pub struct Projection {
    pub changes: Vec<Change>,
    pub dependencies: Vec<Snapshot>,
    pub notices: Vec<String>,
}

/// Project one model into native files without touching the filesystem.
pub fn project(
    paths: &Paths,
    settings: &Settings,
    model: &ModelConfig,
    agent: AgentKind,
) -> AppResult<Projection> {
    if !agent.supports(model.protocol) {
        return Err(format!("{} 不支持该模型协议", agent.label()));
    }
    let path = paths.target(settings, agent)?;
    let before = Snapshot::read(&path)?;
    match agent {
        AgentKind::Workbuddy => Ok(Projection {
            changes: vec![Change {
                after: Some(workbuddy(before.text()?, model)?),
                before,
            }],
            dependencies: vec![],
            notices: vec![
                "配置写入后，请在 WorkBuddy 新会话的模型列表中手动选择；现有任务不会切换。".into(),
            ],
        }),
        AgentKind::Claude => Ok(Projection {
            changes: vec![Change {
                after: Some(claude(before.text()?, model)?),
                before,
            }],
            dependencies: vec![],
            notices: vec![
                "请重新启动 Claude Code。项目设置、显式模型参数可能覆盖用户级配置。".into(),
            ],
        }),
        AgentKind::Codex => codex(before, model),
    }
}

/// Parse a JSON configuration without exposing parse context containing credentials.
fn parse_json(text: Option<&str>, default: Value) -> AppResult<Value> {
    text.map_or(Ok(default), |text| {
        serde_json::from_str(text)
            .map_err(|_| "现有 JSON 无效，请先修复配置；未写入任何内容".into())
    })
}

/// Encode JSON consistently while retaining unknown fields and their relative order.
fn encode(value: &Value) -> AppResult<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|_| "无法序列化配置")?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Upsert a WorkBuddy model by its native ID, preserving the existing container shape.
pub fn workbuddy(text: Option<&str>, model: &ModelConfig) -> AppResult<Vec<u8>> {
    let mut root = parse_json(text, json!([]))?;
    let models = if root.is_array() {
        root.as_array_mut().unwrap()
    } else {
        let obj = root.as_object_mut().ok_or("WorkBuddy 配置应为数组或对象")?;
        obj.entry("models")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("WorkBuddy models 字段必须为数组")?
    };
    if models
        .iter()
        .any(|v| !v.is_object() || !v.get("id").is_some_and(Value::is_string))
    {
        return Err("WorkBuddy 存在无效的模型条目，已停止写入".into());
    }
    if models.iter().filter(|v| v["id"] == model.model_id).count() > 1 {
        return Err("WorkBuddy 存在重复模型 ID，请先整理配置".into());
    }
    let index = models
        .iter()
        .position(|v| v["id"] == model.model_id)
        .unwrap_or_else(|| {
            models.push(json!({}));
            models.len() - 1
        });
    let entry = models[index].as_object_mut().unwrap();
    for (key, val) in [
        ("id", json!(model.model_id)),
        ("name", json!(model.name)),
        ("url", json!(format!("{}/chat/completions", model.base_url))),
        ("apiKey", json!(model.api_key)),
        ("supportsToolCall", json!(model.supports_tool_call)),
        ("supportsImages", json!(model.supports_images)),
        (
            "supportsReasoning",
            json!(!model.reasoning_levels.is_empty()),
        ),
        ("useCustomProtocol", json!(false)),
    ] {
        entry.insert(key.into(), val);
    }
    entry.entry("vendor").or_insert(json!("Custom"));
    if let Some(window) = model.context_window {
        entry.insert("maxInputTokens".into(), json!(window));
    }
    encode(&root)
}

/// Merge Claude's environment and remove credential/model aliases that could override it.
pub fn claude(text: Option<&str>, model: &ModelConfig) -> AppResult<Vec<u8>> {
    let mut root = parse_json(text, json!({}))?;
    let obj = root.as_object_mut().ok_or("Claude Code 配置必须为对象")?;
    // Top-level model also participates in precedence and is part of this explicit switch.
    if obj.contains_key("model") {
        obj.insert("model".into(), json!(model.model_id));
    }
    let env = obj
        .entry("env")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("Claude Code env 必须为对象")?;
    for key in [
        "ANTHROPIC_AUTH_TOKEN",
        "CLAUDE_CODE_OAUTH_TOKEN",
        "ANTHROPIC_SMALL_FAST_MODEL",
    ] {
        env.remove(key);
    }
    let stale_names: Vec<_> = env
        .keys()
        .filter(|k| k.starts_with("ANTHROPIC_DEFAULT_") && k.ends_with("_MODEL_NAME"))
        .cloned()
        .collect();
    for key in stale_names {
        env.remove(&key);
    }
    env.insert("ANTHROPIC_BASE_URL".into(), json!(model.base_url));
    env.insert("ANTHROPIC_API_KEY".into(), json!(model.api_key));
    for key in [
        "ANTHROPIC_MODEL",
        "ANTHROPIC_DEFAULT_FABLE_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "CLAUDE_CODE_SUBAGENT_MODEL",
    ] {
        env.insert(key.into(), json!(model.model_id));
    }
    encode(&root)
}

/// Generate a schema-compatible Codex model record from explicitly supplied capabilities.
pub fn catalog_entry(model: &ModelConfig) -> AppResult<Value> {
    let window = model
        .context_window
        .ok_or("应用到 Codex 前，请在高级设置填写模型上下文窗口")?;
    let levels: Vec<_> = model
        .reasoning_levels
        .iter()
        .map(|s| json!({"effort":s,"description":s}))
        .collect();
    Ok(json!({
        "slug":model.model_id, "display_name":model.name, "description":model.name,
        "default_reasoning_level":model.reasoning_levels.first(), "supported_reasoning_levels":levels,
        "shell_type":"shell_command", "visibility":"list", "supported_in_api":true, "priority":0,
        "base_instructions":"You are Codex, a coding agent. You and the user share the same workspace and collaborate to achieve the user's goals.",
        "supports_reasoning_summaries":!model.reasoning_levels.is_empty(), "default_reasoning_summary":"none", "support_verbosity":false,
        "truncation_policy":{"mode":"bytes","limit":10000}, "context_window":window, "max_context_window":window,
        "effective_context_window_percent":95, "supports_parallel_tool_calls":false,
        "experimental_supported_tools":[], "input_modalities":if model.supports_images {vec!["text","image"]} else {vec!["text"]}
    }))
}

/// Build the provider and a merged, separately owned catalog as a single transaction.
fn codex(before: Snapshot, model: &ModelConfig) -> AppResult<Projection> {
    let mut doc = before
        .text()?
        .unwrap_or("")
        .parse::<DocumentMut>()
        .map_err(|_| "现有 Codex TOML 无效，请先修复配置")?;
    let dir = before.path.parent().ok_or("Codex 配置目录无效")?;
    let catalog_path = dir.join("power-switch-models.json");
    let catalog_before = Snapshot::read(&catalog_path)?;
    let mut dependencies = vec![];
    let mut catalog = json!({"models":[]});
    if let Some(item) = doc.get("model_catalog_json") {
        let raw = item
            .as_str()
            .ok_or("Codex model_catalog_json 必须为字符串")?;
        let source = resolve_catalog_path(dir, raw)?;
        if source != catalog_path {
            let snapshot = Snapshot::read(&source)?;
            catalog = parse_json(
                Some(snapshot.text()?.ok_or("Codex 已配置的模型目录不存在")?),
                json!({"models":[]}),
            )?;
            dependencies.push(snapshot);
        } else if let Some(text) = catalog_before.text()? {
            catalog = parse_json(Some(text), json!({"models":[]}))?;
        }
    } else if let Some(text) = catalog_before.text()? {
        catalog = parse_json(Some(text), json!({"models":[]}))?;
    }
    let rows = catalog
        .as_object_mut()
        .and_then(|o| o.get_mut("models"))
        .and_then(Value::as_array_mut)
        .ok_or("Codex 模型目录必须包含 models 数组")?;
    if rows
        .iter()
        .any(|r| !r.is_object() || !r.get("slug").is_some_and(Value::is_string))
    {
        return Err("Codex 模型目录中存在无效条目".into());
    }
    let new_entry = catalog_entry(model)?;
    if let Some(entry) = rows.iter_mut().find(|r| r["slug"] == model.model_id) {
        let obj = entry.as_object_mut().unwrap();
        for (k, v) in new_entry.as_object().unwrap() {
            obj.insert(k.clone(), v.clone());
        }
    } else {
        rows.push(new_entry);
    }
    let provider = format!("power_switch_{}", model.id.replace('-', ""));
    doc["model"] = value(&model.model_id);
    doc["model_provider"] = value(&provider);
    doc["model_catalog_json"] = value(catalog_path.to_string_lossy().as_ref());
    if !doc.contains_key("model_providers") {
        doc["model_providers"] = Item::Table(Table::new());
    }
    let providers = doc["model_providers"]
        .as_table_mut()
        .ok_or("Codex model_providers 必须为表")?;
    let mut table = Table::new();
    table["name"] = value(&model.name);
    table["base_url"] = value(&model.base_url);
    table["wire_api"] = value("responses");
    if !model.api_key.is_empty() {
        table["experimental_bearer_token"] = value(&model.api_key);
    }
    providers[&provider] = Item::Table(table);
    // A prior provider's global capability overrides must not leak into this model.
    for key in [
        "model_reasoning_effort",
        "model_reasoning_summary",
        "model_supports_reasoning_summaries",
        "model_verbosity",
        "model_context_window",
        "model_auto_compact_token_limit",
    ] {
        doc.remove(key);
    }
    if let Some(level) = model.reasoning_levels.first() {
        doc["model_reasoning_effort"] = value(level);
    }
    Ok(Projection {
        changes: vec![
            Change {
                before: catalog_before,
                after: Some(encode(&catalog)?),
            },
            Change {
                before,
                after: Some(doc.to_string().into_bytes()),
            },
        ],
        dependencies,
        notices: vec![
            "Codex CLI 与桌面端共用配置。请完全退出后重启桌面端，并新建会话。".into(),
            "显式 profile、启动参数或管理策略可能覆盖此用户级设置。auth.json 保持不变。".into(),
        ],
    })
}

/// Resolve existing Codex catalog pointers; relative paths are relative to CODEX_HOME.
fn resolve_catalog_path(dir: &Path, raw: &str) -> AppResult<std::path::PathBuf> {
    if let Some(rest) = raw.strip_prefix("~/") {
        return Ok(dirs::home_dir()
            .ok_or("无法展开模型目录的用户路径")?
            .join(rest));
    }
    let path = Path::new(raw);
    Ok(if path.is_absolute() {
        path.to_owned()
    } else {
        dir.join(path)
    })
}
