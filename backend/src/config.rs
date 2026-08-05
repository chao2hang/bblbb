use std::{net::SocketAddr, path::PathBuf};

use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

use crate::db::pool::{validate_database_url, DbOptions};

/// 配置登记条目（M01-CONFIG-01）：环境变量 → 类型化字段 → 默认值 →
/// 环境适用范围 → 运行时变更方式。
#[derive(Debug, Clone, Copy)]
pub struct ConfigEntry {
    /// 环境变量名（`BBLBB__` 前缀完整形式）
    pub env_var: &'static str,
    /// `AppConfig` 中的类型化字段
    pub field: &'static str,
    /// 默认值（无默认 = 未设置/空）
    pub default: &'static str,
    /// 环境适用范围：`all` / `dev` / `ci` / `production`
    pub scope: &'static str,
    /// 运行时变更方式：`restart`（重启生效）/ `reload`（在线重载）/ `rotation`（密钥轮换流程）
    pub reload: &'static str,
}

/// 当前已实现的配置登记表（事实来源）。
///
/// 不变量（由测试强制）：
/// 1. `BBLBB__<后缀>` 的后缀小写后必须等于 `AppConfig` 字段名；
/// 2. 每个登记项必须在 `backend/.env.example` 中记录；
/// 3. `.env.example` 不得出现未登记的环境变量。
pub const CONFIG_REGISTRY: &[ConfigEntry] = &[
    ConfigEntry {
        env_var: "BBLBB__BIND_ADDRESS",
        field: "bind_address",
        default: "127.0.0.1:8080",
        scope: "all",
        reload: "restart",
    },
    ConfigEntry {
        env_var: "BBLBB__LOG_FILTER",
        field: "log_filter",
        default: "bblbb_backend=info,tower_http=info",
        scope: "all",
        reload: "restart",
    },
    ConfigEntry {
        env_var: "BBLBB__OPENAPI_PATH",
        field: "openapi_path",
        default: "../openapi/openapi.yaml",
        scope: "all",
        reload: "restart",
    },
    ConfigEntry {
        env_var: "BBLBB__DATABASE_URL",
        field: "database_url",
        default: "sqlite://../data/bblbb.sqlite",
        scope: "all",
        reload: "restart",
    },
    ConfigEntry {
        env_var: "BBLBB__MIGRATIONS_DIR",
        field: "migrations_dir",
        default: "../migrations/sqlite",
        scope: "all",
        reload: "restart",
    },
    ConfigEntry {
        env_var: "BBLBB__STORAGE_DIR",
        field: "storage_dir",
        default: "../uploads",
        scope: "all",
        reload: "restart",
    },
    // 生产服务启动不得自动应用未知迁移（M01-DB-06）；生产环境应显式运行
    // `bblbb-migrate apply`，故 AUTO_MIGRATE 仅限 dev/ci。
    ConfigEntry {
        env_var: "BBLBB__AUTO_MIGRATE",
        field: "auto_migrate",
        default: "false",
        scope: "dev,ci",
        reload: "restart",
    },
    ConfigEntry {
        env_var: "BBLBB__ALLOWED_HOSTS",
        field: "allowed_hosts",
        default: "（空 = 宽松模式，仅记录）",
        scope: "all",
        reload: "restart",
    },
    ConfigEntry {
        env_var: "BBLBB__ALLOWED_ORIGINS",
        field: "allowed_origins",
        default: "（空 = 宽松模式，仅记录）",
        scope: "all",
        reload: "restart",
    },
    ConfigEntry {
        env_var: "BBLBB__DB_MAX_CONNECTIONS",
        field: "db_max_connections",
        default: "8",
        scope: "all",
        reload: "restart",
    },
    ConfigEntry {
        env_var: "BBLBB__DB_MIN_CONNECTIONS",
        field: "db_min_connections",
        default: "1",
        scope: "all",
        reload: "restart",
    },
    ConfigEntry {
        env_var: "BBLBB__DB_CONNECT_TIMEOUT_MS",
        field: "db_connect_timeout_ms",
        default: "10000",
        scope: "all",
        reload: "restart",
    },
    ConfigEntry {
        env_var: "BBLBB__DB_IDLE_TIMEOUT_MS",
        field: "db_idle_timeout_ms",
        default: "300000",
        scope: "all",
        reload: "restart",
    },
    ConfigEntry {
        env_var: "BBLBB__DB_SLOW_QUERY_MS",
        field: "db_slow_query_ms",
        default: "500",
        scope: "all",
        reload: "restart",
    },
    // 运行环境：development / test / production。生产模式触发 M01-CONFIG-02 校验
    // （拒绝未知键/占位 Secret/不安全 Origin/非 loopback 端口/冲突配置）。
    ConfigEntry {
        env_var: "BBLBB__ENV",
        field: "env",
        default: "development",
        scope: "all",
        reload: "restart",
    },
];

