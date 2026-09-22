//! Skill documents are untrusted data: discovery and installation never execute their instructions.
use crate::{
    model::{AgentKind, AppResult},
    paths::{Paths, Settings},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const MAX_BYTES: u64 = 100 * 1024 * 1024;
const MAX_FILES: usize = 10000;
const MAX_DOCUMENT: u64 = 512 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Global,
    Claude,
    Codex,
    Workbuddy,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub scope: Scope,
    pub path: PathBuf,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub linked: bool,
    pub link_target: Option<PathBuf>,
    pub problem: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillGroup {
    pub scope: Scope,
    pub path: PathBuf,
    pub skills: Vec<SkillEntry>,
    pub error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDetail {
    pub document: String,
    pub path: PathBuf,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum InstallSource {
    Local { path: PathBuf },
    Git { url: String, subdir: Option<String> },
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallCandidate {
    pub index: usize,
    pub name: String,
    pub description: String,
    pub folder: String,
    pub exists: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPreview {
    pub token: String,
    pub destination: PathBuf,
    pub skills: Vec<InstallCandidate>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangePreview {
    pub token: String,
    pub title: String,
    pub paths: Vec<PathBuf>,
    pub message: String,
}

enum Action {
    Install {
        _stage: tempfile::TempDir,
        sources: Vec<PathBuf>,
        candidates: Vec<InstallCandidate>,
    },
    Link {
        source: PathBuf,
        hash: String,
        targets: Vec<PathBuf>,
    },
    Delete {
        items: Vec<(PathBuf, String, PathBuf)>,
        global_source: Option<(PathBuf, bool)>,
    },
}
struct Pending {
    roots: Vec<Location>,
    resolved_roots: Vec<PathBuf>,
    created: Instant,
    action: Action,
}

/// Resolve even missing roots through their nearest existing ancestor to detect redirected links.
fn resolved_roots(roots: &[Location]) -> AppResult<Vec<PathBuf>> {
    roots
        .iter()
        .map(|r| {
            let ancestor = r
                .path
                .ancestors()
                .find(|p| p.exists())
                .ok_or("技能路径没有可用父目录")?;
            let canonical = fs::canonicalize(ancestor).map_err(|_| "无法解析技能目录")?;
            Ok(canonical.join(r.path.strip_prefix(ancestor).map_err(|_| "技能路径无效")?))
        })
        .collect()
}

/// Find all direct Agent links that would become invalid if a global skill disappeared.
fn global_references(
    roots: &[Location],
    source: &Path,
    is_link: bool,
) -> AppResult<Vec<(PathBuf, String, PathBuf)>> {
    let global = root(roots, Scope::Global)?;
    let mut result = vec![];
    for location in roots.iter().filter(|r| r.scope != Scope::Global) {
        if location.path == global
            || (location.path.exists()
                && fs::canonicalize(&location.path).ok() == fs::canonicalize(global).ok())
        {
            continue;
        }
        let mut links = vec![];
        referring_links(&location.path, source, is_link, &mut links, &mut 0, 0)?;
        for link in links {
            if !result.iter().any(|(p, _, _)| p == &link) {
                result.push((link.clone(), tree_hash(&link)?, location.path.clone()));
            }
        }
    }
    Ok(result)
}

#[derive(Default)]
pub struct SkillManager {
    pending: std::collections::HashMap<String, Pending>,
}

/// Resolve skill locations alongside the configured Agent directories and global user home.
pub fn locations(paths: &Paths, settings: &Settings) -> AppResult<Vec<Location>> {
    let mut result = vec![Location {
        scope: Scope::Global,
        path: paths.home.join(".agents/skills"),
    }];
    for (scope, agent) in [
        (Scope::Claude, AgentKind::Claude),
        (Scope::Codex, AgentKind::Codex),
        (Scope::Workbuddy, AgentKind::Workbuddy),
    ] {
        let target = paths.target(settings, agent)?;
        result.push(Location {
            scope,
            path: target
                .parent()
                .ok_or("Agent 配置没有父目录")?
                .join("skills"),
        });
    }
    Ok(result)
}

/// Find one fixed scope rather than accepting a caller-supplied destination directory.
fn root(roots: &[Location], scope: Scope) -> AppResult<&Path> {
    roots
        .iter()
        .find(|r| r.scope == scope)
        .map(|r| r.path.as_path())
        .ok_or("技能目录未配置".into())
}

/// Reject traversal and platform-specific reserved names for managed entries.
fn valid_relative(id: &str) -> AppResult<PathBuf> {
    let path = PathBuf::from(id);
    if id.is_empty()
        || id.contains('\\')
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("技能路径必须是目录内的相对路径".into());
    }
    for part in id.split('/') {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        if part.starts_with(".power-switch-")
            || part == ".git"
            || part
                .chars()
                .any(|c| c.is_control() || "<>:\"|?*".contains(c))
            || part.ends_with(['.', ' '])
            || [
                "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
                "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8",
                "LPT9",
            ]
            .contains(&stem.as_str())
        {
            return Err("技能目录名包含不支持的字符或保留名称".into());
        }
    }
    Ok(path)
}

/// Validate parents without following a skill link outside its scope during mutations.
fn entry_path(base: &Path, id: &str) -> AppResult<PathBuf> {
    let relative = valid_relative(id)?;
    let mut path = base.to_owned();
    let parts: Vec<_> = relative.components().collect();
    for (index, part) in parts.iter().enumerate() {
        path.push(part);
        if index + 1 < parts.len() {
            let meta = fs::symlink_metadata(&path).map_err(|_| "技能父目录不存在")?;
            if !meta.is_dir() || meta.file_type().is_symlink() {
                return Err("技能位于共享软链接目录中，请管理该父链接".into());
            }
        }
    }
    Ok(path)
}

/// Skip repository internals, dependencies and our own transaction directories.
fn ignored(name: &str) -> bool {
    [".git", "node_modules", ".venv", "__pycache__"].contains(&name)
        || name.starts_with(".power-switch-")
}

/// Read bounded UTF-8 Markdown as text; it is never interpreted as executable instructions.
fn document(path: &Path) -> AppResult<String> {
    let file = path.join("SKILL.md");
    let meta = fs::metadata(&file).map_err(|_| "没有可读取的 SKILL.md")?;
    if !meta.is_file() || meta.len() > MAX_DOCUMENT {
        return Err("SKILL.md 必须为不超过 512 KiB 的文件".into());
    }
    fs::read_to_string(file).map_err(|_| "SKILL.md 不是有效 UTF-8 或无法读取".into())
}

/// Parse standard YAML front matter while treating all values as untrusted display data.
fn metadata(text: &str, fallback: &str) -> AppResult<(String, String)> {
    let mut lines = text.trim_start_matches('\u{feff}').lines();
    if lines.next() != Some("---") {
        return Ok((fallback.into(), String::new()));
    }
    let mut yaml = String::new();
    let mut closed = false;
    for line in lines {
        if line.trim() == "---" {
            closed = true;
            break;
        }
        yaml.push_str(line);
        yaml.push('\n');
    }
    if !closed {
        return Err("SKILL.md 的 YAML 元数据未闭合".into());
    }
    let value: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&yaml).map_err(|_| "SKILL.md 的 YAML 元数据无效")?;
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(fallback);
    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    Ok((
        name.chars().take(256).collect(),
        description.chars().take(2000).collect(),
    ))
}

/// Enumerate nested skills and linked folders without entering cycles or unbounded trees.
fn walk(
    base: &Path,
    dir: &Path,
    entries: &mut Vec<SkillEntry>,
    ancestors: &mut HashSet<PathBuf>,
    visited: &mut usize,
    depth: usize,
    inherited_link: bool,
) -> AppResult<()> {
    if depth > 12 {
        return Err("技能目录层级超过 12 层".into());
    }
    let canonical = fs::canonicalize(dir).map_err(|_| "无法访问技能目录")?;
    if !ancestors.insert(canonical.clone()) {
        return Ok(());
    }
    let mut children: Vec<_> = fs::read_dir(dir)
        .map_err(|_| "无法读取技能目录")?
        .collect::<Result<_, _>>()
        .map_err(|_| "无法读取技能条目")?;
    children.sort_by_key(|e| e.file_name());
    for child in children {
        let name = child.file_name().to_string_lossy().into_owned();
        if ignored(&name) {
            continue;
        }
        *visited += 1;
        if *visited > MAX_FILES {
            return Err("技能目录条目超过 10000，请缩小目录范围".into());
        }
        let path = child.path();
        let meta = fs::symlink_metadata(&path).map_err(|_| "无法读取技能属性")?;
        if !meta.is_dir() && !meta.file_type().is_symlink() {
            continue;
        }
        let linked = meta.file_type().is_symlink() || inherited_link;
        let link_target = if linked {
            fs::canonicalize(&path)
                .ok()
                .or_else(|| fs::read_link(&path).ok())
        } else {
            None
        };
        let id = path
            .strip_prefix(base)
            .map_err(|_| "技能路径无效")?
            .to_string_lossy()
            .replace('\\', "/");
        let skill_file = path.join("SKILL.md");
        if skill_file.exists() || (meta.file_type().is_symlink() && !path.exists()) {
            let info = document(&path).and_then(|text| metadata(&text, &name));
            let (title, description, problem) = match info {
                Ok((title, description)) => (title, description, None),
                Err(e) => (name.clone(), String::new(), Some(e)),
            };
            entries.push(SkillEntry {
                id,
                name: title,
                description,
                path: path.clone(),
                linked,
                link_target,
                problem,
            });
        }
        if path.is_dir() {
            walk(base, &path, entries, ancestors, visited, depth + 1, linked)?;
        }
    }
    ancestors.remove(&canonical);
    Ok(())
}

/// Read all four scopes independently so one unavailable directory does not hide the others.
pub fn list(roots: &[Location]) -> Vec<SkillGroup> {
    roots
        .iter()
        .map(|r| {
            let mut skills = vec![];
            let error = if !r.path.exists() {
                if fs::symlink_metadata(&r.path).is_ok() {
                    Some("技能目录的软链接已失效".into())
                } else {
                    None
                }
            } else {
                walk(
                    &r.path,
                    &r.path,
                    &mut skills,
                    &mut HashSet::new(),
                    &mut 0,
                    0,
                    false,
                )
                .err()
            };
            skills.sort_by_key(|s| s.name.to_lowercase());
            SkillGroup {
                scope: r.scope,
                path: r.path.clone(),
                skills,
                error,
            }
        })
        .collect()
}

/// Return a selected document using a validated relative identifier.
pub fn detail(roots: &[Location], scope: Scope, id: &str) -> AppResult<SkillDetail> {
    let path = root(roots, scope)?.join(valid_relative(id)?);
    Ok(SkillDetail {
        document: document(&path)?,
        path: path.join("SKILL.md"),
    })
}

/// Bound copy/hash work and refuse special files such as sockets and devices.
#[derive(Default)]
struct Budget {
    bytes: u64,
    files: usize,
}

impl Budget {
    /// Charge one regular file or directory to the bounded filesystem operation.
    fn add(&mut self, bytes: u64, depth: usize) -> AppResult<()> {
        self.bytes += bytes;
        self.files += 1;
        if self.bytes > MAX_BYTES || self.files > MAX_FILES || depth > 20 {
            return Err("单次技能操作限 100 MiB、10000 个文件及 20 层目录".into());
        }
        Ok(())
    }
}

/// Snapshot a source tree without traversing symbolic links or copying repository internals.
fn copy_tree(
    source: &Path,
    destination: &Path,
    budget: &mut Budget,
    depth: usize,
) -> AppResult<()> {
    let meta = fs::symlink_metadata(source).map_err(|_| "无法读取技能源文件")?;
    budget.add(meta.len(), depth)?;
    if meta.file_type().is_symlink() {
        return Err("安装来源包含软链接，请选择实际技能目录；安装后可向 Agent 建立软链接".into());
    }
    if meta.is_file() {
        fs::copy(source, destination).map_err(|e| format!("复制技能文件失败：{e}"))?;
    } else if meta.is_dir() {
        fs::create_dir(destination).map_err(|e| format!("创建技能目录失败：{e}"))?;
        for item in fs::read_dir(source).map_err(|_| "读取技能源目录失败")? {
            let item = item.map_err(|_| "读取技能源条目失败")?;
            if ignored(&item.file_name().to_string_lossy()) {
                continue;
            }
            copy_tree(
                &item.path(),
                &destination.join(item.file_name()),
                budget,
                depth + 1,
            )?;
        }
    } else {
        return Err("技能不能包含设备、套接字等特殊文件".into());
    }
    Ok(())
}

/// Fingerprint the complete entry, hashing symlink targets without following them.
fn tree_hash(path: &Path) -> AppResult<String> {
    let mut hash = Sha256::new();
    hash_tree(path, &mut hash, &mut Budget::default(), 0)?;
    Ok(format!("{:x}", hash.finalize()))
}

/// Feed deterministic paths, types and bytes into a bounded tree fingerprint.
fn hash_tree(path: &Path, hash: &mut Sha256, budget: &mut Budget, depth: usize) -> AppResult<()> {
    let meta = fs::symlink_metadata(path).map_err(|_| "技能文件已变化，请刷新")?;
    budget.add(meta.len(), depth)?;
    if meta.file_type().is_symlink() {
        hash.update(b"link");
        hash.update(
            fs::read_link(path)
                .map_err(|_| "读取软链接失败")?
                .to_string_lossy()
                .as_bytes(),
        );
    } else if meta.is_file() {
        hash.update(b"file");
        hash.update(meta.len().to_le_bytes());
        hash.update(fs::read(path).map_err(|_| "读取技能文件失败")?);
    } else if meta.is_dir() {
        hash.update(b"dir");
        let mut children: Vec<_> = fs::read_dir(path)
            .map_err(|_| "读取技能目录失败")?
            .collect::<Result<_, _>>()
            .map_err(|_| "读取技能目录失败")?;
        children.sort_by_key(|e| e.file_name());
        for item in children {
            let name = item.file_name().to_string_lossy().into_owned();
            hash.update((name.len() as u64).to_le_bytes());
            hash.update(name);
            hash_tree(&item.path(), hash, budget, depth + 1)?;
        }
    } else {
        return Err("技能中包含不支持的特殊文件".into());
    }
    Ok(())
}

/// Resolve relative link paths lexically, including links whose targets are now missing.
fn normalized(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other),
        }
    }
    result
}

/// Collect links referencing a global entry without descending through any link.
fn referring_links(
    base: &Path,
    source: &Path,
    source_is_link: bool,
    results: &mut Vec<PathBuf>,
    visited: &mut usize,
    depth: usize,
) -> AppResult<()> {
    if !base.exists() {
        return Ok(());
    }
    if depth > 12 {
        return Err("Agent 技能目录层级过深".into());
    }
    for item in fs::read_dir(base).map_err(|_| "无法检查 Agent 技能引用")? {
        let item = item.map_err(|_| "无法检查技能引用")?;
        *visited += 1;
        if *visited > MAX_FILES {
            return Err("Agent 技能目录条目超过扫描上限".into());
        }
        if ignored(&item.file_name().to_string_lossy()) {
            continue;
        }
        let path = item.path();
        let meta = fs::symlink_metadata(&path).map_err(|_| "读取引用属性失败")?;
        if meta.file_type().is_symlink() {
            let target = fs::read_link(&path).map_err(|_| "读取技能链接失败")?;
            let absolute = if target.is_absolute() {
                target
            } else {
                base.join(target)
            };
            let matches = if source_is_link {
                normalized(&absolute).starts_with(normalized(source))
            } else {
                fs::canonicalize(&path)
                    .is_ok_and(|p| fs::canonicalize(source).is_ok_and(|s| p.starts_with(s)))
            };
            if matches {
                results.push(path);
            }
        } else if meta.is_dir() {
            referring_links(&path, source, source_is_link, results, visited, depth + 1)?;
        }
    }
    Ok(())
}

impl SkillManager {
    /// Keep a bounded, expiring token that owns exactly the files shown in a preview.
    fn stage(&mut self, roots: &[Location], action: Action) -> AppResult<String> {
        self.pending
            .retain(|_, p| p.created.elapsed() < Duration::from_secs(600));
        if self.pending.len() >= 8 {
            return Err("待确认技能操作过多，请先关闭预览".into());
        }
        let token = uuid::Uuid::new_v4().to_string();
        self.pending.insert(
            token.clone(),
            Pending {
                roots: roots.to_vec(),
                resolved_roots: resolved_roots(roots)?,
                created: Instant::now(),
                action,
            },
        );
        Ok(token)
    }

    /// Drop canceled installation snapshots and any pending destructive action.
    pub fn cancel(&mut self, token: &str) {
        self.pending.remove(token);
    }

    /// Snapshot local/Git content into private staging and preview only valid skill packages.
    pub fn preview_install(
        &mut self,
        roots: &[Location],
        data_dir: &Path,
        source: InstallSource,
    ) -> AppResult<InstallPreview> {
        let staging = data_dir.join("skill-staging");
        crate::files::private_dir(&staging)?;
        let temp = tempfile::tempdir_in(&staging).map_err(|_| "无法创建技能安装暂存目录")?;
        let stage = temp.path().join("source");
        let fallback;
        match source {
            InstallSource::Local { path } => {
                crate::paths::validate_absolute(&path)?;
                let path = if path.file_name().is_some_and(|n| n == "SKILL.md") {
                    path.parent().ok_or("技能路径无效")?.to_owned()
                } else {
                    path
                };
                let source = fs::canonicalize(&path).map_err(|_| "本地技能目录不存在")?;
                let canonical_staging =
                    fs::canonicalize(&staging).map_err(|_| "无法解析暂存目录")?;
                if source.starts_with(&canonical_staging) || canonical_staging.starts_with(&source)
                {
                    return Err("不能将应用数据目录作为技能安装来源".into());
                }
                fallback = source
                    .file_name()
                    .ok_or("请选择技能目录")?
                    .to_string_lossy()
                    .into_owned();
                copy_tree(&source, &stage, &mut Budget::default(), 0)?;
            }
            InstallSource::Git { url, subdir } => {
                let (url, directory, branch, repo_name) = git_source(&url, subdir.as_deref())?;
                fallback = if directory.is_empty() {
                    repo_name
                } else {
                    Path::new(&directory)
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned()
                };
                let checkout = temp.path().join("checkout");
                clone_repository(&url, branch.as_deref(), &checkout)?;
                let source = checkout.join(&directory);
                let canonical =
                    fs::canonicalize(&source).map_err(|_| "仓库中没有指定的技能子目录")?;
                if !canonical.starts_with(fs::canonicalize(&checkout).map_err(|_| "无法读取仓库")?)
                {
                    return Err("仓库子目录不能指向仓库外部".into());
                }
                copy_tree(&canonical, &stage, &mut Budget::default(), 0)?;
            }
        }
        let mut packages = vec![];
        if stage.join("SKILL.md").is_file() {
            packages.push((stage.clone(), fallback));
        } else {
            let mut entries = vec![];
            walk(
                &stage,
                &stage,
                &mut entries,
                &mut HashSet::new(),
                &mut 0,
                0,
                false,
            )?;
            for entry in entries {
                if entry.problem.is_none() {
                    packages.push((
                        entry.path.clone(),
                        entry
                            .path
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .into_owned(),
                    ));
                }
            }
        }
        if packages.is_empty() || packages.len() > 200 {
            return Err("来源需包含 1 至 200 个带 SKILL.md 的技能".into());
        }
        let global = root(roots, Scope::Global)?.to_owned();
        let mut candidates = vec![];
        let mut sources = vec![];
        let mut names = HashSet::new();
        for (index, (path, folder)) in packages.into_iter().enumerate() {
            valid_relative(&folder)?;
            if !names.insert(folder.to_lowercase()) {
                return Err("来源中存在同名技能目录，请改为选择具体子目录安装".into());
            }
            let (name, description) = metadata(&document(&path)?, &folder)?;
            let exists = fs::symlink_metadata(global.join(&folder)).is_ok();
            candidates.push(InstallCandidate {
                index,
                name,
                description,
                folder,
                exists,
            });
            sources.push(path);
        }
        let token = self.stage(
            roots,
            Action::Install {
                _stage: temp,
                sources,
                candidates: candidates.clone(),
            },
        )?;
        Ok(InstallPreview {
            token,
            destination: global,
            skills: candidates,
        })
    }

    /// Preview global-to-Agent links, refusing collisions and aliases of the global directory.
    pub fn preview_link(
        &mut self,
        roots: &[Location],
        id: &str,
        scopes: &[Scope],
    ) -> AppResult<ChangePreview> {
        if scopes.is_empty() || scopes.contains(&Scope::Global) {
            return Err("请选择至少一个 Agent；不能链接到全局自身".into());
        }
        let global = root(roots, Scope::Global)?;
        let source = entry_path(global, id)?;
        document(&source)?;
        let mut targets = vec![];
        for &scope in scopes {
            let destination = root(roots, scope)?;
            if destination == global
                || (destination.exists()
                    && fs::canonicalize(destination).ok() == fs::canonicalize(global).ok())
            {
                return Err("Agent 技能目录与全局相同，无需创建链接".into());
            }
            let relative = valid_relative(id)?;
            // Parent symlinks must not turn a local link operation into a write outside this Agent.
            let target = safe_destination(destination, &relative)?;
            if targets.contains(&target) {
                continue;
            }
            if fs::symlink_metadata(&target).is_ok() {
                return Err(format!("目标已存在，不会覆盖：{}", target.display()));
            }
            targets.push(target);
        }
        let hash = tree_hash(&source)?;
        let token = self.stage(
            roots,
            Action::Link {
                source: source.clone(),
                hash,
                targets: targets.clone(),
            },
        )?;
        Ok(ChangePreview {
            token,
            title: "连接全局技能".into(),
            paths: targets,
            message: format!(
                "以下 Agent 将通过软链接使用 {}，技能内容只保留在全局目录。",
                source.display()
            ),
        })
    }

    /// Preview deletion with global references included, and fingerprint everything to be moved.
    pub fn preview_delete(
        &mut self,
        roots: &[Location],
        scope: Scope,
        id: &str,
    ) -> AppResult<ChangePreview> {
        let base = root(roots, scope)?;
        let path = entry_path(base, id)?;
        let meta = fs::symlink_metadata(&path).map_err(|_| "技能已不存在，请刷新")?;
        if !meta.file_type().is_symlink() {
            document(&path)?;
        }
        if scope != Scope::Global
            && fs::canonicalize(base).ok() == fs::canonicalize(root(roots, Scope::Global)?).ok()
        {
            return Err("该 Agent 与全局共用同一目录，请在全局页面管理".into());
        }
        let mut removals = if scope == Scope::Global {
            global_references(roots, &path, meta.file_type().is_symlink())?
        } else {
            vec![]
        };
        removals.push((path.clone(), tree_hash(&path)?, base.to_owned()));
        let paths = removals.iter().map(|(p, _, _)| p.clone()).collect();
        let token = self.stage(
            roots,
            Action::Delete {
                items: removals,
                global_source: (scope == Scope::Global)
                    .then_some((path, meta.file_type().is_symlink())),
            },
        )?;
        Ok(ChangePreview {
            token,
            title: "删除技能".into(),
            paths,
            message: if scope == Scope::Global {
                "移除该全局技能及下列引用它的 Agent 软链接。其他技能不受影响。".into()
            } else if meta.file_type().is_symlink() {
                "仅移除这个软链接，全局技能和其他 Agent 不受影响。".into()
            } else {
                "移除这个 Agent 下的本地技能及其目录内容。".into()
            },
        })
    }

    /// Commit a single-use preview, with preflight guards and rollback of partial file operations.
    pub fn commit(
        &mut self,
        roots: &[Location],
        token: &str,
        selected: &[usize],
    ) -> AppResult<String> {
        let pending = self.pending.remove(token).ok_or("技能预览已失效，请重试")?;
        if pending.created.elapsed() >= Duration::from_secs(600)
            || pending.roots != roots
            || pending.resolved_roots != resolved_roots(roots)?
        {
            return Err("预览已过期或配置路径已修改，请重新预览".into());
        }
        match pending.action {
            Action::Install {
                sources,
                candidates,
                _stage,
            } => {
                let indexes: Vec<_> = selected
                    .iter()
                    .copied()
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();
                if indexes.is_empty()
                    || indexes.len() > 50
                    || indexes.iter().any(|&i| i >= candidates.len())
                {
                    return Err("请选择 1 至 50 个有效技能".into());
                }
                let global = root(roots, Scope::Global)?;
                fs::create_dir_all(global).map_err(|_| "无法创建全局技能目录")?;
                let mut prepared = vec![];
                for i in &indexes {
                    let dest = safe_destination(global, &valid_relative(&candidates[*i].folder)?)?;
                    if fs::symlink_metadata(&dest).is_ok() {
                        return Err("技能目录已存在，请刷新；不会覆盖已有技能".into());
                    }
                    let stage = tempfile::Builder::new()
                        .prefix(".power-switch-install-")
                        .tempdir_in(global)
                        .map_err(|_| "无法暂存技能")?;
                    copy_tree(
                        &sources[*i],
                        &stage.path().join("skill"),
                        &mut Budget::default(),
                        0,
                    )?;
                    prepared.push((stage, dest));
                }
                let mut installed = vec![];
                for (stage, dest) in &prepared {
                    let result = if fs::symlink_metadata(dest).is_ok() {
                        Err("目标被其他程序创建".into())
                    } else {
                        fs::rename(stage.path().join("skill"), dest).map_err(|e| e.to_string())
                    };
                    if let Err(error) = result {
                        // Keep a recovery directory instead of deleting newly installed content blindly.
                        let rollback = rollback_moves(&installed);
                        return Err(format!("安装失败：{error}；{rollback}"));
                    }
                    installed.push((stage.path().join("skill"), dest.clone()));
                }
                Ok(format!("已将 {} 个技能安装到全局。", indexes.len()))
            }
            Action::Link {
                source,
                hash,
                targets,
            } => {
                if tree_hash(&source)? != hash {
                    return Err("全局技能已变化，请重新预览".into());
                }
                for target in &targets {
                    if fs::symlink_metadata(target).is_ok() {
                        return Err("Agent 目标已变化，请刷新".into());
                    }
                }
                // Prepare every parent before creating the first link, so a later parent error
                // cannot bypass rollback and leave an earlier Agent unexpectedly connected.
                for target in &targets {
                    let location = roots
                        .iter()
                        .find(|r| r.scope != Scope::Global && target.starts_with(&r.path))
                        .ok_or("链接目标不在 Agent 目录内")?;
                    safe_destination(
                        &location.path,
                        target
                            .strip_prefix(&location.path)
                            .map_err(|_| "目标无效")?,
                    )?;
                    fs::create_dir_all(target.parent().ok_or("目标目录无效")?)
                        .map_err(|_| "无法创建 Agent 技能目录")?;
                }
                let mut made: Vec<PathBuf> = vec![];
                for target in targets {
                    if let Err(error) = symlink_dir(&source, &target) {
                        let mut failures = vec![];
                        for link in made.iter().rev() {
                            if fs::read_link(link).ok().as_deref() != Some(source.as_path())
                                || remove_link(link).is_err()
                            {
                                failures.push(link.display().to_string());
                            }
                        }
                        return Err(format!("建立软链接失败：{error}。Windows 可能需要启用开发者模式或管理员权限。{}", if failures.is_empty() { "已回滚本次链接".into() } else { format!("这些链接需要检查：{}", failures.join("、")) }));
                    }
                    made.push(target);
                }
                Ok(format!("已连接到 {} 个 Agent。", made.len()))
            }
            Action::Delete {
                items,
                global_source,
            } => {
                if let Some((source, is_link)) = global_source {
                    let current: HashSet<_> = global_references(roots, &source, is_link)?
                        .into_iter()
                        .map(|(p, _, _)| p)
                        .collect();
                    let expected: HashSet<_> = items[..items.len() - 1]
                        .iter()
                        .map(|(p, _, _)| p.clone())
                        .collect();
                    if current != expected {
                        return Err("Agent 技能引用已变化，请重新预览删除".into());
                    }
                }
                for (path, hash, base) in &items {
                    entry_path(
                        base,
                        &path
                            .strip_prefix(base)
                            .map_err(|_| "技能路径无效")?
                            .to_string_lossy()
                            .replace('\\', "/"),
                    )?;
                    if tree_hash(path)? != *hash {
                        return Err("技能或引用链接已变化，请重新预览".into());
                    }
                }
                let transaction = uuid::Uuid::new_v4().to_string();
                let mut moves = vec![];
                for (path, hash, base) in &items {
                    let result: AppResult<()> = (|| {
                        let trash = safe_destination(
                            base,
                            &PathBuf::from(".power-switch-trash").join(&transaction),
                        )?;
                        crate::files::private_dir(&trash)?;
                        if tree_hash(path)? != *hash {
                            return Err("技能在删除过程中变化，已停止".into());
                        }
                        let target = trash.join(format!(
                            "{}-{}",
                            moves.len(),
                            path.file_name().unwrap().to_string_lossy()
                        ));
                        fs::rename(path, &target).map_err(|e| format!("移动技能失败：{e}"))?;
                        moves.push((path.clone(), target));
                        Ok(())
                    })();
                    if let Err(error) = result {
                        return Err(format!("{error}；{}", rollback_moves(&moves)));
                    }
                }
                Ok(format!("已移除 {} 个技能目录或链接。可从对应技能目录的 .power-switch-trash/{transaction} 手动恢复。", moves.len()))
            }
        }
    }
}

/// Reject redirected parent folders before creating an installation or soft link.
fn safe_destination(base: &Path, relative: &Path) -> AppResult<PathBuf> {
    let mut current = base.to_path_buf();
    let parts: Vec<_> = relative.components().collect();
    for part in parts.iter().take(parts.len().saturating_sub(1)) {
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(meta) if !meta.is_dir() || meta.file_type().is_symlink() => {
                return Err("目标父目录不是实际目录，请更换路径".into())
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(format!("无法读取目标父目录：{e}"))
            }
            _ => {}
        }
    }
    Ok(base.join(relative))
}

