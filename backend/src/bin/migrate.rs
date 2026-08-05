//! BBLBB 数据库迁移工具（`bblbb-migrate`）。
//!
//! 当前实现 `--check` 只读检查（M01-DB-05）：校验迁移文件的版本、顺序与
//! checksum，不创建迁移表、不写入任何数据。显式应用命令由 M01-DB-06 提供。

use std::path::PathBuf;
use std::process::ExitCode;

use sqlx::Either;

use bblbb_backend::db::migrate::{self, CheckMode};
use bblbb_backend::db::pool::{create_pool_with_options, DatabasePool};
use bblbb_backend::AppConfig;

const USAGE: &str = "\
bblbb-migrate — BBLBB 数据库迁移工具

用法：
  bblbb-migrate --check [--db-url <URL>] [--migrations-dir <DIR>]

选项：
  --check            只读检查迁移文件的版本、顺序与 checksum，不改变数据库
  --db-url <URL>     覆盖数据库连接串（默认取自 BBLBB__DATABASE_URL / .env）
  --migrations-dir <DIR>  覆盖迁移目录（默认取自 BBLBB__MIGRATIONS_DIR / .env）
  -h, --help         显示帮助
";

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }

    if !args.iter().any(|a| a == "--check") {
        eprintln!("error: expected --check（当前仅实现只读检查）");
        eprintln!();
        eprint!("{USAGE}");
        return ExitCode::FAILURE;
    }

    let database_url_override = flag_value(&args, "--db-url");
    let migrations_dir_override = flag_value(&args, "--migrations-dir").map(PathBuf::from);

    let config = match AppConfig::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("failed to load configuration: {error}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(error) = config.validate_db_config() {
        eprintln!("invalid database configuration: {error}");
        return ExitCode::FAILURE;
    }

    let database_url = database_url_override.unwrap_or(config.database_url.clone());
    let migrations_dir = migrations_dir_override.unwrap_or_else(|| config.migrations_dir.clone());

    // 1) 读取迁移文件（纯文件操作，不触碰数据库）
    let files = match migrate::read_migration_files(&migrations_dir) {
        Ok(files) => files,
        Err(error) => {
            eprintln!("failed to read migration files: {error}");
            return ExitCode::FAILURE;
        }
    };

    // 2) 顺序检查：版本严格递增且无重复（纯文件校验，不触碰数据库）
    if let Err(error) = migrate::validate_file_order(&files) {
        eprintln!("migration order invalid: {error}");
        return ExitCode::FAILURE;
    }

    let first = files.first().map(|f| f.version).unwrap_or(0);
    let last = files.last().map(|f| f.version).unwrap_or(0);

    // 3) 连接数据库并执行只读检查（不创建迁移表、不写入）
    let pool = match create_pool_with_options(&database_url, &config.db_options()).await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("failed to create database pool: {error}");
            return ExitCode::FAILURE;
        }
    };

    let result = match migrate::check_migrations_with_mode(CheckMode::ReadOnly, &pool, &files).await
    {
        Ok(result) => result,
        Err(error) => {
            eprintln!("migration check failed: {error}");
            close_pool(&pool).await;
            return ExitCode::FAILURE;
        }
    };

    close_pool(&pool).await;

    // 4) 输出报告
    println!(
        "migration files: {} (versions {first}..{last}, ordered)",
        files.len()
    );
    println!("applied: {} / {}", result.applied_count, result.total_count);
    if result.pending.is_empty() {
        println!("pending: none");
    } else {
        println!("pending: {:?}", result.pending);
    }
    for (version, name, db_checksum, file_checksum) in &result.checksum_mismatches {
        println!(
            "checksum mismatch: version {version} ({name}) db={}.. file={}..",
            &db_checksum[..8.min(db_checksum.len())],
            &file_checksum[..8.min(file_checksum.len())]
        );
    }
    if !result.future_versions.is_empty() {
        println!(
            "future versions (applied but not in files): {:?}",
            result.future_versions
        );
    }

    if result.is_consistent() {
        println!("OK: migration state is consistent（只读检查，未修改数据库）");
        ExitCode::SUCCESS
    } else {
        eprintln!("ERROR: migration state is inconsistent");
        ExitCode::FAILURE
    }
}

/// 读取 `--flag <value>` 形式参数的值。
fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let position = args.iter().position(|arg| arg == flag)?;
    args.get(position + 1).cloned()
}

async fn close_pool(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => p.close().await,
        Either::Right(p) => p.close().await,
    }
}
