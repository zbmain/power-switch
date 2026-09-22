use crate::{
    adapters::{self, Change},
    files::{self, Snapshot},
    import,
    model::{AgentKind, AppResult, ModelConfig},
    paths::{AgentPath, Paths, Settings},
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Store {
    version: u32,
    models: Vec<ModelConfig>,
    settings: Settings,
}

impl Default for Store {
    /// Start with an empty model library and follow the system appearance.
    fn default() -> Self {
        Self {
            version: 1,
            models: vec![],
            settings: Settings {
                theme: "system".into(),
                ..Settings::default()
            },
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePreview {
    pub path: PathBuf,
    pub before: String,
    pub after: String,
    pub fingerprint: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyPreview {
    pub token: String,
    pub title: String,
    pub files: Vec<FilePreview>,
    pub notices: Vec<String>,
}

struct Pending {
    title: String,
    changes: Vec<Change>,
    dependencies: Vec<Snapshot>,
    created_at: u64,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BackupRecord {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    pub status: String,
    pub paths: Vec<PathBuf>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupFile {
    before: Snapshot,
    after_hash: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Backup {
    record: BackupRecord,
    files: Vec<BackupFile>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub backup_id: String,
    pub paths: Vec<PathBuf>,
    pub message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppData {
    pub models: Vec<ModelConfig>,
    pub settings: Settings,
    pub agents: Vec<AgentPath>,
    pub data_dir: PathBuf,
    pub backups: Vec<BackupRecord>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRow {
    pub index: usize,
    pub name: String,
    pub model_id: String,
    pub protocol: crate::model::Protocol,
    pub base_url: String,
    pub has_api_key: bool,
    pub duplicate: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub token: String,
    pub rows: Vec<ImportRow>,
}

pub struct Engine {
    pub paths: Paths,
    pending: HashMap<String, Pending>,
    imports: HashMap<String, (u64, Vec<ModelConfig>)>,
    _lock: File,
}

impl Engine {
    /// Hold an exclusive app-data lock and initialize private storage without reading Agent secrets.
    pub fn open(paths: Paths) -> AppResult<Self> {
        files::private_dir(&paths.data)?;
        files::private_dir(&paths.data.join("backups"))?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(paths.data.join(".lock"))
            .map_err(|e| format!("打开数据锁失败：{e}"))?;
        lock.try_lock_exclusive()
            .map_err(|_| "另一个 power-switch 正在使用此数据目录")?;
        let engine = Self {
            paths,
            pending: HashMap::new(),
            imports: HashMap::new(),
            _lock: lock,
        };
        engine.load()?;
        Ok(engine)
    }

    /// Read the current app store, refusing unsupported or damaged data instead of resetting it.
    fn load(&self) -> AppResult<Store> {
        let snapshot = Snapshot::read(&self.paths.data.join("models.json"))?;
        match snapshot.bytes {
            None => Ok(Store::default()),
            Some(bytes) => {
                let store: Store = serde_json::from_slice(&bytes)
                    .map_err(|_| "power-switch 模型库格式无效，请恢复备份")?;
                if store.version != 1 {
                    return Err("模型库版本不受支持，请升级 power-switch".into());
                }
                Ok(store)
            }
        }
    }

    /// Persist all library changes atomically and invalidate stale apply previews.
    fn save(&mut self, store: &Store) -> AppResult<()> {
        let bytes = serde_json::to_vec_pretty(store).map_err(|_| "无法序列化模型库")?;
        files::atomic_write(&self.paths.data.join("models.json"), &bytes)?;
        self.pending.clear();
        Ok(())
    }

    /// Fetch the UI's local library, resolved paths and backup summaries.
    pub fn data(&self) -> AppResult<AppData> {
        let store = self.load()?;
        Ok(AppData {
            agents: self.paths.describe(&store.settings)?,
            models: store.models,
            settings: store.settings,
            data_dir: self.paths.data.clone(),
            backups: self.list_backups()?,
        })
    }

    /// Add or edit a model after server-side validation, independent of UI checks.
    pub fn upsert(&mut self, mut model: ModelConfig) -> AppResult<ModelConfig> {
        model.validate()?;
        let mut store = self.load()?;
        if model.id.is_empty() {
            model.id = uuid::Uuid::new_v4().to_string();
            store.models.push(model.clone());
        } else {
            let old = store
                .models
                .iter_mut()
                .find(|m| m.id == model.id)
                .ok_or("模型已被删除，请刷新")?;
            *old = model.clone();
        }
        self.save(&store)?;
        Ok(model)
    }

    /// Save a connector-owned stable ID and refresh sibling protocols after a deliberate key replacement.
    pub fn upsert_from_new_api(
        &mut self,
        mut model: ModelConfig,
        siblings: &[String],
    ) -> AppResult<ModelConfig> {
        model.validate()?;
        uuid::Uuid::parse_str(&model.id).map_err(|_| "New API 模型缺少有效的内部 ID")?;
        let mut store = self.load()?;
        if let Some(old) = store.models.iter_mut().find(|m| m.id == model.id) {
            if !old.same_endpoint(&model) {
                return Err("关联模型已手动改为其他接口，请先移除该模型记录，再重新导入".into());
            }
            *old = model.clone();
        } else {
            store.models.push(model.clone());
        }
        let base = if model.protocol == crate::model::Protocol::AnthropicMessages {
            model.base_url.clone()
        } else {
            model
                .base_url
                .strip_suffix("/v1")
                .unwrap_or(&model.base_url)
                .to_string()
        };
        for other in &mut store.models {
            if siblings.contains(&other.id)
                && other.model_id == model.model_id
                && other.base_url == crate::new_api::api_base(&base, other.protocol)
            {
                other.api_key = model.api_key.clone();
            }
        }
        self.save(&store)?;
        Ok(model)
    }

    /// Remove only a library record; already applied Agent files remain under their own lifecycle.
    pub fn delete(&mut self, id: &str) -> AppResult<()> {
        let mut store = self.load()?;
        store.models.retain(|m| m.id != id);
        self.save(&store)
    }

    /// Validate and persist explicit target-path and appearance preferences.
    pub fn settings(&mut self, settings: Settings) -> AppResult<()> {
        if !["light", "dark", "system"].contains(&settings.theme.as_str()) {
            return Err("无效的主题设置".into());
        }
        let paths = self.paths.describe(&settings)?;
        let mut unique = std::collections::HashSet::new();
        for p in paths {
            if !unique.insert(p.path) {
                return Err("不同 Agent 不能使用同一个配置文件".into());
            }
        }
        let mut store = self.load()?;
        store.settings = settings;
        self.save(&store)
    }

    /// Prepare a credential-redacted, expiring preview with immutable in-memory write contents.
    pub fn preview_apply(&mut self, id: &str, agents: &[AgentKind]) -> AppResult<ApplyPreview> {
        if agents.is_empty() {
            return Err("请至少选择一个 Agent".into());
        }
        let store = self.load()?;
        let mut model = store
            .models
            .iter()
            .find(|m| m.id == id)
            .ok_or("模型不存在")?
            .clone();
        model.validate()?;
        let mut changes = vec![];
        let mut dependencies = vec![];
        let mut notices = vec![];
        for (index, agent) in agents.iter().enumerate() {
            if agents[..index].contains(agent) {
                continue;
            }
            let p = adapters::project(&self.paths, &store.settings, &model, *agent)?;
            changes.extend(p.changes);
            dependencies.extend(p.dependencies);
            notices.extend(p.notices);
        }
        if model.api_key.is_empty() {
            notices.push("此模型未填写 API Key；仅适用于无需认证的服务。".into());
        }
        self.prepare(
            format!("应用模型 · {}", model.name),
            changes,
            dependencies,
            notices,
        )
    }

    /// Install a preview without writing Agent files; only the returned token can authorize commit.
    fn prepare(
        &mut self,
        title: String,
        changes: Vec<Change>,
        dependencies: Vec<Snapshot>,
        notices: Vec<String>,
    ) -> AppResult<ApplyPreview> {
        self.pending
            .retain(|_, p| now().saturating_sub(p.created_at) < 600);
        if self.pending.len() >= 32 {
            return Err("未完成的预览过多，请关闭弹窗后重启应用".into());
        }
        for c in &changes {
            crate::paths::validate_absolute(&c.before.path)?;
            let nearest = c
                .before
                .path
                .ancestors()
                .find(|p| p.exists())
                .ok_or("找不到配置的父目录")?;
            if fs::metadata(nearest)
                .map_err(|_| "无法读取目标权限")?
                .permissions()
                .readonly()
            {
                return Err(format!("目标为只读：{}", c.before.path.display()));
            }
        }
        let token = uuid::Uuid::new_v4().to_string();
        let files = changes
            .iter()
            .map(|c| FilePreview {
                path: c.before.path.clone(),
                before: files::redacted(c.before.bytes.as_deref()),
                after: files::redacted(c.after.as_deref()),
                fingerprint: c.before.fingerprint(),
            })
            .collect();
        self.pending.insert(
            token.clone(),
            Pending {
                title: title.clone(),
                changes,
                dependencies,
                created_at: now(),
            },
        );
        Ok(ApplyPreview {
            token,
            title,
            files,
            notices,
        })
    }

    /// Cancel a preview to drop its secret-bearing in-memory projection promptly.
    pub fn cancel_preview(&mut self, token: &str) {
        self.pending.remove(token);
        self.imports.remove(token);
    }

    /// Consume a confirmed preview and commit with rollback on write or verification failure.
    pub fn apply(&mut self, token: &str) -> AppResult<ApplyResult> {
        self.commit_with(token, files::write_optional)
    }

    /// Execute the guarded transaction; injected writers allow deterministic failure tests.
    fn commit_with<F>(&mut self, token: &str, mut writer: F) -> AppResult<ApplyResult>
    where
        F: FnMut(&std::path::Path, Option<&[u8]>) -> AppResult<()>,
    {
        let p = self.pending.remove(token).ok_or("预览已失效，请重新预览")?;
        if now().saturating_sub(p.created_at) >= 600 {
            return Err("预览已过期，请重新预览".into());
        }
        for snapshot in p
            .changes
            .iter()
            .map(|c| &c.before)
            .chain(p.dependencies.iter())
        {
            if !snapshot.unchanged()? {
                return Err(format!(
                    "配置已被外部修改，请重新预览：{}",
                    snapshot.path.display()
                ));
            }
        }
        let id = uuid::Uuid::new_v4().to_string();
        let mut backup = Backup {
            record: BackupRecord {
                id: id.clone(),
                title: p.title,
                created_at: now(),
                status: "pending".into(),
                paths: p.changes.iter().map(|c| c.before.path.clone()).collect(),
            },
            files: p
                .changes
                .iter()
                .map(|c| BackupFile {
                    before: c.before.clone(),
                    after_hash: files::fingerprint(c.after.as_deref()),
                })
                .collect(),
        };
        self.save_backup(&backup)?;
        let mut attempted = 0;
        let operation: AppResult<()> = (|| {
            for (index, c) in p.changes.iter().enumerate() {
                if !c.before.unchanged()? {
                    return Err(format!("写入前发现外部修改：{}", c.before.path.display()));
                }
                attempted = index + 1;
                writer(&c.before.path, c.after.as_deref())?;
                if Snapshot::read(&c.before.path)?.fingerprint()
                    != files::fingerprint(c.after.as_deref())
                {
                    return Err("回读校验失败".into());
                }
            }
            backup.record.status = "completed".into();
            self.save_backup(&backup)?;
            Ok(())
        })();
        if let Err(error) = operation {
            let mut failures = vec![];
            for c in p.changes[..attempted].iter().rev() {
                match Snapshot::read(&c.before.path) {
                    Ok(current) if current.fingerprint() == c.before.fingerprint() => {}
                    Ok(current)
                        if current.fingerprint() == files::fingerprint(c.after.as_deref()) =>
                    {
                        if let Err(e) =
                            files::write_optional(&c.before.path, c.before.bytes.as_deref())
                        {
                            failures.push(e);
                        }
                    }
                    _ => failures.push(format!(
                        "文件出现外部修改或无法读取，未自动回滚：{}",
                        c.before.path.display()
                    )),
                }
            }
            backup.record.status = if failures.is_empty() {
                "rolled_back"
            } else {
                "rollback_failed"
            }
            .into();
            if let Err(e) = self.save_backup(&backup) {
                failures.push(e);
            }
            return Err(if failures.is_empty() {
                format!("{error}；已恢复原始配置")
            } else {
                format!("{error}；需要手动恢复备份 {id}：{}", failures.join("；"))
            });
        }
        Ok(ApplyResult {
            backup_id: id,
            paths: backup.record.paths,
            message: "配置已写入，尚未验证模型调用。".into(),
        })
    }

    /// Persist the full recovery journal before mutation and after each terminal state.
    fn save_backup(&self, backup: &Backup) -> AppResult<()> {
        files::atomic_write(
            &self
                .paths
                .data
                .join("backups")
                .join(format!("{}.json", backup.record.id)),
            &serde_json::to_vec(backup).map_err(|_| "备份序列化失败")?,
        )
    }

    /// Read a UUID-addressed recovery record, never an arbitrary caller-supplied path.
    fn backup(&self, id: &str) -> AppResult<Backup> {
        uuid::Uuid::parse_str(id).map_err(|_| "无效备份 ID")?;
        let bytes = fs::read(self.paths.data.join("backups").join(format!("{id}.json")))
            .map_err(|_| "无法读取备份")?;
        serde_json::from_slice(&bytes).map_err(|_| "备份格式无效".into())
    }

    /// Return summaries only so backup credentials never enter the frontend.
    pub fn list_backups(&self) -> AppResult<Vec<BackupRecord>> {
        let mut records = vec![];
        for entry in
            fs::read_dir(self.paths.data.join("backups")).map_err(|_| "无法读取备份目录")?
        {
            let entry = entry.map_err(|_| "无法读取备份条目")?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                let id = path.file_stem().unwrap().to_string_lossy();
                records.push(self.backup(&id)?.record);
            }
        }
        records.sort_by_key(|r| std::cmp::Reverse(r.created_at));
        Ok(records)
    }

    /// Delete only the UUID-addressed backup record after UI confirmation, never Agent files.
    pub fn delete_backup(&mut self, id: &str) -> AppResult<()> {
        uuid::Uuid::parse_str(id).map_err(|_| "无效备份 ID")?;
        let path = self.paths.data.join("backups").join(format!("{id}.json"));
        let meta = fs::symlink_metadata(&path).map_err(|_| "备份不存在，请刷新")?;
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err("备份必须是普通文件".into());
        }
        fs::remove_file(path).map_err(|e| format!("删除备份失败：{e}"))?;
        // A prepared restore must not outlive the recovery record the user removed.
        self.pending.clear();
        Ok(())
    }

    /// Read the current path preferences without loading credential-bearing backup bodies.
    pub fn current_settings(&self) -> AppResult<Settings> {
        Ok(self.load()?.settings)
    }

    /// Preview restoration against today's files, with a new backup made on confirmation.
    pub fn preview_restore(&mut self, id: &str) -> AppResult<ApplyPreview> {
        let backup = self.backup(id)?;
        let mut changes = vec![];
        for file in backup.files {
            crate::paths::validate_absolute(&file.before.path)?;
            if file.before.path.starts_with(&self.paths.data) {
                return Err("备份不能覆盖应用自身数据".into());
            }
            changes.push(Change {
                before: Snapshot::read(&file.before.path)?,
                after: file.before.bytes,
            });
        }
        self.prepare(
            format!("恢复备份 · {}", backup.record.title),
            changes,
            vec![],
            vec!["恢复将覆盖这些文件的当前内容；确认后会先备份当前状态。".into()],
        )
    }

    /// Parse and stage an import; return descriptions rather than secret-bearing payloads.
    pub fn preview_import(&mut self, link: &str) -> AppResult<ImportPreview> {
        let models = import::parse_link(link)?;
        let store = self.load()?;
        let rows = models
            .iter()
            .enumerate()
            .map(|(index, m)| ImportRow {
                index,
                name: m.name.clone(),
                model_id: m.model_id.clone(),
                protocol: m.protocol,
                base_url: m.base_url.clone(),
                has_api_key: !m.api_key.is_empty(),
                duplicate: store
                    .models
                    .iter()
                    .chain(models[..index].iter())
                    .any(|old| m.same_endpoint(old)),
            })
            .collect();
        self.imports
            .retain(|_, (created, _)| now().saturating_sub(*created) < 600);
        if self.imports.len() >= 32 {
            return Err("待导入清单过多，请先完成或取消导入".into());
        }
        let token = uuid::Uuid::new_v4().to_string();
        self.imports.insert(token.clone(), (now(), models));
        Ok(ImportPreview { token, rows })
    }

    /// Confirm a staged batch, preserving existing keys when a shared update omits them.
    pub fn confirm_import(&mut self, token: &str, updates: &[usize]) -> AppResult<usize> {
        let (created, models) = self.imports.remove(token).ok_or("导入预览已失效")?;
        if now().saturating_sub(created) >= 600 {
            return Err("导入预览已过期".into());
        }
        if updates.iter().any(|i| *i >= models.len()) {
            return Err("更新条目索引无效".into());
        }
        let mut store = self.load()?;
        let mut count = 0;
        for (index, mut model) in models.into_iter().enumerate() {
            if let Some(old) = store.models.iter_mut().find(|m| m.same_endpoint(&model)) {
                if !updates.contains(&index) {
                    continue;
                }
                model.id = old.id.clone();
                if model.api_key.is_empty() {
                    model.api_key = old.api_key.clone();
                }
                *old = model;
            } else {
                model.id = uuid::Uuid::new_v4().to_string();
                store.models.push(model);
            }
            count += 1;
        }
        self.save(&store)?;
        Ok(count)
    }

    /// Generate a link from a saved model with explicit credential inclusion.
    pub fn share(&self, id: &str, include_secret: bool) -> AppResult<String> {
        let store = self.load()?;
        import::share_link(
            store
                .models
                .iter()
                .find(|m| m.id == id)
                .ok_or("模型不存在")?,
            include_secret,
        )
    }
}

/// Return epoch seconds for sorting and limiting sensitive preview lifetimes.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify a second-file failure restores the first and journals the failure.
    #[test]
    fn transaction_rolls_back_on_second_write_failure() {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths {
            home: temp.path().join("home"),
            data: temp.path().join("app"),
            workbuddy_env: None,
            codex_env: None,
        };
        fs::create_dir_all(&paths.home).unwrap();
        let mut engine = Engine::open(paths.clone()).unwrap();
        let a = paths.home.join("a.json");
        let b = paths.home.join("b.json");
        fs::write(&a, b"{}").unwrap();
        fs::write(&b, b"{}").unwrap();
        let changes = [&a, &b]
            .iter()
            .map(|p| Change {
                before: Snapshot::read(p).unwrap(),
                after: Some(b"{\"new\":true}".to_vec()),
            })
            .collect();
        let p = engine
            .prepare("test".into(), changes, vec![], vec![])
            .unwrap();
        let mut writes = 0;
        let error = engine
            .commit_with(&p.token, |path, bytes| {
                writes += 1;
                if writes == 2 {
                    return Err("simulated disk full".into());
                }
                files::write_optional(path, bytes)
            })
            .err()
            .unwrap();
        assert!(error.contains("已恢复"));
        assert_eq!(fs::read(&a).unwrap(), b"{}");
        assert_eq!(fs::read(&b).unwrap(), b"{}");
        assert_eq!(engine.list_backups().unwrap()[0].status, "rolled_back");
    }
}
