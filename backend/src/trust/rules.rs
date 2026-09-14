//! 信任等级阈值与纯评估逻辑（移植 linux.do / Discourse 默认数值）。
//!
//! 本模块只做纯计算，不接触数据库：`Requirements` 从
//! `trust_level_rules.requirements_json` 反序列化（缺省键 = 0，即不要求），
//! `level_met` 判定某级是否达标，`requirement_items` 产出逐项进度报告。
//! 数值语义见 `docs/TRUST-LEVELS.md` §4。

use serde::Deserialize;
use serde_json::Value;

/// 等级名称（与迁移种子一致；规则表 name 为权威显示名，此处兜底）。
pub const LEVEL_NAMES: [&str; 5] = ["新用户", "基本用户", "成员", "活跃用户", "领导者"];

/// 代码内置默认规则元数据（name, summary；与 0070 迁移种子一致，reset 恢复用）。
pub const DEFAULT_RULE_META: [(&str, &str); 5] = [
    ("新用户", "注册默认等级；能力受反垃圾限制。"),
    ("基本用户", "愿意阅读即可达到的第一级。"),
    ("成员", "持续活跃并参与讨论的正式成员。"),
    ("活跃用户", "滚动窗口考核，不达标降级（2 周宽限）。"),
    ("领导者", "仅可由工作人员手动授予。"),
];

/// 阈值条件支持键（管理端可配置；requirements_json 未知键拒绝写入）。
pub const KNOWN_KEYS: [&str; 21] = [
    "window_days",
    "topics_entered",
    "posts_read",
    "time_read_seconds",
    "days_visited",
    "likes_given",
    "likes_received",
    "topics_replied_to",
    "visit_ratio",
    "replied_topics_window",
    "viewed_ratio",
    "viewed_cap",
    "read_ratio",
    "read_cap",
    "likes_received_window",
    "likes_given_window",
    "like_distinct_user_divisor",
    "like_distinct_day_divisor",
    "max_flags",
    "no_sanction_months",
    "manual_only",
];

/// 代码内置默认规则（与 0070 迁移种子一致；规则表缺行/解析失败时回退）。
pub const DEFAULT_REQUIREMENTS: [(&str, &str); 5] = [
    ("{}", "0"),
    (
        r#"{"topics_entered":5,"posts_read":30,"time_read_seconds":600}"#,
        "1",
    ),
    (
        r#"{"days_visited":15,"likes_given":1,"likes_received":1,"topics_replied_to":3,"topics_entered":20,"posts_read":100,"time_read_seconds":3600}"#,
        "2",
    ),
    (
        r#"{"window_days":100,"visit_ratio":0.5,"replied_topics_window":10,"viewed_ratio":0.25,"viewed_cap":500,"read_ratio":0.25,"read_cap":20000,"likes_received_window":20,"likes_given_window":30,"like_distinct_user_divisor":5,"like_distinct_day_divisor":4,"max_flags":5,"no_sanction_months":6}"#,
        "3",
    ),
    (r#"{"manual_only":true}"#, "4"),
];

/// 单级阈值（JSON 缺省键 = 不要求）。
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize)]
pub struct Requirements {
    /// > 0 = 滚动窗口口径（天）；0 = 累计口径。
    #[serde(default)]
    pub window_days: i64,
    #[serde(default)]
    pub topics_entered: i64,
    #[serde(default)]
    pub posts_read: i64,
    #[serde(default)]
    pub time_read_seconds: i64,
    #[serde(default)]
    pub days_visited: i64,
    #[serde(default)]
    pub likes_given: i64,
    #[serde(default)]
    pub likes_received: i64,
    #[serde(default)]
    pub topics_replied_to: i64,
    /// 窗口内访问天数占窗口天数比例（TL3 默认 0.5）。
    #[serde(default)]
    pub visit_ratio: f64,
    #[serde(default)]
    pub replied_topics_window: i64,
    /// 窗口期新建话题的浏览比例与上限（TL3 默认 25% / 500）。
    #[serde(default)]
    pub viewed_ratio: f64,
    #[serde(default)]
    pub viewed_cap: i64,
    /// 窗口期新建楼层的阅读比例与上限（TL3 默认 25% / 20000）。
    #[serde(default)]
    pub read_ratio: f64,
    #[serde(default)]
    pub read_cap: i64,
    #[serde(default)]
    pub likes_received_window: i64,
    #[serde(default)]
    pub likes_given_window: i64,
    /// 点赞多样性：不同用户数 ≥ 总数 / divisor（TL3 默认 5）。
    #[serde(default)]
    pub like_distinct_user_divisor: i64,
    /// 点赞多样性：不同天数 ≥ 总数 / divisor（TL3 默认 4）。
    #[serde(default)]
    pub like_distinct_day_divisor: i64,
    /// 被确认不当标记数上限（≤ 才达标；TL3 默认 5）。
    #[serde(default)]
    pub max_flags: i64,
    /// 近 N 个月无禁言/封禁（TL3 默认 6）。
    #[serde(default)]
    pub no_sanction_months: i64,
    /// 仅可手动授予（TL4）。
    #[serde(default)]
    pub manual_only: bool,
}