/// Undo completed renames without overwriting concurrent user changes.
fn rollback_moves(moves: &[(PathBuf, PathBuf)]) -> String {
    let failures: Vec<_> = moves
        .iter()
        .rev()
        .filter_map(|(original, moved)| {
            if fs::symlink_metadata(original).is_ok() || fs::rename(moved, original).is_err() {
                Some(moved.display().to_string())
            } else {
                None
            }
        })
        .collect();
    if failures.is_empty() {
        "已回滚本次变更".into()
    } else {
        format!("部分回滚未完成，文件保留在：{}", failures.join("、"))
    }
}

/// Create real directory symlinks on each supported operating system; never substitute a copy.
fn symlink_dir(source: &Path, target: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, target)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(source, target)
    }
}

/// Remove the link itself on Windows/Unix without visiting its target.
fn remove_link(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        fs::remove_file(path)
    }
    #[cfg(windows)]
    {
        fs::remove_dir(path)
    }
}

/// Accept HTTPS repositories and GitHub tree links, excluding embedded credentials and options.
fn git_source(
    raw: &str,
    explicit_subdir: Option<&str>,
) -> AppResult<(String, String, Option<String>, String)> {
    let mut url = url::Url::parse(raw.trim()).map_err(|_| "请输入 HTTPS Git 仓库地址")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("仅支持不含凭据和查询参数的 HTTPS Git 地址".into());
    }
    let parts: Vec<_> = url
        .path()
        .trim_matches('/')
        .split('/')
        .map(str::to_owned)
        .collect();
    let mut branch = None;
    let mut subdir = explicit_subdir.unwrap_or("").trim_matches('/').to_owned();
    if url.host_str() == Some("github.com") && parts.len() >= 4 && parts[2] == "tree" {
        branch = Some(parts[3].clone());
        if subdir.is_empty() {
            subdir = parts[4..].join("/");
        }
        url.set_path(&format!(
            "/{}/{}.git",
            parts[0],
            parts[1].trim_end_matches(".git")
        ));
    }
    if !subdir.is_empty() {
        valid_relative(&subdir)?;
    }
    let name = url
        .path_segments()
        .and_then(|mut s| s.next_back())
        .unwrap_or("")
        .trim_end_matches(".git")
        .to_owned();
    valid_relative(&name)?;
    Ok((url.to_string(), subdir, branch, name))
}

