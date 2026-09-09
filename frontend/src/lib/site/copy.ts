// 全站文案统一（0065）：站点级文案的唯一解析入口。
//
// 文案来源链：site_settings（管理台「系统设置 → 站点文案」编辑）→
// GET /api/v1/site 公开投影 → 根 layout SSR 注入 `data.site` → 各页面消费。
// 数据库中的空串表示「未定制」，由本模块解析为内置通用文案兜底——
// 站点默认是一套中性的开源论坛文案，运营者在后台覆盖后即全站生效。
//
// 约定：
// - 兜底值只在此文件定义（不再散落在页面里），页面永远消费解析后的
//   `SiteCopyView`，不直接读 `SitePublicResult` 原始字段；
// - 后端不可达 / 接口失败时 layout 传 null，同样解析出完整兜底文案，
//   页面渲染永不因站点信息缺失而报错（与 activeTheme 的容错策略一致）。

import type { SitePublicResult } from '$lib/api/types';

/** 程序名兜底（后端不可达时使用；正常路径站点名来自数据库）。 */
export const FALLBACK_SITE_NAME = 'BBLBB';

/** 各文案字段兜底（「站点描述」兜底为 `{站点名称} 社区论坛」）。 */
export const COPY_DEFAULTS = {
  siteDescription: '社区论坛',
  loginEyebrow: 'WELCOME BACK',
  loginTitlePrefix: '登录',
  registerEyebrowPrefix: 'JOIN',
  registerTitle: '创建账号'
} as const;

/** 解析后的站点文案视图（页面消费的唯一形状；字段恒非空）。 */
export interface SiteCopyView {
  siteName: string;
  siteDescription: string;
  loginEyebrow: string;
  loginTitle: string;
  loginSubtitle: string;
  registerEyebrow: string;
  registerTitle: string;
  registerSubtitle: string;
  /** 维护模式公开标记（前台维护提示用；默认 false）。 */
  maintenanceMode: boolean;
  /** 第三方登录是否开启（0066）。 */
  googleLoginEnabled: boolean;
  githubLoginEnabled: boolean;
}

/** 站点信息原始投影的宽松输入（接口失败/测试隔离时可缺省）。 */
export type SiteCopyInput = SitePublicResult | null | undefined;

function text(value: string | null | undefined): string {
  const v = (value ?? '').trim();
  return v;
}

/**
 * 解析站点文案：空字段逐项回退到内置通用文案。
 *
 * - 站点描述：`site_description` → 「{站点名} 社区论坛」；
 * - 登录/注册页眉题、标题：空 → 通用兜底（登录标题「登录 {站点名}」、
 *   注册眉题「JOIN {站点名}」）；
 * - 登录/注册页说明：空 → 站点描述（两级兜底）。
 */
export function resolveSiteCopy(site: SiteCopyInput): SiteCopyView {
  const siteName = text(site?.site_name) || FALLBACK_SITE_NAME;
  const siteDescription =
    text(site?.site_description) || `${siteName} ${COPY_DEFAULTS.siteDescription}`;
  return {
    siteName,
    siteDescription,
    loginEyebrow: text(site?.login_eyebrow) || COPY_DEFAULTS.loginEyebrow,
    loginTitle: text(site?.login_title) || `${COPY_DEFAULTS.loginTitlePrefix} ${siteName}`,
    loginSubtitle: text(site?.login_subtitle) || siteDescription,
    registerEyebrow: text(site?.register_eyebrow) || `${COPY_DEFAULTS.registerEyebrowPrefix} ${siteName}`,
    registerTitle: text(site?.register_title) || COPY_DEFAULTS.registerTitle,
    registerSubtitle: text(site?.register_subtitle) || siteDescription,
    maintenanceMode: site?.maintenance_mode === true,
    googleLoginEnabled: site?.google_login_enabled === true,
    githubLoginEnabled: site?.github_login_enabled === true
  };
}

/** 页面标题约定：「{页面名} — {站点名}」（全站统一格式）。 */
export function pageTitle(title: string, siteName: string): string {
  return `${title} — ${siteName}`;
}
