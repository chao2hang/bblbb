//! 动态 CSP frame-src/media-src/connect-src + sandbox/referrerpolicy/allow
//! 最小权限（M10-VIDEO-07）。
//!
//! 播放边界：Direct/HLS 默认由浏览器直连已验证 HTTPS 来源，服务端不代理媒体；
//! HTML 页面按当前启用 Provider 生成精确 `media-src`、`connect-src`、
//! `img-src`、`frame-src`。西瓜视频仅使用确认过的官方 iframe；元素级
//! `referrerpolicy=no-referrer` 优先于站点全局策略。默认不自动播放、不启用
//! 摄像头/麦克风。

/// CSP 指令投影（渲染方拼装为响应头/元素属性；不含敏感 URL）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CspDirectives {
    pub frame_src: Vec<String>,
    pub media_src: Vec<String>,
    pub connect_src: Vec<String>,
    pub img_src: Vec<String>,
    pub script_src: Vec<String>,
    /// iframe sandbox 最小权限（空 = 原生 video 元素，无 iframe）。
    pub sandbox: Vec<String>,
    /// iframe allow（autoplay/camera/mic 一律缺席）。
    pub allow: Vec<String>,
    pub referrer_policy: &'static str,
}

/// 渲染模式（前端据此投影播放器/外链/占位）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderPolicy {
    /// `direct_player` | `hls_player` | `xigua_iframe` | `external_link` | `none`
    pub mode: &'static str,
    pub csp: CspDirectives,
    /// xigua ready 且官方嵌入允许时的 iframe URL（仅官方嵌入 Host）。
    pub iframe_url: Option<String>,
}

impl CspDirectives {
    fn empty() -> Self {
        CspDirectives {
            frame_src: Vec::new(),
            media_src: Vec::new(),
            connect_src: Vec::new(),
            img_src: Vec::new(),
            script_src: Vec::new(),
            sandbox: Vec::new(),
            allow: Vec::new(),
            referrer_policy: "no-referrer",
        }
    }

    /// origin 形式（`https://{host}`），供 media-src/connect-src/frame-src。
    fn origin(host: &str) -> String {
        format!("https://{host}")
    }
}

