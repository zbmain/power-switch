use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use power_switch::{
    adapters,
    engine::Engine,
    files,
    import::{parse_link, share_link},
    model::{AgentKind, ModelConfig, Protocol},
    paths::{Paths, Settings},
};
use serde_json::{json, Value};
use std::fs;

/// Make a deterministic, non-secret model fixture using an RFC-reserved endpoint.
fn model(protocol: Protocol) -> ModelConfig {
    ModelConfig {
        id: String::new(),
        name: "测试模型 🟠".into(),
        protocol,
        base_url: "https://api.example.com/v1".into(),
        model_id: "example-model".into(),
        api_key: "sk-TEST-ONLY".into(),
        supports_tool_call: true,
        supports_images: false,
        context_window: Some(128000),
        reasoning_levels: vec![],
    }
}

/// Create a fully isolated home and app-data directory for real filesystem tests.
fn isolated() -> (tempfile::TempDir, Engine) {
    let temp = tempfile::tempdir().unwrap();
    let paths = Paths {
        home: temp.path().join("home"),
        data: temp.path().join("app"),
        workbuddy_env: None,
        codex_env: None,
    };
    fs::create_dir_all(&paths.home).unwrap();
    let engine = Engine::open(paths).unwrap();
    (temp, engine)
}

/// Re-imports keep stable IDs and key rotation updates only still-associated local endpoints.
#[test]
fn new_api_import_reuses_records_and_rotates_only_owned_models() {
    let (_temp, mut engine) = isolated();
    let mut chat = model(Protocol::OpenaiChat);
    chat.id = uuid::Uuid::new_v4().to_string();
    let mut claude = model(Protocol::AnthropicMessages);
    claude.model_id = "other-model".into();
    claude.id = uuid::Uuid::new_v4().to_string();
    claude.base_url = "https://api.example.com".into();
    let siblings = vec![chat.id.clone(), claude.id.clone()];
    engine.upsert_from_new_api(chat.clone(), &siblings).unwrap();
    engine
        .upsert_from_new_api(claude.clone(), &siblings)
        .unwrap();
    let unrelated = engine.upsert(model(Protocol::OpenaiChat)).unwrap();
    chat.api_key = "sk-ROTATED-TEST-ONLY".into();
    engine.upsert_from_new_api(chat.clone(), &siblings).unwrap();
    let data = engine.data().unwrap();
    assert_eq!(data.models.len(), 3);
    assert_eq!(
        data.models
            .iter()
            .find(|m| m.id == claude.id)
            .unwrap()
            .api_key,
        chat.api_key
    );
    assert_eq!(
        data.models
            .iter()
            .find(|m| m.id == unrelated.id)
            .unwrap()
            .api_key,
        "sk-TEST-ONLY"
    );
    claude.base_url = "https://another.example.com".into();
    claude.api_key = "sk-MANUAL-TEST-ONLY".into();
    engine.upsert(claude.clone()).unwrap();
    chat.api_key = "sk-SECOND-ROTATION-TEST".into();
    engine.upsert_from_new_api(chat, &siblings).unwrap();
    assert_eq!(
        engine
            .data()
            .unwrap()
            .models
            .iter()
            .find(|m| m.id == claude.id)
            .unwrap()
            .api_key,
        claude.api_key
    );
    assert!(fs::read_dir(&engine.paths.home).unwrap().next().is_none());
}

/// A user-modified linked endpoint must not be overwritten by a resumed New API import.
#[test]
fn new_api_reimport_preserves_manually_changed_endpoint() {
    let (_temp, mut engine) = isolated();
    let mut original = model(Protocol::OpenaiChat);
    original.id = uuid::Uuid::new_v4().to_string();
    engine.upsert_from_new_api(original.clone(), &[]).unwrap();
    let mut edited = original.clone();
    edited.base_url = "https://another.example.com/v1".into();
    engine.upsert(edited.clone()).unwrap();
    assert!(engine.upsert_from_new_api(original, &[]).is_err());
    assert_eq!(engine.data().unwrap().models[0].base_url, edited.base_url);
}

