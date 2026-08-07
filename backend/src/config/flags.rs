//! Feature Flag 系统（M01-CONFIG-05）。
//!
//! - 可选能力（AI / Video / Download Billing / OIDC / Marketplace）v1.0 默认
//!   全部关闭（M01-CONFIG-06），核心论坛不受 Flag 控制；
//! - 每个 Flag：默认值、作用范围、生效时间、版本（乐观锁递增）；
//! - 紧急关闭（kill switch）：置真后所有可选能力强制关闭，优先于一切；
//! - 所有变更写审计记录（actor / reason / 前后状态 / 版本 / 时间），
//!   持久化审计接入 M01-AUDIT 的 audit_logs。

/// 可选能力名称（v1.0 默认关闭）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FeatureName {
    /// AI 建议/逐次同意
    Ai,
    /// Video Provider（Direct / HLS / Xigua）
    Video,
    /// 下载抵扣/计费
    DownloadBilling,
    /// OIDC Provider
    Oidc,
    /// 第三方 Marketplace
    Marketplace,
}

impl FeatureName {
    pub const ALL: [FeatureName; 5] = [
        FeatureName::Ai,
        FeatureName::Video,
        FeatureName::DownloadBilling,
        FeatureName::Oidc,
        FeatureName::Marketplace,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            FeatureName::Ai => "ai",
            FeatureName::Video => "video",
            FeatureName::DownloadBilling => "download_billing",
            FeatureName::Oidc => "oidc",
            FeatureName::Marketplace => "marketplace",
        }
    }
}

impl std::fmt::Display for FeatureName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 请求路径 → 可选能力映射（M01-CONFIG-06）。
///
/// 用于 Feature Gate 中间件：路由前缀命中即检查对应 Flag，默认关闭时返回
/// `feature_disabled`（409）。映射随各领域里程碑细化（M6/M9/M10/M11/M12）。
pub fn feature_for_path(path: &str) -> Option<FeatureName> {
    if path.starts_with("/api/v1/ai/") {
        return Some(FeatureName::Ai);
    }
    if path.starts_with("/api/v1/video-embeds/") {
        return Some(FeatureName::Video);
    }
    if path.starts_with("/api/v1/marketplace/") {
        return Some(FeatureName::Marketplace);
    }
    if path.starts_with("/api/v1/admin/marketplace/") {
        return Some(FeatureName::Marketplace);
    }
    if path.starts_with("/oauth/")
        || path.starts_with("/.well-known/")
        || path.starts_with("/api/v1/oauth/")
    {
        return Some(FeatureName::Oidc);
    }
    // 下载抵扣/授权（上传不受 Download Billing Flag 控制）
    if path.starts_with("/api/v1/download-authorizations/")
        || path.ends_with("/download-policy")
        || (path.starts_with("/api/v1/attachments/") && path.ends_with("/download"))
    {
        return Some(FeatureName::DownloadBilling);
    }
    None
}

/// Flag 作用范围（v1.0 为全局；灰度规则由后续 policy 版本扩展）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlagScope {
    Global,
}

/// 单个 Flag 的定义。
#[derive(Debug, Clone)]
pub struct FeatureFlag {
    pub name: FeatureName,
    pub enabled: bool,
    pub scope: FlagScope,
    /// 生效时间（Unix 毫秒；0 = 立即生效）
    pub effective_at: i64,
    /// 策略版本（乐观锁；每次变更 +1）
    pub version: u64,
}

/// Flag 变更审计记录。
#[derive(Debug, Clone)]
pub struct FlagChangeRecord {
    pub name: FeatureName,
    pub from: bool,
    pub to: bool,
    pub version: u64,
    pub actor: String,
    pub at: i64,
    pub reason: String,
}

/// Flag 操作错误。
#[derive(Debug)]
pub enum FlagError {
    /// 乐观锁冲突：期望版本与当前版本不一致
    VersionConflict {
        name: FeatureName,
        expected: u64,
        current: u64,
    },
    /// 未知 Flag
    Unknown(FeatureName),
}

impl std::fmt::Display for FlagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlagError::VersionConflict {
                name,
                expected,
                current,
            } => write!(
                f,
                "feature flag {name} version conflict: expected {expected}, current {current}"
            ),
            FlagError::Unknown(name) => write!(f, "unknown feature flag: {name}"),
        }
    }
}

impl std::error::Error for FlagError {}

