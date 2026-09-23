use crate::{
    engine::{AppData, ApplyPreview, ApplyResult, Engine, ImportPreview},
    model::{AgentKind, AppResult, ModelConfig},
    paths::{Paths, Settings},
};
use std::sync::Mutex;
use tauri::{Manager, State};
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_opener::OpenerExt;

type Shared<'a> = State<'a, Mutex<Engine>>;

/// Serialize desktop operations through a single engine and propagate safe errors.
fn with_engine<T>(state: Shared<'_>, f: impl FnOnce(&mut Engine) -> AppResult<T>) -> AppResult<T> {
    let mut engine = state.lock().map_err(|_| "应用数据锁异常，请重启")?;
    f(&mut engine)
}

/// Load models, settings, paths and redacted backup summaries.
#[tauri::command]
fn get_data(state: Shared<'_>) -> AppResult<AppData> {
    with_engine(state, |e| e.data())
}
/// Save one validated model.
#[tauri::command]
fn save_model(state: Shared<'_>, model: ModelConfig) -> AppResult<ModelConfig> {
    with_engine(state, |e| e.upsert(model))
}
/// Test a draft without holding the model-store lock or writing any Agent configuration.
#[tauri::command]
async fn test_model(model: ModelConfig) -> AppResult<crate::model_probe::ModelTestResult> {
    crate::model_probe::test_model(&model).await
}

/// Fetch selectable model IDs from the draft connection without saving it.
#[tauri::command]
async fn list_models(connection: crate::model_catalog::ModelConnection) -> AppResult<Vec<String>> {
    crate::model_catalog::list_models(connection).await
}
/// Delete a library record without changing an Agent's files.
#[tauri::command]
fn delete_model(state: Shared<'_>, id: String) -> AppResult<()> {
    with_engine(state, |e| e.delete(&id))
}
/// Update theme and explicit config locations.
#[tauri::command]
fn save_settings(state: Shared<'_>, settings: Settings) -> AppResult<()> {
    with_engine(state, |e| e.settings(settings))
}
/// Prepare a model application for user review.
#[tauri::command]
fn preview_apply(
    state: Shared<'_>,
    id: String,
    agents: Vec<AgentKind>,
    select_workbuddy_model: bool,
) -> AppResult<ApplyPreview> {
    with_engine(state, |e| {
        e.preview_apply_with_selection(&id, &agents, select_workbuddy_model)
    })
}
/// Commit the confirmed preview, then allow WorkBuddy to reload its model list before opening home.
#[tauri::command]
async fn apply_preview(
    app: tauri::AppHandle,
    state: Shared<'_>,
    token: String,
) -> AppResult<ApplyResult> {
    let mut result = with_engine(state, |e| e.apply(&token))?;
    // The model-file watcher reloads asynchronously; wait before opening the new-task page.
    if result.workbuddy_model_id.is_some() {
        tokio::time::sleep(std::time::Duration::from_millis(1_200)).await;
    }
    crate::workbuddy_link::finish_selection(&mut result, |url| {
        app.opener()
            .open_url(url, None::<&str>)
            .map_err(|_| "无法唤起 WorkBuddy".into())
    });
    Ok(result)
}
/// Release canceled preview contents.
#[tauri::command]
fn cancel_preview(state: Shared<'_>, token: String) -> AppResult<()> {
    with_engine(state, |e| {
        e.cancel_preview(&token);
        Ok(())
    })
}
/// Prepare an existing backup for restoration.
#[tauri::command]
fn preview_restore(state: Shared<'_>, id: String) -> AppResult<ApplyPreview> {
    with_engine(state, |e| e.preview_restore(&id))
}
/// Remove one confirmed backup without affecting any configured Agent.
#[tauri::command]
fn delete_backup(state: Shared<'_>, id: String) -> AppResult<()> {
    with_engine(state, |e| e.delete_backup(&id))
}
/// Decode an incoming model link without applying it.
#[tauri::command]
fn preview_import(state: Shared<'_>, link: String) -> AppResult<ImportPreview> {
    with_engine(state, |e| e.preview_import(&link))
}
/// Import selected updates after review.
#[tauri::command]
fn confirm_import(state: Shared<'_>, token: String, updates: Vec<usize>) -> AppResult<usize> {
    with_engine(state, |e| e.confirm_import(&token, &updates))
}
/// Generate a share link with opt-in credential inclusion.
#[tauri::command]
fn share_model(state: Shared<'_>, id: String, include_secret: bool) -> AppResult<String> {
    with_engine(state, |e| e.share(&id, include_secret))
}

/// Configure single-instance delivery before registering the deep-link plugin.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let paths = Paths::discover().map_err(std::io::Error::other)?;
            crate::new_api_desktop::setup(app.handle(), paths.data.clone());
            let engine = Engine::open(paths).map_err(std::io::Error::other)?;
            app.manage(Mutex::new(engine));
            app.manage(Mutex::new(crate::skills::SkillManager::default()));
            #[cfg(any(target_os = "linux", target_os = "windows"))]
            app.deep_link().register_all()?;
            app.deep_link().on_open_url({
                let handle = app.handle().clone();
                move |_| {
                    if let Some(window) = handle.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_data,
            save_model,
            test_model,
            list_models,
            delete_model,
            save_settings,
            preview_apply,
            apply_preview,
            cancel_preview,
            preview_restore,
            delete_backup,
            preview_import,
            confirm_import,
            share_model,
            crate::skills_desktop::skills_list,
            crate::skills_desktop::skills_detail,
            crate::skills_desktop::skills_preview_install,
            crate::skills_desktop::skills_preview_link,
            crate::skills_desktop::skills_preview_delete,
            crate::skills_desktop::skills_commit,
            crate::skills_desktop::skills_cancel,
            crate::new_api_desktop::new_api_check,
            crate::new_api_desktop::new_api_status,
            crate::new_api_desktop::new_api_login,
            crate::new_api_desktop::new_api_cancel_login,
            crate::new_api_desktop::new_api_catalog,
            crate::new_api_desktop::new_api_import,
            crate::new_api_desktop::new_api_test,
            crate::new_api_desktop::new_api_disconnect
        ])
        .run(tauri::generate_context!())
        .expect("power-switch 启动失败");
}
