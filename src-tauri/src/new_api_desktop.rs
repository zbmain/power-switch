use crate::{
    engine::Engine,
    new_api::{
        self, vault::SystemVault, Catalog, Connection, Error, ImportRequest, ImportResult,
        LoginStatus, NewApi, Result,
    },
};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{
    AppHandle, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tokio::sync::Mutex as AsyncMutex;

pub type Connector = AsyncMutex<NewApi>;

#[derive(Default)]
struct LoginCancels(Mutex<HashMap<String, Arc<AtomicBool>>>);

/// Cancel an in-flight exchange without waiting for the async service's network operation.
fn signal_cancel(app: &AppHandle, id: &str) {
    if let Ok(mut flows) = app.state::<LoginCancels>().0.lock() {
        if let Some(flag) = flows.remove(id) {
            flag.store(true, Ordering::SeqCst);
        }
    }
}

/// Register the connector beside the existing model engine, using a separate metadata file.
pub fn setup(app: &AppHandle, data: std::path::PathBuf) {
    app.manage(AsyncMutex::new(NewApi::new(data, Box::new(SystemVault))));
    app.manage(LoginCancels::default());
}

/// Only the trusted local main window can invoke connector operations.
fn require_main(window: &WebviewWindow) -> Result<()> {
    let url = window
        .url()
        .map_err(|_| Error::new("forbidden", "无法验证本地窗口来源"))?;
    let local = (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (["http", "https"].contains(&url.scheme()) && url.host_str() == Some("tauri.localhost"))
        || (cfg!(debug_assertions)
            && url.scheme() == "http"
            && [Some("localhost"), Some("127.0.0.1")].contains(&url.host_str())
            && url.port() == Some(1420));
    if window.label() != "main" || !local {
        return Err(Error::new("forbidden", "此窗口不能访问本地 New API 凭证"));
    }
    Ok(())
}

/// Inspect an instance without requesting credentials or starting an OAuth flow.
#[tauri::command]
pub async fn new_api_check(
    window: WebviewWindow,
    state: State<'_, Connector>,
    base_url: String,
) -> Result<Connection> {
    require_main(&window)?;
    state.lock().await.check(&base_url).await
}

/// Restore a private session or return progress for the active browser authorization.
#[tauri::command]
pub async fn new_api_status(
    window: WebviewWindow,
    state: State<'_, Connector>,
    base_url: String,
) -> Result<LoginStatus> {
    require_main(&window)?;
    state.lock().await.status(&base_url).await
}

/// Launch the IdP in an isolated remote window with no application IPC capabilities.
#[tauri::command]
pub async fn new_api_login(
    window: WebviewWindow,
    app: AppHandle,
    state: State<'_, Connector>,
    base_url: String,
) -> Result<String> {
    require_main(&window)?;
    let start = state.lock().await.start_login(&base_url).await?;
    if let Ok(mut flows) = app.state::<LoginCancels>().0.lock() {
        flows.insert(start.id.clone(), start.canceled.clone());
    }
    let label = format!("new-api-login-{}", start.id);
    let completed = Arc::new(AtomicBool::new(false));
    let navigation_app = app.clone();
    let callback = start.callback;
    let navigation_id = start.id.clone();
    let navigation_label = label.clone();
    let once = completed.clone();
    let popup_app = app.clone();
    let popup_label = label.clone();
    let built = WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(start.url))
        .title("登录 New API · 钉钉 / Keycloak")
        .inner_size(560.0, 740.0)
        .incognito(true)
        .devtools(false)
        .on_navigation(move |url| {
            if url.origin() == callback.origin() && url.path() == callback.path() {
                if !once.swap(true, Ordering::SeqCst) {
                    let handle = navigation_app.clone();
                    let id = navigation_id.clone();
                    let callback_url = url.clone();
                    let window_label = navigation_label.clone();
                    tauri::async_runtime::spawn(async move {
                        // Completion errors are available through status(); no callback URL is logged.
                        let _ = handle
                            .state::<Connector>()
                            .lock()
                            .await
                            .finish_login(&id, &callback_url)
                            .await;
                        if let Ok(mut flows) = handle.state::<LoginCancels>().0.lock() {
                            flows.remove(&id);
                        }
                        if let Some(login) = handle.get_webview_window(&window_label) {
                            let _ = login.close();
                        }
                        if let Some(main) = handle.get_webview_window("main") {
                            let _ = main.set_focus();
                        }
                    });
                }
                return false;
            }
            url.scheme() == "https" || url.as_str() == "about:blank"
        })
        .on_new_window(move |url, _| {
            if url.scheme() == "https" {
                if let Some(login) = popup_app.get_webview_window(&popup_label) {
                    let _ = login.navigate(url);
                }
            }
            tauri::webview::NewWindowResponse::Deny
        })
        .build();
    let login_window = match built {
        Ok(window) => window,
        Err(_) => {
            signal_cancel(&app, &start.id);
            let error = Error::new("window", "无法打开独立登录窗口，请重试");
            state.lock().await.fail_login(&start.id, error.clone());
            return Err(error);
        }
    };
    let close_app = app.clone();
    let close_id = start.id.clone();
    login_window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            signal_cancel(&close_app, &close_id);
        }
        if matches!(event, WindowEvent::Destroyed) && !completed.load(Ordering::SeqCst) {
            let handle = close_app.clone();
            let id = close_id.clone();
            tauri::async_runtime::spawn(async move {
                handle.state::<Connector>().lock().await.cancel_login(&id);
            });
        }
    });
    let timeout_app = app.clone();
    let timeout_id = start.id.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(new_api::LOGIN_TIMEOUT)).await;
        signal_cancel(&timeout_app, &timeout_id);
        timeout_app.state::<Connector>().lock().await.fail_login(
            &timeout_id,
            Error::new("oauth_timeout", "登录已超时，请重新发起"),
        );
        if let Some(login) = timeout_app.get_webview_window(&label) {
            let _ = login.close();
        }
    });
    Ok(start.id)
}

