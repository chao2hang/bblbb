//! M07-LEDGER-11：账本属性、并发、故障注入与三数据库锁语义测试
//! （SQLite 全量 + 跨库 #[ignore]）。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::economy::ledger::service as ledger;
use bblbb_backend::economy::ledger::service::{
    apply_operation, get_account, AccountState, AdminGrantInput, LedgerCommand, LedgerError,
    LedgerKind, CURRENCY_COIN, CURRENCY_EXP,
};
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

#[path = "../common/mod.rs"]
mod common;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-ledger-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../migrations/sqlite"),
    )
    .unwrap();
    run_migrations(&pool, &files).await.unwrap();
    bblbb_backend::authz::roles::seed_builtin_roles(&pool)
        .await
        .unwrap();
    (pool, dir)
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
    let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
}

async fn close_pool(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => p.close().await,
        Either::Right(p) => p.close().await,
    }
}

async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', 5, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(now - 30 * 86_400 * 1000)
            .bind(now - 30 * 86_400 * 1000)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    user_id
}

async fn role_id(pool: &DatabasePool, name: &str) -> String {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
            .bind(name)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn assign_global_role(pool: &DatabasePool, user_id: &str, role_name: &str) {
    let role_id = role_id(pool, role_name).await;
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at)
                 VALUES (?, ?, NULL, ?, NULL)",
            )
            .bind(user_id)
            .bind(&role_id)
            .bind(now - 60_000)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

fn base_cmd(
    user_id: &str,
    currency_id: &str,
    key: &str,
    delta_balance: i64,
    delta_frozen: i64,
) -> LedgerCommand {
    LedgerCommand {
        idempotency_scope: "test".to_string(),
        idempotency_key: key.to_string(),
        kind: LedgerKind::Award,
        actor_id: None,
        user_id: user_id.to_string(),
        currency_id: currency_id.to_string(),
        delta_balance,
        delta_frozen,
        source_type: None,
        source_id: None,
        memo: "test op".to_string(),
        reverses_operation_id: None,
    }
}

