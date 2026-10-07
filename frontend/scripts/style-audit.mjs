#!/usr/bin/env node
/**
 * style-audit.mjs — Zero-dependency Style & Design Token Architecture Guard
 *
 * Checks:
 * 1. Ghost Tokens: Disallows references to undefined CSS custom properties (var(--...))
 *    ensuring all variables either exist in design tokens or are documented dynamic properties.
 * 2. Color Literals: Detects raw hex/rgb/rgba color literals in user-facing components,
 *    ensuring styles use semantic tokens rather than arbitrary Tailwind or hardcoded colors.
 */

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { resolve, relative, extname } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = fileURLToPath(new URL('.', import.meta.url));
const FRONTEND_ROOT = resolve(SCRIPT_DIR, '..');
const SRC_DIR = resolve(FRONTEND_ROOT, 'src');
const STYLES_DIR = resolve(SRC_DIR, 'lib/styles');

/**
 * 允许包含字面颜色或外部第三方资产的文件白名单（带原因）
 */
const COLOR_LITERAL_ALLOWLIST = new Map([
  // 外部第三方 OAuth 品牌标准 Logo 矢量（Google G 标志等）
  ['src/routes/login/+page.svelte', 'Third-party OAuth branding SVG (Google logo)'],
  ['src/routes/register/+page.svelte', 'Third-party OAuth branding SVG (Google logo)'],

  // MFA TOTP 二维码容器必须纯白底以保证光学扫描对比度
  ['src/routes/mfa/+page.svelte', 'TOTP QR container requires white background for contrast'],

  // 衣柜/商城装扮商品属性预设（用户购买的固定外观：彩虹色、烈焰、霓虹发光等）
  ['src/lib/components/wardrobe/CosmeticName.svelte', 'Cosmetic nickname effects (rainbow, flame, glow presets)'],
  ['src/lib/components/wardrobe/CosmeticAvatar.svelte', 'Cosmetic avatar frames (gold ring, neon pulse presets)'],
  ['src/lib/components/wardrobe/tokens.ts', 'Cosmetic effect token presets'],
  ['src/lib/components/wardrobe/tokens.test.ts', 'Cosmetic effect unit tests'],
  ['src/lib/components/wardrobe/profile-effect.ts', 'Profile effect video/glow presets'],
  ['src/lib/components/wardrobe/EntitlementPreview.svelte', 'Cosmetic item thumbnail fallback'],
  ['src/routes/me/wardrobe/+page.svelte', 'Wardrobe dress-up preview canvas'],
  ['src/lib/components/UserHoverCard.svelte', 'User profile hover card video letterbox'],

  // 主题管理与个人设置的主题色板调色预览卡片数据
  ['src/routes/settings/+page.svelte', 'Theme preset definition cards data swatches'],
  ['src/routes/admin/themes/+page.svelte', 'Theme management preview palette swatches'],

  // 商城管理与 Steam 装扮工坊预览舞台（还原 Steam 客户端真实深色环境）
  ['src/routes/admin/shop/+page.svelte', 'Shop cosmetic management previews'],
  ['src/lib/components/admin/shop/SteamBackgroundPricingModal.svelte', 'Steam background preview stage'],
  ['src/lib/components/admin/shop/SteamFramePricingModal.svelte', 'Steam frame preview stage'],
  ['src/lib/components/admin/shop/QuickNicknameModal.svelte', 'Cosmetic nickname preview modal'],
  ['src/lib/components/admin/shop/CosmeticPreviewThumbnail.svelte', 'Cosmetic thumbnail canvas'],
  ['src/lib/components/admin/shop/studio/SteamBackgroundPickerModal.svelte', 'Steam background studio modal'],
  ['src/lib/components/admin/shop/studio/SteamFramePickerModal.svelte', 'Steam frame studio modal'],
  ['src/lib/components/admin/shop/studio/StyleParamsForm.svelte', 'Studio parameter configuration form'],
  ['src/lib/components/admin/shop/studio/PreviewStage.svelte', 'Studio preview stage canvas'],
  ['src/lib/components/admin/shop/studio/ProfileEffectPreview.svelte', 'Studio profile effect preview'],
  ['src/lib/components/admin/shop/studio/PostEffectPreview.svelte', 'Studio post effect preview'],
  ['src/lib/components/admin/shop/studio/StudioBadgeChip.svelte', 'Studio badge chip preview'],

  // 代码差分查看器（Git Diff 标准行高亮红绿底色）
  ['src/lib/components/admin/GitDiffViewer.svelte', 'Git diff syntax highlights'],

  // 商城装扮工坊发布与颜色选择控件
  ['src/lib/components/admin/shop/studio/PublishPanel.svelte', 'Shop studio publish preview canvas'],
  ['src/lib/components/admin/shop/studio/controls/GradientEditor.svelte', 'Studio gradient editor swatches'],

  // 后台管理旧版表格与审计操作状态徽标（等待后续后台组件批次统一）
  ['src/routes/admin/ai/+page.svelte', 'Admin AI management status indicators'],
  ['src/routes/admin/attachments/+page.svelte', 'Admin attachments media preview canvas'],
  ['src/routes/admin/content/+page.svelte', 'Admin content moderation indicators'],
  ['src/routes/admin/moderation/+page.svelte', 'Admin moderation workbench status badges'],
  ['src/routes/admin/moderation/cases/+page.svelte', 'Admin moderation case list badges'],
  ['src/routes/admin/moderation/cases/[id]/+page.svelte', 'Admin moderation case detail badges'],
  ['src/routes/admin/notifications/+page.svelte', 'Admin notifications table indicators'],
  ['src/routes/admin/oauth/+page.svelte', 'Admin oauth provider table badges'],
  ['src/routes/admin/posts/+page.svelte', 'Admin posts management status indicators'],
  ['src/routes/admin/settings/+page.svelte', 'Admin system settings layout previews'],
  ['src/routes/admin/storage/+page.svelte', 'Admin storage health indicators'],
  ['src/routes/admin/users/+page.svelte', 'Admin users moderation actions indicators'],

  // 独立电商商品卡片媒体渐变底色
  ['src/routes/shop/+page.svelte', 'Shop item media preview container fallback'],

  // AI 审核提示与标记
  ['src/routes/tags/load.test.ts', 'Test mock fixtures']
]);