/// WorkBuddy upserts preserve existing capabilities not owned by this tool and other models.
#[test]
fn workbuddy_preserves_array_and_object_formats() {
    for root in [
        json!([{"id":"other","apiKey":"OTHER","unknown":42},{"id":"example-model","extension":"keep"}]),
        json!({"otherRoot":true,"models":[{"id":"other","apiKey":"OTHER"},{"id":"example-model","extension":"keep"}]}),
    ] {
        let result =
            adapters::workbuddy(Some(&root.to_string()), &model(Protocol::OpenaiChat)).unwrap();
        let output: Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(output.is_array(), root.is_array());
        let rows = output
            .as_array()
            .or_else(|| output["models"].as_array())
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["apiKey"], "OTHER");
        assert_eq!(rows[1]["extension"], "keep");
        assert_eq!(
            rows[1]["url"],
            "https://api.example.com/v1/chat/completions"
        );
        assert_eq!(rows[1]["apiKey"], "sk-TEST-ONLY");
        if root.is_object() {
            assert_eq!(output["otherRoot"], true);
        }
    }
}

/// Invalid JSON or structurally malformed rows must not silently reset a user's files.
#[test]
fn malformed_configs_are_rejected() {
    for text in [
        "{bad",
        "null",
        "{\"models\":1}",
        "[5]",
        "[{\"id\":\"example-model\"},{\"id\":\"example-model\"}]",
    ] {
        assert!(adapters::workbuddy(Some(text), &model(Protocol::OpenaiChat)).is_err());
    }
    assert!(
        adapters::claude(Some("{\"env\":false}"), &model(Protocol::AnthropicMessages)).is_err()
    );
}

/// Claude changes the relevant route and model aliases while keeping unrelated user settings.
#[test]
fn claude_merges_without_touching_permissions_or_hooks() {
    let root = json!({"model":"old","permissions":{"allow":["Read"]},"hooks":{"custom":true},"env":{"HTTP_PROXY":"http://localhost:1","ANTHROPIC_AUTH_TOKEN":"old-secret","ANTHROPIC_DEFAULT_OPUS_MODEL_NAME":"old-name"}});
    let result: Value = serde_json::from_slice(
        &adapters::claude(Some(&root.to_string()), &model(Protocol::AnthropicMessages)).unwrap(),
    )
    .unwrap();
    assert_eq!(result["permissions"], root["permissions"]);
    assert_eq!(result["hooks"], root["hooks"]);
    assert_eq!(result["env"]["HTTP_PROXY"], root["env"]["HTTP_PROXY"]);
    assert!(result["env"].get("ANTHROPIC_AUTH_TOKEN").is_none());
    assert!(result["env"]
        .get("ANTHROPIC_DEFAULT_OPUS_MODEL_NAME")
        .is_none());
    assert_eq!(
        result["env"]["ANTHROPIC_DEFAULT_HAIKU_MODEL"],
        "example-model"
    );
    assert_eq!(result["model"], "example-model");
}

/// A saved model alone never creates or writes the target file, and cancellation is final.
#[test]
fn save_and_cancel_have_no_agent_side_effects() {
    let (_temp, mut engine) = isolated();
    let model = engine.upsert(model(Protocol::OpenaiChat)).unwrap();
    let path = engine.paths.home.join(".workbuddy/models.json");
    assert!(!path.exists());
    let preview = engine
        .preview_apply(&model.id, &[AgentKind::Workbuddy])
        .unwrap();
    assert!(!path.exists());
    engine.cancel_preview(&preview.token);
    assert!(engine.apply(&preview.token).is_err());
    assert!(!path.exists());
}

