//! resolve 的短效一次性 resolution_id 存储（M10-VIDEO-03）。
//!
//! `POST /video-embeds/resolve` 只返回一个短效 opaque resolution_id；create
//! 用它取回分类元数据（客户端不得再次提交 source URL）。本存储为进程内
//! TTL 存储（单实例后端；与 in-process RateLimiter 同架构，多实例再引入
//! Redis）。restart 后过期 → 客户端重新 resolve，安全降级。
//!
//! 安全边界：记录只含非敏感分类元数据（provider/规范化 URL/hash/policy
//! version）；签名 URL、Key、iframe HTML 在分类阶段已被拒绝，绝不落库。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::video::{Provider, RESOLUTION_TTL_MS};

/// 一次 resolve 的分类记录（一次性消费）。
#[derive(Debug, Clone)]
pub struct ResolutionRecord {
    pub resolution_id: String,
    pub user_id: String,
    pub provider: Provider,
    pub source: String,
    pub source_hash: String,
    pub official_url: String,
    pub host: String,
    pub media_type: Option<String>,
    pub external_id: Option<String>,
    pub title: Option<String>,
    pub embeddable: bool,
    pub policy_version: i64,
    pub issued_at: i64,
    pub expires_at: i64,
}

/// 进程内 TTL 存储（容量上限 + 过期清理 + 最旧驱逐）。
pub struct ResolutionStore {
    inner: Mutex<HashMap<String, ResolutionRecord>>,
    max_entries: usize,
}

impl ResolutionStore {
    pub fn new(max_entries: usize) -> Self {
        ResolutionStore {
            inner: Mutex::new(HashMap::new()),
            max_entries,
        }
    }

    fn cleanup_locked(records: &mut HashMap<String, ResolutionRecord>, now: i64) {
        records.retain(|_, r| r.expires_at > now);
    }

    fn insert(&self, record: ResolutionRecord, now: i64) {
        let mut records = self.inner.lock().unwrap();
        Self::cleanup_locked(&mut records, now);
        if records.len() >= self.max_entries {
            if let Some(oldest) = records
                .values()
                .min_by_key(|r| r.issued_at)
                .map(|r| r.resolution_id.clone())
            {
                records.remove(&oldest);
            }
        }
        records.insert(record.resolution_id.clone(), record);
    }

    fn consume(&self, user_id: &str, resolution_id: &str, now: i64) -> Option<ResolutionRecord> {
        let mut records = self.inner.lock().unwrap();
        Self::cleanup_locked(&mut records, now);
        let record = records.get(resolution_id)?;
        if record.user_id != user_id || record.expires_at <= now {
            return None;
        }
        records.remove(resolution_id)
    }
}

static STORE: OnceLock<ResolutionStore> = OnceLock::new();

fn store() -> &'static ResolutionStore {
    STORE.get_or_init(|| ResolutionStore::new(4096))
}

/// 签发一次性短效 resolution_id（`user_id` 作用域；过期/容量由存储管理）。
#[allow(clippy::too_many_arguments)] // 有界 resolve 契约：全部字段均为绑定/审计必需且显式
pub fn issue_resolution(
    user_id: &str,
    provider: Provider,
    source: String,
    source_hash: String,
    official_url: String,
    host: String,
    media_type: Option<String>,
    external_id: Option<String>,
    title: Option<String>,
    embeddable: bool,
    policy_version: i64,
    now: i64,
) -> String {
    let resolution_id = uuid::Uuid::now_v7().to_string();
    store().insert(
        ResolutionRecord {
            resolution_id: resolution_id.clone(),
            user_id: user_id.to_string(),
            provider,
            source,
            source_hash,
            official_url,
            host,
            media_type,
            external_id,
            title,
            embeddable,
            policy_version,
            issued_at: now,
            expires_at: now + RESOLUTION_TTL_MS,
        },
        now,
    );
    resolution_id
}

/// 一次性消费（校验 user 作用域 + 未过期）；消费后不可再用。
pub fn consume_resolution(
    user_id: &str,
    resolution_id: &str,
    now: i64,
) -> Option<ResolutionRecord> {
    store().consume(user_id, resolution_id, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> i64 {
        1_750_000_000_000
    }

    #[test]
    fn issue_and_consume_once() {
        let id = issue_resolution(
            "u1",
            Provider::Direct,
            "https://cdn.example.com/a.mp4".into(),
            "h".into(),
            "https://cdn.example.com/a.mp4".into(),
            "cdn.example.com".into(),
            Some("video/mp4".into()),
            None,
            None,
            true,
            1,
            now(),
        );
        let rec = consume_resolution("u1", &id, now()).unwrap();
        assert_eq!(rec.source, "https://cdn.example.com/a.mp4");
        // 一次性：二次消费返回 None。
        assert!(consume_resolution("u1", &id, now()).is_none());
    }

    #[test]
    fn consume_is_scoped_to_owner() {
        let id = issue_resolution(
            "u1",
            Provider::Direct,
            "https://cdn.example.com/a.mp4".into(),
            "h".into(),
            "https://cdn.example.com/a.mp4".into(),
            "cdn.example.com".into(),
            None,
            None,
            None,
            true,
            1,
            now(),
        );
        assert!(consume_resolution("u2", &id, now()).is_none());
        assert!(consume_resolution("u1", &id, now()).is_some());
    }

    #[test]
    fn resolution_expires_after_ttl() {
        let id = issue_resolution(
            "u1",
            Provider::Direct,
            "https://cdn.example.com/a.mp4".into(),
            "h".into(),
            "https://cdn.example.com/a.mp4".into(),
            "cdn.example.com".into(),
            None,
            None,
            None,
            true,
            1,
            now(),
        );
        assert!(consume_resolution("u1", &id, now() + RESOLUTION_TTL_MS + 1).is_none());
    }

    #[test]
    fn duplicates_issue_distinct_ids() {
        let a = issue_resolution(
            "u1",
            Provider::Direct,
            "https://cdn.example.com/a.mp4".into(),
            "h".into(),
            "https://cdn.example.com/a.mp4".into(),
            "cdn.example.com".into(),
            None,
            None,
            None,
            true,
            1,
            now(),
        );
        // 立即消费 a，避免与同进程并行测试共享的全局 TTL 存储在满载时按最旧
        // 驱逐本测试的固定时间戳记录（M10 既有并发抖动，测试级加固）。
        assert!(consume_resolution("u1", &a, now()).is_some());
        let b = issue_resolution(
            "u1",
            Provider::Direct,
            "https://cdn.example.com/a.mp4".into(),
            "h".into(),
            "https://cdn.example.com/a.mp4".into(),
            "cdn.example.com".into(),
            None,
            None,
            None,
            true,
            1,
            now(),
        );
        assert_ne!(a, b);
        assert!(consume_resolution("u1", &b, now()).is_some());
        // 一次性：a 已消费，二次消费返回 None。
        assert!(consume_resolution("u1", &a, now()).is_none());
    }
}
