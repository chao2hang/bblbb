//! 任务 Worker 状态机（M01-JOBS-03）与 worker 租约/重试策略
//! （M01-JOBS-04 / M01-JOBS-05）。
//!
//! 状态：
//! ```text
//! queued ──→ running ──→ succeeded
//!    │          │  └────→ retry_wait ──→ running
//!    │          │  └────→ dead
//!    │          └─────(lease 超时)──→ queued
//!    ├─→ cancelled           └──→ dead
//!    └─→ dead ──(人工重放)──→ queued
//! ```
//!
//! `succeeded`/`cancelled` 无出边；`dead → queued` 是人工重放边
//! （M01-JOBS-05，管理员审计操作）。非法迁移一律拒绝并记录。

pub mod classify;
pub mod dispatch;
pub mod metrics;
pub mod payload;
pub mod retry;
pub mod worker;
pub mod worker_loop;

use std::fmt;

/// 当前 Unix 毫秒（跨库时间约定 SCHEMA §2.2）。
pub(crate) fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Job 状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobStatus {
    Queued,
    Running,
    RetryWait,
    Succeeded,
    Cancelled,
    Dead,
}

impl JobStatus {
    pub const ALL: [JobStatus; 6] = [
        JobStatus::Queued,
        JobStatus::Running,
        JobStatus::RetryWait,
        JobStatus::Succeeded,
        JobStatus::Cancelled,
        JobStatus::Dead,
    ];

    /// 数据库表示（与 jobs.status 列一致）。
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Running => "running",
            JobStatus::RetryWait => "retry_wait",
            JobStatus::Succeeded => "succeeded",
            JobStatus::Cancelled => "cancelled",
            JobStatus::Dead => "dead",
        }
    }

    pub fn parse(value: &str) -> Option<JobStatus> {
        Self::ALL
            .iter()
            .find(|status| status.as_str() == value)
            .copied()
    }

    /// 是否允许从 `from` 迁移到 `to`。
    ///
    /// `(Dead, Queued)` 是人工重放边（M01-JOBS-05）：仅管理员在审计下把
    /// dead-letter 任务重新入队，普通执行路径不会经过它。
    pub fn allowed_transition(from: JobStatus, to: JobStatus) -> bool {
        use JobStatus::*;
        matches!(
            (from, to),
            // 领取与取消/直接死信
            (Queued, Running)
                | (Queued, Cancelled)
                | (Queued, Dead)
                // 运行中：成功 / 重试 / 死信 / lease 超时重新入队
                | (Running, Succeeded)
                | (Running, RetryWait)
                | (Running, Dead)
                | (Running, Queued)
                // 等待重试：重跑 / 死信 / 取消
                | (RetryWait, Running)
                | (RetryWait, Dead)
                | (RetryWait, Cancelled)
                // 人工重放：dead → queued（管理员审计操作）
                | (Dead, Queued)
        )
    }
}

impl fmt::Display for JobStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 非法状态迁移错误。
#[derive(Debug, Clone)]
pub struct IllegalTransitionError {
    pub from: JobStatus,
    pub to: JobStatus,
}

impl fmt::Display for IllegalTransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "illegal job transition: {} → {}", self.from, self.to)
    }
}

impl std::error::Error for IllegalTransitionError {}

/// 任务（领域模型）。
#[derive(Debug, Clone)]
pub struct Job {
    pub id: String,
    pub kind: String,
    pub status: JobStatus,
    pub attempts: u32,
    pub max_attempts: u32,
}