impl Requirements {
    pub fn parse(json: &str) -> Result<Requirements, String> {
        serde_json::from_str(json).map_err(|e| format!("invalid requirements_json: {e}"))
    }

    /// 管理端写入校验（先于 parse）：requirements 原始 JSON 的每个键必须在
    /// `KNOWN_KEYS` 内——未知键（含拼写错误）拒绝，避免静默丢弃导致
    /// 「配置了但不生效」。
    pub fn validate_keys(value: &Value) -> Result<(), String> {
        let Some(map) = value.as_object() else {
            return Err("requirements must be a JSON object".into());
        };
        for key in map.keys() {
            if !KNOWN_KEYS.contains(&key.as_str()) {
                return Err(format!("unknown requirement key: {key}"));
            }
        }
        Ok(())
    }

    /// 逐级语义校验（管理端写入；与评估引擎语义一致）：
    /// - TL0：默认等级，不允许任何阈值条件（全零）；
    /// - TL1–TL3：manual_only 必须为 false（手动授予语义仅 TL4）；
    /// - TL4：manual_only 必须为 true（唯一手动授予通道）；
    /// - 数值域：计数非负、ratio ∈ [0,1]、window_days ≤ 3650。
    pub fn validate_for_level(&self, level: i64) -> Result<(), String> {
        let ints = [
            self.window_days,
            self.topics_entered,
            self.posts_read,
            self.time_read_seconds,
            self.days_visited,
            self.likes_given,
            self.likes_received,
            self.topics_replied_to,
            self.replied_topics_window,
            self.viewed_cap,
            self.read_cap,
            self.likes_received_window,
            self.likes_given_window,
            self.like_distinct_user_divisor,
            self.like_distinct_day_divisor,
            self.max_flags,
            self.no_sanction_months,
        ];
        if ints.iter().any(|v| *v < 0) {
            return Err("requirement values must be >= 0".into());
        }
        if self.window_days > 3650 {
            return Err("window_days must be 0..=3650".into());
        }
        if self.no_sanction_months > 120 {
            return Err("no_sanction_months must be 0..=120".into());
        }
        for (name, ratio) in [
            ("visit_ratio", self.visit_ratio),
            ("viewed_ratio", self.viewed_ratio),
            ("read_ratio", self.read_ratio),
        ] {
            if !ratio.is_finite() || !(0.0..=1.0).contains(&ratio) {
                return Err(format!("{name} must be within [0, 1]"));
            }
        }
        match level {
            0 => {
                if *self != Requirements::default() {
                    return Err("level 0 is the default level and cannot have requirements".into());
                }
            }
            1..=3 => {
                if self.manual_only {
                    return Err(format!(
                        "manual_only is only allowed on level 4 (got level {level})"
                    ));
                }
            }
            4 => {
                if !self.manual_only {
                    return Err("level 4 must stay manual_only (the only grant channel)".into());
                }
            }
            _ => return Err("level must be 0..=4".into()),
        }
        Ok(())
    }

    /// 兜底：按等级取代码内置默认（0..=4 之外按 TL0）。
    pub fn default_for_level(level: i64) -> Requirements {
        let idx = level.clamp(0, 4) as usize;
        let (json, _) = DEFAULT_REQUIREMENTS[idx];
        Requirements::parse(json).unwrap_or_else(|_| Requirements::default())
    }
}

