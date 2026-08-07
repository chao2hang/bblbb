//! M07-LEDGER：账本内核。
//!
//! 构建在 0047 迁移之上（currencies/point_accounts/point_operations/
//! point_transactions/point_balance_snapshots），见
//! `docs/SCHEMA.md §9` 与 `docs/MARKETPLACE-ACCOUNTING.md`：
//!
//! - 整数最小单位、不可变流水、幂等唯一键、账户 version 乐观并发；
//! - 无充值/提现/现金兑换/普通用户转账/现实价值承诺；
//! - SQLite `BEGIN IMMEDIATE`、MySQL/MariaDB 行锁。

pub mod service;