async fn account_row(pool: &DatabasePool, user_id: &str, currency_id: &str) -> (i64, i64, i64) {
    match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT balance, frozen_balance, version FROM point_accounts WHERE user_id = ? AND currency_id = ?",
        )
        .bind(user_id)
        .bind(currency_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn operation_count(pool: &DatabasePool) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM point_operations")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

#[tokio::test]
async fn credit_debit_freeze_unfreeze_conserve_total() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let now = now_millis();

    // 入账 1000
    ledger::credit(&pool, base_cmd(&user, CURRENCY_COIN, "k1", 1000, 0), now)
        .await
        .unwrap();
    assert_eq!(account_row(&pool, &user, CURRENCY_COIN).await, (1000, 0, 1));

    // 消费 300
    ledger::debit(&pool, base_cmd(&user, CURRENCY_COIN, "k2", -300, 0), now)
        .await
        .unwrap();
    assert_eq!(account_row(&pool, &user, CURRENCY_COIN).await, (700, 0, 2));

    // 冻结 200：可用 500 / 冻结 200，总额守恒
    ledger::freeze(&pool, base_cmd(&user, CURRENCY_COIN, "k3", 0, 0), 200, now)
        .await
        .unwrap();
    assert_eq!(
        account_row(&pool, &user, CURRENCY_COIN).await,
        (500, 200, 3)
    );

    // 解冻 150：可用 650 / 冻结 50
    ledger::unfreeze(&pool, base_cmd(&user, CURRENCY_COIN, "k4", 0, 0), 150, now)
        .await
        .unwrap();
    assert_eq!(account_row(&pool, &user, CURRENCY_COIN).await, (650, 50, 4));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn prohibited_operations_rejected() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let now = now_millis();

    // 普通用户转账（transfer）不可用
    let transfer = LedgerCommand {
        kind: LedgerKind::Transfer,
        ..base_cmd(&user, CURRENCY_COIN, "t1", -100, 0)
    };
    let err = apply_operation(&pool, transfer, now).await.unwrap_err();
    assert!(matches!(err, LedgerError::Invalid(_)));

    // 现金/提现/充值形态被拒
    let cash = base_cmd(&user, CURRENCY_COIN, "t2", 100, 0);
    let cash = LedgerCommand {
        memo: "充值 100 元".to_string(),
        ..cash
    };
    let err = apply_operation(&pool, cash, now).await.unwrap_err();
    assert!(matches!(err, LedgerError::Invalid(_)));

    assert_eq!(operation_count(&pool).await, 0, "被拒命令不留流水");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn insufficient_negative_overflow_rollback() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let now = now_millis();

    ledger::credit(&pool, base_cmd(&user, CURRENCY_COIN, "a1", 100, 0), now)
        .await
        .unwrap();

    // 余额不足：拒绝且账户不变
    let err = ledger::debit(&pool, base_cmd(&user, CURRENCY_COIN, "a2", -200, 0), now)
        .await
        .unwrap_err();
    assert_eq!(err, LedgerError::InsufficientBalance);
    assert_eq!(account_row(&pool, &user, CURRENCY_COIN).await, (100, 0, 1));
    assert_eq!(operation_count(&pool).await, 1, "失败不回滚既有流水");

    // 解冻超出冻结额 → 冻结转负被拒
    ledger::freeze(&pool, base_cmd(&user, CURRENCY_COIN, "a3", 0, 0), 60, now)
        .await
        .unwrap();
    let err = ledger::unfreeze(&pool, base_cmd(&user, CURRENCY_COIN, "a4", 0, 0), 100, now)
        .await
        .unwrap_err();
    assert_eq!(err, LedgerError::NegativeBalance);

    // 溢出：i64 上界
    ledger::credit(
        &pool,
        base_cmd(&user, CURRENCY_COIN, "a5", i64::MAX - 50, 0),
        now,
    )
    .await
    .unwrap();
    let err = ledger::credit(&pool, base_cmd(&user, CURRENCY_COIN, "a6", 100, 0), now)
        .await
        .unwrap_err();
    assert_eq!(err, LedgerError::Overflow);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn idempotency_replay_and_conflict() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let now = now_millis();

    let cmd = base_cmd(&user, CURRENCY_COIN, "same-key", 100, 0);
    let first = apply_operation(&pool, cmd.clone(), now).await.unwrap();
    // 同键同摘要重放：返回原流水，不重复扣款
    let replay = apply_operation(&pool, cmd.clone(), now).await.unwrap();
    assert_eq!(replay.operation_id, first.operation_id);
    assert_eq!(account_row(&pool, &user, CURRENCY_COIN).await, (100, 0, 1));
    assert_eq!(operation_count(&pool).await, 1);

    // 同键不同摘要 → 冲突
    let diff = LedgerCommand {
        delta_balance: 200,
        ..cmd
    };
    let err = apply_operation(&pool, diff, now).await.unwrap_err();
    assert_eq!(err, LedgerError::IdempotencyConflict);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn concurrent_double_debit_only_one_succeeds() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let now = now_millis();

    ledger::credit(&pool, base_cmd(&user, CURRENCY_COIN, "c1", 100, 0), now)
        .await
        .unwrap();

    // 并发双扣：各 80，总共 160 > 100 → 恰好一个成功。
    let pool_a = pool.clone();
    let user_a = user.clone();
    let pool_b = pool.clone();
    let user_b = user.clone();
    let now_a = now + 1;
    let now_b = now + 2;
    let (r1, r2) = tokio::join!(
        async move {
            ledger::debit(
                &pool_a,
                base_cmd(&user_a, CURRENCY_COIN, "cc-1", -80, 0),
                now_a,
            )
            .await
        },
        async move {
            ledger::debit(
                &pool_b,
                base_cmd(&user_b, CURRENCY_COIN, "cc-2", -80, 0),
                now_b,
            )
            .await
        }
    );
    let ok_count = [r1.is_ok(), r2.is_ok()].iter().filter(|x| **x).count();
    assert_eq!(ok_count, 1, "并发双扣必须只有一个成功");
    assert!(r1.is_err() || r2.is_err());
    let err = if r1.is_err() { r1 } else { r2 }.unwrap_err();
    assert_eq!(err, LedgerError::InsufficientBalance);
    assert_eq!(account_row(&pool, &user, CURRENCY_COIN).await, (20, 0, 2));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn property_invariant_initial_plus_deltas_equals_balance() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let now = now_millis();

    // 确定性操作序列（credit/debit/freeze/unfreeze）。
    let ops: Vec<(i64, i64, LedgerKind)> = vec![
        (500, 0, LedgerKind::Award),
        (100, 0, LedgerKind::Award),
        (-120, 0, LedgerKind::Consume),
        (-200, 200, LedgerKind::Freeze),
        (50, -50, LedgerKind::Unfreeze),
        (-10, 10, LedgerKind::Freeze),
        (-9999, 0, LedgerKind::Consume), // 预期失败：余额不足
    ];

    let mut balance = 0i64;
    let mut frozen = 0i64;
    let mut version = 0i64;
    for (i, (db, df, kind)) in ops.iter().enumerate() {
        let before = AccountState {
            balance,
            frozen_balance: frozen,
            version,
            allow_negative: false,
        };
        let applied = match kind {
            LedgerKind::Award => {
                ledger::credit(
                    &pool,
                    base_cmd(&user, CURRENCY_COIN, &format!("p{i}"), *db, *df),
                    now + i as i64,
                )
                .await
            }
            LedgerKind::Consume => {
                ledger::debit(
                    &pool,
                    base_cmd(&user, CURRENCY_COIN, &format!("p{i}"), *db, *df),
                    now + i as i64,
                )
                .await
            }
            LedgerKind::Freeze => {
                ledger::freeze(
                    &pool,
                    base_cmd(&user, CURRENCY_COIN, &format!("p{i}"), 0, 0),
                    -*db,
                    now + i as i64,
                )
                .await
            }
            LedgerKind::Unfreeze => {
                ledger::unfreeze(
                    &pool,
                    base_cmd(&user, CURRENCY_COIN, &format!("p{i}"), 0, 0),
                    *db,
                    now + i as i64,
                )
                .await
            }
            _ => unreachable!(),
        };
        match applied {
            Ok(result) => {
                let tx = &result.transactions[0];
                // 恒等式：balance_after = balance_before + delta；frozen 同理。
                let expected_balance = before.balance + tx.delta_balance;
                let expected_frozen = before.frozen_balance + tx.delta_frozen;
                assert_eq!(
                    tx.balance_after, expected_balance,
                    "balance 恒等式（op {i}）"
                );
                assert_eq!(tx.frozen_after, expected_frozen, "frozen 恒等式（op {i}）");
                // 冻结/解冻总额守恒：balance + frozen 不变。
                if *db != 0 && *df != 0 {
                    assert_eq!(
                        tx.balance_after + tx.frozen_after,
                        before.balance + before.frozen_balance,
                        "冻结恒等式（op {i}）"
                    );
                }
                balance = tx.balance_after;
                frozen = tx.frozen_after;
                version += 1;
                assert_eq!(
                    account_row(&pool, &user, CURRENCY_COIN).await,
                    (balance, frozen, version)
                );
            }
            Err(LedgerError::InsufficientBalance) => {
                // 失败步不改余额（回滚语义）。
                assert_eq!(
                    account_row(&pool, &user, CURRENCY_COIN).await,
                    (balance, frozen, version)
                );
            }
            Err(other) => panic!("op {i}: unexpected {other:?}"),
        }
    }

    // 最终：initial(0) + Σ(delta_balance) = balance；冻结恒等式单独成立。
    let sum_delta: i64 = ops.iter().take(6).map(|(db, _, _)| *db).sum();
    let final_account = get_account(&pool, &user, CURRENCY_COIN).await.unwrap();
    assert_eq!(
        final_account.balance, sum_delta,
        "initial + Σ(delta) = balance"
    );
    // 冻结链：+200（freeze）−50（unfreeze）+10（freeze）= 160。
    assert_eq!(final_account.frozen_balance, 160, "冻结恒等式");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn admin_grant_requires_reason_permission_and_dual_review() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "target").await;
    let admin = insert_user(&pool, "admin").await;
    assign_global_role(&pool, &admin, "administrator").await;
    common::enroll_totp(&pool, &admin).await;
    let member = insert_user(&pool, "member").await;
    let now = now_millis();

    // 无 reason → Invalid
    let err = ledger::admin_grant(
        &pool,
        &admin,
        AdminGrantInput {
            user_id: user.clone(),
            currency_id: CURRENCY_COIN.to_string(),
            amount: 100,
            reason: "  ".to_string(),
            idempotency_scope: "g".to_string(),
            idempotency_key: "g1".to_string(),
        },
        now,
        false,
        None,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, LedgerError::Invalid(_)));

    // 无 points.adjust 权限（普通成员）→ Forbidden
    let err = ledger::admin_grant(
        &pool,
        &member,
        AdminGrantInput {
            user_id: user.clone(),
            currency_id: CURRENCY_COIN.to_string(),
            amount: 100,
            reason: "奖励".to_string(),
            idempotency_scope: "g".to_string(),
            idempotency_key: "g2".to_string(),
        },
        now,
        false,
        None,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, LedgerError::Forbidden(_)));

    // 双人复核：缺少第二审批人 → Invalid
    let err = ledger::admin_grant(
        &pool,
        &admin,
        AdminGrantInput {
            user_id: user.clone(),
            currency_id: CURRENCY_COIN.to_string(),
            amount: 100,
            reason: "奖励".to_string(),
            idempotency_scope: "g".to_string(),
            idempotency_key: "g3".to_string(),
        },
        now,
        true,
        None,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, LedgerError::Invalid(_)));

    // 双人复核通过 + 正常发放
    let second = insert_user(&pool, "second").await;
    let result = ledger::admin_grant(
        &pool,
        &admin,
        AdminGrantInput {
            user_id: user.clone(),
            currency_id: CURRENCY_COIN.to_string(),
            amount: 100,
            reason: "活动奖励".to_string(),
            idempotency_scope: "g".to_string(),
            idempotency_key: "g4".to_string(),
        },
        now,
        true,
        Some(&second),
    )
    .await
    .unwrap();
    assert_eq!(result.transactions[0].delta_balance, 100);
    assert_eq!(account_row(&pool, &user, CURRENCY_COIN).await, (100, 0, 1));

    // 审计
    let audit: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE action IN ('ledger.admin_grant','ledger.admin_grant.second_approval')",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(audit, 2);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn reversal_appends_compensation_without_mutating_history() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let now = now_millis();

    let original = ledger::credit(&pool, base_cmd(&user, CURRENCY_COIN, "r1", 500, 0), now)
        .await
        .unwrap();
    let original_id = original.operation_id.clone();
    let original_created = original.transactions[0].created_at;

    // 撤销：反向补偿流水
    let rev = ledger::reversal(
        &pool,
        "test",
        "r2",
        None,
        &original_id,
        "奖励撤销",
        now + 1000,
    )
    .await
    .unwrap();
    assert_eq!(rev.transactions[0].delta_balance, -500);
    assert_eq!(rev.transactions[0].balance_after, 0);

    // 历史不更新不删除：原 operation 行与原流水不变
    let (kind, created) = match &pool {
        Either::Left(p) => sqlx::query_as::<_, (String, i64)>(
            "SELECT kind, created_at FROM point_operations WHERE id = ?",
        )
        .bind(&original_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(kind, "award");
    assert_eq!(created, original_created);
    let orig_tx_count: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM point_transactions WHERE operation_id = ?")
                .bind(&original_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(orig_tx_count, 1, "原流水只追加，不删除");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn seeded_currencies_and_snapshot() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let now = now_millis();

    // 种子货币（M07-LEDGER-01）
    let (exp_kind, coin_kind): (String, String) = match &pool {
        Either::Left(p) => {
            let e: String = sqlx::query_scalar("SELECT kind FROM currencies WHERE id = ?")
                .bind(CURRENCY_EXP)
                .fetch_one(p)
                .await
                .unwrap();
            let c: String = sqlx::query_scalar("SELECT kind FROM currencies WHERE id = ?")
                .bind(CURRENCY_COIN)
                .fetch_one(p)
                .await
                .unwrap();
            (e, c)
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(exp_kind, "experience");
    assert_eq!(coin_kind, "spendable");

    // 快照
    ledger::credit(&pool, base_cmd(&user, CURRENCY_COIN, "s1", 100, 0), now)
        .await
        .unwrap();
    ledger::snapshot_balance(&pool, &user, CURRENCY_COIN, "daily", now + 1000)
        .await
        .unwrap();
    let snap: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM point_balance_snapshots WHERE user_id = ?")
                .bind(&user)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(snap, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}
