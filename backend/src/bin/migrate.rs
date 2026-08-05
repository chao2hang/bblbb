//! BBLBB 数据库迁移工具（`bblbb-migrate`）。
//!
//! - `apply`：显式应用待执行迁移（M01-DB-06）。数据库超前（未来版本）或
//!   checksum 不匹配时拒绝应用，返回非零退出码。
//! - `--check`：只读检查迁移文件的版本、顺序与 checksum（M01-DB-05），
//!   不创建迁移表、不写入任何数据。

use std::path::PathBuf;
use std::process::ExitCode;

use sqlx::Either;

use bblbb_backend::db::migrate::{self, CheckMode};
use bblbb_backend::db::pool::{create_pool_with_options, DatabasePool};
use bblbb_backend::AppConfig;

const USAGE: &str = "\
bblbb-migrate — BBLBB 数据库迁移工具

用法：
  bblbb-migrate apply [--db-url <URL>] [--migrations-dir <DIR>]
  bblbb-migrate --check [--db-url <URL>] [--migrations-dir <DIR>]

子命令：
  apply              显式应用待执行迁移（幂等；checksum 不匹配或数据库
                    超前于迁移文件时拒绝应用，不部分执行）
  --check            只读检查迁移文件的版本、顺序与 checksum，不改变数据库

选项：
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

    let check = args.iter().any(|a| a == "--check");
    let apply = args.iter().any(|a| a == "apply");
    if check == apply {
        eprintln!("error: 需要且只能指定一个操作：apply 或 --check");
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

    // 3) 连接数据库
    let pool = match create_pool_with_options(&database_url, &config.db_options()).await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!("failed to create database pool: {error}");
            return ExitCode::FAILURE;
        }
    };

    if apply {
        apply_command(&pool, &files, first, last).await
    } else {
        check_command(&pool, &files, first, last).await
    }
}

/// `bblbb-migrate apply`：显式应用待执行迁移。
async fn apply_command(
    pool: &DatabasePool,
    files: &[migrate::MigrationFile],
    first: u64,
    last: u64,
) -> ExitCode {
    println!(
        "applying migrations: {} files (versions {first}..{last}, ordered)",
        files.len()
    );

    let result = migrate::run_migrations(pool, files).await;

    // 无论成功失败都关闭连接池
    close_pool(pool).await;

    match result {
        Ok(applied) => {
            println!("applied {applied} migration(s)");
            println!("OK: database is up to date");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("migration failed: {error}");
            eprintln!("database left unchanged（迁移在事务内执行，失败即回滚）");
            ExitCode::FAILURE
        }
    }
}

/// `bblbb-migrate --check`：只读检查（不创建迁移表、不写入）。
async fn check_command(
    pool: &DatabasePool,
    files: &[migrate::MigrationFile],
    first: u64,
    last: u64,
) -> ExitCode {
    println!(
        "migration files: {} (versions {first}..{last}, ordered)",
        files.len()
    );

    let result = match migrate::check_migrations_with_mode(CheckMode::ReadOnly, pool, files).await {
        Ok(result) => result,
        Err(error) => {
            eprintln!("migration check failed: {error}");
            close_pool(pool).await;
            return ExitCode::FAILURE;
        }
    };

    close_pool(pool).await;

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