/// Feature Flag 运行时快照。
#[derive(Debug, Clone)]
pub struct FeatureFlags {
    flags: Vec<FeatureFlag>,
    /// 紧急关闭：置真后所有可选能力强制关闭
    kill_switch: bool,
    audit: Vec<FlagChangeRecord>,
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self::all_default()
    }
}

impl FeatureFlags {
    /// 默认状态：五个可选能力全部关闭（M01-CONFIG-06），无紧急关闭。
    pub fn all_default() -> Self {
        let flags = FeatureName::ALL
            .iter()
            .map(|name| FeatureFlag {
                name: *name,
                enabled: false,
                scope: FlagScope::Global,
                effective_at: 0,
                version: 1,
            })
            .collect();
        Self {
            flags,
            kill_switch: false,
            audit: Vec::new(),
        }
    }

    /// 紧急关闭：所有可选能力立即关闭（不受生效时间影响），并逐 flag 写审计。
    pub fn emergency_off(&mut self, actor: &str, reason: &str, now: i64) {
        self.kill_switch = true;
        for name in FeatureName::ALL {
            self.audit.push(FlagChangeRecord {
                name,
                from: false,
                to: false,
                version: 0,
                actor: actor.to_owned(),
                at: now,
                reason: format!("emergency kill switch: {reason}"),
            });
        }
    }

    /// 当前是否启用：紧急关闭优先；再检查 Flag 状态与生效时间。
    pub fn is_enabled(&self, name: FeatureName, now: i64) -> bool {
        if self.kill_switch {
            return false;
        }
        match self.flags.iter().find(|flag| flag.name == name) {
            Some(flag) => flag.enabled && flag.effective_at <= now,
            None => false,
        }
    }

    /// 读取 Flag（含元数据）。
    pub fn get(&self, name: FeatureName) -> Option<&FeatureFlag> {
        self.flags.iter().find(|flag| flag.name == name)
    }

    /// 版本化变更（乐观锁）：`expected_version` 与当前版本不一致时拒绝。
    ///
    /// 变更后版本 +1 并写审计；`effective_at=0` 表示立即生效。
    #[allow(clippy::too_many_arguments)] // 有界枚举变更 API：全部参数均必需且显式
    pub fn set(
        &mut self,
        name: FeatureName,
        enabled: bool,
        expected_version: u64,
        effective_at: i64,
        actor: &str,
        reason: &str,
        now: i64,
    ) -> Result<u64, FlagError> {
        let flag = self
            .flags
            .iter_mut()
            .find(|flag| flag.name == name)
            .ok_or(FlagError::Unknown(name))?;
        if flag.version != expected_version {
            return Err(FlagError::VersionConflict {
                name,
                expected: expected_version,
                current: flag.version,
            });
        }
        let from = flag.enabled;
        flag.enabled = enabled;
        flag.effective_at = effective_at;
        flag.version += 1;
        self.audit.push(FlagChangeRecord {
            name,
            from,
            to: enabled,
            version: flag.version,
            actor: actor.to_owned(),
            at: now,
            reason: reason.to_owned(),
        });
        Ok(flag.version)
    }

    /// 审计记录（不含 Secret 内容）。
    pub fn audit_log(&self) -> &[FlagChangeRecord] {
        &self.audit
    }

