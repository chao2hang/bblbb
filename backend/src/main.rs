use std::process::ExitCode;

use bblbb_backend::observability::{self, LogFormat};
use bblbb_backend::{build_router_full, storage::StorageService, AppConfig};

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

    // M02-MFA-PK：Passkey 配置非法立即失败（未配置 rp_id = 关闭，合法）。
    if let Err(error) = config.validate_passkey_config() {
        eprintln!("invalid passkey configuration: {error}");
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

    // M15-OBSERVE-01：结构化日志。`BBLBB__LOG_FORMAT=json` 时输出每行一个
    // JSON 事件（timestamp/service/level/request_id/route + 脱敏字段）。
    observability::init(&config.log_filter, LogFormat::parse(&config.log_format));

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
            // M15-OBSERVE-04：数据库连接失败指标（metric 白名单见 observability/metrics.rs）
            bblbb_backend::observability::metrics::registry()
                .counter_inc("bblbb_db_connect_failures_total", 1);
            tracing::warn!(
                url = %bblbb_backend::db::pool::redact_dsn(&config.database_url),
                %error,
                "failed to create database pool, starting without database"
            );
            None
        }
    };

    // M03-AUTHZ-02：数据库可用时幂等种子内置角色与权限（INSERT OR IGNORE；
    // 注册表单一事实来源 backend/src/authz/roles.rs + PERMISSION_REGISTRY）。
    // 数据库已连接但 schema 未就绪（未迁移）时启动失败，避免半可用服务。
    if let Some(pool) = &db_pool {
        if let Err(error) = bblbb_backend::authz::roles::seed_builtin_roles(pool).await {
            tracing::error!(error = %error, "failed to seed builtin roles and permissions");
            return ExitCode::FAILURE;
        }
        tracing::info!("builtin roles and permissions seeded");
    }

    // M13-THEME：数据库可用时幂等写入内置 default 主题（INSERT OR IGNORE；
    // 不覆盖管理员显式默认主题与已有 Token 编辑——见 theme::ensure_default_theme）。
    if let Some(pool) = &db_pool {
        if let Err(error) = bblbb_backend::theme::ensure_default_theme(pool).await {
            tracing::error!(error = %error, "failed to ensure builtin default theme");
            return ExitCode::FAILURE;
        }
        tracing::info!("builtin default theme ensured");
    }

    // M15-PACKAGE-04 / M15-UPGRADE-06：`--worker` 模式。
    // 独立 worker 进程：停止领取 → 收尾运行中任务（受 drain_timeout 约束）→
    // 退出；租约到期由其他 worker 安全重领（backend/src/jobs/worker_loop.rs）。
    if std::env::args().any(|arg| arg == "--worker") {
        return run_worker_mode(config, db_pool).await;
    }

    // P0 整改：`--bootstrap` 模式（docs/OPERATIONS.md §15）。
    // 生成一次性首管理员引导 token：只存 SHA-256 哈希，明文打印一次；
    // 已有 active administrator 时拒绝（实例已初始化）。需先完成迁移。
    if std::env::args().any(|arg| arg == "--bootstrap") {
        return run_bootstrap_mode(db_pool).await;
    }

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

    // 初始化对象存储服务（优先读取数据库中保存的在线配置，回退环境变量与默认值）。
    let initial_storage_cfg = if let Some(pool) = &db_pool {
        match bblbb_backend::routes::admin_storage::load_storage_settings(pool).await {
            Ok(Some(row)) => {
                tracing::info!(backend = %row.storage_backend, "从数据库加载在线存储配置");
                bblbb_backend::routes::admin_storage::build_storage_config_from_db(&config, &row)
            }
            _ => config.storage_config(),
        }
    } else {
        config.storage_config()
    };

    // P0 整改：Feature Flag 从 `feature_flags` 表加载（运行时持久化事实来源）；
    // 表缺失/查询失败回退默认全关。kill switch（环境变量）在加载后叠加。
    let flags = if let Some(pool) = &db_pool {
        let mut loaded = bblbb_backend::config::flags::FeatureFlags::load(pool).await;
        if config.feature_kill_switch {
            loaded.emergency_off("system", "BBLBB__FEATURE_KILL_SWITCH=true at startup", 0);
        }
        loaded
    } else {
        config.feature_flags()
    };

    let storage = match StorageService::new(&initial_storage_cfg).await {
        Ok(storage) => Some(storage),
        Err(error) => {
            tracing::error!(%error, "failed to initialize storage service");
            return ExitCode::FAILURE;
        }
    };

    // `into_make_service_with_connect_info`：为 /metrics 提供真实对端地址以
    // 实施 loopback 访问限制（M15-PACKAGE-07 / M15-OBSERVE-04）。
    // P0 整改：同进程启动通用 Outbox 消费者（事务提交事件的最终投递）；
    // 独立信号监听（重复安装信号处理器是安全的），收到 SIGTERM/SIGINT 后
    // 与 HTTP 服务器一起优雅退出。
    if let Some(pool) = db_pool.clone() {
        let outbox_shutdown = bblbb_backend::jobs::worker_loop::worker_shutdown_signal().await;
        let settings_key = config.settings_encryption_key.clone();
        tokio::spawn(async move {
            bblbb_backend::jobs::outbox_consumer::run(pool, settings_key, outbox_shutdown).await;
        });
    }

    if let Err(error) = axum::serve(
        listener,
        build_router_full(config, db_pool, flags, storage)
            .into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown)
    .await
    {
        tracing::error!(%error, "server stopped unexpectedly");
        return ExitCode::FAILURE;
    }

    tracing::info!("server shutdown complete");
    ExitCode::SUCCESS
}

