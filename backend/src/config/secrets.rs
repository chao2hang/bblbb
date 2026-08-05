//! Secret provider 接口与内置实现（M01-CONFIG-03）。
//!
//! 目标：
//! - 统一的 `SecretProvider` trait，写接口只读、不落日志；
//! - 内置 provider：受限环境文件（一个 Secret 一个文件，文件名 = 名称）、
//!   systemd credentials（`/run/credentials/<unit>/<name>`）、环境变量（兜底）；
//! - `ChainProvider` 按序尝试，第一个命中即返回；
//! - 生产模式强制受限文件为 owner-only 权限（0600/0400）；
//! - 为后续托管 Secret（如 Vault/云 Secret Manager）预留扩展点：
//!   实现 `SecretProvider` 即可，无需改动调用方。

use std::path::{Path, PathBuf};

/// Secret 读取错误。
#[derive(Debug)]
pub enum SecretError {
    /// Secret 不存在或来源未配置
    NotFound(String),
    /// 读取失败
    Io(String),
    /// 生产模式权限不安全（非 owner-only）
    InsecurePermissions(String),
}

impl std::fmt::Display for SecretError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecretError::NotFound(name) => write!(f, "secret not found: {name}"),
            SecretError::Io(message) => write!(f, "secret read failed: {message}"),
            SecretError::InsecurePermissions(path) => {
                write!(
                    f,
                    "secret file has insecure permissions (must be owner-only): {path}"
                )
            }
        }
    }
}

impl std::error::Error for SecretError {}

/// Secret 值：持原始字节，不实现 Debug/Display 输出内容。
#[derive(Clone)]
pub struct SecretValue {
    bytes: Vec<u8>,
    /// 来源类别（GET 元数据返回）
    pub source_class: &'static str,
    /// 最近修改时间（Unix 秒；0 = 未知）
    pub updated_at: i64,
    /// 来源版本（文件 mtime 或单调计数；0 = 未知）
    pub version: u64,
}

impl SecretValue {
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// UTF-8 文本视图（多数 Secret 是文本）。
    pub fn as_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.bytes)
    }

    fn new(bytes: Vec<u8>, source_class: &'static str, updated_at: i64, version: u64) -> Self {
        Self {
            bytes,
            source_class,
            updated_at,
            version,
        }
    }
}

impl std::fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "SecretValue {{ source_class: {}, updated_at: {}, version: {} }}",
            self.source_class, self.updated_at, self.version
        )
    }
}

/// Secret provider：按名称读取 Secret 值，只读不写。
///
/// 扩展托管 Secret（Vault / 云 Secret Manager）：实现本 trait 后注册进
/// `ChainProvider` 即可，调用方与写入接口不感知来源。
pub trait SecretProvider: Send + Sync {
    /// 来源类别，例如 `env_file` / `systemd_credentials` / `managed` / `env`。
    fn source_class(&self) -> &'static str;

    /// 读取 Secret；未配置或不存在返回 `Ok(None)`。
    fn get(&self, name: &str) -> Result<Option<SecretValue>, SecretError>;

    /// 是否已配置该名称（不含值；供写接口判断写路径）。
    fn is_configured(&self, name: &str) -> bool {
        matches!(self.get(name), Ok(Some(_)))
    }
}

/// 环境变量 provider（兜底；生产不建议作为 Secret 主来源）。
pub struct EnvProvider {
    /// 变量名前缀，例如 `BBLBB__`
    prefix: String,
}

impl EnvProvider {
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_owned(),
        }
    }

    fn var_name(&self, name: &str) -> String {
        format!("{}{}", self.prefix, name.to_uppercase())
    }
}

impl SecretProvider for EnvProvider {
    fn source_class(&self) -> &'static str {
        "env"
    }

    fn get(&self, name: &str) -> Result<Option<SecretValue>, SecretError> {
        let var = self.var_name(name);
        match std::env::var(&var) {
            Ok(value) => Ok(Some(SecretValue::new(
                value.into_bytes(),
                self.source_class(),
                0,
                0,
            ))),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(std::env::VarError::NotUnicode(_)) => {
                Err(SecretError::Io(format!("env var {var} is not unicode")))
            }
        }
    }
}