impl Job {
    pub fn new(id: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            status: JobStatus::Queued,
            attempts: 0,
            max_attempts: 5,
        }
    }

    /// 状态迁移；非法迁移返回 `IllegalTransitionError` 且不改状态。
    pub fn transition(&mut self, to: JobStatus) -> Result<(), IllegalTransitionError> {
        if !JobStatus::allowed_transition(self.status, to) {
            return Err(IllegalTransitionError {
                from: self.status,
                to,
            });
        }
        self.status = to;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_str_round_trips() {
        for status in JobStatus::ALL {
            assert_eq!(JobStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(JobStatus::parse("unknown"), None);
        assert_eq!(JobStatus::parse(""), None);
    }

    #[test]
    fn happy_path_queued_running_succeeded() {
        let mut job = Job::new("j1", "mail");
        job.transition(JobStatus::Running).unwrap();
        job.transition(JobStatus::Succeeded).unwrap();
        assert_eq!(job.status, JobStatus::Succeeded);
    }

    #[test]
    fn retry_cycle_queued_running_retrywait_running() {
        let mut job = Job::new("j1", "mail");
        job.transition(JobStatus::Running).unwrap();
        job.transition(JobStatus::RetryWait).unwrap();
        assert_eq!(job.status, JobStatus::RetryWait);
        job.transition(JobStatus::Running).unwrap();
        assert_eq!(job.status, JobStatus::Running);
    }

    #[test]
    fn cancel_and_dead_are_allowed() {
        // queued → cancelled（取消尚未运行的任务）
        let mut job = Job::new("j1", "mail");
        job.transition(JobStatus::Cancelled).unwrap();
        assert_eq!(job.status, JobStatus::Cancelled);

        // queued → dead（直接死信）
        let mut job = Job::new("j2", "mail");
        job.transition(JobStatus::Dead).unwrap();

        // running → dead（永久失败）
        let mut job = Job::new("j3", "mail");
        job.transition(JobStatus::Running).unwrap();
        job.transition(JobStatus::Dead).unwrap();

        // running → queued（lease 超时重新入队）
        let mut job = Job::new("j4", "mail");
        job.transition(JobStatus::Running).unwrap();
        job.transition(JobStatus::Queued).unwrap();
    }

    #[test]
    fn terminal_states_have_no_outgoing_edges() {
        // succeeded/cancelled 是真正的终态；dead 只有人工重放边
        // （dead → queued，见 dead_can_be_replayed_to_queued_only）。
        for terminal in [JobStatus::Succeeded, JobStatus::Cancelled] {
            for to in JobStatus::ALL {
                assert!(
                    !JobStatus::allowed_transition(terminal, to),
                    "终态 {terminal} 不得迁往 {to}"
                );
            }
        }
    }

    #[test]
    fn dead_can_be_replayed_to_queued_only() {
        // 人工重放：dead → queued 合法
        let mut job = Job::new("j1", "mail");
        job.transition(JobStatus::Dead).unwrap();
        job.transition(JobStatus::Queued).unwrap();
        assert_eq!(job.status, JobStatus::Queued);

        // 其他出边一律拒绝
        for to in [
            JobStatus::Running,
            JobStatus::RetryWait,
            JobStatus::Succeeded,
            JobStatus::Cancelled,
            JobStatus::Dead,
        ] {
            let mut job = Job::new("j1", "mail");
            job.transition(JobStatus::Dead).unwrap();
            assert!(
                !JobStatus::allowed_transition(JobStatus::Dead, to),
                "dead 不得直接迁往 {to}"
            );
            assert!(job.transition(to).is_err());
            assert_eq!(job.status, JobStatus::Dead);
        }
    }

    #[test]
    fn illegal_transitions_are_rejected_and_do_not_mutate() {
        // queued → succeeded 非法
        let mut job = Job::new("j1", "mail");
        let err = job.transition(JobStatus::Succeeded).unwrap_err();
        assert_eq!(err.from, JobStatus::Queued);
        assert_eq!(err.to, JobStatus::Succeeded);
        assert_eq!(job.status, JobStatus::Queued, "非法迁移不得改变状态");

        // running → cancelled 非法（只能取消尚未运行的任务）
        let mut job = Job::new("j2", "mail");
        job.transition(JobStatus::Running).unwrap();
        assert!(job.transition(JobStatus::Cancelled).is_err());
        assert_eq!(job.status, JobStatus::Running);

        // retry_wait → succeeded 非法
        let mut job = Job::new("j3", "mail");
        job.transition(JobStatus::Running).unwrap();
        job.transition(JobStatus::RetryWait).unwrap();
        assert!(job.transition(JobStatus::Succeeded).is_err());

        // dead → running 非法（死信不可自动复活）
        let mut job = Job::new("j4", "mail");
        job.transition(JobStatus::Dead).unwrap();
        assert!(job.transition(JobStatus::Running).is_err());
    }
}