/// WorkBuddy handoff belongs to a confirmed apply and cannot accompany another Agent or restore.
#[test]
fn workbuddy_selection_is_bound_to_successful_apply_only() {
    let (_temp, mut engine) = isolated();
    let model = engine.upsert(model(Protocol::OpenaiChat)).unwrap();
    assert!(engine
        .preview_apply_with_selection(&model.id, &[AgentKind::Claude], true)
        .is_err());
    let unchecked = engine
        .preview_apply_with_selection(&model.id, &[AgentKind::Workbuddy], false)
        .unwrap();
    assert!(!unchecked
        .notices
        .iter()
        .any(|notice| notice.contains("唤起")));
    engine.cancel_preview(&unchecked.token);
    let checked = engine
        .preview_apply_with_selection(&model.id, &[AgentKind::Workbuddy], true)
        .unwrap();
    assert!(checked.notices.iter().any(|notice| notice.contains("打开")));
    assert!(checked
        .notices
        .iter()
        .any(|notice| notice.contains("手动选择")));
    let result = engine.apply(&checked.token).unwrap();
    assert_eq!(result.workbuddy_model_id.as_deref(), Some("example-model"));
    assert!(engine.apply(&checked.token).is_err());
    let restore = engine.preview_restore(&result.backup_id).unwrap();
    let restored = engine.apply(&restore.token).unwrap();
    assert!(restored.workbuddy_model_id.is_none());
}

/// Apply and restore create independent recovery records and preserve original absence.
#[test]
fn apply_and_restore_missing_file() {
    let (_temp, mut engine) = isolated();
    let model = engine.upsert(model(Protocol::OpenaiChat)).unwrap();
    let preview = engine
        .preview_apply(&model.id, &[AgentKind::Workbuddy])
        .unwrap();
    let result = engine.apply(&preview.token).unwrap();
    assert!(result.paths[0].exists());
    assert_eq!(engine.list_backups().unwrap().len(), 1);
    let restore = engine.preview_restore(&result.backup_id).unwrap();
    let restored = engine.apply(&restore.token).unwrap();
    assert!(!result.paths[0].exists());
    assert_eq!(engine.list_backups().unwrap().len(), 2);
    let redo = engine.preview_restore(&restored.backup_id).unwrap();
    engine.apply(&redo.token).unwrap();
    assert!(result.paths[0].exists());
}

/// Deleting a backup cannot change the current Agent file or leave a usable stale restore token.
#[test]
fn deleting_backup_preserves_targets_and_invalidates_restore_preview() {
    let (_temp, mut engine) = isolated();
    let model = engine.upsert(model(Protocol::OpenaiChat)).unwrap();
    let preview = engine
        .preview_apply(&model.id, &[AgentKind::Workbuddy])
        .unwrap();
    let applied = engine.apply(&preview.token).unwrap();
    let current = fs::read(&applied.paths[0]).unwrap();
    let restore = engine.preview_restore(&applied.backup_id).unwrap();
    engine.delete_backup(&applied.backup_id).unwrap();
    assert!(engine.list_backups().unwrap().is_empty());
    assert_eq!(fs::read(&applied.paths[0]).unwrap(), current);
    assert!(engine.apply(&restore.token).is_err());
    assert!(engine.delete_backup(&applied.backup_id).is_err());
    assert!(engine.delete_backup("../models").is_err());
    assert!(engine.paths.data.join("models.json").is_file());
}

/// Omitted imported capabilities use the new default, while explicit existing choices survive.
#[test]
fn image_input_defaults_on_but_explicit_false_survives() {
    let mut value = serde_json::to_value(model(Protocol::OpenaiChat)).unwrap();
    value.as_object_mut().unwrap().remove("supportsImages");
    assert!(
        serde_json::from_value::<ModelConfig>(value.clone())
            .unwrap()
            .supports_images
    );
    value["supportsImages"] = json!(false);
    assert!(
        !serde_json::from_value::<ModelConfig>(value)
            .unwrap()
            .supports_images
    );
}