/// Cancel the native flow before closing its window so a delayed redirect cannot sign in.
#[tauri::command]
pub async fn new_api_cancel_login(
    window: WebviewWindow,
    app: AppHandle,
    state: State<'_, Connector>,
    login_id: String,
) -> Result<()> {
    require_main(&window)?;
    signal_cancel(&app, &login_id);
    if let Some(login) = app.get_webview_window(&format!("new-api-login-{login_id}")) {
        let _ = login.close();
    }
    state.lock().await.cancel_login(&login_id);
    Ok(())
}

/// Load groups and model/protocol capabilities for the exact displayed account.
#[tauri::command]
pub async fn new_api_catalog(
    window: WebviewWindow,
    state: State<'_, Connector>,
    base_url: String,
    user_id: i64,
    group: Option<String>,
) -> Result<Catalog> {
    require_main(&window)?;
    state
        .lock()
        .await
        .catalog(&base_url, user_id, group.as_deref())
        .await
}

/// Serialize token creation through the connector, then atomically save the associated local models.
#[tauri::command]
pub async fn new_api_import(
    window: WebviewWindow,
    state: State<'_, Connector>,
    engine: State<'_, Mutex<Engine>>,
    request: ImportRequest,
) -> Result<ImportResult> {
    require_main(&window)?;
    let mut connector = state.lock().await;
    let prepared = connector.prepare_import(request).await?;
    let model = engine
        .lock()
        .map_err(|_| Error::new("storage", "模型库已锁定，请重试"))?
        .upsert_from_new_api(prepared.model, &prepared.siblings)
        .map_err(|message| {
            Error::new(
                "storage",
                format!("{message}；密钥创建记录已保存，重试会复用已有密钥"),
            )
        })?;
    Ok(ImportResult {
        model,
        reused: prepared.reused,
        message: "模型已保存，密钥访问已校验；实际模型调用尚未验证，尚未应用到 Agent。".into(),
    })
}

/// Perform one paid, explicitly requested test using the saved model's credentials.
#[tauri::command]
pub async fn new_api_test(
    window: WebviewWindow,
    state: State<'_, Connector>,
    engine: State<'_, Mutex<Engine>>,
    model_id: String,
) -> Result<String> {
    require_main(&window)?;
    let model = {
        let data = engine
            .lock()
            .map_err(|_| Error::new("storage", "模型库已锁定"))?
            .data()
            .map_err(|_| Error::new("storage", "无法读取模型库"))?;
        data.models
            .into_iter()
            .find(|m| m.id == model_id)
            .ok_or_else(|| Error::new("model", "模型已被删除，请重新导入"))?
    };
    state.lock().await.test_model(&model).await
}

/// Disconnect only the selected instance; imported model keys remain usable.
#[tauri::command]
pub async fn new_api_disconnect(
    window: WebviewWindow,
    state: State<'_, Connector>,
    base_url: String,
) -> Result<()> {
    require_main(&window)?;
    state.lock().await.disconnect(&base_url)
}
