use std::time::Duration;

use sqlx::{
    mysql::{MySqlConnectOptions, MySqlPoolOptions},
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Either, MySqlPool, SqlitePool,
};
use std::future::Future;
use std::str::FromStr;
use tracing::info;
use tracing::warn;

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

/// 数据库连接池与慢查询配置（M01-DB-02）
///
/// 由 `AppConfig` 的 `db_*` 字段构建；`validate()` 在启动时拒绝非法组合。
#[derive(Clone, Debug)]
pub struct DbOptions {
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout: Duration,
    /// `None` 表示不主动剔除空闲连接（等价的配置值 0）。
    pub idle_timeout: Option<Duration>,
    /// 连接最大生存时间；`None` 表示不限。
    pub max_lifetime: Option<Duration>,
    /// 慢查询阈值；查询执行超过该时长会输出 `tracing::warn`（M15 观测接入点）。
    pub slow_query: Duration,
}

impl Default for DbOptions {
    fn default() -> Self {
        Self {
            max_connections: 8,
            min_connections: 1,
            connect_timeout: Duration::from_secs(10),
            idle_timeout: Some(Duration::from_secs(300)),
            max_lifetime: Some(Duration::from_secs(1800)),
            slow_query: Duration::from_millis(500),
        }
    }
}

impl DbOptions {
    /// 校验连接池参数；返回最先命中的错误说明。
    pub fn validate(&self) -> Result<(), String> {
        if self.max_connections == 0 {
            return Err("db_max_connections must be >= 1".to_owned());
        }
        if self.min_connections == 0 {
            return Err("db_min_connections must be >= 1".to_owned());
        }
        if self.min_connections > self.max_connections {
            return Err(format!(
                "db_min_connections ({}) must be <= db_max_connections ({})",
                self.min_connections, self.max_connections
            ));
        }
        if self.connect_timeout.is_zero() {
            return Err("db_connect_timeout must be > 0".to_owned());
        }
        if self.slow_query.is_zero() {
            return Err("db_slow_query_ms must be > 0".to_owned());
        }
        Ok(())
    }
}

/// 校验数据库 URL 的 scheme；仅接受 sqlite / mysql / mariadb。
pub fn validate_database_url(url: &str) -> Result<(), String> {
    let supported =
        url.starts_with("sqlite:") || url.starts_with("mysql://") || url.starts_with("mariadb://");
    if supported {
        Ok(())
    } else {
        Err(format!(
            "unsupported database URL scheme (expected sqlite://, mysql:// or mariadb://): {url:?}"
        ))
    }
}

/// 从数据库 URL 创建连接池（默认连接参数）
pub async fn create_pool(url: &str) -> Result<DatabasePool, sqlx::Error> {
    create_pool_with_options(url, &DbOptions::default()).await
}

/// 从数据库 URL 与连接参数创建连接池
pub async fn create_pool_with_options(
    url: &str,
    opts: &DbOptions,
) -> Result<DatabasePool, sqlx::Error> {
    opts.validate()
        .map_err(|msg| sqlx::Error::Configuration(msg.into()))?;
    validate_database_url(url).map_err(|msg| sqlx::Error::Configuration(msg.into()))?;

    if url.starts_with("sqlite:") {
        create_sqlite_pool(url, opts).await.map(Either::Left)
    } else {
        create_mysql_pool(url, opts).await.map(Either::Right)
    }
}

/// 执行并测量一个异步查询闭包；超过 slow_query 阈值时输出 `tracing::warn`。
///
/// 慢查询观测的统一入口：M15 接入指标时在此补充计数/直方图，业务代码无需改动。
pub async fn with_slow_query_log<T, F, Fut>(opts: &DbOptions, label: &str, f: F) -> Fut::Output
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T>,
{
    let start = std::time::Instant::now();
    let out = f().await;
    let elapsed = start.elapsed();
    if elapsed > opts.slow_query {
        warn!(
            label,
            elapsed_ms = elapsed.as_millis() as u64,
            threshold_ms = opts.slow_query.as_millis() as u64,
            "slow query"
        );
    }
    out
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

async fn create_sqlite_pool(url: &str, opts: &DbOptions) -> Result<SqlitePool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(url)?
        .foreign_keys(true)
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5))
        .pragma("timezone", "UTC");

    info!(url = %redact_dsn(url), "creating SQLite connection pool");

    SqlitePoolOptions::new()
        .max_connections(opts.max_connections)
        .min_connections(opts.min_connections)
        .acquire_timeout(opts.connect_timeout)
        .idle_timeout(opts.idle_timeout)
        .connect_with(options)
        .await
}

