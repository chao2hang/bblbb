//! M05-NOTIFY：邮件投递。
//!
//! 构建在 M01-JOBS 之上：邮件以 Job 投递，临时/永久失败分类、
//! 指数退避、dead-letter 与管理员重放由 `jobs::retry` 提供。
//! 日志安全（M05-NOTIFY-08）：payload 只存 user_id 引用，完整邮箱在
//! 投递时查库；`sanitize_log` 掩码邮箱、剥离正文、脱敏 token 与
//! Provider 响应后才进入日志。

pub mod service;
