use std::process::ExitCode;

use bblbb_backend::{build_router, AppConfig};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    let config = match AppConfig::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("failed to load configuration: {error}");
            return ExitCode::FAILURE;
        }
    };

    // M01-DB-02：启动前校验数据库 URL 与连接池参数，非法配置立即失败。
    if let Err(error) = config.validate_db_config() {
        eprintln!("invalid database configuration: {error}");
        return ExitCode::FAILURE;
    }

    // M01-CONFIG-02：生产模式拒绝未知键/占位 Secret/不安全 Origin/
    // 非 loopback 端口/冲突配置。
    if config.is_production() {
        if let Err(error) = config.validate_production() {
            eprintln!("invalid production configuration: {error}");
            return ExitCode::FAILURE;
        }
    }

    // 迁移仅在显式开启时执行（M01-DB-06：生产服务启动不得自动应用未知迁移）。
    // 开关：环境变量 BBLBB__AUTO_MIGRATE=true 或 CLI 参数 --migrate。
    let auto_migrate = config.auto_migrate || std::env::args().any(|arg| arg == "--migrate");

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(&config.log_filter))
        .init();

    // 初始化数据库连接池
    let db_pool = match bblbb_backend::db::pool::create_pool_with_options(
        &config.database_url,
        &config.db_options(),
    )
    .await
    {
        Ok(pool) => {
            tracing::info!(
                url = %bblbb_backend::db::pool::redact_dsn(&config.database_url),
                "database pool created"
            );

            // M01-DB-04：MySQL/MariaDB 会话前置检查（字符集/时区/隔离/sql_mode）。
            if let Err(error) = bblbb_backend::db::pool::check_session(&pool).await {
                tracing::error!(error = %error, "database session pre-flight check failed");
                return ExitCode::FAILURE;
            }

            if auto_migrate {
                // 运行迁移
                let migrations_dir = &config.migrations_dir;
                match bblbb_backend::db::migrate::read_migration_files(migrations_dir) {
                    Ok(files) => {
                        if let Err(e) =
                            bblbb_backend::db::migrate::run_migrations(&pool, &files).await
                        {
                            tracing::error!(error = %e, "migration failed");
                            return ExitCode::FAILURE;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, dir = %migrations_dir.display(), "failed to read migration files, skipping migrations");
                    }
                }
            } else {
                tracing::info!(
                    "auto-migrate disabled, skipping migrations (set BBLBB__AUTO_MIGRATE=true or pass --migrate to apply)"
                );
            }
            Some(pool)
        }
        Err(error) => {
            tracing::warn!(
                url = %bblbb_backend::db::pool::redact_dsn(&config.database_url),
                %error,
                "failed to create database pool, starting without database"
            );
            None
        }
    };

    let listener = match tokio::net::TcpListener::bind(config.bind_address).await {
        Ok(listener) => listener,
        Err(error) => {
            tracing::error!(address = %config.bind_address, %error, "failed to bind server");
            return ExitCode::FAILURE;
        }
    };

    tracing::info!(address = %config.bind_address, "server listening");

    // 优雅停机：监听 SIGTERM/SIGINT
    let shutdown = async {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM handler");
            let mut sigint = signal(SignalKind::interrupt()).expect("install SIGINT handler");
            tokio::select! {
                _ = sigterm.recv() => tracing::info!("received SIGTERM, shutting down"),
                _ = sigint.recv() => tracing::info!("received SIGINT, shutting down"),
            }
        }
        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c()
                .await
                .expect("install Ctrl-C handler");
            tracing::info!("received Ctrl-C, shutting down");
        }
    };

    if let Err(error) = axum::serve(listener, build_router(config, db_pool))
        .with_graceful_shutdown(shutdown)
        .await
    {
        tracing::error!(%error, "server stopped unexpectedly");
        return ExitCode::FAILURE;
    }

    tracing::info!("server shutdown complete");
    ExitCode::SUCCESS
}