async fn create_mysql_pool(url: &str, opts: &DbOptions) -> Result<MySqlPool, sqlx::Error> {
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
        .max_connections(opts.max_connections)
        .min_connections(opts.min_connections)
        .acquire_timeout(opts.connect_timeout)
        .idle_timeout(opts.idle_timeout)
        .max_lifetime(opts.max_lifetime)
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

    // ── M01-DB-02：数据库 URL 与连接池参数校验 ──────────────────────────────

    #[test]
    fn validate_database_url_accepts_supported_schemes() {
        for url in [
            "sqlite://../data/bblbb.sqlite",
            "sqlite:../data/bblbb.sqlite",
            "mysql://bblbb:s3cret@db.example.com:3306/bblbb",
            "mariadb://bblbb:s3cret@db.example.com:3306/bblbb",
        ] {
            assert!(
                validate_database_url(url).is_ok(),
                "{url} should be accepted"
            );
        }
    }

    #[test]
    fn validate_database_url_rejects_unknown_schemes() {
        for url in [
            "postgres://user@host/db",
            "file:///tmp/x.sqlite",
            "redis://x",
        ] {
            let err = validate_database_url(url).unwrap_err();
            assert!(
                err.contains("unsupported database URL scheme"),
                "{url}: {err}"
            );
        }
    }

    #[test]
    fn db_options_validates_pool_bounds() {
        let base = DbOptions::default();
        assert!(base.validate().is_ok());

        let zero_max = DbOptions {
            max_connections: 0,
            ..base.clone()
        };
        assert!(zero_max
            .validate()
            .unwrap_err()
            .contains("db_max_connections"));

        let zero_min = DbOptions {
            min_connections: 0,
            ..base.clone()
        };
        assert!(zero_min
            .validate()
            .unwrap_err()
            .contains("db_min_connections"));

        let min_gt_max = DbOptions {
            min_connections: 5,
            max_connections: 3,
            ..base.clone()
        };
        let err = min_gt_max.validate().unwrap_err();
        assert!(err.contains("<= db_max_connections"), "{err}");

        let zero_timeout = DbOptions {
            connect_timeout: Duration::ZERO,
            ..base.clone()
        };
        assert!(zero_timeout
            .validate()
            .unwrap_err()
            .contains("db_connect_timeout"));

        let zero_slow = DbOptions {
            slow_query: Duration::ZERO,
            ..base.clone()
        };
        assert!(zero_slow
            .validate()
            .unwrap_err()
            .contains("db_slow_query_ms"));
    }

    #[tokio::test]
    async fn create_pool_rejects_invalid_options() {
        let bad = DbOptions {
            max_connections: 0,
            ..DbOptions::default()
        };
        let err = create_pool_with_options("sqlite://../data/bblbb.sqlite", &bad)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("db_max_connections"));

        let err = create_pool_with_options("postgres://x/y", &DbOptions::default())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("unsupported database URL scheme"));
    }

    #[tokio::test]
    async fn slow_query_warn_helper_measures_elapsed() {
        // 慢查询阈值设为 1ms，模拟一次超过阈值的异步操作；
        // 正常路径返回结果，观测副作用由 tracing 输出（不在此断言）。
        let opts = DbOptions {
            slow_query: Duration::from_millis(1),
            ..DbOptions::default()
        };
        let value = with_slow_query_log(&opts, "test", || async { 42 }).await;
        assert_eq!(value, 42);
    }
}