/**
 * 动态/运行时由 JS 写入或底层 blbui/浏览器标准提供的合法变量前缀或名称
 */
const DYNAMIC_VARIABLES = new Set([
  '--avatar-frame-color',
  '--avatar-frame-glow',
  '--avatar-radius',
  '--avatar-bg',
  '--avatar-color',
  '--board-color',
  '--cat-color',
  '--swatch',
  '--swatch-color',
  '--w',
  '--frame-scale',
  '--color-total',
  '--color-income',
  '--color-expense',
  '--chart-border',
  '--chart-muted',
  '--credits-rule',
  '--mobile-bottom-nav-offset',
  '--profile-effect-opacity',
  '--profile-effect-accent',
  '--profile-effect-duration'
]);

function isDynamicVar(name) {
  if (DYNAMIC_VARIABLES.has(name)) return true;
  if (name.startsWith('--bb-')) return true; // 数据型主题投影变量
  if (name.startsWith('--aui-')) return true; // blbui-core 组件系统变量
  return false;
}

function walkDir(dir, filter, list = []) {
  for (const entry of readdirSync(dir)) {
    const full = resolve(dir, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) {
      if (entry !== 'node_modules' && entry !== '.svelte-kit' && entry !== 'build') {
        walkDir(full, filter, list);
      }
    } else if (filter(full)) {
      list.push(full);
    }
  }
  return list;
}