/// 允许的运行环境
pub const ACCEPTED_ENVS: &[&str] = &["development", "test", "production"];

#[derive(Clone, Debug, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_bind_address")]
    pub bind_address: SocketAddr,
    #[serde(default = "default_log_filter")]
    pub log_filter: String,
    #[serde(default = "default_openapi_path")]
    pub openapi_path: PathBuf,
    #[serde(default = "default_database_url")]
    pub database_url: String,
    #[serde(default = "default_migrations_dir")]
    pub migrations_dir: PathBuf,
    #[serde(default = "default_storage_dir")]
    pub storage_dir: PathBuf,
    /// 启动时是否自动应用数据库迁移（M01-DB-06：生产默认关闭）
    #[serde(default = "default_auto_migrate")]
    pub auto_migrate: bool,
    /// 严格模式下允许的 Host 头集合（默认空 = 宽松模式，仅记录日志）
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    /// 严格模式下允许的 Origin 集合（默认空 = 宽松模式，仅记录日志）
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    /// 运行环境（development / test / production；M01-CONFIG-02）
    #[serde(default = "default_env")]
    pub env: String,
    // ── M01-DB-02：数据库连接池与慢查询参数（经 AppConfig::validate 校验）──
    #[serde(default = "default_db_max_connections")]
    pub db_max_connections: u32,
    #[serde(default = "default_db_min_connections")]
    pub db_min_connections: u32,
    #[serde(default = "default_db_connect_timeout_ms")]
    pub db_connect_timeout_ms: u64,
    /// 空闲连接剔除时间（毫秒）；0 = 不主动剔除。
    #[serde(default = "default_db_idle_timeout_ms")]
    pub db_idle_timeout_ms: u64,
    /// 慢查询阈值（毫秒）；超过时输出 tracing::warn。
    #[serde(default = "default_db_slow_query_ms")]
    pub db_slow_query_ms: u64,
}

/// 构建 `BBLBB__` 环境变量源。
///
/// config-rs 0.15 需要 `try_parsing(true)` 才能启用布尔/整数解析与列表拆分；
/// `list_separator(",")` 只对登记的列表键生效（`with_list_parse_key`），
/// 其余变量保持字符串类型。
fn environment_source() -> Environment {
    Environment::with_prefix("BBLBB")
        .separator("__")
        .try_parsing(true)
        .list_separator(",")
        .with_list_parse_key("allowed_hosts")
        .with_list_parse_key("allowed_origins")
}

