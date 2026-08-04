use std::time::Duration;

use sqlx::{
    mysql::{MySqlConnectOptions, MySqlPoolOptions},
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Either, MySqlPool, SqlitePool,
};
use std::str::FromStr;
use tracing::info;

/// 数据库连接池类型
///
/// 使用 `Either` 同时支持 SQLite 和 MySQL/MariaDB。
pub type DatabasePool = Either<SqlitePool, MySqlPool>;

/// 数据库类型
#[derive(Clone, Debug)]
pub enum DatabaseKind {
    Sqlite,
    MySql,
}

/// 从数据库 URL 创建连接池
pub async fn create_pool(url: &str) -> Result<DatabasePool, sqlx::Error> {
    if url.starts_with("sqlite://") || url.starts_with("sqlite:") {
        create_sqlite_pool(url).await.map(Either::Left)
    } else if url.starts_with("mysql://") || url.starts_with("mariadb://") {
        create_mysql_pool(url).await.map(Either::Right)
    } else {
        Err(sqlx::Error::Configuration(
            format!("unsupported database URL scheme: {url}").into(),
        ))
    }
}

/// 脱敏数据库 DSN，避免在日志中泄漏密码等敏感信息（M15-OBSERVE-02）
///
/// - `sqlite://...` → `sqlite://**`
/// - `mysql://user:password@host/db` → `mysql://user:***@host/db`
/// - 其他 URL → `scheme://**`；无法解析 → `**`
pub fn redact_dsn(url: &str) -> String {
    if url.starts_with("sqlite://") || url.starts_with("sqlite:") {
        return "sqlite://**".to_owned();
    }
    let Some((scheme, rest)) = url.split_once("://") else {
        return "**".to_owned();
    };
    if let Some((userinfo, host_part)) = rest.split_once('@') {
        let user = userinfo.split(':').next().unwrap_or("");
        return format!("{scheme}://{user}:***@{host_part}");
    }
    format!("{scheme}://**")
}

async fn create_sqlite_pool(url: &str) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(url)?
        .foreign_keys(true)
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5))
        .pragma("timezone", "UTC");

    info!(url = %redact_dsn(url), "creating SQLite connection pool");

    SqlitePoolOptions::new()
        .max_connections(8)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(10))
        .idle_timeout(Some(Duration::from_secs(300)))
        .connect_with(options)
        .await
}

async fn create_mysql_pool(url: &str) -> Result<MySqlPool, sqlx::Error> {
    // mariadb:// 和 mysql:// 使用相同协议
    let normalized_url = if let Some(rest) = url.strip_prefix("mariadb://") {
        format!("mysql://{rest}")
    } else {
        url.to_string()
    };

    let options = MySqlConnectOptions::from_str(&normalized_url)?
        .charset("utf8mb4")
        .collation("utf8mb4_bin");

    info!(url = %redact_dsn(&normalized_url), "creating MySQL/MariaDB connection pool");

    MySqlPoolOptions::new()
        .max_connections(16)
        .min_connections(2)
        .acquire_timeout(Duration::from_secs(10))
        .idle_timeout(Some(Duration::from_secs(300)))
        .max_lifetime(Some(Duration::from_secs(1800)))
        .connect_with(options)
        .await
}

/// 判断连接池的数据库类型
pub fn kind(pool: &DatabasePool) -> DatabaseKind {
    match pool {
        Either::Left(_) => DatabaseKind::Sqlite,
        Either::Right(_) => DatabaseKind::MySql,
    }
}

/// Ping 数据库验证连接
pub async fn ping(pool: &DatabasePool) -> Result<(), sqlx::Error> {
    match pool {
        Either::Left(p) => {
            sqlx::query("SELECT 1").execute(p).await?;
        }
        Either::Right(p) => {
            sqlx::query("SELECT 1").execute(p).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_dsn_masks_credentials() {
        assert_eq!(redact_dsn("sqlite://../data/bblbb.sqlite"), "sqlite://**");
        assert_eq!(redact_dsn("sqlite:../data/bblbb.sqlite"), "sqlite://**");
        assert_eq!(
            redact_dsn("mysql://bblbb:s3cret@db.example.com:3306/bblbb"),
            "mysql://bblbb:***@db.example.com:3306/bblbb"
        );
        assert_eq!(redact_dsn("mysql://db.example.com/bblbb"), "mysql://**");
        assert_eq!(redact_dsn("not-a-url"), "**");
    }
}
