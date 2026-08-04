use std::{net::SocketAddr, path::PathBuf};

use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

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
}

impl AppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        Config::builder()
            .add_source(File::with_name(".env").required(false))
            .add_source(Environment::with_prefix("BBLBB").separator("__"))
            .build()?
            .try_deserialize()
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
    }
}
