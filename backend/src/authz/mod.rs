//! M03-AUTHZ-01：`resource.action` 权限注册表与数据库未知权限拒绝。
//!
//! - 注册表是 v1 全部权限名的唯一事实来源（68 项，取自
//!   docs/PERMISSION-MATRIX.md §2-8 动作表 + 附录 operation 级 x-permission
//!   注册表；`public`/`authenticated` 是身份级标记，不是权限）；
//! - 权限名格式强制 `resource[.sub].action`：至少一个点、无空段、
//!   小写字母数字与下划线（三段式如 `ai.task.manage` 合法）；
//!   `PermissionNameError` 拒绝畸形名；
//! - `verify_db_permissions` 读取 permissions 表并**拒绝未知权限名**
//!   （未知 = 不在注册表）；缺失已知权限只报告（种子由 M03-AUTHZ-02
//!   角色聚合落地时补写）；
//! - `RiskLevel`：normal / sensitive / system。sensitive 取
//!   PERMISSION-MATRIX §8 高风险重新认证清单（storage/download_billing/
//!   marketplace/marketplace.refund_admin/ai/video .manage）；system
//!   = 可改变访问控制本身的权限（role.manage/user.manage/admin.manage），
//!   对应 permissions.is_system=1（不可删除/改名）。

use sqlx::Either;

pub mod roles;

use crate::db::DatabasePool;

/// 权限风险等级（permissions.risk_level 稳定枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    Normal,
    Sensitive,
    System,
}

impl RiskLevel {
    pub const ALL: [RiskLevel; 3] = [RiskLevel::Normal, RiskLevel::Sensitive, RiskLevel::System];

    /// 数据库表示（与 permissions.risk_level CHECK 一致）。
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::Normal => "normal",
            RiskLevel::Sensitive => "sensitive",
            RiskLevel::System => "system",
        }
    }

    pub fn parse(value: &str) -> Option<RiskLevel> {
        Self::ALL.iter().find(|r| r.as_str() == value).copied()
    }
}

/// 注册表权限项（与 permissions 表列一一对应，作为种子与校验的事实来源）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permission {
    pub name: &'static str,
    pub risk_level: RiskLevel,
    /// `is_system=1`：不可删除/改名（STATE-MACHINES §Authorization）。
    pub is_system: bool,
    pub description: &'static str,
}

/// 权限名格式错误（非 `resource[.sub].action`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionNameError(String);

impl std::fmt::Display for PermissionNameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid permission name {:?}: 必须为 resource[.sub].action（无空段、小写字母数字下划线；允许单段如 openid）",
            self.0
        )
    }
}

impl std::error::Error for PermissionNameError {}

/// 校验权限名是否符合 `resource[.sub].action` 命名约定。
///
/// 规则：非空、无空段，每段为小写字母数字下划线。多段（`post.read`、
/// `ai.task.manage`）与协议兼容单段（OIDC `openid`）均合法；空串、空段、
/// 大写、空格与非法字符拒绝。
pub fn parse_permission_name(name: &str) -> Result<(), PermissionNameError> {
    let valid = !name.is_empty()
        && name.split('.').all(|seg| {
            !seg.is_empty()
                && seg
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        });
    if valid {
        Ok(())
    } else {
        Err(PermissionNameError(name.to_string()))
    }
}

