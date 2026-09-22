use crate::{
    engine::Engine,
    model::AppResult,
    skills::{
        self, ChangePreview, InstallPreview, InstallSource, Location, Scope, SkillDetail,
        SkillGroup, SkillManager,
    },
};
use std::{path::PathBuf, sync::Mutex};
use tauri::Manager;

/// Resolve current settings under the core lock, then release it before filesystem/network work.
fn context(app: &tauri::AppHandle) -> AppResult<(Vec<Location>, PathBuf)> {
    let state = app.state::<Mutex<Engine>>();
    let engine = state.lock().map_err(|_| "应用数据锁异常")?;
    Ok((
        skills::locations(&engine.paths, &engine.current_settings()?)?,
        engine.paths.data.clone(),
    ))
}

/// Read skill inventories on a worker so the native window remains responsive.
#[tauri::command]
pub async fn skills_list(app: tauri::AppHandle) -> AppResult<Vec<SkillGroup>> {
    tauri::async_runtime::spawn_blocking(move || {
        let (roots, _) = context(&app)?;
        Ok(skills::list(&roots))
    })
    .await
    .map_err(|_| "技能扫描任务中断")?
}

/// Read one selected skill as plain text without rendering active content.
#[tauri::command]
pub async fn skills_detail(
    app: tauri::AppHandle,
    scope: Scope,
    id: String,
) -> AppResult<SkillDetail> {
    tauri::async_runtime::spawn_blocking(move || {
        let (roots, _) = context(&app)?;
        skills::detail(&roots, scope, &id)
    })
    .await
    .map_err(|_| "技能详情读取中断")?
}

/// Stage installation only into global skills, never a client-supplied target.
#[tauri::command]
pub async fn skills_preview_install(
    app: tauri::AppHandle,
    source: InstallSource,
) -> AppResult<InstallPreview> {
    tauri::async_runtime::spawn_blocking(move || {
        let (roots, data) = context(&app)?;
        app.state::<Mutex<SkillManager>>()
            .lock()
            .map_err(|_| "技能操作锁异常")?
            .preview_install(&roots, &data, source)
    })
    .await
    .map_err(|_| "技能安装预览中断")?
}

/// Prepare soft links from an existing global skill to selected Agent directories.
#[tauri::command]
pub async fn skills_preview_link(
    app: tauri::AppHandle,
    id: String,
    scopes: Vec<Scope>,
) -> AppResult<ChangePreview> {
    tauri::async_runtime::spawn_blocking(move || {
        let (roots, _) = context(&app)?;
        app.state::<Mutex<SkillManager>>()
            .lock()
            .map_err(|_| "技能操作锁异常")?
            .preview_link(&roots, &id, &scopes)
    })
    .await
    .map_err(|_| "软链接预览中断")?
}

/// Prepare a deletion confirmation including affected global references.
#[tauri::command]
pub async fn skills_preview_delete(
    app: tauri::AppHandle,
    scope: Scope,
    id: String,
) -> AppResult<ChangePreview> {
    tauri::async_runtime::spawn_blocking(move || {
        let (roots, _) = context(&app)?;
        app.state::<Mutex<SkillManager>>()
            .lock()
            .map_err(|_| "技能操作锁异常")?
            .preview_delete(&roots, scope, &id)
    })
    .await
    .map_err(|_| "技能删除预览中断")?
}

/// Commit an expiring preview after the explicit confirmation dialog.
#[tauri::command]
pub async fn skills_commit(
    app: tauri::AppHandle,
    token: String,
    selected: Vec<usize>,
) -> AppResult<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (roots, _) = context(&app)?;
        app.state::<Mutex<SkillManager>>()
            .lock()
            .map_err(|_| "技能操作锁异常")?
            .commit(&roots, &token, &selected)
    })
    .await
    .map_err(|_| "技能操作中断，请刷新检查")?
}

/// Dispose of canceled staged downloads and in-memory operation tokens.
#[tauri::command]
pub fn skills_cancel(app: tauri::AppHandle, token: String) -> AppResult<()> {
    app.state::<Mutex<SkillManager>>()
        .lock()
        .map_err(|_| "技能操作锁异常")?
        .cancel(&token);
    Ok(())
}