/// A modification after preview cannot be overwritten by confirming an old projection.
#[test]
fn external_changes_invalidate_apply() {
    let (_temp, mut engine) = isolated();
    let model = engine.upsert(model(Protocol::OpenaiChat)).unwrap();
    let preview = engine
        .preview_apply(&model.id, &[AgentKind::Workbuddy])
        .unwrap();
    let path = &preview.files[0].path;
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"[]").unwrap();
    assert!(engine
        .apply(&preview.token)
        .err()
        .unwrap()
        .contains("外部修改"));
    assert_eq!(fs::read(path).unwrap(), b"[]");
    assert!(engine.list_backups().unwrap().is_empty());
}

/// Edits to a saved model invalidate already opened configuration confirmations.
#[test]
fn model_edit_invalidates_pending_preview() {
    let (_temp, mut engine) = isolated();
    let mut model = engine.upsert(model(Protocol::OpenaiChat)).unwrap();
    let preview = engine
        .preview_apply(&model.id, &[AgentKind::Workbuddy])
        .unwrap();
    model.api_key = "NEW-TEST".into();
    engine.upsert(model).unwrap();
    assert!(engine.apply(&preview.token).is_err());
}

/// Backend validation enforces compatibility even if a modified UI sends an unsupported target.
#[test]
fn incompatible_targets_and_missing_context_are_rejected() {
    let (_temp, mut engine) = isolated();
    let m = engine.upsert(model(Protocol::OpenaiChat)).unwrap();
    assert!(engine.preview_apply(&m.id, &[AgentKind::Codex]).is_err());
    let mut m = model(Protocol::OpenaiResponses);
    m.context_window = None;
    let m = engine.upsert(m).unwrap();
    assert!(engine.preview_apply(&m.id, &[AgentKind::Codex]).is_err());
}

/// Codex writes a separate merged catalog, retains TOML comments and leaves auth untouched.
#[test]
fn codex_preserves_catalog_comments_auth_and_other_providers() {
    let (_temp, mut engine) = isolated();
    let dir = engine.paths.home.join(".codex");
    fs::create_dir_all(&dir).unwrap();
    let original="# keep this comment\nmodel = \"old\"\nmodel_catalog_json = \"original.json\"\n[model_providers.other]\nname = \"other\"\nbase_url = \"https://other.example.com\"\n[mcp_servers.example]\ncommand = \"test\"\n";
    fs::write(dir.join("config.toml"), original).unwrap();
    fs::write(dir.join("auth.json"), b"PRIVATE-UNCHANGED").unwrap();
    let catalog = json!({"extension":true,"models":[{"slug":"existing","customField":true}]});
    fs::write(dir.join("original.json"), catalog.to_string()).unwrap();
    let model = engine.upsert(model(Protocol::OpenaiResponses)).unwrap();
    let p = engine
        .preview_apply(&model.id, &[AgentKind::Codex])
        .unwrap();
    assert_eq!(p.files.len(), 2);
    assert!(!p.files.iter().any(|f| f.after.contains("sk-TEST-ONLY")));
    let result = engine.apply(&p.token).unwrap();
    let config = fs::read_to_string(dir.join("config.toml")).unwrap();
    assert!(config.contains("# keep this comment"));
    assert!(config.contains("[mcp_servers.example]"));
    assert!(config.contains("[model_providers.other]"));
    assert!(config.contains("experimental_bearer_token"));
    assert_eq!(
        fs::read(dir.join("auth.json")).unwrap(),
        b"PRIVATE-UNCHANGED"
    );
    assert_eq!(
        fs::read_to_string(dir.join("original.json")).unwrap(),
        catalog.to_string()
    );
    let output: Value =
        serde_json::from_slice(&fs::read(dir.join("power-switch-models.json")).unwrap()).unwrap();
    assert_eq!(output["models"].as_array().unwrap().len(), 2);
    assert_eq!(output["extension"], true);
    assert_eq!(output["models"][1]["context_window"], 128000);
    assert_eq!(output["models"][1]["input_modalities"], json!(["text"]));
    let restore = engine.preview_restore(&result.backup_id).unwrap();
    engine.apply(&restore.token).unwrap();
    assert_eq!(
        fs::read_to_string(dir.join("config.toml")).unwrap(),
        original
    );
    assert!(!dir.join("power-switch-models.json").exists());
}

