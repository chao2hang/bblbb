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
];

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

impl AppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        Config::builder()
            .add_source(File::with_name(".env").required(false))
            .add_source(
                Environment::with_prefix("BBLBB")
                    .separator("__")
                    .list_separator(","),
            )
            .build()?
            .try_deserialize()
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
        }
    }
}

fn default_bind_address() -> SocketAddr {
    "127.0.0.1:8080".parse().expect("default address is valid")
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
}