impl Default for Requirements {
    fn default() -> Self {
        Requirements {
            window_days: 0,
            topics_entered: 0,
            posts_read: 0,
            time_read_seconds: 0,
            days_visited: 0,
            likes_given: 0,
            likes_received: 0,
            topics_replied_to: 0,
            visit_ratio: 0.0,
            replied_topics_window: 0,
            viewed_ratio: 0.0,
            viewed_cap: 0,
            read_ratio: 0.0,
            read_cap: 0,
            likes_received_window: 0,
            likes_given_window: 0,
            like_distinct_user_divisor: 0,
            like_distinct_day_divisor: 0,
            max_flags: 0,
            no_sanction_months: 0,
            manual_only: false,
        }
    }
}

/// 累计口径统计（全历史）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CumulativeStats {
    pub topics_entered: i64,
    pub posts_read: i64,
    pub time_read_seconds: i64,
    pub days_visited: i64,
    pub likes_given: i64,
    pub likes_received: i64,
    pub topics_replied_to: i64,
}

/// 滚动窗口口径统计（TL3；窗口长度取 TL3 规则的 window_days）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowStats {
    pub window_days: i64,
    /// 窗口内访问天数。
    pub visit_days: i64,
    /// 窗口内回复的不同话题数（非删除楼层）。
    pub topics_replied_to: i64,
    /// 窗口内新建（未删除、非草稿）话题总数（全体用户）。
    pub topics_created_in_window: i64,
    /// 其中该用户浏览过的话题数。
    pub topics_viewed_in_window: i64,
    /// 窗口内新建（未删除）楼层总数（全体用户）。
    pub posts_created_in_window: i64,
    /// 其中该用户阅读过的楼层数。
    pub posts_read_in_window: i64,
    pub likes_received: i64,
    pub likes_received_users: i64,
    pub likes_received_days: i64,
    pub likes_given: i64,
    pub likes_given_users: i64,
    pub likes_given_days: i64,
    /// 窗口内被版主确认的不当标记数（reports resolved）。
    pub confirmed_flags: i64,
    /// 近 no_sanction_months 个月内有未撤销的禁言/封禁。
    pub sanctioned_recently: bool,
}

/// 逐项进度报告条目。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RequirementItem {
    pub key: String,
    pub label: String,
    pub current: i64,
    pub required: i64,
    pub met: bool,
}

impl RequirementItem {
    fn new(key: &str, label: &str, current: i64, required: i64, at_least: bool) -> Self {
        RequirementItem {
            key: key.to_string(),
            label: label.to_string(),
            current,
            required,
            met: if at_least {
                current >= required
            } else {
                current <= required
            },
        }
    }
}

/// `ceil(ratio * total)`，上限 `cap`；total <= 0 时为 0（无要求即达标）。
pub fn window_required(ratio: f64, total: i64, cap: i64) -> i64 {
    if total <= 0 {
        return 0;
    }
    let raw = (total as f64 * ratio).ceil() as i64;
    raw.clamp(0, cap.max(0))
}

/// 点赞多样性块：总数达标 + 不同用户数 ≥ 总数/user_div + 不同天数 ≥ 总数/day_div。
fn like_block(
    required_total: i64,
    total: i64,
    distinct_users: i64,
    distinct_days: i64,
    user_div: i64,
    day_div: i64,
) -> bool {
    if total < required_total {
        return false;
    }
    if required_total <= 0 {
        return true;
    }
    if user_div > 1 && distinct_users * user_div < total {
        return false;
    }
    if day_div > 1 && distinct_days * day_div < total {
        return false;
    }
    true
}

/// 某级是否达标。窗口规则取 `win`，累计规则取 `cum`。
pub fn level_met(req: &Requirements, cum: &CumulativeStats, win: &WindowStats) -> bool {
    if req.manual_only {
        return false; // TL4 不能自动晋升
    }
    if req.window_days > 0 {
        let required_visits = window_required(req.visit_ratio, req.window_days, i64::MAX);
        like_block(
            req.likes_received_window,
            win.likes_received,
            win.likes_received_users,
            win.likes_received_days,
            req.like_distinct_user_divisor,
            req.like_distinct_day_divisor,
        ) && like_block(
            req.likes_given_window,
            win.likes_given,
            win.likes_given_users,
            win.likes_given_days,
            req.like_distinct_user_divisor,
            req.like_distinct_day_divisor,
        ) && win.visit_days >= required_visits
            && win.topics_replied_to >= req.replied_topics_window
            && win.topics_viewed_in_window
                >= window_required(
                    req.viewed_ratio,
                    win.topics_created_in_window,
                    req.viewed_cap,
                )
            && win.posts_read_in_window
                >= window_required(req.read_ratio, win.posts_created_in_window, req.read_cap)
            && win.confirmed_flags <= req.max_flags
            && !win.sanctioned_recently
    } else {
        cum.topics_entered >= req.topics_entered
            && cum.posts_read >= req.posts_read
            && cum.time_read_seconds >= req.time_read_seconds
            && cum.days_visited >= req.days_visited
            && cum.likes_given >= req.likes_given
            && cum.likes_received >= req.likes_received
            && cum.topics_replied_to >= req.topics_replied_to
    }
}