/// v1 权限注册表（68 项，唯一事实来源）。
pub static PERMISSION_REGISTRY: &[Permission] = &[
    // ── 会话 / 用户 / MFA（§1 身份和标记 + 附录）──────────────────────────
    Permission {
        name: "session.revoke_own",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "撤销本人会话",
    },
    Permission {
        name: "session.read_own",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "读取本人会话列表",
    },
    Permission {
        name: "user.read_public",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "读取公开用户投影",
    },
    Permission {
        name: "user.read_own",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "读取本人 Me 投影",
    },
    Permission {
        name: "user.edit_own",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "编辑本人资料（If-Match 版本）",
    },
    Permission {
        name: "mfa.enroll",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "MFA 启用/确认/取消",
    },
    Permission {
        name: "mfa.recovery_codes",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "读取恢复码（近期认证）",
    },
    Permission {
        name: "mfa.disable",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "关闭 MFA（近期认证）",
    },
    Permission {
        name: "mfa.reauth",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "近期重新认证/step-up",
    },
    Permission {
        name: "user.manage",
        risk_level: RiskLevel::System,
        is_system: true,
        description: "管理员用户管理（system）",
    },
    Permission {
        name: "role.manage",
        risk_level: RiskLevel::System,
        is_system: true,
        description: "角色/权限管理（system）",
    },
    Permission {
        name: "admin.manage",
        risk_level: RiskLevel::System,
        is_system: true,
        description: "管理端通用入口（system）",
    },
    // ── 核心论坛 / 内容（§2 + 附录）────────────────────────────────────────
    Permission {
        name: "board.read",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "读取板块",
    },
    Permission {
        name: "board.manage",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "管理员板块管理",
    },
    Permission {
        name: "tag.manage",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "管理员标签管理",
    },
    Permission {
        name: "post.read",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "读取公开帖子",
    },
    Permission {
        name: "post.read_own",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "读取本人草稿",
    },
    Permission {
        name: "post.read_revision",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "读取修订历史",
    },
    Permission {
        name: "post.create",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "创建帖子/草稿",
    },
    Permission {
        name: "post.edit_own",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "编辑/删除本人帖子/草稿",
    },
    Permission {
        name: "post.moderate",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "管理他人帖子（板块范围+reason）",
    },
    Permission {
        name: "comment.read",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "读取评论",
    },
    Permission {
        name: "comment.create",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "创建评论",
    },
    Permission {
        name: "comment.edit_own",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "编辑/删除本人评论",
    },
    Permission {
        name: "attachment.read",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "读取附件/签发 URL",
    },
    Permission {
        name: "attachment.upload",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "创建/完成附件",
    },
    Permission {
        name: "storage.manage",
        risk_level: RiskLevel::Sensitive,
        is_system: false,
        description: "管理存储配置（sensitive，§8 近期重新认证）",
    },
    Permission {
        name: "appeal.create_own",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "发起本人申诉",
    },
    Permission {
        name: "appeal.read_own",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "读取本人申诉",
    },
    Permission {
        name: "moderation.review",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "审核案件/申诉",
    },
    Permission {
        name: "moderation.sanction",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "裁决处罚",
    },
    Permission {
        name: "level.manage",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "修改等级附件配额",
    },
    // ── 下载计费 / 经济（§3）──────────────────────────────────────────────
    Permission {
        name: "download.read",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "查询本人下载策略/授权",
    },
    Permission {
        name: "download.create",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "创建下载授权（幂等）",
    },
    Permission {
        name: "download.read_own",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "查询本人下载流水",
    },
    Permission {
        name: "download_billing.manage",
        risk_level: RiskLevel::Sensitive,
        is_system: false,
        description: "管理下载计费策略（sensitive，§8 近期重新认证）",
    },
    Permission {
        name: "points.adjust",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "管理员积分调整（双重确认）",
    },
    // ── Marketplace（§4）──────────────────────────────────────────────────
    Permission {
        name: "marketplace_offer.manage_own",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "创建/修改本人 Offer",
    },
    Permission {
        name: "marketplace_purchase.create",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "创建 Checkout Intent",
    },
    Permission {
        name: "marketplace_purchase.confirm_own",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "托管确认购买",
    },
    Permission {
        name: "marketplace_purchase.read_own",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "查询本 Client Purchase",
    },
    Permission {
        name: "marketplace_refund.create_own",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "请求退款（service credential）",
    },
    Permission {
        name: "marketplace.manage",
        risk_level: RiskLevel::Sensitive,
        is_system: false,
        description: "管理/审批 Client（sensitive，§8 近期重新认证）",
    },
    Permission {
        name: "marketplace.refund_admin",
        risk_level: RiskLevel::Sensitive,
        is_system: false,
        description: "管理员强制退款（sensitive，§8）",
    },
    Permission {
        name: "marketplace_secret.rotate",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "轮换 Webhook Secret",
    },
    Permission {
        name: "marketplace_webhook.replay",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "重放 Webhook",
    },
    // ── AI（§5）───────────────────────────────────────────────────────────
    Permission {
        name: "ai.format",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "格式化本人草稿",
    },
    Permission {
        name: "ai.seo",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "生成公开帖 SEO",
    },
    Permission {
        name: "ai.moderation_request",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "请求审核建议",
    },
    Permission {
        name: "ai.consent_own",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "管理本人 AI 同意",
    },
    Permission {
        name: "ai.manage",
        risk_level: RiskLevel::Sensitive,
        is_system: false,
        description: "管理 Provider/Policy（sensitive，§8）",
    },
    Permission {
        name: "ai.task.manage",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "重试/取消管理任务",
    },
    // ── 商城 / 活跃（§6）──────────────────────────────────────────────────
    Permission {
        name: "shop.read",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "浏览商品",
    },
    Permission {
        name: "shop.purchase",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "购买商品（幂等）",
    },
    Permission {
        name: "shop.entitlement.manage_own",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "查看/装备/卸下本人持有物",
    },
    Permission {
        name: "activity.claim_own",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "领取签到/活跃奖励",
    },
    Permission {
        name: "reaction.create",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "使用互动反应",
    },
    Permission {
        name: "shop.manage",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "管理商品与库存",
    },
    Permission {
        name: "shop.refund",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "管理商城退款",
    },
    Permission {
        name: "activity.manage",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "管理活跃规则",
    },
    // ── 视频（§7）─────────────────────────────────────────────────────────
    Permission {
        name: "video.embed",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "Resolve/创建/修改/刷新 Embed",
    },
    Permission {
        name: "video.manage",
        risk_level: RiskLevel::Sensitive,
        is_system: false,
        description: "管理 Provider Policy（sensitive，§8）",
    },
    // ── OIDC / OAuth（附录）───────────────────────────────────────────────
    Permission {
        name: "oauth.token",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "OAuth token 签发",
    },
    Permission {
        name: "oauth.revoke",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "OAuth token 撤销",
    },
    Permission {
        name: "oauth.interaction",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "OAuth 交互确认",
    },
    Permission {
        name: "openid",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "OpenID userinfo",
    },
    Permission {
        name: "openid.logout",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "OpenID logout",
    },
    Permission {
        name: "oauth_client.manage",
        risk_level: RiskLevel::Normal,
        is_system: false,
        description: "OAuth Client 管理",
    },
];

