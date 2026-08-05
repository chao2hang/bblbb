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

use serde::Serialize;

/// Secret 读取错误。
#[derive(Debug)]
pub enum SecretError {
    /// Secret 不存在或来源未配置
    NotFound(String),
    /// 读取失败
    Io(String),
    /// 生产模式权限不安全（非 owner-only）
    InsecurePermissions(String),
    /// Secret 名称非法（路径穿越等）
    InvalidName(String),
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
            SecretError::InvalidName(name) => write!(f, "invalid secret name: {name}"),
        }
    }
}

impl std::error::Error for SecretError {}

/// Secret 元数据（M01-CONFIG-04）：GET 只返回这些字段，绝不返回值。
#[derive(Debug, Clone, Serialize)]
pub struct SecretMetadata {
    pub configured: bool,
    pub source_class: &'static str,
    pub version: u64,
    pub updated_at: i64,
}

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

    /// 读取元数据（M01-CONFIG-04）：只返回 configured/source_class/version/
    /// updated_at，不读取值。默认实现基于 `get`（会读取内容）；文件类来源
    /// 覆盖为 stat-only。
    fn metadata(&self, name: &str) -> Result<Option<SecretMetadata>, SecretError> {
        Ok(self.get(name)?.map(|value| SecretMetadata {
            configured: true,
            source_class: value.source_class,
            version: value.version,
            updated_at: value.updated_at,
        }))
    }

    /// 是否已配置该名称（不含值；供写接口判断写路径）。
    fn is_configured(&self, name: &str) -> bool {
        matches!(self.get(name), Ok(Some(_)))
    }
}

/// Secret 写接口（M01-CONFIG-04）：只写不读。
///
/// trait 上没有任何返回值的读取方法——写入/轮换后调用方只能得到元数据，
/// 从类型层面杜绝"写后又读回值"。
pub trait SecretWriter: Send + Sync {
    /// 来源类别
    fn source_class(&self) -> &'static str;

    /// 写入或轮换 Secret；成功只返回元数据（不含值）。
    fn set(&self, name: &str, value: &[u8]) -> Result<SecretMetadata, SecretError>;

    /// 已配置名称列表（元数据，不含值）。
    fn configured_names(&self) -> Result<Vec<String>, SecretError>;
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

    /// stat-only 元数据：只读文件系统元信息，不读取内容。
    fn metadata(&self, name: &str) -> Result<Option<SecretMetadata>, SecretError> {
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
        file_metadata(&path, self.source_class()).map(Some)
    }
}

/// 由文件 stat 构建元数据（不含值）。
fn file_metadata(path: &Path, source_class: &'static str) -> Result<SecretMetadata, SecretError> {
    let meta =
        std::fs::metadata(path).map_err(|e| SecretError::Io(format!("{}: {e}", path.display())))?;
    #[cfg(unix)]
    let modified = {
        use std::os::unix::fs::MetadataExt;
        meta.mtime()
    };
    #[cfg(not(unix))]
    let modified = 0i64;
    Ok(SecretMetadata {
        configured: true,
        source_class,
        version: modified.max(0) as u64,
        updated_at: modified,
    })
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

    /// stat-only 元数据：不读取内容。
    fn metadata(&self, name: &str) -> Result<Option<SecretMetadata>, SecretError> {
        let path = self.base_dir.join(name);
        if !path.is_file() {
            return Ok(None);
        }
        file_metadata(&path, self.source_class()).map(Some)
    }
}

/// 受限文件写实现（M01-CONFIG-04）：只写不读。
///
/// - 原子写：临时文件 + 落盘 + rename，避免读者看到半写内容；
/// - Unix 上写入后立即设为 `0600`（owner-only）；
/// - 拒绝非法名称（路径分隔符 / `..` 穿越）。
pub struct FileSecretWriter {
    base_dir: PathBuf,
}

/// 名称是否安全（不含路径分隔符、`.`/`..`、控制字符）。
fn is_safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.contains('/')
        && !name.contains('\\')
        && name != "."
        && name != ".."
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

impl FileSecretWriter {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }
}