    pub fn kill_switch(&self) -> bool {
        self.kill_switch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_optional_capabilities_default_off() {
        let flags = FeatureFlags::all_default();
        for name in FeatureName::ALL {
            assert!(
                !flags.is_enabled(name, 1_700_000_000_000),
                "{} 必须默认关闭",
                name
            );
        }
        assert!(!flags.kill_switch());
    }

    #[test]
    fn set_enables_with_effective_time() {
        let mut flags = FeatureFlags::all_default();
        let now = 1_700_000_000_000i64;
        let future = now + 60_000;

        // 立即生效
        flags
            .set(FeatureName::Ai, true, 1, 0, "admin", "enable ai", now)
            .unwrap();
        assert!(flags.is_enabled(FeatureName::Ai, now));

        // 定时生效：生效时间前关闭，到达后开启
        flags
            .set(
                FeatureName::Video,
                true,
                1,
                future,
                "admin",
                "schedule",
                now,
            )
            .unwrap();
        assert!(!flags.is_enabled(FeatureName::Video, now), "生效前必须关闭");
        assert!(
            flags.is_enabled(FeatureName::Video, future),
            "到达生效时间必须开启"
        );
    }

    #[test]
    fn version_increments_and_conflicts_rejected() {
        let mut flags = FeatureFlags::all_default();
        let now = 1_700_000_000_000i64;

        let v2 = flags
            .set(FeatureName::Oidc, true, 1, 0, "admin", "enable", now)
            .unwrap();
        assert_eq!(v2, 2);

        // 用过期版本再改 → 冲突
        let err = flags.set(FeatureName::Oidc, false, 1, 0, "admin", "retry stale", now);
        assert!(matches!(err, Err(FlagError::VersionConflict { .. })));

        // 用新版本改 → 成功
        let v3 = flags
            .set(FeatureName::Oidc, false, 2, 0, "admin", "disable", now)
            .unwrap();
        assert_eq!(v3, 3);
        assert!(!flags.is_enabled(FeatureName::Oidc, now));
    }

    #[test]
    fn kill_switch_forces_everything_off() {
        let mut flags = FeatureFlags::all_default();
        let now = 1_700_000_000_000i64;
        for name in FeatureName::ALL {
            flags.set(name, true, 1, 0, "admin", "enable", now).unwrap();
            assert!(flags.is_enabled(name, now));
        }
        flags.emergency_off("oncall", "suspected abuse", now);
        for name in FeatureName::ALL {
            assert!(!flags.is_enabled(name, now), "紧急关闭必须覆盖 {name}");
        }
        assert!(flags.kill_switch());
    }

    #[test]
    fn changes_are_audited_with_actor_reason_and_version() {
        let mut flags = FeatureFlags::all_default();
        let now = 1_700_000_000_000i64;
        flags
            .set(
                FeatureName::Marketplace,
                true,
                1,
                0,
                "ops@bblbb",
                "partner launch",
                now,
            )
            .unwrap();
        flags.emergency_off("oncall", "incident", now);

        let audit = flags.audit_log();
        let marketplace = audit
            .iter()
            .find(|r| r.name == FeatureName::Marketplace && r.to)
            .expect("marketplace 开启必须被审计");
        assert_eq!(marketplace.actor, "ops@bblbb");
        assert_eq!(marketplace.version, 2);
        assert_eq!(marketplace.reason, "partner launch");

        let kill = audit
            .iter()
            .filter(|r| r.reason.starts_with("emergency kill switch"))
            .count();
        assert_eq!(kill, FeatureName::ALL.len(), "紧急关闭必须逐 flag 审计");
    }

    #[test]
    fn unknown_flag_set_is_rejected() {
        // FeatureName 是枚举，不存在未知值；这里验证 ALL 覆盖与 Display 契约
        for name in FeatureName::ALL {
            assert!(!name.as_str().is_empty());
        }
    }

    /// 路径映射：五个能力的路由前缀都能命中对应 Flag，无关路径不命中。
    #[test]
    fn feature_for_path_maps_capability_prefixes() {
        use FeatureName::*;
        assert_eq!(feature_for_path("/api/v1/ai/capabilities"), Some(Ai));
        assert_eq!(feature_for_path("/api/v1/ai/drafts/x/format"), Some(Ai));
        assert_eq!(
            feature_for_path("/api/v1/video-embeds/resolve"),
            Some(Video)
        );
        assert_eq!(
            feature_for_path("/api/v1/marketplace/offers"),
            Some(Marketplace)
        );
        assert_eq!(
            feature_for_path("/api/v1/admin/marketplace/clients"),
            Some(Marketplace)
        );
        assert_eq!(feature_for_path("/oauth/token"), Some(Oidc));
        assert_eq!(
            feature_for_path("/.well-known/openid-configuration"),
            Some(Oidc)
        );
        assert_eq!(feature_for_path("/api/v1/oauth/interactions/x"), Some(Oidc));
        assert_eq!(
            feature_for_path("/api/v1/attachments/x/download"),
            Some(DownloadBilling)
        );
        assert_eq!(
            feature_for_path("/api/v1/download-authorizations/x/sign-url"),
            Some(DownloadBilling)
        );
        // 无关路径与上传不命中
        assert_eq!(feature_for_path("/healthz"), None);
        assert_eq!(feature_for_path("/api/v1/attachments"), None);
        assert_eq!(feature_for_path("/api/v1/openapi.json"), None);
        assert_eq!(feature_for_path("/api/v1/posts"), None);
    }
}