/// 受限环境文件 provider：一个 Secret 一个文件，文件名 = Secret 名称。
///
/// systemd `EnvironmentFile` 的受限替代：目录本身应为 `0700`，
/// Secret 文件应为 `0600`/`0400`。生产模式强制校验，失败返回
/// `InsecurePermissions`。
pub struct FileSecretProvider {
    base_dir: PathBuf,
    require_secure_permissions: bool,
}

impl FileSecretProvider {
    pub fn new(base_dir: impl Into<PathBuf>, require_secure_permissions: bool) -> Self {
        Self {
            base_dir: base_dir.into(),
            require_secure_permissions,
        }
    }

    fn secret_path(&self, name: &str) -> PathBuf {
        self.base_dir.join(name)
    }

    fn ensure_secure_permissions(path: &Path) -> Result<(), SecretError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let meta = std::fs::metadata(path)
                .map_err(|e| SecretError::Io(format!("{}: {e}", path.display())))?;
            // 不允许 group/other 的任何读/写/执行位
            if meta.mode() & 0o077 != 0 {
                return Err(SecretError::InsecurePermissions(path.display().to_string()));
            }
        }
        Ok(())
    }
}

impl SecretProvider for FileSecretProvider {
    fn source_class(&self) -> &'static str {
        "env_file"
    }

    fn get(&self, name: &str) -> Result<Option<SecretValue>, SecretError> {
        if !self.base_dir.is_dir() {
            return Ok(None);
        }
        let path = self.secret_path(name);
        if !path.is_file() {
            return Ok(None);
        }
        if self.require_secure_permissions {
            Self::ensure_secure_permissions(&path)?;
        }
        let meta = std::fs::metadata(&path)
            .map_err(|e| SecretError::Io(format!("{}: {e}", path.display())))?;
        let bytes = std::fs::read(&path)
            .map_err(|e| SecretError::Io(format!("{}: {e}", path.display())))?;
        #[cfg(unix)]
        let modified = {
            use std::os::unix::fs::MetadataExt;
            meta.mtime()
        };
        #[cfg(not(unix))]
        let modified = 0i64;
        Ok(Some(SecretValue::new(
            bytes,
            self.source_class(),
            modified,
            modified.max(0) as u64,
        )))
    }
}

/// systemd credentials provider：读取 `/run/credentials/<unit>/<name>`。
///
/// 由 systemd `LoadCredential=` / `SetCredential=` 注入；base_dir 可覆盖用于测试。
pub struct SystemdCredentialProvider {
    base_dir: PathBuf,
}

impl SystemdCredentialProvider {
    /// 以指定 unit 构造；读取 `/run/credentials/<unit>/`。
    pub fn for_unit(unit: &str) -> Self {
        Self {
            base_dir: PathBuf::from(format!("/run/credentials/{unit}")),
        }
    }

    /// 以自定义目录构造（测试用）。
    pub fn with_base_dir(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }
}

impl SecretProvider for SystemdCredentialProvider {
    fn source_class(&self) -> &'static str {
        "systemd_credentials"
    }

    fn get(&self, name: &str) -> Result<Option<SecretValue>, SecretError> {
        let path = self.base_dir.join(name);
        if !path.is_file() {
            return Ok(None);
        }
        let meta = std::fs::metadata(&path)
            .map_err(|e| SecretError::Io(format!("{}: {e}", path.display())))?;
        let bytes = std::fs::read(&path)
            .map_err(|e| SecretError::Io(format!("{}: {e}", path.display())))?;
        #[cfg(unix)]
        let modified = {
            use std::os::unix::fs::MetadataExt;
            meta.mtime()
        };
        #[cfg(not(unix))]
        let modified = 0i64;
        Ok(Some(SecretValue::new(
            bytes,
            self.source_class(),
            modified,
            modified.max(0) as u64,
        )))
    }
}

/// 链式 provider：按注册顺序尝试，第一个命中即返回。
///
/// 顺序决定优先级；前面的来源未配置时自动落到下一个。
pub struct ChainProvider {
    providers: Vec<Box<dyn SecretProvider>>,
}

impl ChainProvider {
    pub fn new(providers: Vec<Box<dyn SecretProvider>>) -> Self {
        Self { providers }
    }
}