/// 逐项进度报告（下一级或当前级的达标情况）。
pub fn requirement_items(
    req: &Requirements,
    cum: &CumulativeStats,
    win: &WindowStats,
) -> Vec<RequirementItem> {
    if req.manual_only {
        return vec![RequirementItem::new(
            "manual_only",
            "仅可由工作人员手动授予",
            0,
            0,
            true,
        )];
    }
    if req.window_days > 0 {
        let required_visits = window_required(req.visit_ratio, req.window_days, i64::MAX);
        let required_viewed = window_required(
            req.viewed_ratio,
            win.topics_created_in_window,
            req.viewed_cap,
        );
        let required_read =
            window_required(req.read_ratio, win.posts_created_in_window, req.read_cap);
        let mut items = vec![
            RequirementItem::new(
                "visit_days",
                "窗口内访问天数",
                win.visit_days,
                required_visits,
                true,
            ),
            RequirementItem::new(
                "replied_topics_window",
                "窗口内回复的不同话题",
                win.topics_replied_to,
                req.replied_topics_window,
                true,
            ),
            RequirementItem::new(
                "topics_viewed_window",
                "浏览窗口期新建话题",
                win.topics_viewed_in_window,
                required_viewed,
                true,
            ),
            RequirementItem::new(
                "posts_read_window",
                "阅读窗口期新建楼层",
                win.posts_read_in_window,
                required_read,
                true,
            ),
            RequirementItem::new(
                "likes_received_window",
                "窗口内收到的赞",
                win.likes_received,
                req.likes_received_window,
                true,
            ),
            RequirementItem::new(
                "likes_given_window",
                "窗口内送出的赞",
                win.likes_given,
                req.likes_given_window,
                true,
            ),
            RequirementItem::new(
                "confirmed_flags",
                "被确认的不当标记数（上限）",
                win.confirmed_flags,
                req.max_flags,
                false,
            ),
        ];
        // 点赞多样性（有要求时才展示，required 随总数动态变化）。
        if req.like_distinct_user_divisor > 1 && req.likes_received_window > 0 {
            items.push(RequirementItem::new(
                "likes_received_users",
                "收到赞的不同用户数",
                win.likes_received_users,
                div_ceil(win.likes_received, req.like_distinct_user_divisor),
                true,
            ));
        }
        if req.like_distinct_day_divisor > 1 && req.likes_received_window > 0 {
            items.push(RequirementItem::new(
                "likes_received_days",
                "收到赞的不同天数",
                win.likes_received_days,
                div_ceil(win.likes_received, req.like_distinct_day_divisor),
                true,
            ));
        }
        if req.like_distinct_user_divisor > 1 && req.likes_given_window > 0 {
            items.push(RequirementItem::new(
                "likes_given_users",
                "送出赞的不同用户数",
                win.likes_given_users,
                div_ceil(win.likes_given, req.like_distinct_user_divisor),
                true,
            ));
        }
        if req.like_distinct_day_divisor > 1 && req.likes_given_window > 0 {
            items.push(RequirementItem::new(
                "likes_given_days",
                "送出赞的不同天数",
                win.likes_given_days,
                div_ceil(win.likes_given, req.like_distinct_day_divisor),
                true,
            ));
        }
        items.push(RequirementItem::new(
            "no_recent_sanction",
            "近 6 个月无禁言/封禁",
            i64::from(win.sanctioned_recently),
            0,
            false,
        ));
        items
    } else {
        vec![
            RequirementItem::new(
                "topics_entered",
                "进入话题数",
                cum.topics_entered,
                req.topics_entered,
                true,
            ),
            RequirementItem::new(
                "posts_read",
                "阅读楼层数",
                cum.posts_read,
                req.posts_read,
                true,
            ),
            RequirementItem::new(
                "time_read_seconds",
                "累计阅读时长（秒）",
                cum.time_read_seconds,
                req.time_read_seconds,
                true,
            ),
            RequirementItem::new(
                "days_visited",
                "访问天数",
                cum.days_visited,
                req.days_visited,
                true,
            ),
            RequirementItem::new(
                "likes_given",
                "送出的赞",
                cum.likes_given,
                req.likes_given,
                true,
            ),
            RequirementItem::new(
                "likes_received",
                "收到的赞",
                cum.likes_received,
                req.likes_received,
                true,
            ),
            RequirementItem::new(
                "topics_replied_to",
                "回复的不同话题",
                cum.topics_replied_to,
                req.topics_replied_to,
                true,
            ),
        ]
        .into_iter()
        .filter(|item| item.required > 0)
        .collect()
    }
}