/// 判断权限名是否在注册表中。
pub fn is_registered(name: &str) -> bool {
    PERMISSION_REGISTRY.iter().any(|p| p.name == name)
}

/// 注册表自查：全部名称唯一且符合 `resource.action` 格式。
pub fn registry_is_consistent() -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for p in PERMISSION_REGISTRY {
        parse_permission_name(p.name).map_err(|e| e.to_string())?;
        if !seen.insert(p.name) {
            return Err(format!("duplicate permission name: {}", p.name));
        }
    }
    Ok(())
}

/// DB 权限校验结果（校验非错误时的诊断信息）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbPermissionCheck {
    /// 数据库中存在且已注册的权限数。
    pub known_in_db: usize,
    /// 注册表中存在但数据库尚未种子的权限名（信息性，种子由 M03-AUTHZ-02 落地）。
    pub missing_from_db: Vec<String>,
}

/// DB 权限校验错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DbPermissionError {
    /// 数据库中存在未注册的权限名（必须拒绝/修复）。
    UnknownPermissions(Vec<String>),
    Database(String),
}

impl std::fmt::Display for DbPermissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownPermissions(names) => {
                write!(
                    f,
                    "database contains permissions not in the registry: {}",
                    names.join(", ")
                )
            }
            Self::Database(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for DbPermissionError {}

/// 读取 permissions 表并**拒绝未知权限名**（未知 = 不在注册表）。
///
/// 缺失的已知权限不视为错误（种子由 M03-AUTHZ-02 角色聚合补写），但会
/// 在 [`DbPermissionCheck::missing_from_db`] 中报告。
pub async fn verify_db_permissions(
    pool: &DatabasePool,
) -> Result<DbPermissionCheck, DbPermissionError> {
    let names: Vec<String> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT name FROM permissions")
            .fetch_all(p)
            .await
            .map_err(|e| DbPermissionError::Database(e.to_string()))?,
        Either::Right(p) => sqlx::query_scalar("SELECT name FROM permissions")
            .fetch_all(p)
            .await
            .map_err(|e| DbPermissionError::Database(e.to_string()))?,
    };

    let unknown: Vec<String> = names
        .iter()
        .filter(|name| !is_registered(name))
        .cloned()
        .collect();
    if !unknown.is_empty() {
        return Err(DbPermissionError::UnknownPermissions(unknown));
    }

    let missing_from_db: Vec<String> = PERMISSION_REGISTRY
        .iter()
        .map(|p| p.name)
        .filter(|name| !names.iter().any(|db_name| db_name == name))
        .map(str::to_string)
        .collect();

    Ok(DbPermissionCheck {
        known_in_db: names.len(),
        missing_from_db,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_names_are_unique_and_valid_resource_action() {
        registry_is_consistent().expect("注册表必须自洽");
        // 每个权限名非空、小写字母数字下划线加点（无空段）
        for p in PERMISSION_REGISTRY {
            assert!(!p.name.is_empty(), "权限名不得为空");
            assert!(
                p.name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.'),
                "权限名必须小写字母数字下划线加点: {}",
                p.name
            );
            assert!(
                !p.name.starts_with('.') && !p.name.ends_with('.') && !p.name.contains(".."),
                "权限名不得有空段: {}",
                p.name
            );
        }
        // 风险等级与 is_system 一致性：system 权限必须 is_system=1
        for p in PERMISSION_REGISTRY {
            assert_eq!(
                p.is_system,
                p.risk_level == RiskLevel::System,
                "is_system 必须与 risk_level=system 一致: {}",
                p.name
            );
        }
    }

    #[test]
    fn registry_covers_expected_v1_permission_set() {
        // 68 项：PERMISSION-MATRIX §2-8 动作表 + 附录 operation 级注册表
        // （不含 public/authenticated 身份标记）
        let expected = [
            "session.revoke_own",
            "session.read_own",
            "user.read_public",
            "user.read_own",
            "user.edit_own",
            "mfa.enroll",
            "mfa.recovery_codes",
            "mfa.disable",
            "mfa.reauth",
            "user.manage",
            "role.manage",
            "admin.manage",
            "board.read",
            "board.manage",
            "tag.manage",
            "post.read",
            "post.read_own",
            "post.read_revision",
            "post.create",
            "post.edit_own",
            "post.moderate",
            "comment.read",
            "comment.create",
            "comment.edit_own",
            "attachment.read",
            "attachment.upload",
            "storage.manage",
            "appeal.create_own",
            "appeal.read_own",
            "moderation.review",
            "moderation.sanction",
            "level.manage",
            "download.read",
            "download.create",
            "download.read_own",
            "download_billing.manage",
            "points.adjust",
            "marketplace_offer.manage_own",
            "marketplace_purchase.create",
            "marketplace_purchase.confirm_own",
            "marketplace_purchase.read_own",
            "marketplace_refund.create_own",
            "marketplace.manage",
            "marketplace.refund_admin",
            "marketplace_secret.rotate",
            "marketplace_webhook.replay",
            "ai.format",
            "ai.seo",
            "ai.moderation_request",
            "ai.consent_own",
            "ai.manage",
            "ai.task.manage",
            "shop.read",
            "shop.purchase",
            "shop.entitlement.manage_own",
            "activity.claim_own",
            "reaction.create",
            "shop.manage",
            "shop.refund",
            "activity.manage",
            "video.embed",
            "video.manage",
            "oauth.token",
            "oauth.revoke",
            "oauth.interaction",
            "openid",
            "openid.logout",
            "oauth_client.manage",
        ];
        let registry_names: std::collections::HashSet<&str> =
            PERMISSION_REGISTRY.iter().map(|p| p.name).collect();
        assert_eq!(registry_names.len(), expected.len());
        for name in expected {
            assert!(registry_names.contains(name), "注册表缺少权限: {name}");
        }
        for p in PERMISSION_REGISTRY {
            assert!(expected.contains(&p.name), "注册表多出权限: {}", p.name);
        }
        // §8 高风险重新认证清单必须标记为 sensitive
        for sensitive in [
            "storage.manage",
            "download_billing.manage",
            "marketplace.manage",
            "marketplace.refund_admin",
            "ai.manage",
            "video.manage",
        ] {
            let p = PERMISSION_REGISTRY
                .iter()
                .find(|p| p.name == sensitive)
                .unwrap();
            assert_eq!(
                p.risk_level,
                RiskLevel::Sensitive,
                "{sensitive} 必须 sensitive"
            );
        }
    }

    #[test]
    fn parse_rejects_malformed_names() {
        for bad in ["a.", ".b", "A.b", "a b", "", "a..b", "é.b", ".a.b"] {
            assert!(parse_permission_name(bad).is_err(), "{bad:?} 必须拒绝");
        }
        for good in [
            "post.read",
            "user.manage_own",
            "mfa.recovery_codes",
            "a1.b2",
            "ai.task.manage",
            "shop.entitlement.manage_own",
            "openid",
        ] {
            assert!(parse_permission_name(good).is_ok(), "{good:?} 必须接受");
        }
    }
}
