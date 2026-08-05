//! 版本化配置存储（M01-CONFIG-08）。
//!
//! 模拟 `config_revisions`/policy 版本机制：
//! - 管理更新走乐观锁版本（`expected_version` 不一致即冲突）；
//! - 变更先进入暂存区，**重启**（`apply_restart`）后才生效——与登记表
//!   "运行时变更 = 重启" 的语义一致；
//! - Secret 轮换走 `SecretWriter`（见 secrets.rs），每次轮换更新版本与时间。

use std::collections::HashMap;

/// 版本化配置项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigEntry {
    pub key: String,
    pub value: String,
    pub version: u64,
    pub updated_at: i64,
    pub actor: String,
}

/// 配置存储错误。
#[derive(Debug)]
pub enum ConfigStoreError {
    NotFound(String),
    VersionConflict {
        key: String,
        expected: u64,
        current: u64,
    },
}

impl std::fmt::Display for ConfigStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigStoreError::NotFound(key) => write!(f, "config key not found: {key}"),
            ConfigStoreError::VersionConflict {
                key,
                expected,
                current,
            } => write!(
                f,
                "config key {key} version conflict: expected {expected}, current {current}"
            ),
        }
    }
}

impl std::error::Error for ConfigStoreError {}

/// 版本化配置存储。
#[derive(Debug, Default)]
pub struct ConfigStore {
    entries: HashMap<String, ConfigEntry>,
    staged: HashMap<String, ConfigEntry>,
}

impl ConfigStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 读取当前生效配置。
    pub fn read(&self, key: &str) -> Option<&ConfigEntry> {
        self.entries.get(key)
    }

    /// 当前生效版本。
    pub fn version_of(&self, key: &str) -> Option<u64> {
        self.entries.get(key).map(|entry| entry.version)
    }

    /// 管理更新（乐观锁）：`expected_version` 与当前版本一致才允许。
    ///
    /// 变更进入暂存区，调用 `apply_restart` 后生效。返回新版本号。
    pub fn update(
        &mut self,
        key: &str,
        value: String,
        expected_version: u64,
        actor: &str,
        now: i64,
    ) -> Result<u64, ConfigStoreError> {
        let current = self
            .entries
            .get(key)
            .ok_or_else(|| ConfigStoreError::NotFound(key.to_owned()))?;
        if current.version != expected_version {
            return Err(ConfigStoreError::VersionConflict {
                key: key.to_owned(),
                expected: expected_version,
                current: current.version,
            });
        }
        let next = ConfigEntry {
            key: key.to_owned(),
            value,
            version: current.version + 1,
            updated_at: now,
            actor: actor.to_owned(),
        };
        self.staged.insert(key.to_owned(), next.clone());
        Ok(next.version)
    }

    /// 待重启生效的变更数量与键名。
    pub fn pending_count(&self) -> usize {
        self.staged.len()
    }

    pub fn pending_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.staged.keys().cloned().collect();
        keys.sort();
        keys
    }

    /// 重启：把暂存变更应用到生效区。返回生效的变更数。
    pub fn apply_restart(&mut self) -> usize {
        let count = self.staged.len();
        for (key, entry) in self.staged.drain() {
            self.entries.insert(key, entry);
        }
        count
    }

    /// 种子写入（测试/初始配置用）：直接生效，版本 1。
    pub fn seed(&mut self, key: &str, value: String, now: i64) {
        self.entries.insert(
            key.to_owned(),
            ConfigEntry {
                key: key.to_owned(),
                value,
                version: 1,
                updated_at: now,
                actor: "system".to_owned(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000_000;

    #[test]
    fn read_returns_none_for_missing_then_stored_value() {
        let mut store = ConfigStore::new();
        assert!(store.read("mail.smtp_host").is_none());
        store.seed("mail.smtp_host", "smtp.internal".to_owned(), NOW);
        let entry = store.read("mail.smtp_host").expect("seeded");
        assert_eq!(entry.value, "smtp.internal");
        assert_eq!(entry.version, 1);
    }

    #[test]
    fn update_stages_and_applies_on_restart() {
        let mut store = ConfigStore::new();
        store.seed("mail.smtp_host", "old.internal".to_owned(), NOW);

        // 更新进入暂存区：生效区仍为旧值（重启生效语义）
        let version = store
            .update(
                "mail.smtp_host",
                "new.internal".to_owned(),
                1,
                "admin",
                NOW + 10,
            )
            .unwrap();
        assert_eq!(version, 2);
        assert_eq!(store.pending_count(), 1);
        assert_eq!(store.pending_keys(), vec!["mail.smtp_host"]);
        assert_eq!(store.read("mail.smtp_host").unwrap().value, "old.internal");

        // 重启后生效
        assert_eq!(store.apply_restart(), 1);
        assert_eq!(store.pending_count(), 0);
        let entry = store.read("mail.smtp_host").unwrap();
        assert_eq!(entry.value, "new.internal");
        assert_eq!(entry.version, 2);
        assert_eq!(entry.updated_at, NOW + 10);
        assert_eq!(entry.actor, "admin");
    }

    #[test]
    fn concurrent_update_with_stale_version_conflicts() {
        let mut store = ConfigStore::new();
        store.seed("rate_limit", "100".to_owned(), NOW);

        // 两个并发管理员都基于版本 1 更新
        let a = store.update("rate_limit", "200".to_owned(), 1, "admin-a", NOW + 1);
        assert!(a.is_ok());
        store.apply_restart();
        // 第二个基于过期版本 1 → 冲突
        let b = store.update("rate_limit", "300".to_owned(), 1, "admin-b", NOW + 2);
        match b {
            Err(ConfigStoreError::VersionConflict {
                key,
                expected,
                current,
            }) => {
                assert_eq!(key, "rate_limit");
                assert_eq!(expected, 1);
                assert_eq!(current, 2);
            }
            other => panic!("应返回版本冲突，得到 {other:?}"),
        }
        // 用新版本重试成功
        let retry = store.update("rate_limit", "300".to_owned(), 2, "admin-b", NOW + 3);
        assert!(retry.is_ok());
    }

    #[test]
    fn update_unknown_key_errors() {
        let mut store = ConfigStore::new();
        let err = store.update("absent", "x".to_owned(), 1, "admin", NOW);
        assert!(matches!(err, Err(ConfigStoreError::NotFound(_))));
    }

    #[test]
    fn restart_applies_only_staged_changes() {
        let mut store = ConfigStore::new();
        store.seed("a", "1".to_owned(), NOW);
        store.seed("b", "2".to_owned(), NOW);
        store
            .update("a", "10".to_owned(), 1, "admin", NOW + 1)
            .unwrap();
        assert_eq!(store.apply_restart(), 1);
        assert_eq!(store.read("a").unwrap().value, "10");
        assert_eq!(store.read("b").unwrap().value, "2", "未变更键保持原值");
    }
}