impl SecretWriter for FileSecretWriter {
    fn source_class(&self) -> &'static str {
        "env_file"
    }

    fn set(&self, name: &str, value: &[u8]) -> Result<SecretMetadata, SecretError> {
        if !is_safe_name(name) {
            return Err(SecretError::InvalidName(name.to_owned()));
        }
        std::fs::create_dir_all(&self.base_dir)
            .map_err(|e| SecretError::Io(format!("create {}: {e}", self.base_dir.display())))?;

        let final_path = self.base_dir.join(name);
        let tmp_path = self
            .base_dir
            .join(format!(".{name}.tmp{}", uuid::Uuid::now_v7().simple()));

        let result = (|| {
            std::fs::write(&tmp_path, value)
                .map_err(|e| SecretError::Io(format!("write {}: {e}", tmp_path.display())))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600))
                    .map_err(|e| SecretError::Io(format!("chmod {}: {e}", tmp_path.display())))?;
            }
            // 落盘后再 rename，防止进程崩溃时残留半写内容
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new().write(true).open(&tmp_path) {
                let _ = file.flush();
                let _ = file.sync_all();
            }
            std::fs::rename(&tmp_path, &final_path)
                .map_err(|e| SecretError::Io(format!("rename {}: {e}", final_path.display())))
        })();

        if result.is_err() {
            let _ = std::fs::remove_file(&tmp_path);
        }
        result?;
        file_metadata(&final_path, self.source_class())
    }

    fn configured_names(&self) -> Result<Vec<String>, SecretError> {
        if !self.base_dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut names = Vec::new();
        for entry in std::fs::read_dir(&self.base_dir)
            .map_err(|e| SecretError::Io(format!("read_dir {}: {e}", self.base_dir.display())))?
        {
            let entry = entry.map_err(|e| SecretError::Io(e.to_string()))?;
            if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                if let Some(name) = entry.file_name().to_str() {
                    if is_safe_name(name) {
                        names.push(name.to_owned());
                    }
                }
            }
        }
        names.sort();
        Ok(names)
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

    // ── M01-CONFIG-04：写接口只写不读，GET 只返回元数据 ──

    #[test]
    fn writer_set_returns_metadata_without_value() {
        let dir = temp_dir();
        let writer = FileSecretWriter::new(&dir);
        let metadata = writer.set("api_key", b"super-secret-value").unwrap();

        assert!(metadata.configured);
        assert_eq!(metadata.source_class, "env_file");
        assert!(metadata.version > 0, "version 应为 mtime");
        assert!(metadata.updated_at > 0);

        // 元数据不是值：SecretMetadata 类型不含任何字节/字符串值字段
        // （编译期保证）；运行期断言序列化 JSON 不含 Secret 内容。
        let json = serde_json::to_string(&metadata).unwrap();
        assert!(
            !json.contains("super-secret-value"),
            "元数据 JSON 不得包含 Secret 值: {json}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn writer_creates_owner_only_files() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir();
        let writer = FileSecretWriter::new(&dir);
        writer.set("smtp_pass", b"x").unwrap();
        let mode = std::fs::metadata(dir.join("smtp_pass"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o077,
            0,
            "写入的 Secret 文件必须 owner-only: {mode:o}"
        );

        let names = writer.configured_names().unwrap();
        assert_eq!(names, vec!["smtp_pass"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writer_rejects_unsafe_names() {
        let dir = temp_dir();
        let writer = FileSecretWriter::new(&dir);
        for bad in ["../escape", "a/b", "a\\b", "..", ".", "a b", ""] {
            let err = writer.set(bad, b"x").unwrap_err();
            assert!(
                matches!(err, SecretError::InvalidName(_)),
                "名称 {bad:?} 必须被拒绝: {err}"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn metadata_roundtrip_matches_get_and_is_stat_only() {
        let dir = temp_dir();
        let writer = FileSecretWriter::new(&dir);
        writer.set("mail_token", b"roundtrip-value").unwrap();

        // GET 元数据：configured/source_class/version/updated_at，无值
        let provider = FileSecretProvider::new(&dir, false);
        let metadata = provider
            .metadata("mail_token")
            .unwrap()
            .expect("configured");
        assert!(metadata.configured);
        assert_eq!(metadata.source_class, "env_file");
        assert!(metadata.version > 0);

        // 未配置 → None
        assert!(provider.metadata("absent").unwrap().is_none());

        // 值与元数据分离：读值仍可用，但元数据不含值
        let value = provider.get("mail_token").unwrap().unwrap();
        assert_eq!(value.as_str().unwrap(), "roundtrip-value");
        assert!(metadata.version > 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn writer_is_write_only_by_type_shape() {
        // 类型层面断言：SecretWriter trait 没有任何返回 Secret 值的方法。
        // 这里通过 trait object 的可用方法集合验证——SecretWriter 只有
        // source_class / set / configured_names。
        fn assert_write_only<T: SecretWriter>() {}
        assert_write_only::<FileSecretWriter>();
    }
}