impl SecretProvider for ChainProvider {
    fn source_class(&self) -> &'static str {
        "chain"
    }

    fn get(&self, name: &str) -> Result<Option<SecretValue>, SecretError> {
        let mut last_error: Option<SecretError> = None;
        for provider in &self.providers {
            match provider.get(name) {
                Ok(Some(value)) => return Ok(Some(value)),
                Ok(None) => continue,
                Err(e) => last_error = Some(e),
            }
        }
        match last_error {
            Some(e) => Err(e),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("bblbb-secrets-{}", uuid::Uuid::now_v7()))
    }

    #[test]
    fn env_provider_reads_and_reports_absent() {
        std::env::set_var("BBLBB_TEST_SECRET", "s3cr3t");
        let provider = EnvProvider::new("BBLBB_TEST_");
        let value = provider.get("SECRET").unwrap().expect("must be present");
        assert_eq!(value.as_str().unwrap(), "s3cr3t");
        assert_eq!(value.source_class, "env");
        assert!(provider.get("ABSENT").unwrap().is_none());
        std::env::remove_var("BBLBB_TEST_SECRET");
    }

    #[test]
    fn file_provider_reads_secrets_by_filename() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("api_key"), b"file-secret").unwrap();

        let provider = FileSecretProvider::new(&dir, false);
        let value = provider.get("api_key").unwrap().expect("present");
        assert_eq!(value.as_str().unwrap(), "file-secret");
        assert_eq!(value.source_class, "env_file");
        assert!(provider.get("missing").unwrap().is_none());
        assert!(provider.is_configured("api_key"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn file_provider_enforces_owner_only_permissions_in_secure_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let insecure_path = dir.join("insecure");
        std::fs::write(&insecure_path, b"x").unwrap();
        std::fs::set_permissions(&insecure_path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let secure_path = dir.join("secure");
        std::fs::write(&secure_path, b"y").unwrap();
        std::fs::set_permissions(&secure_path, std::fs::Permissions::from_mode(0o600)).unwrap();

        let provider = FileSecretProvider::new(&dir, true);
        let err = provider.get("insecure").unwrap_err();
        assert!(matches!(err, SecretError::InsecurePermissions(_)), "{err}");
        assert!(provider.get("secure").unwrap().is_some());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn systemd_provider_reads_from_run_credentials_layout() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("mail_token"), b"systemd-secret").unwrap();

        let provider = SystemdCredentialProvider::with_base_dir(&dir);
        let value = provider.get("mail_token").unwrap().expect("present");
        assert_eq!(value.as_str().unwrap(), "systemd-secret");
        assert_eq!(value.source_class, "systemd_credentials");
        assert!(provider.get("absent").unwrap().is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn chain_provider_uses_first_hit_and_falls_through() {
        let dir = temp_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("only_file"), b"from-file").unwrap();

        std::env::set_var("BBLBB_TEST_CHAIN_FROM_ENV", "from-env");
        let env = EnvProvider::new("BBLBB_TEST_CHAIN_");
        let file = FileSecretProvider::new(&dir, false);

        let chain = ChainProvider::new(vec![Box::new(file), Box::new(env)]);
        // 两个来源都存在：文件优先
        std::env::set_var("BBLBB_TEST_CHAIN_ONLY_FILE", "env-shadow");
        assert_eq!(
            chain.get("only_file").unwrap().unwrap().as_str().unwrap(),
            "from-file"
        );
        // 文件缺失：落到 env
        assert_eq!(
            chain.get("FROM_ENV").unwrap().unwrap().as_str().unwrap(),
            "from-env"
        );
        // 都没有
        assert!(chain.get("absent").unwrap().is_none());

        std::env::remove_var("BBLBB_TEST_CHAIN_FROM_ENV");
        std::env::remove_var("BBLBB_TEST_CHAIN_ONLY_FILE");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn secret_value_never_debugs_content() {
        let value = SecretValue::new(b"topsecret".to_vec(), "env", 0, 0);
        let debug = format!("{value:?}");
        assert!(
            !debug.contains("topsecret"),
            "Debug 不得包含 Secret 内容: {debug}"
        );
    }
}