impl AppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let config = Config::builder()
            .add_source(File::with_name(".env").required(false))
            .add_source(environment_source())
            .build()?;

        // M01-CONFIG-02：生产模式拒绝未知配置键。
        let env_value = config.get_string("env").unwrap_or_else(|_| default_env());
        if env_value == "production" {
            reject_unknown_keys(&config)?;
        }

        config.try_deserialize()
    }

    /// 是否生产模式（M01-CONFIG-02）。
    pub fn is_production(&self) -> bool {
        self.env == "production"
    }

    /// 生产模式校验：拒绝占位 Secret、不安全 Origin、非 loopback 内部端口
    /// 和冲突配置。启动时调用，任何一项失败立即退出。
    pub fn validate_production(&self) -> Result<(), String> {
        let mut errors = Vec::new();

        if !ACCEPTED_ENVS.contains(&self.env.as_str()) {
            errors.push(format!(
                "env must be one of {:?}, got {:?}",
                ACCEPTED_ENVS, self.env
            ));
        }

        // 占位 Secret：数据库 DSN 含占位密码/示例主机
        if is_placeholder_secret(&self.database_url) {
            errors.push("database_url contains a placeholder secret or example host".to_owned());
        }

        // 不安全 Origin：生产必须 HTTPS（loopback 除外）
        for origin in &self.allowed_origins {
            if !is_secure_origin(origin) {
                errors.push(format!(
                    "insecure allowed_origin (must be https://): {origin}"
                ));
            }
        }

        // 非 loopback 内部端口：生产禁止对外监听
        if !is_loopback_address(&self.bind_address) {
            errors.push(format!(
                "bind_address must be loopback in production: {}",
                self.bind_address
            ));
        }

        // 冲突配置：生产不得自动应用迁移（M01-DB-06，显式 bblbb-migrate apply）
        if self.auto_migrate {
            errors.push(
                "auto_migrate must be false in production (apply migrations via bblbb-migrate apply)"
                    .to_owned(),
            );
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    /// 数据库连接相关的配置校验（M01-DB-02）：启动时调用，非法配置立即失败。
    pub fn validate_db_config(&self) -> Result<(), String> {
        validate_database_url(&self.database_url)?;
        let options = self.db_options();
        options.validate()
    }

    /// 将 db_* 字段组装为连接池参数。
    pub fn db_options(&self) -> DbOptions {
        DbOptions {
            max_connections: self.db_max_connections,
            min_connections: self.db_min_connections,
            connect_timeout: std::time::Duration::from_millis(self.db_connect_timeout_ms),
            idle_timeout: if self.db_idle_timeout_ms == 0 {
                None
            } else {
                Some(std::time::Duration::from_millis(self.db_idle_timeout_ms))
            },
            max_lifetime: if self.db_idle_timeout_ms == 0 {
                None
            } else {
                // 仅 MySQL/MariaDB 使用；沿用默认 30 分钟。
                Some(std::time::Duration::from_secs(1800))
            },
            slow_query: std::time::Duration::from_millis(self.db_slow_query_ms),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            bind_address: default_bind_address(),
            log_filter: default_log_filter(),
            openapi_path: default_openapi_path(),
            database_url: default_database_url(),
            migrations_dir: default_migrations_dir(),
            storage_dir: default_storage_dir(),
            auto_migrate: default_auto_migrate(),
            allowed_hosts: Vec::new(),
            allowed_origins: Vec::new(),
            db_max_connections: default_db_max_connections(),
            db_min_connections: default_db_min_connections(),
            db_connect_timeout_ms: default_db_connect_timeout_ms(),
            db_idle_timeout_ms: default_db_idle_timeout_ms(),
            db_slow_query_ms: default_db_slow_query_ms(),
            env: default_env(),
        }
    }
}

fn default_bind_address() -> SocketAddr {
    "127.0.0.1:8080".parse().expect("default address is valid")
}

fn default_env() -> String {
    "development".to_owned()
}

fn default_log_filter() -> String {
    "bblbb_backend=info,tower_http=info".to_owned()
}

fn default_openapi_path() -> PathBuf {
    PathBuf::from("../openapi/openapi.yaml")
}

fn default_database_url() -> String {
    "sqlite://../data/bblbb.sqlite".to_owned()
}

fn default_migrations_dir() -> PathBuf {
    PathBuf::from("../migrations/sqlite")
}

fn default_storage_dir() -> PathBuf {
    PathBuf::from("../uploads")
}

fn default_auto_migrate() -> bool {
    false
}

fn default_db_max_connections() -> u32 {
    8
}

fn default_db_min_connections() -> u32 {
    1
}

fn default_db_connect_timeout_ms() -> u64 {
    10_000
}

fn default_db_idle_timeout_ms() -> u64 {
    300_000
}

fn default_db_slow_query_ms() -> u64 {
    500
}

/// 生产模式拒绝未知配置键（M01-CONFIG-02）。
///
/// 收集配置的全部顶层键（环境变量经 `BBLBB__` 前缀剥离并小写化，.env 文件
/// 键同理），与 `CONFIG_REGISTRY` 字段比对；出现未登记键即失败。
fn reject_unknown_keys(config: &Config) -> Result<(), ConfigError> {
    use std::collections::HashMap;
    let map: HashMap<String, config::Value> = config
        .clone()
        .try_deserialize()
        .map_err(|e| ConfigError::Message(format!("production config is invalid: {e}")))?;
    let known: Vec<&str> = CONFIG_REGISTRY.iter().map(|e| e.field).collect();
    let unknown: Vec<String> = map
        .keys()
        .map(|key| key.to_lowercase().trim_start_matches("bblbb__").to_string())
        .filter(|normalized| !known.contains(&normalized.as_str()))
        .collect();
    if unknown.is_empty() {
        Ok(())
    } else {
        Err(ConfigError::Message(format!(
            "production mode rejects unknown config keys: {}",
            unknown.join(", ")
        )))
    }
}

/// 数据库 DSN 是否含占位 Secret 或示例主机（生产拒绝）。
fn is_placeholder_secret(database_url: &str) -> bool {
    let lower = database_url.to_lowercase();
    const PLACEHOLDER_MARKERS: &[&str] = &[
        "changeme",
        "your_password",
        "your-password",
        "yourpassword",
        "password123",
        "secret123",
        "example.com",
        "example.org",
        "placeholder",
        "insert_your",
        "put_your",
        "xxxxx",
    ];
    PLACEHOLDER_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

/// Origin 是否安全（生产必须 HTTPS；localhost/loopback 允许明文）。
fn is_secure_origin(origin: &str) -> bool {
    let lower = origin.trim().to_lowercase();
    lower.starts_with("https://")
        || lower.starts_with("http://localhost")
        || lower.starts_with("http://127.0.0.1")
        || lower.starts_with("http://[::1]")
}

/// 监听地址是否 loopback（生产禁止对外监听内部端口）。
fn is_loopback_address(addr: &std::net::SocketAddr) -> bool {
    addr.ip().is_loopback()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_runnable() {
        let config = AppConfig::default();
        assert_eq!(config.bind_address.port(), 8080);
        assert_eq!(
            config.openapi_path,
            PathBuf::from("../openapi/openapi.yaml")
        );
        assert!(config.database_url.starts_with("sqlite://"));
        assert!(!config.auto_migrate);
        assert!(config.validate_db_config().is_ok());
    }

    #[test]
    fn db_options_round_trip_from_fields() {
        let config = AppConfig::default();
        let options = config.db_options();
        assert_eq!(options.max_connections, 8);
        assert_eq!(options.min_connections, 1);
        assert_eq!(options.connect_timeout, std::time::Duration::from_secs(10));
        assert_eq!(options.slow_query, std::time::Duration::from_millis(500));
        assert!(options.idle_timeout.is_some());
    }

    #[test]
    fn db_options_zero_idle_timeout_means_no_eviction() {
        let config = AppConfig {
            db_idle_timeout_ms: 0,
            ..AppConfig::default()
        };
        let options = config.db_options();
        assert!(options.idle_timeout.is_none());
    }

    #[test]
    fn validate_db_config_rejects_bad_pool_bounds() {
        let config = AppConfig {
            db_min_connections: 9,
            db_max_connections: 4,
            ..AppConfig::default()
        };
        let err = config.validate_db_config().unwrap_err();
        assert!(err.contains("db_min_connections"), "{err}");
    }

    #[test]
    fn validate_db_config_rejects_bad_url() {
        let config = AppConfig {
            database_url: "postgres://user@host/db".to_owned(),
            ..AppConfig::default()
        };
        let err = config.validate_db_config().unwrap_err();
        assert!(err.contains("unsupported database URL scheme"), "{err}");
    }

    // ── M01-CONFIG-01：配置登记表不变量 ──

    /// 命名约定：`BBLBB__<后缀>` 的后缀小写后必须等于 AppConfig 字段名。
    #[test]
    fn registry_env_var_suffix_matches_field_name() {
        for entry in CONFIG_REGISTRY {
            let suffix = entry
                .env_var
                .strip_prefix("BBLBB__")
                .unwrap_or_else(|| panic!("{} 缺少 BBLBB__ 前缀", entry.env_var));
            assert_eq!(
                suffix.to_lowercase(),
                entry.field,
                "{} 的后缀应等于字段 {}",
                entry.env_var,
                entry.field
            );
        }
    }

    /// 登记表字段唯一，且每个字段都有登记项。
    #[test]
    fn registry_fields_are_unique_and_cover_app_config() {
        let mut fields: Vec<&str> = CONFIG_REGISTRY.iter().map(|e| e.field).collect();
        fields.sort_unstable();
        fields.dedup();
        assert_eq!(fields.len(), CONFIG_REGISTRY.len(), "登记表存在重复字段");

        let expected = [
            "allowed_hosts",
            "allowed_origins",
            "auto_migrate",
            "bind_address",
            "database_url",
            "db_connect_timeout_ms",
            "db_idle_timeout_ms",
            "db_max_connections",
            "db_min_connections",
            "db_slow_query_ms",
            "env",
            "log_filter",
            "migrations_dir",
            "openapi_path",
            "storage_dir",
        ];
        assert_eq!(
            fields, expected,
            "登记表必须覆盖 AppConfig 的全部环境变量映射字段"
        );
    }

    /// `.env.example` 与登记表双向同步：
    /// 每个登记项都记录在 .env.example；示例不得出现未登记变量。
    #[test]
    fn registry_syncs_with_env_example() {
        let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let example_path = std::path::Path::new(&manifest).join(".env.example");
        let content = std::fs::read_to_string(&example_path)
            .unwrap_or_else(|e| panic!("读取 .env.example 失败: {e}"));

        let documented: std::collections::BTreeSet<&str> = content
            .lines()
            .filter_map(|line| {
                let stripped = line.trim().trim_start_matches('#').trim();
                let var = stripped.split('=').next().unwrap_or("");
                var.strip_prefix("BBLBB__").map(|_| var)
            })
            .collect();

        for entry in CONFIG_REGISTRY {
            assert!(
                documented.contains(entry.env_var),
                "登记项 {} 未在 .env.example 中记录",
                entry.env_var
            );
        }
        for var in &documented {
            assert!(
                CONFIG_REGISTRY.iter().any(|e| e.env_var == *var),
                ".env.example 记录了未登记的变量 {}",
                var
            );
        }
    }

    // ── M01-CONFIG-02：生产模式校验 ──

    #[test]
    fn production_accepts_clean_config() {
        let config = AppConfig {
            env: "production".to_owned(),
            allowed_origins: vec!["https://forum.example.com".to_owned()],
            bind_address: "127.0.0.1:8080".parse().unwrap(),
            database_url: "mysql://user:real-secret@db.internal:3306/bblbb".to_owned(),
            ..AppConfig::default()
        };
        assert!(config.validate_production().is_ok());
    }

    #[test]
    fn production_rejects_placeholder_secret() {
        for dsn in [
            "mysql://user:changeme@db.internal:3306/bblbb",
            "mysql://user:Your_Password@example.com:3306/bblbb",
        ] {
            let config = AppConfig {
                env: "production".to_owned(),
                database_url: dsn.to_owned(),
                ..AppConfig::default()
            };
            let err = config.validate_production().unwrap_err();
            assert!(err.contains("placeholder secret"), "{err}");
        }
    }

    #[test]
    fn production_rejects_insecure_origin() {
        let config = AppConfig {
            env: "production".to_owned(),
            allowed_origins: vec!["http://forum.example.com".to_owned()],
            ..AppConfig::default()
        };
        let err = config.validate_production().unwrap_err();
        assert!(err.contains("insecure allowed_origin"), "{err}");

        // HTTPS 与 loopback 明文都接受
        let ok = AppConfig {
            env: "production".to_owned(),
            allowed_origins: vec![
                "https://forum.example.com".to_owned(),
                "http://localhost:5173".to_owned(),
            ],
            ..AppConfig::default()
        };
        assert!(ok.validate_production().is_ok());
    }

    #[test]
    fn production_rejects_non_loopback_bind() {
        let config = AppConfig {
            env: "production".to_owned(),
            bind_address: "0.0.0.0:8080".parse().unwrap(),
            ..AppConfig::default()
        };
        let err = config.validate_production().unwrap_err();
        assert!(err.contains("loopback"), "{err}");
    }

    #[test]
    fn production_rejects_auto_migrate_conflict() {
        let config = AppConfig {
            env: "production".to_owned(),
            auto_migrate: true,
            ..AppConfig::default()
        };
        let err = config.validate_production().unwrap_err();
        assert!(err.contains("auto_migrate must be false"), "{err}");
    }

    #[test]
    fn production_rejects_invalid_env_value() {
        let config = AppConfig {
            env: "staging".to_owned(),
            ..AppConfig::default()
        };
        let err = config.validate_production().unwrap_err();
        assert!(err.contains("env must be one of"), "{err}");
    }

    #[test]
    fn production_rejects_unknown_keys() {
        use config::Config;
        let cfg = Config::builder()
            .set_override("bogus_key", "x")
            .unwrap()
            .build()
            .unwrap();
        let err = reject_unknown_keys(&cfg).unwrap_err();
        assert!(
            format!("{err}").contains("bogus_key"),
            "未知键必须被拒绝: {err}"
        );
    }

    #[test]
    fn unknown_key_detection_is_case_and_prefix_insensitive() {
        use config::Config;
        // BBLBB__ 前缀 + 大写：归一化后必须等于登记字段（已知）或被识别为未知
        let known = Config::builder()
            .set_override("BBLBB__BIND_ADDRESS", "127.0.0.1:8080")
            .unwrap()
            .build()
            .unwrap();
        assert!(
            reject_unknown_keys(&known).is_ok(),
            "BBLBB__BIND_ADDRESS 是登记键，不得被当作未知"
        );

        let unknown = Config::builder()
            .set_override("BBLBB__PROBE_ONLY", "1")
            .unwrap()
            .build()
            .unwrap();
        let err = reject_unknown_keys(&unknown).unwrap_err();
        assert!(
            format!("{err}").contains("probe_only"),
            "未知键应归一化为小写后拒绝: {err}"
        );
    }

    /// 逗号分隔的列表键（allowed_hosts/allowed_origins）经环境变量正确解析为 Vec，
    /// 其余键保持字符串/原生类型（config-rs 0.15 的 try_parsing + list_parse_key）。
    #[test]
    fn env_list_keys_parse_as_vec_and_others_stay_scalar() {
        use config::Config;
        let mut fake = std::collections::HashMap::new();
        fake.insert(
            "BBLBB__ALLOWED_HOSTS".to_string(),
            "a.example.com,b.example.com".to_string(),
        );
        fake.insert(
            "BBLBB__ALLOWED_ORIGINS".to_string(),
            "https://a.example.com,https://b.example.com".to_string(),
        );
        fake.insert("BBLBB__LOG_FILTER".to_string(), "info,debug".to_string());
        fake.insert("BBLBB__AUTO_MIGRATE".to_string(), "true".to_string());
        fake.insert("BBLBB__DB_MAX_CONNECTIONS".to_string(), "16".to_string());
        fake.insert("BBLBB__ENV".to_string(), "test".to_string());

        let cfg = Config::builder()
            .add_source(environment_source().source(Some(fake)))
            .build()
            .unwrap();
        let hosts: Vec<String> = cfg.get("allowed_hosts").unwrap();
        let origins: Vec<String> = cfg.get("allowed_origins").unwrap();
        let log_filter: String = cfg.get("log_filter").unwrap();
        let auto_migrate: bool = cfg.get("auto_migrate").unwrap();
        let db_max: u32 = cfg.get("db_max_connections").unwrap();
        let env: String = cfg.get("env").unwrap();

        assert_eq!(hosts, vec!["a.example.com", "b.example.com"]);
        assert_eq!(
            origins,
            vec!["https://a.example.com", "https://b.example.com"]
        );
        assert_eq!(log_filter, "info,debug", "非列表键必须保持字符串（含逗号）");
        assert!(auto_migrate);
        assert_eq!(db_max, 16, "整数键必须解析为原生类型");
        assert_eq!(env, "test");
    }
}
