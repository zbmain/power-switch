use super::{Error, Result};

/// Keep login credentials out of the model library and ordinary configuration files.
pub trait Vault: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>>;
    fn set(&self, account: &str, secret: &str) -> Result<()>;
    fn remove(&self, account: &str) -> Result<()>;
}

pub struct SystemVault;

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl SystemVault {
    /// Use the canonical server URL as the account within an app-specific keychain service.
    fn entry(account: &str) -> Result<keyring::Entry> {
        keyring::Entry::new("com.powerswitch.desktop.new-api", account)
            .map_err(|_| Error::new("keychain", "无法打开系统钥匙串"))
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl Vault for SystemVault {
    /// Distinguish a missing login from a locked or inaccessible keychain.
    fn get(&self, account: &str) -> Result<Option<String>> {
        match Self::entry(account)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(Error::new("keychain", "无法读取系统钥匙串，请解锁后重试")),
        }
    }

    /// Persist the session without ever including keychain errors or secrets in UI output.
    fn set(&self, account: &str, secret: &str) -> Result<()> {
        Self::entry(account)?
            .set_password(secret)
            .map_err(|_| Error::new("keychain", "无法保存登录会话到系统钥匙串"))
    }

    /// Disconnect locally without rotating the account's shared management token.
    fn remove(&self, account: &str) -> Result<()> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(Error::new("keychain", "无法从系统钥匙串移除登录会话")),
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl Vault for SystemVault {
    /// Fail explicitly on unsupported credential stores instead of writing plaintext sessions.
    fn get(&self, _account: &str) -> Result<Option<String>> {
        Ok(None)
    }
    /// New API persistent login currently supports macOS and Windows system credential stores.
    fn set(&self, _account: &str, _secret: &str) -> Result<()> {
        Err(Error::new(
            "keychain",
            "此平台尚未实现 New API 系统凭证存储",
        ))
    }
    /// There is no persisted credential to remove on an unsupported platform.
    fn remove(&self, _account: &str) -> Result<()> {
        Ok(())
    }
}