/// An upstream catalog change also invalidates a projection that copied its contents.
#[test]
fn catalog_dependencies_are_fingerprinted() {
    let (_temp, mut engine) = isolated();
    let dir = engine.paths.home.join(".codex");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("config.toml"),
        "model_catalog_json = 'custom.json'",
    )
    .unwrap();
    fs::write(dir.join("custom.json"), "{\"models\":[]}").unwrap();
    let m = engine.upsert(model(Protocol::OpenaiResponses)).unwrap();
    let p = engine.preview_apply(&m.id, &[AgentKind::Codex]).unwrap();
    fs::write(dir.join("custom.json"), "{\"models\":[],\"new\":true}").unwrap();
    assert!(engine.apply(&p.token).is_err());
    assert!(!dir.join("power-switch-models.json").exists());
}

/// Path selection is exercised on each host in the macOS/Windows/Linux CI matrix.
#[test]
fn native_paths_and_environment_overrides() {
    let (_temp, mut e) = isolated();
    let settings = Settings::default();
    assert_eq!(
        e.paths.target(&settings, AgentKind::Workbuddy).unwrap(),
        e.paths.home.join(".workbuddy/models.json")
    );
    assert_eq!(
        e.paths.target(&settings, AgentKind::Claude).unwrap(),
        e.paths.home.join(".claude/settings.json")
    );
    e.paths.codex_env = Some(e.paths.home.join("custom-codex"));
    e.paths.workbuddy_env = Some(e.paths.home.join("custom-wb"));
    assert_eq!(
        e.paths.target(&settings, AgentKind::Codex).unwrap(),
        e.paths.home.join("custom-codex/config.toml")
    );
    assert_eq!(
        e.paths.target(&settings, AgentKind::Workbuddy).unwrap(),
        e.paths.home.join("custom-wb/models.json")
    );
    let override_settings = Settings {
        theme: "system".into(),
        codex_dir: Some(e.paths.home.join("override")),
        ..Settings::default()
    };
    assert_eq!(
        e.paths
            .target(&override_settings, AgentKind::Codex)
            .unwrap(),
        e.paths.home.join("override/config.toml")
    );
    assert!(e
        .settings(Settings {
            theme: "system".into(),
            workbuddy_path: Some("relative/models.json".into()),
            ..Settings::default()
        })
        .is_err());
}

/// UTF-8 model names and reserved URL characters survive share-link serialization.
#[test]
fn deep_link_round_trip_and_secret_policy() {
    let m = model(Protocol::OpenaiChat);
    let link = share_link(&m, false).unwrap();
    let parsed = parse_link(&link).unwrap();
    assert_eq!(parsed[0].name, m.name);
    assert!(parsed[0].api_key.is_empty());
    assert_eq!(
        parse_link(&share_link(&m, true).unwrap()).unwrap()[0].api_key,
        m.api_key
    );
    for link in [
        "power-switch://model/import?v=2&data=e30",
        "power-switch://model/import?v=1&data=***",
        "power-switch://model/import?v=1&data=e30&apply=true",
        "https://model/import?v=1&data=e30",
    ] {
        assert!(parse_link(link).is_err());
    }
    assert!(parse_link(&"x".repeat(65537)).is_err());
    let bytes=serde_json::to_vec(&json!({"models":[{"name":"bad","protocol":"openai-chat","modelId":"x","baseUrl":"https://example.com","filePath":"/tmp/not-allowed"}]})).unwrap();
    assert!(parse_link(&format!(
        "power-switch://model/import?v=1&data={}",
        URL_SAFE_NO_PAD.encode(bytes)
    ))
    .is_err());
}