function extractDefinedCustomProps() {
  const defined = new Set();
  const cssFiles = walkDir(STYLES_DIR, (f) => f.endsWith('.css'));
  cssFiles.push(resolve(SRC_DIR, 'app.css'));

  for (const f of cssFiles) {
    const text = readFileSync(f, 'utf8');
    const matches = text.matchAll(/^\s*(--[a-zA-Z0-9_-]+)\s*:/gm);
    for (const m of matches) {
      defined.add(m[1]);
    }
  }
  return defined;
}

function runAudit() {
  console.log('>>> [style-audit] 开始前端样式架构与 Design Token 合规性检查...\n');

  const definedProps = extractDefinedCustomProps();
  console.log(`[style-audit] 已加载 ${definedProps.size} 个定义好的 CSS 自定义属性.`);

  const svelteFiles = walkDir(SRC_DIR, (f) => f.endsWith('.svelte') && !f.includes('/testing/'));
  const cssFiles = walkDir(STYLES_DIR, (f) => f.endsWith('.css'));
  const allFiles = [...svelteFiles, ...cssFiles];

  let ghostTokenViolations = 0;
  let colorLiteralViolations = 0;

  // 1. 检查幽灵 Token：任何 var(--foo) 必须有定义或属于受控动态变量
  for (const file of allFiles) {
    const rel = relative(FRONTEND_ROOT, file);
    const content = readFileSync(file, 'utf8');

    // 匹配 var(--variable-name...)
    const varMatches = content.matchAll(/var\(\s*(--[a-zA-Z0-9_-]+)([\s,)])/g);
    for (const m of varMatches) {
      const varName = m[1];
      if (!definedProps.has(varName) && !isDynamicVar(varName)) {
        // 查找所在行号
        const line = content.slice(0, m.index).split('\n').length;
        console.error(`  [GHOST TOKEN] ${rel}:${line} 引用了未定义的变量 "${varName}"`);
        ghostTokenViolations++;
      }
    }
  }

  // 2. 检查硬编码颜色字面量（在非白名单的 Svelte 用户界面组件中）
  for (const file of svelteFiles) {
    const rel = relative(FRONTEND_ROOT, file);
    if (COLOR_LITERAL_ALLOWLIST.has(rel)) {
      continue;
    }

    const content = readFileSync(file, 'utf8');
    const lines = content.split('\n');

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      // 忽略 HTML 注释或 JS 注释
      const trimmed = line.trim();
      if (trimmed.startsWith('//') || trimmed.startsWith('/*') || trimmed.startsWith('*')) {
        continue;
      }

      // 匹配 #hex (3, 4, 6, 8 位)
      const hexMatches = line.matchAll(/#([0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{3,4})\b/g);
      for (const m of hexMatches) {
        // 排除 HTML 实体如 &#123;
        if (m.index > 0 && line[m.index - 1] === '&') continue;
        console.error(`  [COLOR LITERAL] ${rel}:${i + 1} 发现硬编码 Hex 颜色: "${m[0]}"`);
        colorLiteralViolations++;
      }

      // 匹配 rgb/rgba 或 hsl/hsla
      const rgbMatches = line.matchAll(/\b(rgba?|hsla?)\s*\([^)]*\)/g);
      for (const m of rgbMatches) {
        console.error(`  [COLOR LITERAL] ${rel}:${i + 1} 发现硬编码 RGB/HSL 颜色: "${m[0]}"`);
        colorLiteralViolations++;
      }
    }
  }

  console.log('\n--- 审计结果汇总 ---');
  console.log(`  检查文件总数: ${allFiles.length}`);
  console.log(`  未定义幽灵 Token 违规: ${ghostTokenViolations}`);
  console.log(`  硬编码颜色违规: ${colorLiteralViolations}`);

  if (ghostTokenViolations > 0 || colorLiteralViolations > 0) {
    console.error('\n❌ [style-audit] 样式合规性检查失败！请使用语义 Token 代替硬编码颜色或修复变量名。\n');
    process.exit(1);
  }

  console.log('\n✅ [style-audit] 样式架构与 Design Token 检查全部通过！\n');
}

runAudit();