/// Clone without hooks, submodules, credential prompts or shell interpolation, with a bounded wait.
fn clone_repository(url: &str, branch: Option<&str>, destination: &Path) -> AppResult<()> {
    let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let mut command = Command::new("git");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Background installs should not open a separate console window.
        command.creation_flags(0x08000000);
    }
    command.args([
        "-c",
        &format!("core.hooksPath={null}"),
        "-c",
        "protocol.file.allow=never",
        "-c",
        "protocol.ext.allow=never",
        "-c",
        "http.lowSpeedLimit=1000",
        "-c",
        "http.lowSpeedTime=30",
        "clone",
        "--depth",
        "1",
        "--no-tags",
    ]);
    if let Some(branch) = branch {
        command.args(["--branch", branch]);
    }
    command
        .arg("--")
        .arg(url)
        .arg(destination)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", null)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|_| "无法启动 Git，请先安装 Git 后重试")?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|_| "读取 Git 状态失败")? {
            return if status.success() {
                Ok(())
            } else {
                Err("仓库下载失败，请检查地址、分支及网络；当前仅支持无需登录的 HTTPS 仓库".into())
            };
        }
        if started.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("仓库下载超过 60 秒，请下载到本地后安装".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resolve repository/tree links without letting credentials, query strings or traversal through.
    #[test]
    fn git_urls_are_explicit_and_subdirectories_stay_relative() {
        let (url, directory, branch, name) = git_source(
            "https://github.com/example/skills/tree/main/packages/demo",
            None,
        )
        .unwrap();
        assert_eq!(url, "https://github.com/example/skills.git");
        assert_eq!(directory, "packages/demo");
        assert_eq!(branch.as_deref(), Some("main"));
        assert_eq!(name, "skills");
        assert_eq!(
            git_source("https://example.com/skills.git", Some("packages/中文"))
                .unwrap()
                .1,
            "packages/中文"
        );
        for url in [
            "http://example.com/repo",
            "file:///tmp/repo",
            "https://user:password@example.com/repo",
            "https://example.com/repo?token=secret",
            "https://example.com/repo#branch",
        ] {
            assert!(git_source(url, None).is_err());
        }
        assert!(git_source("https://example.com/repo", Some("../outside")).is_err());
        assert!(git_source("https://example.com/repo", Some("C:\\outside")).is_err());
    }
}