/// Duplicate imports are skipped by default and optional updates retain an omitted API key.
#[test]
fn duplicate_import_requires_explicit_update() {
    let (_temp, mut e) = isolated();
    let m = e.upsert(model(Protocol::OpenaiChat)).unwrap();
    let mut updated = m.clone();
    updated.name = "Updated".into();
    let p = e
        .preview_import(&share_link(&updated, false).unwrap())
        .unwrap();
    assert!(p.rows[0].duplicate);
    assert!(!serde_json::to_string(&p).unwrap().contains(&m.api_key));
    assert_eq!(e.confirm_import(&p.token, &[]).unwrap(), 0);
    let p = e
        .preview_import(&share_link(&updated, false).unwrap())
        .unwrap();
    assert_eq!(e.confirm_import(&p.token, &[0]).unwrap(), 1);
    let data = e.data().unwrap();
    assert_eq!(data.models.len(), 1);
    assert_eq!(data.models[0].api_key, m.api_key);
    assert_eq!(data.models[0].name, "Updated");
}

/// Both JSON and TOML preview serialization remove credentials, including nested custom keys.
#[test]
fn secrets_are_redacted_from_previews() {
    let json=br#"{"env":{"ANTHROPIC_API_KEY":"SECRETA","HTTP_PROXY":"safe"},"nested":[{"refresh_token":"SECRETB"}]}"#;
    let toml=b"# SECRET-COMMENT\n[model_providers.x]\nexperimental_bearer_token = 'SECRETC'\nbase_url = 'https://safe.example.com'";
    for text in [json.as_slice(), toml.as_slice()] {
        let output = files::redacted(Some(text));
        assert!(!output.contains("SECRET"));
        assert!(output.contains("safe"));
    }
    assert_ne!(files::fingerprint(None), files::fingerprint(Some(b"")));
    let extensions = br#"{"headers":{"x-custom":"SECRET"},"env":{"AZURE_KEY":"SECRET"},"url":"https://name:SECRET@safe.example.com/v1?access=SECRET#SECRET"}"#;
    let output = files::redacted(Some(extensions));
    assert!(!output.contains("SECRET"));
    assert!(output.contains("safe.example.com/v1"));
}

/// Credential-bearing files are private on Unix and persist atomically through repeated writes.
#[test]
fn private_files_and_exclusive_store_lock() {
    let (_temp, mut e) = isolated();
    e.upsert(model(Protocol::OpenaiChat)).unwrap();
    assert!(Engine::open(e.paths.clone()).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(e.paths.data.join("models.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&e.paths.data).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}

/// Pasted endpoints normalize once, and credentials cannot be embedded in URLs.
#[test]
fn endpoint_normalization() {
    let mut m = model(Protocol::OpenaiChat);
    m.base_url = "https://api.example.com/v1/chat/completions/".into();
    m.validate().unwrap();
    assert_eq!(m.base_url, "https://api.example.com/v1");
    m.protocol = Protocol::AnthropicMessages;
    m.base_url = "https://api.example.com/coding/v1/messages".into();
    m.validate().unwrap();
    assert_eq!(m.base_url, "https://api.example.com/coding");
    m.base_url = "https://user:secret@example.com".into();
    assert!(m.validate().is_err());
    m.base_url = "file:///tmp/config".into();
    assert!(m.validate().is_err());
}

/// Serialized byte-array backups can exceed the target-file read limit and must remain writable.
#[test]
fn large_backup_can_be_completed_and_restored() {
    let (_temp, mut engine) = isolated();
    let target = engine.paths.home.join(".workbuddy/models.json");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    let original =
        serde_json::to_vec(&json!({"models":[], "notes":"a".repeat(3 * 1024 * 1024)})).unwrap();
    fs::write(&target, &original).unwrap();
    let model = engine.upsert(model(Protocol::OpenaiChat)).unwrap();
    let preview = engine
        .preview_apply(&model.id, &[AgentKind::Workbuddy])
        .unwrap();
    let result = engine.apply(&preview.token).unwrap();
    assert_eq!(engine.list_backups().unwrap()[0].status, "completed");
    let preview = engine.preview_restore(&result.backup_id).unwrap();
    engine.apply(&preview.token).unwrap();
    assert_eq!(fs::read(target).unwrap(), original);
}