fn div_ceil(a: i64, b: i64) -> i64 {
    if b <= 0 || a <= 0 {
        return 0;
    }
    // 有符号整数的 div_ceil 尚未稳定（int_roundings），手写避免溢出域外使用。
    (a / b) + i64::from(a % b != 0)
}

/// 从规则 JSON 行（trust_level_rules）解析为 (level, name, summary, Requirements)。
pub fn parse_rule_row(
    level: i64,
    name: &str,
    summary: Option<&str>,
    json: Option<&str>,
) -> (i64, String, Option<String>, Requirements) {
    let req = json
        .and_then(|j| Requirements::parse(j).ok())
        .unwrap_or_else(|| Requirements::default_for_level(level));
    (level, name.to_string(), summary.map(str::to_string), req)
}

/// 把规则 JSON 显式转为 `Value`（路由层输出用；非法 JSON 返回 Null）。
pub fn requirements_value(req: &Requirements) -> Value {
    serde_json::to_value(req).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cum(
        topics_entered: i64,
        posts_read: i64,
        time_read_seconds: i64,
        days_visited: i64,
        likes_given: i64,
        likes_received: i64,
        topics_replied_to: i64,
    ) -> CumulativeStats {
        CumulativeStats {
            topics_entered,
            posts_read,
            time_read_seconds,
            days_visited,
            likes_given,
            likes_received,
            topics_replied_to,
        }
    }

    fn win(overrides: impl FnOnce(&mut WindowStats)) -> WindowStats {
        let mut w = WindowStats {
            window_days: 100,
            ..WindowStats::default()
        };
        overrides(&mut w);
        w
    }

    #[test]
    fn parse_defaults_and_invalid() {
        let r = Requirements::parse("{}").unwrap();
        assert_eq!(r, Requirements::default());
        assert!(Requirements::parse("not json").is_err());
        assert_eq!(Requirements::default_for_level(3).window_days, 100);
    }

    #[test]
    fn tl1_requires_reading() {
        let req = Requirements::default_for_level(1);
        assert!(!level_met(
            &req,
            &cum(4, 30, 600, 1, 0, 0, 0),
            &WindowStats::default()
        ));
        assert!(level_met(
            &req,
            &cum(5, 30, 600, 1, 0, 0, 0),
            &WindowStats::default()
        ));
        // 阅读时长不足
        assert!(!level_met(
            &req,
            &cum(5, 30, 599, 1, 0, 0, 0),
            &WindowStats::default()
        ));
    }

    #[test]
    fn tl2_requires_activity() {
        let req = Requirements::default_for_level(2);
        let ok = cum(20, 100, 3600, 15, 1, 1, 3);
        assert!(level_met(&req, &ok, &WindowStats::default()));
        // 差一天访问
        assert!(!level_met(
            &req,
            &cum(20, 100, 3600, 14, 1, 1, 3),
            &WindowStats::default()
        ));
    }

    #[test]
    fn tl3_window_full_pass() {
        let req = Requirements::default_for_level(3);
        let w = win(|w| {
            w.visit_days = 50;
            w.topics_replied_to = 10;
            w.topics_created_in_window = 1000;
            w.topics_viewed_in_window = 250; // 25% = 250
            w.posts_created_in_window = 4000;
            w.posts_read_in_window = 1000; // 25% = 1000
            w.likes_received = 20;
            w.likes_received_users = 4; // 20/5
            w.likes_received_days = 5; // 20/4
            w.likes_given = 30;
            w.likes_given_users = 6; // 30/5
            w.likes_given_days = 8; // 30/4
        });
        assert!(level_met(&req, &CumulativeStats::default(), &w));
    }

    #[test]
    fn tl3_like_diversity_enforced() {
        let req = Requirements::default_for_level(3);
        let base = |users: i64, days: i64| {
            win(|w| {
                w.visit_days = 50;
                w.topics_replied_to = 10;
                w.topics_viewed_in_window = 250;
                w.posts_read_in_window = 1000;
                w.likes_received = 20;
                w.likes_received_users = users;
                w.likes_received_days = days;
                w.likes_given = 30;
                w.likes_given_users = 6;
                w.likes_given_days = 8;
            })
        };
        // 20 赞来自 3 个用户（< 20/5=4）→ 不达标
        assert!(!level_met(&req, &CumulativeStats::default(), &base(3, 5)));
        // 20 赞来自 4 个用户但只来自 4 天（< 20/4=5）→ 不达标
        assert!(!level_met(&req, &CumulativeStats::default(), &base(4, 4)));
        assert!(level_met(&req, &CumulativeStats::default(), &base(4, 5)));
    }

    #[test]
    fn tl3_window_caps_and_flags_and_sanctions() {
        let req = Requirements::default_for_level(3);
        // 窗口期只新建了 100 话题 → 需要浏览 25；只建 4000 楼层 → 需读 1000；
        // 新建 10 万楼层 → 上限 20000。
        let w = win(|w| {
            w.visit_days = 50;
            w.topics_replied_to = 10;
            w.topics_created_in_window = 100;
            w.topics_viewed_in_window = 24;
            w.posts_created_in_window = 100_000;
            w.posts_read_in_window = 20_000;
            w.likes_received = 20;
            w.likes_received_users = 4;
            w.likes_received_days = 5;
            w.likes_given = 30;
            w.likes_given_users = 6;
            w.likes_given_days = 8;
        });
        assert!(!level_met(&req, &CumulativeStats::default(), &w));
        let w2 = win(|w| {
            w.visit_days = 50;
            w.topics_replied_to = 10;
            w.topics_created_in_window = 100;
            w.topics_viewed_in_window = 25;
            w.posts_created_in_window = 100_000;
            w.posts_read_in_window = 20_000;
            w.likes_received = 20;
            w.likes_received_users = 4;
            w.likes_received_days = 5;
            w.likes_given = 30;
            w.likes_given_users = 6;
            w.likes_given_days = 8;
        });
        assert!(level_met(&req, &CumulativeStats::default(), &w2));
        // 标记超限 / 近期被禁言
        let mut w3 = w2;
        w3.confirmed_flags = 6;
        assert!(!level_met(&req, &CumulativeStats::default(), &w3));
        let mut w4 = w2;
        w4.sanctioned_recently = true;
        assert!(!level_met(&req, &CumulativeStats::default(), &w4));
    }

    #[test]
    fn window_required_math() {
        assert_eq!(window_required(0.25, 0, 500), 0);
        assert_eq!(window_required(0.25, 100, 500), 25);
        assert_eq!(window_required(0.25, 100_000, 500), 500);
        assert_eq!(window_required(0.25, 100_000, 20_000), 20_000);
        assert_eq!(window_required(0.5, 101, i64::MAX), 51); // ceil
    }

    #[test]
    fn requirement_items_shapes() {
        let req = Requirements::default_for_level(2);
        let items = requirement_items(
            &req,
            &cum(20, 100, 3600, 15, 1, 1, 3),
            &WindowStats::default(),
        );
        assert!(items.iter().all(|i| i.met));
        assert_eq!(items.len(), 7);
        let req3 = Requirements::default_for_level(3);
        let items3 = requirement_items(&req3, &CumulativeStats::default(), &WindowStats::default());
        // 窗口项 + 收发赞 + 多样性 + 标记 + 禁言
        assert!(items3.iter().any(|i| i.key == "visit_days"));
        assert!(items3.iter().any(|i| i.key == "likes_received_users"));
        assert!(items3.iter().any(|i| i.key == "no_recent_sanction"));
        let req4 = Requirements::default_for_level(4);
        let items4 = requirement_items(&req4, &CumulativeStats::default(), &WindowStats::default());
        assert_eq!(items4.len(), 1);
        assert_eq!(items4[0].key, "manual_only");
    }
}
