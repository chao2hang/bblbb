use std::{net::SocketAddr, path::PathBuf};

use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

use crate::db::pool::{validate_database_url, DbOptions};

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
}
