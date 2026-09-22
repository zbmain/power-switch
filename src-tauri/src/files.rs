use crate::model::AppResult;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub path: PathBuf,
    pub bytes: Option<Vec<u8>>,
}

impl Snapshot {
    /// Read a regular config file, treating absence differently from an empty file.
    pub fn read(path: &Path) -> AppResult<Self> {
        match fs::symlink_metadata(path) {
            Ok(meta) => {
                if meta.file_type().is_symlink() || !meta.is_file() {
                    return Err(format!(
                        "配置必须是普通文件（不接受符号链接）：{}",
                        path.display()
                    ));
                }
                if meta.len() > MAX_FILE_BYTES {
                    return Err(format!("配置文件超过 8 MiB：{}", path.display()));
                }
                let bytes =
                    fs::read(path).map_err(|e| format!("读取 {} 失败：{e}", path.display()))?;
                Ok(Self {
                    path: path.to_path_buf(),
                    bytes: Some(bytes),
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self {
                path: path.to_path_buf(),
                bytes: None,
            }),
            Err(e) => Err(format!("无法检查 {}：{e}", path.display())),
        }
    }

    /// Hash both presence and bytes so an empty file cannot match a missing file.
    pub fn fingerprint(&self) -> String {
        fingerprint(self.bytes.as_deref())
    }

    /// Detect a change since preview without exposing the file's contents.
    pub fn unchanged(&self) -> AppResult<bool> {
        Ok(Self::read(&self.path)?.fingerprint() == self.fingerprint())
    }

    /// Decode existing text for format-specific adapters.
    pub fn text(&self) -> AppResult<Option<&str>> {
        self.bytes
            .as_deref()
            .map(|b| std::str::from_utf8(b).map_err(|_| "配置文件必须使用 UTF-8".to_string()))
            .transpose()
    }
}

/// Compute an opaque content hash used by compare-before-write guards.
pub fn fingerprint(bytes: Option<&[u8]>) -> String {
    let mut hash = Sha256::new();
    hash.update([u8::from(bytes.is_some())]);
    if let Some(b) = bytes {
        hash.update(b);
    }
    format!("{:x}", hash.finalize())
}

/// Create app-owned storage directories with user-only Unix access.
pub fn private_dir(path: &Path) -> AppResult<()> {
    fs::create_dir_all(path).map_err(|e| format!("创建目录 {} 失败：{e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("设置目录权限失败：{e}"))?;
    }
    Ok(())
}

/// Replace one file using a private, synced temporary file in the same directory.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let parent = path.parent().ok_or("配置路径没有父目录")?;
    fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败：{e}"))?;
    // Recheck file type immediately before replacement to avoid clobbering symlinks.
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_file() => {
            return Err(format!("目标必须是普通文件：{}", path.display()));
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("无法检查目标文件：{e}")),
    }
    let mut file =
        tempfile::NamedTempFile::new_in(parent).map_err(|e| format!("创建临时文件失败：{e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("设置文件权限失败：{e}"))?;
    }
    file.write_all(bytes)
        .and_then(|_| file.as_file().sync_all())
        .map_err(|e| format!("写入临时文件失败：{e}"))?;
    file.persist(path)
        .map_err(|e| format!("替换 {} 失败：{}", path.display(), e.error))?;
    #[cfg(unix)]
    {
        fs::File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|e| format!("同步目录失败：{e}"))?;
    }
    Ok(())
}

/// Restore an original absence or bytes; callers must check concurrent modifications.
pub fn write_optional(path: &Path, bytes: Option<&[u8]>) -> AppResult<()> {
    match bytes {
        Some(bytes) => atomic_write(path, bytes),
        None => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("恢复原始文件缺失状态失败：{e}")),
        },
    }
}

/// Redact sensitive keys recursively and omit TOML comments from UI previews.
pub fn redacted(bytes: Option<&[u8]>) -> String {
    let Some(bytes) = bytes else {
        return "（文件不存在）".into();
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return "（非 UTF-8 文件，内容隐藏）".into();
    };
    let parsed = serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .or_else(|| {
            toml::from_str::<toml::Value>(text)
                .ok()
                .and_then(|v| serde_json::to_value(v).ok())
        });
    match parsed {
        Some(mut value) => {
            redact_value(&mut value);
            serde_json::to_string_pretty(&value).unwrap_or_default()
        }
        None => "（无法解析，内容隐藏）".into(),
    }
}

/// Hide credential fields at any nesting depth, including unknown provider extensions.
fn redact_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, v) in map.iter_mut() {
                let k = key.to_ascii_lowercase().replace(['_', '-'], "");
                if [
                    "key",
                    "token",
                    "secret",
                    "password",
                    "authorization",
                    "credential",
                    "auth",
                    "cookie",
                    "header",
                ]
                .iter()
                .any(|s| k.contains(s))
                {
                    *v = serde_json::Value::String("••••••••".into());
                } else {
                    redact_value(v);
                }
            }
        }
        serde_json::Value::Array(values) => values.iter_mut().for_each(redact_value),
        serde_json::Value::String(text) => {
            // Existing third-party settings can embed credentials in a URL even though
            // our model form rejects them. Never echo these through the preview.
            if let Ok(mut url) = url::Url::parse(text) {
                if matches!(url.scheme(), "http" | "https")
                    && (!url.username().is_empty()
                        || url.password().is_some()
                        || url.query().is_some()
                        || url.fragment().is_some())
                {
                    let _ = url.set_username("");
                    let _ = url.set_password(None);
                    url.set_query(None);
                    url.set_fragment(None);
                    *text = format!("{url}（认证信息与查询参数已隐藏）");
                }
            }
        }
        _ => {}
    }
}