/// 安全渲染决策（纯函数）：
/// - `status`：video_embeds 状态；
/// - `provider`：direct/hls/xigua；
/// - `host`：已验证来源 host；
/// - `external_id`：西瓜平台视频 id（非敏感）；
/// - `embeddable`：xigua 是否命中官方嵌入 Host；
/// - `allow_embed`：策略 `xigua_allow_embed`；
/// - `target_visible`：目标帖/评论是否对当前请求方可见（隐藏/审核中/删除 →
///   不加载第三方播放器）。
pub fn render_for(
    status: &str,
    provider: &crate::video::Provider,
    host: Option<&str>,
    external_id: Option<&str>,
    embeddable: bool,
    allow_embed: bool,
    target_visible: bool,
) -> RenderPolicy {
    if !target_visible || status == "blocked" {
        return RenderPolicy {
            mode: "none",
            csp: CspDirectives::empty(),
            iframe_url: None,
        };
    }
    if status == "ready" {
        match provider {
            crate::video::Provider::Direct => {
                let mut csp = CspDirectives::empty();
                if let Some(host) = host {
                    let origin = CspDirectives::origin(host);
                    csp.media_src.push(origin.clone());
                    csp.connect_src.push(origin);
                }
                RenderPolicy {
                    mode: "direct_player",
                    csp,
                    iframe_url: None,
                }
            }
            crate::video::Provider::Hls => {
                let mut csp = CspDirectives::empty();
                if let Some(host) = host {
                    let origin = CspDirectives::origin(host);
                    csp.media_src.push(origin.clone());
                    csp.connect_src.push(origin);
                }
                RenderPolicy {
                    mode: "hls_player",
                    csp,
                    iframe_url: None,
                }
            }
            crate::video::Provider::Xigua => {
                if embeddable && allow_embed {
                    let mut csp = CspDirectives::empty();
                    csp.frame_src.push(CspDirectives::origin(
                        crate::video::xigua::XiguaHosts::EMBED_HOST,
                    ));
                    csp.sandbox = vec![
                        "allow-scripts".to_string(),
                        "allow-same-origin".to_string(),
                        "allow-presentation".to_string(),
                    ];
                    // 不启用 autoplay/camera/mic；DRM 需要 encrypted-media。
                    csp.allow = vec!["encrypted-media".to_string()];
                    RenderPolicy {
                        mode: "xigua_iframe",
                        csp,
                        iframe_url: external_id.map(crate::video::xigua::iframe_url),
                    }
                } else {
                    // 官方未提供稳定嵌入协议/无嵌入权限 → 降级安全外链。
                    RenderPolicy {
                        mode: "external_link",
                        csp: CspDirectives::empty(),
                        iframe_url: None,
                    }
                }
            }
        }
    } else {
        // pending/error → 安全外链卡片（不加载第三方播放器）；removed 已 404。
        RenderPolicy {
            mode: "external_link",
            csp: CspDirectives::empty(),
            iframe_url: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::Provider;

    #[test]
    fn ready_direct_media_src_scoped_to_source_host() {
        let r = render_for(
            "ready",
            &Provider::Direct,
            Some("cdn.example.com"),
            None,
            true,
            true,
            true,
        );
        assert_eq!(r.mode, "direct_player");
        assert_eq!(r.csp.media_src, vec!["https://cdn.example.com"]);
        assert_eq!(r.csp.connect_src, vec!["https://cdn.example.com"]);
        assert!(r.csp.frame_src.is_empty());
        assert!(r.csp.sandbox.is_empty());
        assert!(r.csp.allow.is_empty());
        assert_eq!(r.csp.referrer_policy, "no-referrer");
    }

    #[test]
    fn ready_hls_connects_to_source_host() {
        let r = render_for(
            "ready",
            &Provider::Hls,
            Some("live.example.com"),
            None,
            true,
            true,
            true,
        );
        assert_eq!(r.mode, "hls_player");
        assert_eq!(r.csp.media_src, vec!["https://live.example.com"]);
        assert_eq!(r.csp.connect_src, vec!["https://live.example.com"]);
    }

    #[test]
    fn xigua_iframe_only_from_official_embed_host_with_minimal_sandbox() {
        let ok = render_for(
            "ready",
            &Provider::Xigua,
            Some("www.ixigua.com"),
            Some("7301234567890123456"),
            true,
            true,
            true,
        );
        assert_eq!(ok.mode, "xigua_iframe");
        assert_eq!(ok.csp.frame_src, vec!["https://www.ixigua.com"]);
        assert_eq!(
            ok.csp.sandbox,
            vec!["allow-scripts", "allow-same-origin", "allow-presentation"]
        );
        assert_eq!(ok.csp.allow, vec!["encrypted-media"]);
        assert_eq!(
            ok.iframe_url.as_deref(),
            Some("https://www.ixigua.com/iframe/7301234567890123456")
        );

        // 非官方嵌入 Host → 降级外链。
        let degrade = render_for(
            "ready",
            &Provider::Xigua,
            Some("m.ixigua.com"),
            Some("7301234567890123456"),
            false,
            true,
            true,
        );
        assert_eq!(degrade.mode, "external_link");
        assert!(degrade.iframe_url.is_none());

        // 策略禁止嵌入 → 降级外链。
        let disabled = render_for(
            "ready",
            &Provider::Xigua,
            Some("www.ixigua.com"),
            Some("7301234567890123456"),
            true,
            false,
            true,
        );
        assert_eq!(disabled.mode, "external_link");
    }

    #[test]
    fn pending_and_error_render_external_link_not_player() {
        for status in ["pending", "error"] {
            let r = render_for(
                status,
                &Provider::Direct,
                Some("cdn.example.com"),
                None,
                true,
                true,
                true,
            );
            assert_eq!(r.mode, "external_link");
            assert!(r.csp.media_src.is_empty());
        }
    }

    #[test]
    fn blocked_or_invisible_target_never_loads_third_party() {
        for status in ["ready", "error"] {
            let blocked = render_for(
                status,
                &Provider::Xigua,
                Some("www.ixigua.com"),
                Some("7301234567890123456"),
                true,
                true,
                false,
            );
            assert_eq!(blocked.mode, "none");
            assert!(blocked.csp.frame_src.is_empty());
            assert!(blocked.iframe_url.is_none());
        }
        let blocked = render_for(
            "blocked",
            &Provider::Direct,
            Some("cdn.example.com"),
            None,
            true,
            true,
            true,
        );
        assert_eq!(blocked.mode, "none");
    }
}