/// `--bootstrap` 模式：生成一次性首管理员引导 token（P0 整改）。
///
/// 明文 token 只输出到 stdout 一次（运维职责：立即转移到 root 可读的
/// 安全位置，不得进入普通日志/前端环境）；数据库只存 SHA-256 哈希，
/// 24 小时有效、消费一次后永久失效。要求迁移已应用（bootstrap_tokens 表）。
async fn run_bootstrap_mode(db_pool: Option<bblbb_backend::db::pool::DatabasePool>) -> ExitCode {
    let Some(pool) = db_pool else {
        eprintln!("bootstrap mode requires a configured and reachable database");
        return ExitCode::FAILURE;
    };
    let now = bblbb_backend::outbox::now_millis();
    match bblbb_backend::bootstrap::create_bootstrap_token(&pool, now).await {
        Ok((_token_id, token)) => {
            println!("================================================================");
            println!("BBLBB bootstrap token (shown once, valid 24h):");
            println!();
            println!("{token}");
            println!();
            println!("Create the first administrator (no session needed):");
            println!("  POST /api/v1/auth/bootstrap");
            println!("  Header: Idempotency-Key: <random string, 16-200 chars>");
            println!("  {{\"token\":\"<token>\",\"username\":\"<name>\",\"email\":\"<addr>\",\"password\":\"<pw>\"}}");
            println!("================================================================");
            ExitCode::SUCCESS
        }
        Err(error) => {
            tracing::error!(
                %error,
                "failed to create bootstrap token (apply migrations first? administrator already exists?)"
            );
            ExitCode::FAILURE
        }
    }
}

/// `--worker` 模式（M15-PACKAGE-04 / M15-UPGRADE-06）。
///
/// 与 HTTP 服务进程分离的 worker 进程：对每个队列运行
/// [`bblbb_backend::jobs::worker_loop::run_worker`]，收到 SIGTERM/SIGINT 后
/// 停止领取新任务、在 `drain_timeout` 内完成在途任务、退出。租约到期任务由
/// 其他 worker 安全重领（不丢任务）。任务分发见
/// [`bblbb_backend::jobs::dispatch`]。
async fn run_worker_mode(
    _config: AppConfig,
    db_pool: Option<bblbb_backend::db::pool::DatabasePool>,
) -> ExitCode {
    use bblbb_backend::jobs::dispatch::WORKER_QUEUES;
    use bblbb_backend::jobs::worker::ClaimedJob;
    use bblbb_backend::jobs::worker_loop::{run_worker, WorkerConfig};

    let Some(pool) = db_pool else {
        tracing::error!("worker mode requires a configured and reachable database");
        return ExitCode::FAILURE;
    };

    if let Err(error) = bblbb_backend::authz::roles::seed_builtin_roles(&pool).await {
        tracing::error!(error = %error, "failed to seed builtin roles and permissions");
        return ExitCode::FAILURE;
    }

    let shutdown = bblbb_backend::jobs::worker_loop::worker_shutdown_signal().await;
    let mut handles = Vec::new();

    // P0 整改：worker 模式同样运行通用 Outbox 消费者。
    {
        let outbox_pool = pool.clone();
        let outbox_shutdown = shutdown.clone();
        let settings_key = _config.settings_encryption_key.clone();
        handles.push(tokio::spawn(async move {
            bblbb_backend::jobs::outbox_consumer::run(outbox_pool, settings_key, outbox_shutdown)
                .await;
        }));
    }

    let dispatch_settings_key = _config.settings_encryption_key.clone();
    for queue in WORKER_QUEUES {
        let dispatch_settings_key = dispatch_settings_key.clone();
        let worker_pool = pool.clone();
        let closure_pool = pool.clone();
        let worker_shutdown = shutdown.clone();
        let queue = queue.to_string();
        handles.push(tokio::spawn(async move {
            let worker_config = WorkerConfig {
                worker_id: format!("worker-{queue}-{}", std::process::id()),
                queue,
                ..WorkerConfig::default()
            };
            run_worker(
                &worker_pool,
                worker_config,
                worker_shutdown,
                move |job: ClaimedJob| {
                    let job_pool = closure_pool.clone();
                    let settings_key = dispatch_settings_key.clone();
                    async move {
                        bblbb_backend::jobs::dispatch::dispatch_job(&job_pool, &settings_key, job)
                            .await
                    }
                },
            )
            .await;
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }

    tracing::info!("all worker queues drained, worker shutdown complete");
    ExitCode::SUCCESS
}
