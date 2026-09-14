// M07-UI-05/06：衣柜 SSR——装备/卸下表单、徽章≤3、过期自动卸下、白名单
// Token 预览（未知 Token 不渲染）。
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import WardrobePage from '../../../routes/me/wardrobe/+page.svelte';
import type { Entitlement, Presentation } from '$lib/api/types';

const presentation: Presentation = {
  version: 4,
  nickname_color_id: 'e-color',
  avatar_frame_id: 'e-frame',
  profile_badge_ids: ['e-badge-1', 'e-badge-2'],
  presentation_tokens: {
    nickname_color: 'blue',
    avatar_frame: 'gold_ring',
    profile_badges: ['contributor', 'early_member'],
    unknown_slot: 'javascript:alert(1)' // 未知槽位必须被忽略
  },
  updated_at: 0
};

const entitlements: Entitlement[] = [
  { id: 'e-color', product_id: 'p1', product_title: '蓝色昵称', kind: 'cosmetic_nickname', slot: 'nickname_color', status: 'equipped', quantity: 1, remaining_quantity: 1, valid_from: 0, expires_at: null, created_at: 0, presentation_tokens: ['nickname.color.blue'] },
  { id: 'e-frame', product_id: 'p2', product_title: '金色头像框', kind: 'cosmetic_avatar', slot: 'avatar_frame', status: 'owned', quantity: 1, remaining_quantity: 1, valid_from: 0, expires_at: null, created_at: 0, presentation_tokens: ['avatar.frame.gold_ring'], asset_attachment_id: 'att-frame-1' },
  { id: 'e-badge-1', product_id: 'p3', product_title: '贡献者徽章', kind: 'cosmetic_badge', slot: 'profile_badges', status: 'equipped', quantity: 2, remaining_quantity: 1, valid_from: 0, expires_at: null, created_at: 0, presentation_tokens: ['badge.contributor'] },
  { id: 'e-badge-2', product_id: 'p4', product_title: '早期成员徽章', kind: 'cosmetic_badge', slot: 'profile_badges', status: 'equipped', quantity: 1, remaining_quantity: 1, valid_from: 0, expires_at: null, created_at: 0, presentation_tokens: ['badge.early_member'] },
  { id: 'e-expired', product_id: 'p5', product_title: '限时烟花', kind: 'profile_effect', slot: 'profile_effect', status: 'expired', quantity: 1, remaining_quantity: 0, valid_from: 0, expires_at: 1, created_at: 0, presentation_tokens: ['profile.effect.sparkle'] },
  { id: 'e-owned-badge', product_id: 'p6', product_title: '活跃达人徽章', kind: 'cosmetic_badge', slot: 'profile_badge', status: 'owned', quantity: 1, remaining_quantity: 1, valid_from: 0, expires_at: null, created_at: 0, presentation_tokens: ['badge.active'] }
];

describe('M07-UI-05 衣柜 SSR', () => {
  it('渲染白名单 Token 预览（昵称颜色/头像框/徽章），未知槽位不渲染', () => {
    const { body } = render(WardrobePage, {
      props: { data: { presentation, entitlements, error: null }, form: null }
    });
    expect(body).toContain('#0969da'); // nickname_color=blue → 固定调色板
    expect(body).toContain('avatar-frame-gold');
    expect(body).toContain('贡献者');
    expect(body).toContain('早期成员');
    // 未知 Token（含脚本串）不得渲染
    expect(body).not.toContain('javascript:');
  });

  it('权益行内预览：头像框 PNG 素材、徽章章面、装饰色板、昵称前缀', () => {
    const { body } = render(WardrobePage, {
      props: { data: { presentation, entitlements, error: null }, form: null }
    });
    // 头像框权益 → CosmeticAvatar 渲染 PNG 素材附件（稳定内容端点）
    expect(body).toContain('/api/v1/attachments/att-frame-1/content');
    // 可装备徽章（含早期 profile_badge 槽位名）→ 行内章面 + 中文槽位
    expect(body).toContain('活跃达人');
    expect(body).not.toContain('profile_badge</'); // 槽位名不再原样输出
    // 过期的主页装饰 → 历史行内星芒色板
    expect(body).toContain('swatch-sparkle');
    // 商品名缺失时回退 Token 中文标签
    const untitled: Entitlement[] = [
      { id: 'e-gold', product_id: 'prod-nick-color-gold', product_title: null, kind: 'cosmetic_nickname', slot: 'nickname_color', status: 'owned', quantity: 1, remaining_quantity: 1, valid_from: 0, expires_at: null, created_at: 0, presentation_tokens: ['nickname.color.gold'] }
    ];
    const { body: fallbackBody } = render(WardrobePage, {
      props: { data: { presentation: { ...presentation, presentation_tokens: { avatar_frame: 'gold_ring' } }, entitlements: untitled, error: null }, form: null }
    });
    expect(fallbackBody).toContain('鎏金'); // nickname.color.gold → 昵称颜色展示名
    expect(fallbackBody).not.toContain('prod-nick-color-gold');
  });

  it('已装备权益 → 卸下表单；未装备 → 装备表单（携带展示版本）', () => {
    const { body } = render(WardrobePage, {
      props: { data: { presentation, entitlements, error: null }, form: null }
    });
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/unequip"/);
    expect(body).toMatch(/<form[^>]*method="POST"[^>]*action="\?\/equip"/);
    expect(body).toContain('name="expected_presentation_version"');
    expect(body).toContain('value="4"'); // presentation.version
  });

  it('徽章最多 3 个：已装备 2 个 + 可装备徽章仍可装备；满 3 个后提示上限', () => {
    const { body } = render(WardrobePage, {
      props: { data: { presentation, entitlements, error: null }, form: null }
    });
    expect(body).toContain('活跃达人徽章');

    const atLimit = {
      ...presentation,
      profile_badge_ids: ['e-badge-1', 'e-badge-2', 'e-badge-3'],
      presentation_tokens: { ...presentation.presentation_tokens, profile_badges: ['contributor', 'early_member', 'veteran'] }
    };
    const fullEntitlements: Entitlement[] = [
      ...entitlements,
      { id: 'e-badge-3', product_id: 'p7', product_title: '资深徽章', kind: 'cosmetic_badge', slot: 'profile_badges', status: 'equipped', quantity: 1, remaining_quantity: 1, valid_from: 0, expires_at: null, created_at: 0 }
    ];
    const { body: full } = render(WardrobePage, {
      props: { data: { presentation: atLimit, entitlements: fullEntitlements, error: null }, form: null }
    });
    expect(full).toContain('资深徽章');
    expect(full).toContain('徽章最多 3 个');
  });

  it('过期权益 → 显示已过期 + 到期自动卸下，不提供装备入口', () => {
    const { body } = render(WardrobePage, {
      props: { data: { presentation, entitlements, error: null }, form: null }
    });
    expect(body).toContain('已过期');
    expect(body).toContain('已到期自动卸下');
  });

  it('load 错误 → 错误横幅', () => {
    const { body } = render(WardrobePage, {
      props: { data: { presentation: null, entitlements: [], error: '服务暂不可用' }, form: null }
    });
    expect(body).toContain('服务暂不可用');
  });
});
